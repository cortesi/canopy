//! Input routing and event dispatch for the canopy facade.

use ruau::vm::Scope;

use super::{AUTOMATION_SERVICE_BUDGET, AdapterEvent, Canopy, RoutePhase, RouteTraceEntry};
use crate::{
    NodeId, commands,
    core::{Core, inputmap, world::scroll::DefaultAction},
    error::Result,
    event::{Event, key, mouse},
    geom::{Point, PointI32, Size},
    path::Path,
    script::LuauFunctionId,
    widget::EventOutcome,
};

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
            if self.core.validate_attached_node(capture).is_ok()
                && self.core.interaction_admits(capture)
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
        phase: RoutePhase,
        node: Option<NodeId>,
        path: &Path,
        detail: impl Into<String>,
    ) {
        self.route_trace.push(RouteTraceEntry {
            phase,
            node,
            path: path.to_string(),
            detail: detail.into(),
        });
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
    ) -> Result<bool> {
        self.with_dispatch_boundary(|canopy| canopy.route_input_inner(start, path, input, scope))
    }

    /// Route one synchronous input inside a shared completion boundary.
    fn route_input_inner(
        &mut self,
        start: Option<NodeId>,
        mut path: Path,
        input: RoutedInput,
        scope: Option<&Scope<'_>>,
    ) -> Result<bool> {
        self.route_trace.clear();
        if self.core.modal_region().is_some()
            && !start.is_some_and(|node| self.core.interaction_admits(node))
        {
            return Ok(true);
        }
        let modal_owner = self.core.modal_owner();
        self.trace_route(
            RoutePhase::Target,
            start,
            &path,
            format!("{} route selected", input.label()),
        );

        let mut target = start;
        while let Some(id) = target {
            if !self.core.nodes.contains_key(id) {
                self.trace_route(
                    RoutePhase::Unhandled,
                    Some(id),
                    &path,
                    "target node disappeared",
                );
                return Ok(false);
            }

            let mut fallback_binding = None;
            if let Some(binding) = self.core.input_map.resolve_match(&path, input.input_spec()) {
                if binding.phase == inputmap::BindingPhase::BeforeWidget {
                    self.trace_route(
                        RoutePhase::PreEventBinding,
                        Some(id),
                        &path,
                        "matched before widget event",
                    );
                    return self
                        .execute_routed_binding_with_scope(id, &path, input, binding, scope);
                }
                fallback_binding = Some(binding);
            }

            let event = input.event_for_node(&self.core, id);
            self.trace_route(
                RoutePhase::WidgetEvent,
                Some(id),
                &path,
                format!("{event:?}"),
            );
            let outcome = if self.core.interaction_admits(id) {
                self.core.dispatch_event_on_node(id, &event)?
            } else {
                EventOutcome::Ignore
            };

            match outcome {
                EventOutcome::Handle => {
                    self.trace_route(RoutePhase::Handled, Some(id), &path, format!("{outcome:?}"));
                    return Ok(true);
                }
                EventOutcome::Ignore => {
                    if let Some(binding) = fallback_binding {
                        self.trace_route(
                            RoutePhase::PostEventBinding,
                            Some(id),
                            &path,
                            "matched after widget ignored event",
                        );
                        return self
                            .execute_routed_binding_with_scope(id, &path, input, binding, scope);
                    }
                    // The callback may have removed the node; a missing node
                    // cannot move.
                    if let Some(action) = input.default_action()
                        && self.core.interaction_admits(id)
                        && self.core.apply_default_action(id, action)
                    {
                        self.trace_route(
                            RoutePhase::DefaultAction,
                            Some(id),
                            &path,
                            format!("{action:?}"),
                        );
                        self.trace_route(
                            RoutePhase::Handled,
                            Some(id),
                            &path,
                            "default action applied",
                        );
                        return Ok(true);
                    }
                    self.trace_route(RoutePhase::Bubble, Some(id), &path, "ignored");
                    if modal_owner == Some(id) {
                        return Ok(true);
                    }
                    target = self.core.nodes.get(id).and_then(|node| node.parent);
                    path.pop();
                }
            }
        }

        self.trace_route(RoutePhase::Unhandled, None, &path, "no handler");
        Ok(false)
    }

    /// Route one key taken by a transient mode.
    ///
    /// The mode pops before its binding runs, so the binding can enter another
    /// mode. No widget sees the key, and a key the mode does not bind only
    /// pops the mode.
    fn route_transient_key(
        &mut self,
        start: NodeId,
        mut path: Path,
        key: key::Key,
        scope: Option<&Scope<'_>>,
    ) -> Result<bool> {
        self.route_trace.clear();
        let input = RoutedInput::Key(key);
        self.trace_route(
            RoutePhase::Target,
            Some(start),
            &path,
            "key route selected for a transient mode",
        );
        let mut node = Some(start);
        let mut winner = None;
        while let Some(id) = node {
            if let Some(binding) = self.core.input_map.resolve_match(&path, input.input_spec()) {
                winner = Some((id, path.clone(), binding));
                break;
            }
            node = self.core.nodes.get(id).and_then(|entry| entry.parent);
            path.pop();
        }
        self.core.input_map.pop_mode();
        let Some((id, path, binding)) = winner else {
            self.trace_route(RoutePhase::Handled, None, &path, "transient mode ended");
            return Ok(true);
        };
        self.trace_route(
            RoutePhase::PreEventBinding,
            Some(id),
            &path,
            "matched in a transient mode",
        );
        self.execute_routed_binding_with_scope(id, &path, input, binding, scope)
    }

    /// Execute a binding after route resolution, preserving an active script
    /// scope.
    fn execute_routed_binding_with_scope(
        &mut self,
        node_id: NodeId,
        path: &Path,
        input: RoutedInput,
        binding: inputmap::ResolvedBinding,
        scope: Option<&Scope<'_>>,
    ) -> Result<bool> {
        self.trace_route(
            RoutePhase::BindingExecution,
            Some(node_id),
            path,
            binding.description,
        );

        let event = input.event_for_node(&self.core, node_id);
        let frame = self.core.command_scope_for_event(&event);
        let depth = self.core.push_command_scope(frame);
        let result = match binding.target {
            inputmap::BindingTarget::Script(binding) => self
                .execute_binding_with_scope(node_id, binding, scope)
                .map(|()| None),
            inputmap::BindingTarget::Command(command) => {
                let target = command
                    .target
                    .unwrap_or(commands::CommandTarget::From(node_id));
                // Eligibility is read here, inside the event scope, rather than
                // taken from the last frame, so a status hook sees the same
                // injections the command would.
                match commands::command_status(&self.core, target, &command.invocation) {
                    Ok(commands::CommandStatus::Disabled(reason)) => Ok(Some(reason)),
                    Ok(commands::CommandStatus::Enabled) => {
                        commands::dispatch_target(&mut self.core, target, &command.invocation)
                            .map(|_| None)
                            .map_err(Into::into)
                    }
                    Err(error) => Err(error),
                }
            }
        };
        self.core.pop_command_scope(depth);
        let skipped = result?;

        // A disabled winner still consumes its input. Falling through would let
        // an ancestor act on a control the user saw as unavailable, and
        // reporting an error would end the run loop over an ordinary click.
        let detail = skipped.map_or_else(
            || "binding completed".to_string(),
            |reason| format!("binding disabled: {reason}"),
        );
        self.trace_route(RoutePhase::Handled, Some(node_id), path, detail);
        Ok(true)
    }

    /// Propagate a mouse event through the node under the event and all its
    /// ancestors.
    ///
    /// `scope` carries an active script scope for a script-originated event.
    pub(crate) fn mouse(&mut self, scope: Option<&Scope<'_>>, m: mouse::MouseEvent) -> Result<()> {
        let (target, path) = self.mouse_route_start(m.location)?;
        let changed = self.route_input(target, path, RoutedInput::Mouse(m), scope)?;
        if changed {
            self.render_pending = true;
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
        let key = tk.into();
        let transient =
            self.core.modal_region().is_none() && self.core.input_map.transient_mode().is_some();
        let changed = if transient {
            self.with_dispatch_boundary(|canopy| {
                canopy.route_transient_key(start, path, key, scope)
            })?
        } else {
            self.route_input(Some(start), path, RoutedInput::Key(key), scope)?
        };
        if changed {
            self.render_pending = true;
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

    /// Dispatch a focus-related event to the focused node, bubbling as needed.
    fn dispatch_focus_event(&mut self, event: &Event) -> Result<()> {
        let start = self.focus_or_root()?;
        self.core.dispatch_event(start, event)?;
        Ok(())
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
                self.render_pending = true;
                self.set_root_size(*s)
            }
            Event::Paste(_) | Event::FocusGained | Event::FocusLost => {
                self.render_pending = true;
                self.dispatch_focus_event(e)
            }
        }
    }

    /// Set the size on the root node.
    pub fn set_root_size(&mut self, size: Size) -> Result<()> {
        self.render_limits.cell_count(size)?;
        self.root_size = Some(size);
        self.render_pending = true;
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
        let host = self.script_host.clone();
        match scope {
            Some(scope) => host.call_function_in_scope(scope, node_id, binding),
            None => host.call_function(self, node_id, binding),
        }
    }

    /// Release removed script closures and return the count of all removed
    /// bindings.
    pub(crate) fn release_removed_bindings(
        &mut self,
        removed: Vec<(inputmap::BindingId, inputmap::BindingTarget)>,
    ) -> usize {
        let removed_count = removed.len();
        for (_, target) in removed {
            if let inputmap::BindingTarget::Script(binding) = target {
                self.release_binding_target(binding);
            }
        }
        removed_count
    }

    /// Release the script host's reference to a bound closure.
    pub(crate) fn release_binding_target(&mut self, binding: LuauFunctionId) {
        if let Some(releases) = &mut self.deferred_binding_releases {
            releases.push(binding);
        } else {
            self.script_host.release_function(binding);
        }
    }
}
