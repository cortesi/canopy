//! Input routing and event dispatch for the canopy facade.

use ruau::vm::Scope;

use super::{AUTOMATION_SERVICE_BUDGET, AdapterEvent, Canopy};
use crate::{
    NodeId, commands,
    core::{
        Core, inputmap,
        inputmap::{ResolvedBinding, RunBinding, RunTarget},
        notice::NoticeSource,
        world::scroll::DefaultAction,
    },
    error::{Error, Result},
    geom::{Point, PointI32, Size},
    input::{
        BindingActionKind, BindingId, BindingPhase, Event, IntentName, KeyDispatchDivergence,
        KeyExpectation, KeyRouteExplanation, KeyRouteStep, RouteOutcome, StepBinding, key, mouse,
    },
    path::Path,
    script::LuauFunctionId,
    widget::EventOutcome,
};

/// What one entry in a key or mouse route trace records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteTraceKind {
    /// The route start was selected.
    Start,
    /// A before-widget binding matched.
    BeforeWidgetBinding,
    /// An intent was offered to the node's widget, or declined there.
    OfferIntent,
    /// The event was offered to the node's widget, or the widget's key
    /// prediction disagreed with its result.
    Widget,
    /// An after-widget binding matched after the widget ignored the event.
    AfterWidgetBinding,
    /// A resolved binding ran.
    RunBinding,
    /// The runtime applied the input's default action to a node.
    DefaultAction,
    /// Routing moved from a node to its parent.
    Bubble,
    /// A widget or binding handled the event.
    Handled,
    /// Routing ended without a handler.
    Unhandled,
    /// A binding or a widget handler failed, and the failure became a notice
    /// that consumed the input.
    Notice,
}

impl RouteTraceKind {
    /// Every trace kind, in the order routing can record them.
    pub(crate) const ALL: [Self; 11] = [
        Self::Start,
        Self::BeforeWidgetBinding,
        Self::OfferIntent,
        Self::Widget,
        Self::AfterWidgetBinding,
        Self::RunBinding,
        Self::DefaultAction,
        Self::Bubble,
        Self::Handled,
        Self::Unhandled,
        Self::Notice,
    ];

    /// Return a stable scripting and diagnostic label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::BeforeWidgetBinding => "before_widget_binding",
            Self::OfferIntent => "offer_intent",
            Self::Widget => "widget",
            Self::AfterWidgetBinding => "after_widget_binding",
            Self::RunBinding => "run_binding",
            Self::DefaultAction => "default_action",
            Self::Bubble => "bubble",
            Self::Handled => "handled",
            Self::Unhandled => "unhandled",
            Self::Notice => "notice",
        }
    }
}

/// One entry in the most recent input route trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteTraceEntry {
    /// What this entry records.
    pub kind: RouteTraceKind,
    /// Node associated with this route step.
    pub node: Option<NodeId>,
    /// Path visible to binding resolution at this route step.
    pub path: String,
    /// Human-readable route detail.
    pub detail: String,
}

/// Input routed through the shared bubbling pipeline.
#[derive(Clone, Copy)]
enum RoutedInput {
    /// Key input.
    Key(key::Key),
    /// Mouse input in screen coordinates.
    Mouse(mouse::MouseEvent),
}

impl RoutedInput {
    /// Return the binding input spec for this routed input.
    fn input_spec(self) -> inputmap::InputSpec {
        match self {
            Self::Key(key) => inputmap::InputSpec::Key(key),
            Self::Mouse(mouse) => inputmap::InputSpec::Mouse(mouse.into()),
        }
    }

    /// Return the event to dispatch to a specific node.
    fn event_for_node(self, core: &Core, node_id: NodeId) -> Event {
        match self {
            Self::Key(key) => Event::Key(key),
            Self::Mouse(mouse) => Event::Mouse(Self::local_mouse(core, node_id, mouse)),
        }
    }

    /// Return the action the runtime applies when a route node declines this
    /// input.
    fn default_action(self) -> Option<DefaultAction> {
        match self {
            Self::Key(_) => None,
            Self::Mouse(mouse) => mouse.action.scroll_delta().map(DefaultAction::Scroll),
        }
    }

    /// Return a short diagnostic label.
    fn label(self) -> &'static str {
        match self {
            Self::Key(_) => "key",
            Self::Mouse(_) => "mouse",
        }
    }

    /// Convert a screen-space mouse event to a node-local event.
    ///
    /// The location becomes relative to the node's content origin. It stays
    /// signed, so padding above or left of the content, and captured events
    /// beyond the node, keep their true offset.
    fn local_mouse(core: &Core, node_id: NodeId, mouse: mouse::MouseEvent) -> mouse::MouseEvent {
        let view = core
            .nodes
            .get(node_id)
            .map(|node| node.view)
            .unwrap_or_default();
        let location = PointI32::clamped_from_i64(
            i64::from(mouse.location.x) - i64::from(view.content.tl.x),
            i64::from(mouse.location.y) - i64::from(view.content.tl.y),
        );
        mouse::MouseEvent { location, ..mouse }
    }
}

