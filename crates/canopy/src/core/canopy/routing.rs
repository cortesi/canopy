//! Input routing and event dispatch for the canopy facade.

use ruau::vm::Scope;

use super::{Canopy, RoutePhase, RouteTraceEntry};
use crate::{
    Core, NodeId, commands,
    core::{help, inputmap},
    error::Result,
    event::{Event, key, mouse},
    geom::{Point, Size},
    path::Path,
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

    /// Return true when an anchored binding may run before widget event dispatch.
    fn allows_pre_event_binding(self) -> bool {
        matches!(self, Self::Key(_))
    }

    /// Return a short diagnostic label.
    fn label(self) -> &'static str {
        match self {
            Self::Key(_) => "key",
            Self::Mouse(_) => "mouse",
        }
    }

    /// Convert a screen-space mouse event to a node-local event.
    fn local_mouse(core: &Core, node_id: NodeId, mouse: mouse::MouseEvent) -> mouse::MouseEvent {
        let view = core
            .nodes
            .get(node_id)
            .map(|node| node.view)
            .unwrap_or_default();
        mouse::MouseEvent {
            action: mouse.action,
            button: mouse.button,
            modifiers: mouse.modifiers,
            location: view.content.to_local_point(mouse.location),
        }
    }
}

impl Canopy {
    /// Return the starting target and binding path for a mouse event.
    fn mouse_route_start(&mut self, location: Point) -> Result<(Option<NodeId>, Path)> {
        if let Some(capture) = self.core.mouse_capture {
            if self.core.validate_attached_node(capture).is_ok() {
                return Ok((Some(capture), self.core.node_path(self.core.root, capture)));
            } else {
                self.core.clear_mouse_capture()?;
            }
        }

        let target = self.core.locate_node(self.core.root, location)?;
        let path = target
            .map(|id| self.core.node_path(self.core.root, id))
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
    fn route_input(
        &mut self,
        start: Option<NodeId>,
        path: Path,
        input: RoutedInput,
    ) -> Result<bool> {
        self.route_input_with_scope(start, path, input, None)
    }

    /// Propagate input while preserving an active script scope for Luau bindings.
    fn route_input_with_scope(
        &mut self,
        start: Option<NodeId>,
        mut path: Path,
        input: RoutedInput,
        scope: Option<&Scope<'_>>,
    ) -> Result<bool> {
        self.route_trace.clear();
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
            if let Some((binding, path_match)) =
                self.keymap.resolve_match(&path, &input.input_spec())
            {
                if input.allows_pre_event_binding()
                    && path_match.anchored_end
                    && path_match.depth > 0
                {
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
            let outcome = self.core.dispatch_event_on_node(id, &event)?;

            match outcome {
                EventOutcome::Handle | EventOutcome::Consume => {
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
                    self.trace_route(RoutePhase::Bubble, Some(id), &path, "ignored");
                    target = self.core.nodes.get(id).and_then(|node| node.parent);
                    path.pop();
                }
            }
        }

        self.trace_route(RoutePhase::Unhandled, None, &path, "no handler");
        Ok(false)
    }

    /// Execute a binding after route resolution, preserving an active script scope.
    fn execute_routed_binding_with_scope(
        &mut self,
        node_id: NodeId,
        path: &Path,
        input: RoutedInput,
        binding: inputmap::BindingTarget,
        scope: Option<&Scope<'_>>,
    ) -> Result<bool> {
        let label = help::binding_label(
            &binding,
            &self.core.commands,
            |sid| self.script_host.script_source(sid),
            |id| self.script_host.function_label(id),
        );
        self.trace_route(RoutePhase::BindingExecution, Some(node_id), path, label);

        let event = input.event_for_node(&self.core, node_id);
        let frame = self.core.command_scope_for_event(&event);
        let depth = self.core.push_command_scope(frame);
        let result = self.execute_binding_with_scope(node_id, binding, scope);
        self.core.pop_command_scope(depth);
        self.fulfill_pending_help_request();
        result?;

        self.trace_route(
            RoutePhase::Handled,
            Some(node_id),
            path,
            "binding completed",
        );
        Ok(true)
    }

    /// Propagate a mouse event through the node under the event and all its ancestors.
    pub(crate) fn mouse(&mut self, m: mouse::MouseEvent) -> Result<()> {
        let (target, path) = self.mouse_route_start(m.location)?;
        let changed = self.route_input(target, path, RoutedInput::Mouse(m))?;
        if changed {
            self.render_pending = true;
        }
        Ok(())
    }

    /// Propagate a script-originated mouse event through the current live scope.
    pub(crate) fn mouse_in_script_scope(
        &mut self,
        scope: &Scope<'_>,
        m: mouse::MouseEvent,
    ) -> Result<()> {
        let (target, path) = self.mouse_route_start(m.location)?;
        let changed =
            self.route_input_with_scope(target, path, RoutedInput::Mouse(m), Some(scope))?;
        if changed {
            self.render_pending = true;
        }
        Ok(())
    }

    /// Propagate a key event through the focus and all its ancestors.
    pub(crate) fn key<T>(&mut self, tk: T) -> Result<()>
    where
        T: Into<key::Key>,
    {
        let key = tk.into();
        if self.core.focus.is_none() {
            self.core.focus_first(self.core.root)?;
        }

        let start = self.core.focus.unwrap_or(self.core.root);
        let path = self.core.node_path(self.core.root, start);
        let changed = self.route_input(Some(start), path, RoutedInput::Key(key))?;
        if changed {
            self.render_pending = true;
        }

        Ok(())
    }

    /// Propagate a script-originated key event through the current live scope.
    pub(crate) fn key_in_script_scope<T>(&mut self, scope: &Scope<'_>, tk: T) -> Result<()>
    where
        T: Into<key::Key>,
    {
        let key = tk.into();
        if self.core.focus.is_none() {
            self.core.focus_first(self.core.root)?;
        }

        let start = self.core.focus.unwrap_or(self.core.root);
        let path = self.core.node_path(self.core.root, start);
        let changed =
            self.route_input_with_scope(Some(start), path, RoutedInput::Key(key), Some(scope))?;
        if changed {
            self.render_pending = true;
        }

        Ok(())
    }

    /// Dispatch a focus-related event to the focused node, bubbling as needed.
    fn dispatch_focus_event(&mut self, event: &Event) -> Result<()> {
        if self.core.focus.is_none() {
            self.core.focus_first(self.core.root)?;
        }

        let start = self.core.focus.unwrap_or(self.core.root);
        self.core.dispatch_event(start, event)?;
        Ok(())
    }

    /// Handle poll events by executing callbacks on each node in the list.
    fn poll(&mut self, ids: &[NodeId]) -> Result<()> {
        for id in ids {
            if self.core.nodes.contains_key(*id) {
                let next = self.core.with_widget_mut(*id, |w, core| {
                    let mut ctx = crate::core::context::CoreContext::new(core, *id);
                    w.poll(&mut ctx)
                })?;
                if let Some(d) = next {
                    self.poller.schedule(*id, d);
                }
            }
        }
        Ok(())
    }

    /// Drain queued automation callbacks that were marshalled onto the UI thread.
    pub(crate) fn service_automation(&mut self) {
        while let Ok(callback) = self.automation_rx.try_recv() {
            callback(self);
        }
    }

    /// Propagate an event through the tree.
    pub(crate) fn event(&mut self, e: Event) -> Result<()> {
        match e {
            Event::Key(k) => self.key(k),
            Event::Mouse(m) => self.mouse(m),
            Event::Resize(s) => {
                self.render_pending = true;
                self.set_root_size(s)
            }
            Event::Poll(ids) => {
                self.render_pending = true;
                self.poll(&ids)
            }
            Event::Paste(content) => {
                self.render_pending = true;
                let event = Event::Paste(content);
                self.dispatch_focus_event(&event)
            }
            Event::Wake => Ok(()),
            Event::FocusGained => {
                self.render_pending = true;
                self.dispatch_focus_event(&Event::FocusGained)
            }
            Event::FocusLost => {
                self.render_pending = true;
                self.dispatch_focus_event(&Event::FocusLost)
            }
        }
    }

    /// Set the size on the root node.
    pub fn set_root_size(&mut self, size: Size) -> Result<()> {
        self.render_limits.cell_count(size)?;
        self.root_size = Some(size);
        self.render_pending = true;
        self.core.update_layout(size)?;
        Ok(())
    }

    /// Execute a resolved binding target, re-entering the live scope for scripts.
    fn execute_binding_with_scope(
        &mut self,
        node_id: NodeId,
        binding: inputmap::BindingTarget,
        scope: Option<&Scope<'_>>,
    ) -> Result<()> {
        match binding {
            inputmap::BindingTarget::Script(sid) => {
                if let Some(scope) = scope {
                    let host = self.script_host.clone();
                    host.execute_in_scope(scope, node_id, sid)
                } else {
                    self.run_script(node_id, sid)
                }
            }
            inputmap::BindingTarget::Command(cmd) => {
                commands::dispatch(&mut self.core, node_id, &cmd)
                    .map(|_| ())
                    .map_err(Into::into)
            }
            inputmap::BindingTarget::CommandSequence(sequence) => {
                for command in sequence {
                    commands::dispatch(&mut self.core, node_id, &command)?;
                }
                Ok(())
            }
            inputmap::BindingTarget::SetInputMode(mode) => self.set_input_mode(&mode),
            inputmap::BindingTarget::LuauFunction(id) => {
                let host = self.script_host.clone();
                if let Some(scope) = scope {
                    host.call_function_in_scope(scope, node_id, id)
                } else {
                    host.call_function(self, node_id, id)
                }
            }
        }
    }

    /// Release any Luau closures referenced by removed bindings.
    pub(crate) fn release_removed_bindings(
        &mut self,
        removed: Vec<(inputmap::BindingId, inputmap::BindingTarget)>,
    ) -> usize {
        let released = removed.len();
        for (_, binding) in removed {
            self.release_binding_target(&binding);
        }
        released
    }

    /// Release script-host resources held by a binding target.
    pub(crate) fn release_binding_target(&mut self, binding: &inputmap::BindingTarget) {
        if let inputmap::BindingTarget::LuauFunction(id) = binding {
            if let Some(releases) = &mut self.deferred_binding_releases {
                releases.push(binding.clone());
            } else {
                self.script_host.release_function(*id);
            }
        }
    }
}