impl Canopy {
    /// Return the node under a screen location, or none off screen.
    fn node_at(&self, location: PointI32) -> Result<Option<NodeId>> {
        match Point::try_from(location) {
            Ok(screen) => self.core.locate_node(self.core.root, screen),
            Err(_) => Ok(None),
        }
    }

    /// Return the starting target and binding path for a mouse event.
    fn mouse_route_start(&mut self, location: PointI32) -> Result<(Option<NodeId>, Path)> {
        if let Some(modal) = self.core.modal_region() {
            let hit = self.node_at(location)?;
            if !hit.is_some_and(|node| self.core.is_ancestor_or_self(modal, node)) {
                return Ok((None, Path::empty()));
            }
        }
        if let Some(capture) = self.core.mouse_capture {
            if self.core.validate_attached_node(capture).is_ok() && self.core.modal_admits(capture)
            {
                return Ok((Some(capture), self.core.path_of(self.core.root, capture)));
            } else {
                self.core.clear_mouse_capture()?;
            }
        }

        let target = self.node_at(location)?;
        let path = target
            .map(|id| self.core.path_of(self.core.root, id))
            .unwrap_or_else(Path::empty);
        Ok((target, path))
    }

    /// Add one entry to the current route trace.
    fn trace_route(
        &mut self,
        kind: RouteTraceKind,
        node: Option<NodeId>,
        path: &Path,
        detail: impl Into<String>,
    ) {
        self.route_trace.push(RouteTraceEntry {
            kind,
            node,
            path: path.to_string(),
            detail: detail.into(),
        });
    }

    /// End a route whose binding or widget handler failed.
    ///
    /// A notice-class failure is recorded and traced, and consumes the input.
    /// Any other failure is fatal and returns.
    fn route_notice(
        &mut self,
        error: Error,
        source: NoticeSource,
        node: NodeId,
        path: &Path,
    ) -> Result<bool> {
        self.notice_or_fail(error, source, Some(node))?;
        let message = self
            .core
            .notices
            .entries()
            .last()
            .map(|notice| notice.message.clone())
            .unwrap_or_default();
        self.trace_route(RouteTraceKind::Notice, Some(node), path, message);
        Ok(true)
    }

    /// Propagate a key or mouse event through one bubbling route.
    ///
    /// `scope` carries an active script scope so Luau bindings run inside it.
    fn route_input(
        &mut self,
        start: Option<NodeId>,
        path: Path,
        input: RoutedInput,
        scope: Option<&Scope<'_>>,
        guard: Option<&mut KeyRouteGuard>,
    ) -> Result<bool> {
        self.with_dispatch_boundary(|canopy| {
            canopy.route_input_inner(start, path, input, scope, guard)
        })
    }

    /// Route one synchronous input inside a shared completion boundary.
    fn route_input_inner(
        &mut self,
        start: Option<NodeId>,
        mut path: Path,
        input: RoutedInput,
        scope: Option<&Scope<'_>>,
        mut guard: Option<&mut KeyRouteGuard>,
    ) -> Result<bool> {
        self.route_trace.clear();
        if self.core.modal_region().is_some()
            && !start.is_some_and(|node| self.core.modal_admits(node))
        {
            return Ok(true);
        }
        let modal_owner = self.core.modal_owner();
        self.trace_route(
            RouteTraceKind::Start,
            start,
            &path,
            format!("{} route selected", input.label()),
        );

        let mut target = start;
        let mut excluded: Vec<BindingId> = Vec::new();
        while let Some(id) = target {
            if !self.core.nodes.contains_key(id) {
                self.trace_route(
                    RouteTraceKind::Unhandled,
                    Some(id),
                    &path,
                    "target node disappeared",
                );
                return Ok(false);
            }
            if let Some(guard) = guard.as_deref_mut() {
                guard.observe_node(id);
                if guard.tripped() {
                    return Ok(true);
                }
            }

            let event = input.event_for_node(&self.core, id);
            let route_focus = start.unwrap_or(id);
            let mut fallback_binding = None;
            let mut selected = self.select_at(input, id, &path, route_focus, &excluded);
            while let Some(binding) = selected {
                match binding {
                    ResolvedBinding::Offer {
                        id: binding_id,
                        intent,
                    } => {
                        self.trace_route(
                            RouteTraceKind::OfferIntent,
                            Some(id),
                            &path,
                            format!("offered intent {intent}"),
                        );
                        let outcome = match self
                            .offer_intent(binding_id, &intent, id, input, &mut guard)
                        {
                            Ok(Some(outcome)) => outcome,
                            Ok(None) => return Ok(true),
                            Err(error) => {
                                return self.route_notice(error, NoticeSource::Widget, id, &path);
                            }
                        };
                        if outcome == EventOutcome::Handle {
                            self.trace_route(
                                RouteTraceKind::Handled,
                                Some(id),
                                &path,
                                format!("intent {intent} handled"),
                            );
                            return Ok(true);
                        }
                        self.trace_route(
                            RouteTraceKind::OfferIntent,
                            Some(id),
                            &path,
                            format!("intent {intent} declined after acceptance"),
                        );
                        excluded.push(binding_id);
                        if !self.core.nodes.contains_key(id) {
                            self.trace_route(
                                RouteTraceKind::Unhandled,
                                Some(id),
                                &path,
                                "node removed while running an intent",
                            );
                            return Ok(true);
                        }
                        selected = self.select_at(input, id, &path, route_focus, &excluded);
                    }
                    ResolvedBinding::Run(binding) => match binding.phase {
                        BindingPhase::BeforeWidget => {
                            self.trace_route(
                                RouteTraceKind::BeforeWidgetBinding,
                                Some(id),
                                &path,
                                "matched before widget event",
                            );
                            if let Some(guard) = guard.as_deref_mut() {
                                guard.observe_binding(binding.id, BindingPhase::BeforeWidget);
                                if guard.tripped() {
                                    return Ok(true);
                                }
                            }
                            return self.execute_routed_binding_with_scope(
                                id, &path, input, binding, scope,
                            );
                        }
                        BindingPhase::AfterWidget => {
                            fallback_binding = Some(binding);
                            break;
                        }
                    },
                }
            }
            self.trace_route(
                RouteTraceKind::Widget,
                Some(id),
                &path,
                format!("{event:?}"),
            );
            let outcome = if self.core.modal_admits(id) {
                // The prediction is read before the widget acts, for the same
                // pre-event state. A widget that cannot be read gives none,
                // and dispatch then reports the failure itself.
                let predicted = match input {
                    RoutedInput::Key(key) => self.core.node_key_outcome(id, key, route_focus).ok(),
                    RoutedInput::Mouse(_) => None,
                };
                let dispatched = self.with_dispatch_boundary(|canopy| {
                    canopy.core.dispatch_event_on_node(id, &event)
                });
                let outcome = match dispatched {
                    Ok(outcome) => outcome,
                    Err(error) => return self.route_notice(error, NoticeSource::Widget, id, &path),
                };
                if let Some(predicted) = predicted {
                    if predicted != outcome {
                        self.trace_route(
                            RouteTraceKind::Widget,
                            Some(id),
                            &path,
                            format!(
                                "key prediction mismatch: predicted {predicted:?}, actual {outcome:?}"
                            ),
                        );
                    }
                    debug_assert_eq!(
                        predicted, outcome,
                        "key prediction mismatch at node {id:?} path {path} for {event:?}"
                    );
                }
                outcome
            } else {
                EventOutcome::Ignore
            };
            if let Some(guard) = guard.as_deref_mut() {
                guard.observe_widget(outcome);
                if guard.tripped() {
                    return Ok(true);
                }
            }

            match outcome {
                EventOutcome::Handle => {
                    self.trace_route(
                        RouteTraceKind::Handled,
                        Some(id),
                        &path,
                        format!("{outcome:?}"),
                    );
                    return Ok(true);
                }
                EventOutcome::Ignore => {
                    if let Some(binding) = fallback_binding {
                        self.trace_route(
                            RouteTraceKind::AfterWidgetBinding,
                            Some(id),
                            &path,
                            "matched after widget ignored event",
                        );
                        if let Some(guard) = guard.as_deref_mut() {
                            guard.observe_binding(binding.id, BindingPhase::AfterWidget);
                            if guard.tripped() {
                                return Ok(true);
                            }
                        }
                        return self
                            .execute_routed_binding_with_scope(id, &path, input, binding, scope);
                    }
                    // The callback may have removed the node; a missing node
                    // cannot move.
                    if let Some(action) = input.default_action()
                        && self.core.modal_admits(id)
                        && self.core.apply_default_action(id, action)
                    {
                        self.trace_route(
                            RouteTraceKind::DefaultAction,
                            Some(id),
                            &path,
                            format!("{action:?}"),
                        );
                        self.trace_route(
                            RouteTraceKind::Handled,
                            Some(id),
                            &path,
                            "default action applied",
                        );
                        return Ok(true);
                    }
                    self.trace_route(RouteTraceKind::Bubble, Some(id), &path, "ignored");
                    if modal_owner == Some(id) {
                        if let Some(guard) = guard.as_deref_mut() {
                            guard.observe_end();
                            if guard.tripped() {
                                return Ok(true);
                            }
                        }
                        return Ok(true);
                    }
                    // Handlers can change the tree, so the next node is read
                    // only now. The modal owner ended the walk above, so this
                    // is the step `Core::route` takes.
                    target = self.core.route_step(id);
                    path.pop();
                }
            }
        }

        if let Some(guard) = guard {
            guard.observe_end();
            if guard.tripped() {
                return Ok(true);
            }
        }
        self.trace_route(RouteTraceKind::Unhandled, None, &path, "no handler");
        Ok(false)
    }

    /// Send `key` only when its prospective route matches `expectation`.
    ///
    /// The analysis runs inside the same dispatch boundary as the route, and a
    /// guard compares each actual step before it acts. An unexpected widget
    /// stops the route with a structured divergence; earlier steps may already
    /// have observed the key.
    pub fn send_key_checked<T>(&mut self, key: T, expectation: KeyExpectation) -> Result<()>
    where
        T: Into<key::Key>,
    {
        self.key_checked(None, key, expectation)
    }

    /// Send a checked key inside an active script scope.
    ///
    /// Script-originated dispatch carries the live scope so bindings the route
    /// executes can re-enter the VM.
    pub(crate) fn key_checked<T>(
        &mut self,
        scope: Option<&Scope<'_>>,
        key: T,
        expectation: KeyExpectation,
    ) -> Result<()>
    where
        T: Into<key::Key>,
    {
        let key = key.into();
        self.with_dispatch_boundary(|canopy| canopy.key_checked_inner(scope, key, expectation))
    }

    /// Run one checked key route inside its completion boundary.
    fn key_checked_inner(
        &mut self,
        scope: Option<&Scope<'_>>,
        key: key::Key,
        expectation: KeyExpectation,
    ) -> Result<()> {
        let start = self.focus_or_root()?;
        let path = self.core.path_of(self.core.root, start);
        let explanation = self.core.explain_key(Some(start), key)?;
        if !expectation.matches(&explanation.outcome) {
            return Err(Error::KeyDispatchDivergence(Box::new(
                KeyDispatchDivergence {
                    expected: expectation,
                    analysis_step: explanation.steps.last().cloned(),
                    actual_widget: None,
                    actual_binding: None,
                },
            )));
        }
        let mut guard = KeyRouteGuard::new(&explanation, expectation);
        let changed = self.route_key(start, path, scope, key, Some(&mut guard))?;
        guard.finish();
        if let Some(divergence) = guard.divergence {
            return Err(Error::KeyDispatchDivergence(Box::new(divergence)));
        }
        if changed {
            self.core.invalidate(crate::Invalidation::Paint);
        }
        Ok(())
    }

    /// Route one key through the transient shortcut or the normal guarded walk.
    fn route_key(
        &mut self,
        start: NodeId,
        path: Path,
        scope: Option<&Scope<'_>>,
        key: key::Key,
        guard: Option<&mut KeyRouteGuard>,
    ) -> Result<bool> {
        self.dismiss_notice()?;
        if self.core.effective_transient_mode().is_some() {
            self.with_dispatch_boundary(|canopy| {
                canopy.route_transient_key(start, &path, key, scope, guard)
            })
        } else {
            self.route_input(Some(start), path, RoutedInput::Key(key), scope, guard)
        }
    }

    /// Route one key taken by a transient mode.
    ///
    /// The mode pops before its binding runs, so the binding can enter another
    /// mode. No widget sees the key, and a key the mode does not bind only
    /// pops the mode. The winner is searched on the modal-bounded route, as
    /// key analysis does.
    fn route_transient_key(
        &mut self,
        start: NodeId,
        path: &Path,
        key: key::Key,
        scope: Option<&Scope<'_>>,
        mut guard: Option<&mut KeyRouteGuard>,
    ) -> Result<bool> {
        self.route_trace.clear();
        let input = RoutedInput::Key(key);
        self.trace_route(
            RouteTraceKind::Start,
            Some(start),
            path,
            "key route selected for a transient mode",
        );
        let winner = self
            .core
            .transient_winner(start, key)
            .map(|(id, path, record)| (id, path, record.resolved()));
        self.core.input_map.pop_mode();
        let Some((id, path, binding)) = winner else {
            self.trace_route(RouteTraceKind::Handled, None, path, "transient mode ended");
            if let Some(guard) = guard.as_deref_mut() {
                guard.observe_end();
            }
            return Ok(true);
        };
        if let Some(guard) = guard.as_deref_mut() {
            guard.observe_node(id);
            if guard.tripped() {
                return Ok(true);
            }
        }
        let binding = match binding {
            ResolvedBinding::Offer {
                id: binding_id,
                intent,
            } => {
                self.trace_route(
                    RouteTraceKind::OfferIntent,
                    Some(id),
                    &path,
                    format!("offered intent {intent} in a transient mode"),
                );
                let outcome = match self.offer_intent(binding_id, &intent, id, input, &mut guard) {
                    Ok(Some(outcome)) => outcome,
                    Ok(None) => return Ok(true),
                    Err(error) => {
                        return self.route_notice(error, NoticeSource::Widget, id, &path);
                    }
                };
                // A transient decision is spent once the mode pops, so a
                // declined intent ends the key as a transient dismissal. It
                // does not reselect into the default tier or run raw.
                let detail = if outcome == EventOutcome::Handle {
                    format!("intent {intent} handled")
                } else {
                    format!("intent {intent} declined after acceptance")
                };
                self.trace_route(RouteTraceKind::Handled, Some(id), &path, detail);
                return Ok(true);
            }
            ResolvedBinding::Run(binding) => binding,
        };
        self.trace_route(
            RouteTraceKind::BeforeWidgetBinding,
            Some(id),
            &path,
            "matched in a transient mode",
        );
        if let Some(guard) = guard {
            guard.observe_binding(binding.id, BindingPhase::BeforeWidget);
            if guard.tripped() {
                return Ok(true);
            }
        }
        self.execute_routed_binding_with_scope(id, &path, input, binding, scope)
    }

    /// Run a command or callback binding after route resolution, preserving
    /// an active script scope.
    fn execute_routed_binding_with_scope(
        &mut self,
        node_id: NodeId,
        path: &Path,
        input: RoutedInput,
        binding: RunBinding,
        scope: Option<&Scope<'_>>,
    ) -> Result<bool> {
        self.trace_route(
            RouteTraceKind::RunBinding,
            Some(node_id),
            path,
            binding.description,
        );

        let event = input.event_for_node(&self.core, node_id);
        let depth = self.core.push_event_scope(&event);
        // The run has its own completion boundary, so a failed run drops the
        // removals it queued even when its failure becomes a notice.
        let result = self.with_dispatch_boundary(|canopy| match binding.target {
            RunTarget::Script(function) => canopy
                .execute_binding_with_scope(node_id, function, scope)
                .map(|()| None),
            RunTarget::Menu(mode) => {
                canopy.push_transient_mode(&mode);
                Ok(None)
            }
            RunTarget::Command(call) => {
                // Eligibility is read here, inside the event scope, rather than
                // taken from the last frame, so a status hook sees the same
                // injections the command would.
                match commands::command_status(&canopy.core, node_id, &call) {
                    Ok(commands::CommandStatus::Disabled(reason)) => Ok(Some(reason)),
                    Ok(commands::CommandStatus::Enabled) => {
                        commands::dispatch(&mut canopy.core, node_id, &call)
                            .map(|_| None)
                            .map_err(Into::into)
                    }
                    Err(error) => Err(error),
                }
            }
        });
        self.core.pop_event_scope(depth);
        let skipped = match result {
            Ok(skipped) => skipped,
            Err(error) => return self.route_notice(error, NoticeSource::Binding, node_id, path),
        };

        // A disabled winner still consumes its input. Falling through would let
        // an ancestor act on a control the user saw as unavailable.
        let detail = skipped.map_or_else(
            || "binding completed".to_string(),
            |reason| format!("binding disabled: {reason}"),
        );
        self.trace_route(RouteTraceKind::Handled, Some(node_id), path, detail);
        Ok(true)
    }

    /// Propagate a mouse event through the node under the event and all its
    /// ancestors.
    ///
    /// `scope` carries an active script scope for a script-originated event.
    pub(crate) fn mouse(&mut self, scope: Option<&Scope<'_>>, m: mouse::MouseEvent) -> Result<()> {
        // A bare pointer move is not a response to a notice.
        if m.action != mouse::Action::Moved {
            self.dismiss_notice()?;
        }
        let (target, path) = self.mouse_route_start(m.location)?;
        let changed = self.route_input(target, path, RoutedInput::Mouse(m), scope, None)?;
        if changed {
            self.core.invalidate(crate::Invalidation::Paint);
        }
        Ok(())
    }

    /// Propagate a key event through the focus and all its ancestors.
    ///
    /// `scope` carries an active script scope for a script-originated event.
    pub(crate) fn key<T>(&mut self, scope: Option<&Scope<'_>>, tk: T) -> Result<()>
    where
        T: Into<key::Key>,
    {
        let start = self.focus_or_root()?;
        let path = self.core.path_of(self.core.root, start);
        let changed = self.route_key(start, path, scope, tk.into(), None)?;
        if changed {
            self.core.invalidate(crate::Invalidation::Paint);
        }
        Ok(())
    }

    /// Return the focused node, focusing the first candidate when nothing holds
    /// focus.
    fn focus_or_root(&mut self) -> Result<NodeId> {
        if self.core.focus.is_none() {
            self.core.focus_first(self.core.root)?;
        }
        Ok(self.core.focus.unwrap_or(self.core.root))
    }

    /// Offer one selected intent to its consumer.
    ///
    /// Returns the consumer's outcome, or `None` when a checked-route guard
    /// tripped; the caller then ends the route without acting further.
    fn offer_intent(
        &mut self,
        binding: BindingId,
        action: &IntentName,
        node: NodeId,
        input: RoutedInput,
        guard: &mut Option<&mut KeyRouteGuard>,
    ) -> Result<Option<EventOutcome>> {
        let event = input.event_for_node(&self.core, node);
        let outcome = self.with_dispatch_boundary(|canopy| {
            canopy
                .core
                .dispatch_action_on_node(node, action.as_str(), &event)
        })?;
        debug_assert_eq!(
            outcome,
            EventOutcome::Handle,
            "accepts_intent promised Handle for {action} at {node:?}"
        );
        if let Some(guard) = guard.as_deref_mut() {
            guard.observe_action(binding, node);
            if guard.tripped() {
                return Ok(None);
            }
        }
        Ok(Some(outcome))
    }

    /// Select the first eligible binding at one route node, and copy it to
    /// run.
    fn select_at(
        &self,
        input: RoutedInput,
        node: NodeId,
        path: &Path,
        focus: NodeId,
        excluded: &[BindingId],
    ) -> Option<ResolvedBinding> {
        self.core
            .select_binding(node, path, input.input_spec(), focus, excluded)
            .map(inputmap::BindingRecord::resolved)
    }

    /// Dispatch a paste or focus event to the focused node, bubbling as
    /// needed.
    ///
    /// A paste is input, so it dismisses the shown notice. A handler's
    /// notice-class failure becomes a notice.
    fn dispatch_focus_event(&mut self, event: &Event) -> Result<()> {
        if matches!(event, Event::Paste(_)) {
            self.dismiss_notice()?;
        }
        let start = self.focus_or_root()?;
        let dispatched =
            self.with_dispatch_boundary(|canopy| canopy.core.dispatch_event(start, event));
        match dispatched {
            Ok(_) => Ok(()),
            Err(error) => self.notice_or_fail(error, NoticeSource::Widget, None),
        }
    }

    /// Service a bounded batch of callbacks marshalled onto the UI thread.
    ///
    /// The in-crate run loop calls this after receiving an adapter wake. The
    /// return value is the number of callbacks executed during this turn.
    pub(crate) fn service_automation(&mut self) -> usize {
        let mut serviced = 0;
        while serviced < AUTOMATION_SERVICE_BUDGET {
            let Ok(callback) = self.automation_rx.try_recv() else {
                break;
            };
            self.service_message(callback);
            serviced += 1;
        }
        if serviced == AUTOMATION_SERVICE_BUDGET {
            let _receiver_closed = self.event_tx.unbounded_send(AdapterEvent::Wake);
        }
        serviced
    }

    /// Propagate an event through the tree.
    pub(crate) fn event(&mut self, e: &Event) -> Result<()> {
        self.with_dispatch_boundary(|canopy| canopy.dispatch_input(e))
    }

    /// Dispatch an event inside its completion boundary.
    fn dispatch_input(&mut self, e: &Event) -> Result<()> {
        match e {
            Event::Key(k) => self.key(None, *k),
            Event::Mouse(m) => self.mouse(None, *m),
            Event::Resize(s) => {
                self.core.invalidate(crate::Invalidation::Paint);
                self.set_screen_size(*s)
            }
            Event::Paste(_) | Event::FocusGained | Event::FocusLost => {
                self.core.invalidate(crate::Invalidation::Paint);
                self.dispatch_focus_event(e)
            }
        }
    }

    /// Set the size on the root node.
    pub fn set_screen_size(&mut self, size: Size) -> Result<()> {
        self.frame.render_limits.cell_count(size)?;
        self.frame.screen_size = Some(size);
        self.core.invalidate(crate::Invalidation::Layout);
        Ok(())
    }

    /// Call a bound Luau closure, re-entering the live scope when one is
    /// active.
    fn execute_binding_with_scope(
        &mut self,
        node_id: NodeId,
        binding: LuauFunctionId,
        scope: Option<&Scope<'_>>,
    ) -> Result<()> {
        let host = self.script.host.clone();
        match scope {
            Some(scope) => host.call_function_in_scope(scope, node_id, binding),
            None => host.call_function(self, node_id, binding),
        }
    }

    /// Release removed script closures and return the count of all removed
    /// bindings.
    pub(crate) fn release_removed_bindings(
        &mut self,
        removed: Vec<(inputmap::BindingId, inputmap::BindingAction)>,
    ) -> usize {
        let removed_count = removed.len();
        for (_, target) in removed {
            if let inputmap::BindingAction::Script(binding) = target {
                self.release_binding_target(binding);
            }
        }
        removed_count
    }

    /// Release the script host's reference to a bound closure.
    pub(crate) fn release_binding_target(&mut self, binding: LuauFunctionId) {
        if let Some(releases) = &mut self.script.deferred_binding_releases {
            releases.push(binding);
        } else {
            self.script.host.release_function(binding);
        }
    }
}

/// One expected event on a checked route.
#[derive(Clone, Debug, PartialEq, Eq)]
enum GuardEvent {
    /// Route reached a node.
    Node(NodeId),
    /// Route resolved and executed a binding.
    Binding(BindingId, BindingPhase),
    /// Route offered an action to a consumer node.
    Action(BindingId, NodeId),
    /// Widget returned an outcome.
    Widget(EventOutcome),
    /// Route ended without a handler.
    End,
}

/// Compare a checked route with its prospective analysis step by step.
struct KeyRouteGuard {
    /// Expected events with the analysis step each belongs to.
    events: Vec<(GuardEvent, Option<usize>)>,
    /// Analysis steps, for divergence detail.
    steps: Vec<KeyRouteStep>,
    /// Expectation the caller supplied.
    expected: KeyExpectation,
    /// Index of the next expected event.
    index: usize,
    /// First divergence observed, if any.
    divergence: Option<KeyDispatchDivergence>,
}

impl KeyRouteGuard {
    /// Build the expected event stream for one analysis.
    fn new(explanation: &KeyRouteExplanation, expected: KeyExpectation) -> Self {
        let mut events: Vec<(GuardEvent, Option<usize>)> = Vec::new();
        for (step_index, step) in explanation.steps.iter().enumerate() {
            let mut push = |event| events.push((event, Some(step_index)));
            push(GuardEvent::Node(step.node));
            match step.binding {
                Some(StepBinding {
                    id,
                    kind: BindingActionKind::Intent,
                    ..
                }) => push(GuardEvent::Action(id, step.node)),
                Some(StepBinding {
                    id,
                    phase: BindingPhase::BeforeWidget,
                    ..
                }) => push(GuardEvent::Binding(id, BindingPhase::BeforeWidget)),
                Some(StepBinding {
                    id,
                    phase: BindingPhase::AfterWidget,
                    ..
                }) => {
                    push(GuardEvent::Widget(step.widget));
                    if explanation
                        .outcome
                        .winner()
                        .is_some_and(|winner| winner.binding == id)
                    {
                        push(GuardEvent::Binding(id, BindingPhase::AfterWidget));
                    }
                }
                None => push(GuardEvent::Widget(step.widget)),
            }
        }
        // A transient explanation carries no route steps, so its outcome
        // supplies the expected stream.
        match &explanation.outcome {
            RouteOutcome::Transient(winner) => {
                events.push((GuardEvent::Node(winner.node), None));
                let event = match winner.kind {
                    BindingActionKind::Intent => GuardEvent::Action(winner.binding, winner.node),
                    BindingActionKind::Script
                    | BindingActionKind::Command
                    | BindingActionKind::Menu => {
                        GuardEvent::Binding(winner.binding, BindingPhase::BeforeWidget)
                    }
                };
                events.push((event, None));
            }
            RouteOutcome::TransientDismiss | RouteOutcome::Unhandled => {
                events.push((GuardEvent::End, None));
            }
            RouteOutcome::Binding(_) | RouteOutcome::Widget { .. } => {}
        }
        Self {
            events,
            steps: explanation.steps.clone(),
            expected,
            index: 0,
            divergence: None,
        }
    }

    /// Return whether a divergence has been observed.
    fn tripped(&self) -> bool {
        self.divergence.is_some()
    }

    /// Observe one visited node.
    fn observe_node(&mut self, node: NodeId) {
        self.expect(&GuardEvent::Node(node), None, None);
    }

    /// Observe one executed binding.
    fn observe_binding(&mut self, binding: BindingId, phase: BindingPhase) {
        self.expect(&GuardEvent::Binding(binding, phase), None, Some(binding));
    }

    /// Observe one action offered to a consumer node.
    fn observe_action(&mut self, binding: BindingId, node: NodeId) {
        self.expect(&GuardEvent::Action(binding, node), None, Some(binding));
    }

    /// Observe one widget outcome before the route acts on it.
    fn observe_widget(&mut self, outcome: EventOutcome) {
        self.expect(&GuardEvent::Widget(outcome), Some(outcome), None);
    }

    /// Observe the route ending without a handler.
    fn observe_end(&mut self) {
        self.expect(&GuardEvent::End, None, None);
    }

    /// Reject a route that returned before consuming its expected events.
    fn finish(&mut self) {
        if !self.tripped() && self.index != self.events.len() {
            self.record_divergence(None, None);
        }
    }

    /// Compare one actual event with the next expected event.
    fn expect(
        &mut self,
        actual: &GuardEvent,
        actual_widget: Option<EventOutcome>,
        actual_binding: Option<BindingId>,
    ) {
        if self.tripped() {
            return;
        }
        let matched = self
            .events
            .get(self.index)
            .is_some_and(|(expected, _)| expected == actual);
        if matched {
            self.index += 1;
            return;
        }
        self.record_divergence(actual_widget, actual_binding);
    }

    /// Record the first divergence at the next expected analysis step.
    fn record_divergence(
        &mut self,
        actual_widget: Option<EventOutcome>,
        actual_binding: Option<BindingId>,
    ) {
        let step = self
            .events
            .get(self.index)
            .and_then(|(_, step)| *step)
            .and_then(|index| self.steps.get(index).cloned());
        self.divergence = Some(KeyDispatchDivergence {
            expected: self.expected,
            analysis_step: step,
            actual_widget,
            actual_binding,
        });
    }
}

#[cfg(test)]
mod guard_tests {
    use super::*;

    #[test]
    fn an_incomplete_checked_route_diverges() -> Result<()> {
        let canopy = super::super::CanopyBuilder::new().build()?;
        let explanation = canopy.core.explain_key(None, 'x'.into())?;
        let mut guard = KeyRouteGuard::new(&explanation, KeyExpectation::Unhandled);

        guard.finish();

        assert!(guard.tripped());
        assert_eq!(
            guard
                .divergence
                .as_ref()
                .and_then(|divergence| divergence.analysis_step.as_ref())
                .map(|step| step.node),
            explanation.steps.first().map(|step| step.node)
        );
        Ok(())
    }

    #[test]
    fn a_widget_divergence_records_the_actual_outcome() -> Result<()> {
        let canopy = super::super::CanopyBuilder::new().build()?;
        let explanation = canopy.core.explain_key(None, 'x'.into())?;
        let mut guard = KeyRouteGuard::new(&explanation, KeyExpectation::Unhandled);
        let step = explanation.steps.first().expect("root step");

        guard.observe_node(step.node);
        guard.observe_widget(EventOutcome::Handle);

        let divergence = guard.divergence.expect("widget divergence");
        assert_eq!(divergence.analysis_step, Some(step.clone()));
        assert_eq!(divergence.actual_widget, Some(EventOutcome::Handle));
        assert_eq!(divergence.actual_binding, None);
        Ok(())
    }

    #[test]
    fn a_binding_divergence_records_the_actual_binding() -> Result<()> {
        let mut canopy = super::super::CanopyBuilder::new().build()?;
        canopy.eval_script(r#"canopy.bind("x", { description = "Expected" }, function() end)"#)?;
        let explanation = canopy.core.explain_key(None, 'x'.into())?;
        let RouteOutcome::Binding(winner) = &explanation.outcome else {
            panic!("expected a binding");
        };
        assert_eq!(winner.phase, BindingPhase::AfterWidget);
        let expected = winner.binding;
        let actual = BindingId::from_u64(expected.as_u64() + 1);
        let mut guard = KeyRouteGuard::new(&explanation, KeyExpectation::Binding(expected));
        let step = explanation.steps.first().expect("root step");

        guard.observe_node(step.node);
        guard.observe_widget(EventOutcome::Ignore);
        guard.observe_binding(actual, BindingPhase::AfterWidget);

        let divergence = guard.divergence.expect("binding divergence");
        assert_eq!(divergence.analysis_step, Some(step.clone()));
        assert_eq!(divergence.actual_widget, None);
        assert_eq!(divergence.actual_binding, Some(actual));
        Ok(())
    }
}
