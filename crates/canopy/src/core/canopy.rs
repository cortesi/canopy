use std::{io::Write, sync::mpsc};

use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL};

use super::{inputmap, poll::Poller, termbuf::TermBuf};
use crate::{
    backend::BackendControl,
    commands,
    core::{
        Core, NodeId, context::CoreViewContext, focus::FocusManager, style::StyleEffect, view::View,
    },
    cursor,
    error::{self, Result},
    event::{Event, key, mouse},
    geom::{Expanse, Point, Rect, RectI32},
    layout::Display,
    path::Path,
    render::{Render, RenderBackend},
    script,
    style::{StyleManager, StyleMap, solarized},
    widget::EventOutcome,
};

/// Application runtime state and renderer coordination.
pub struct Canopy {
    /// Core state.
    pub core: Core,

    /// Stores the focus_gen during the last render.
    last_render_focus_gen: u64,

    /// Last focus path ids, used to detect focus path changes.
    last_focus_path: Vec<NodeId>,

    /// The poller is responsible for tracking nodes that have pending poll events.
    poller: Poller,

    /// Root window size.
    pub(crate) root_size: Option<Expanse>,

    /// Script execution host.
    pub(crate) script_host: script::ScriptHost,
    /// Input mapping table.
    pub(crate) keymap: inputmap::InputMap,

    /// Cached terminal buffer.
    termbuf: Option<TermBuf>,
    /// Whether a render is pending after the most recent event.
    render_pending: bool,

    /// Event sender channel.
    pub(crate) event_tx: mpsc::Sender<Event>,
    /// Event receiver channel.
    pub(crate) event_rx: Option<mpsc::Receiver<Event>>,

    /// Style map used for rendering.
    pub style: StyleMap,
}

/// Rendering traversal scratch state shared across recursion.
struct RenderTraversal<'a> {
    /// Destination buffer for draw operations.
    dest_buf: &'a mut TermBuf,
    /// Style manager stack.
    styl: &'a mut StyleManager,
    /// Accumulated style effects for the current subtree.
    effect_stack: &'a mut Vec<Box<dyn StyleEffect>>,
}

impl Canopy {
    /// Construct a new Canopy instance.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let core = Core::new();
        Self {
            last_render_focus_gen: core.focus_gen,
            last_focus_path: Vec::new(),
            poller: Poller::new(tx.clone()),
            event_tx: tx,
            event_rx: Some(rx),
            keymap: inputmap::InputMap::new(),
            script_host: script::ScriptHost::new(),
            style: solarized::solarized_dark(),
            root_size: None,
            termbuf: None,
            render_pending: true,
            core,
        }
    }

    /// Register a backend controller.
    pub fn register_backend<T: BackendControl + 'static>(&mut self, be: T) {
        self.core.backend = Some(Box::new(be))
    }

    /// Get a reference to the current render buffer, if any.
    pub fn buf(&self) -> Option<&TermBuf> {
        self.termbuf.as_ref()
    }

    /// Run a compiled script by id on the target node.
    pub fn run_script(&mut self, node_id: NodeId, sid: script::ScriptId) -> Result<()> {
        self.script_host.execute(&mut self.core, node_id, sid)
    }

    /// Compile a script and return its identifier.
    pub fn compile_script(&mut self, source: &str) -> Result<script::ScriptId> {
        self.script_host.compile(source)
    }

    /// Bind a mouse action in the global mode with a given path filter to a script.
    pub fn bind_mouse<K>(&mut self, mouse: K, path_filter: &str, script: &str) -> Result<()>
    where
        mouse::Mouse: From<K>,
    {
        self.bind_mode_mouse(mouse, "", path_filter, script)
    }

    /// Bind a mouse action in a specified mode with a given path filter to a script.
    pub fn bind_mode_mouse<K>(
        &mut self,
        mouse: K,
        mode: &str,
        path_filter: &str,
        script: &str,
    ) -> Result<()>
    where
        mouse::Mouse: From<K>,
    {
        self.keymap.bind(
            mode,
            inputmap::InputSpec::Mouse(mouse.into()),
            path_filter,
            self.script_host.compile(script)?,
        )
    }

    /// Bind a mouse action in the global mode with a given path filter to a typed command.
    pub fn bind_mouse_command<K, C>(
        &mut self,
        mouse: K,
        path_filter: &str,
        command: C,
    ) -> Result<()>
    where
        mouse::Mouse: From<K>,
        C: Into<commands::CommandInvocation>,
    {
        self.bind_mode_mouse_command(mouse, "", path_filter, command)
    }

    /// Bind a key in the global mode, with a given path filter to a script.
    pub fn bind_key<K>(&mut self, key: K, path_filter: &str, script: &str) -> Result<()>
    where
        key::Key: From<K>,
    {
        self.bind_mode_key(key, "", path_filter, script)
    }

    /// Bind a key within a given mode, with a given path filter to a script.
    pub fn bind_mode_key<K>(
        &mut self,
        key: K,
        mode: &str,
        path_filter: &str,
        script: &str,
    ) -> Result<()>
    where
        key::Key: From<K>,
    {
        self.keymap.bind(
            mode,
            inputmap::InputSpec::Key(key.into()),
            path_filter,
            self.script_host.compile(script)?,
        )
    }

    /// Bind a key in the global mode with a given path filter to a typed command.
    pub fn bind_key_command<K, C>(&mut self, key: K, path_filter: &str, command: C) -> Result<()>
    where
        key::Key: From<K>,
        C: Into<commands::CommandInvocation>,
    {
        self.bind_mode_key_command(key, "", path_filter, command)
    }

    /// Bind a key within a given mode, with a given path filter, to a typed command.
    pub fn bind_mode_key_command<K, C>(
        &mut self,
        key: K,
        mode: &str,
        path_filter: &str,
        command: C,
    ) -> Result<()>
    where
        key::Key: From<K>,
        C: Into<commands::CommandInvocation>,
    {
        let invocation = command.into();
        self.keymap.bind_command(
            mode,
            inputmap::InputSpec::Key(key.into()),
            path_filter,
            invocation,
        )
    }

    /// Bind a mouse action in a specified mode with a given path filter to a typed command.
    pub fn bind_mode_mouse_command<K, C>(
        &mut self,
        mouse: K,
        mode: &str,
        path_filter: &str,
        command: C,
    ) -> Result<()>
    where
        mouse::Mouse: From<K>,
        C: Into<commands::CommandInvocation>,
    {
        let invocation = command.into();
        self.keymap.bind_command(
            mode,
            inputmap::InputSpec::Mouse(mouse.into()),
            path_filter,
            invocation,
        )
    }

    /// Load the commands from a command node using the default node name.
    pub fn add_commands<T: commands::CommandNode>(&mut self) {
        let cmds = <T>::commands();
        self.core.commands.add(cmds);
        self.script_host.register_commands(cmds);
    }

    /// Output a formatted table of commands to a writer.
    pub fn print_command_table(&self, w: &mut dyn Write) -> Result<()> {
        let mut cmds: Vec<&commands::CommandSpec> =
            self.core.commands.iter().map(|(_, v)| v).collect();

        cmds.sort_by_key(|a| a.id.0);

        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.load_preset(UTF8_FULL);
        for i in cmds {
            table.add_row(vec![
                comfy_table::Cell::new(i.id.0).fg(comfy_table::Color::Green),
                comfy_table::Cell::new(i.signature()),
            ]);
        }
        writeln!(w, "{table}").map_err(|x| error::Error::Internal(x.to_string()))
    }

    /// Has the focus changed since the last render sweep?
    pub(crate) fn focus_changed(&self) -> bool {
        self.core.focus_gen != self.last_render_focus_gen
    }

    /// Render the tree only if a render is pending.
    pub(crate) fn render_if_pending<R: RenderBackend>(&mut self, be: &mut R) -> Result<bool> {
        if !self.render_pending {
            return Ok(false);
        }
        self.render(be)?;
        Ok(true)
    }

    /// Has the focus path status of this node changed since the last render sweep?
    pub fn node_focus_path_changed(&self, node_id: NodeId) -> bool {
        if self.focus_changed() {
            self.core.is_on_focus_path(node_id) || self.last_focus_path.contains(&node_id)
        } else {
            false
        }
    }

    /// Register the poller channel.
    pub(crate) fn start_poller(&mut self, tx: mpsc::Sender<Event>) {
        self.event_tx = tx;
    }

    /// Pre-render sweep of the tree.
    pub(crate) fn pre_render(&mut self) -> Result<bool> {
        let root = self.core.root;
        let mut focus_seen = false;
        let mut layout_dirty = false;
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let hidden = self.core.nodes.get(id).map(|n| n.hidden).unwrap_or(false);
            if hidden {
                continue;
            }

            if self.core.is_focused(id) {
                focus_seen = true;
            }

            let mounted = self.core.nodes.get(id).map(|n| n.mounted).unwrap_or(false);
            if !mounted {
                layout_dirty = true;
                self.core.mount_node(id)?;
            }

            let initialized = self
                .core
                .nodes
                .get(id)
                .map(|n| n.initialized)
                .unwrap_or(false);
            if !initialized {
                layout_dirty = true;
                let next = self.core.with_widget_mut(id, |w, core| {
                    let mut ctx = crate::core::context::CoreContext::new(core, id);
                    w.poll(&mut ctx)
                });
                if let Some(d) = next {
                    self.poller.schedule(id, d);
                }
                if let Some(node) = self.core.nodes.get_mut(id) {
                    node.initialized = true;
                }
            }

            let children = self.core.nodes[id].children.clone();
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }

        if !focus_seen {
            self.core.focus_first(root);
        }

        Ok(layout_dirty)
    }

    /// Render a single node (without children).
    fn render_node(
        &mut self,
        dest_buf: &mut TermBuf,
        styl: &mut StyleManager,
        node_id: NodeId,
        view: View,
        screen_clip: Rect,
        effect_slice: &[Box<dyn StyleEffect>],
    ) -> Result<()> {
        let local_clip = Self::outer_clip_to_local(view.outer, screen_clip);
        let screen_origin = screen_clip.tl;

        let mut rndr = Render::new_shared(&self.style, styl, dest_buf, local_clip, screen_origin)
            .with_effects(effect_slice);

        self.core.with_widget_view(node_id, |widget, core| {
            let ctx = CoreViewContext::new(core, node_id);
            widget.render(&mut rndr, &ctx)
        })
    }

    /// Recursively render a node subtree.
    fn render_recursive(
        &mut self,
        traversal: &mut RenderTraversal<'_>,
        node_id: NodeId,
        parent_clip: Rect,
        active_start: usize,
        active_len: usize,
    ) -> Result<()> {
        let (hidden, layout, view, children, clear_inherited) = {
            let node = &self.core.nodes[node_id];
            (
                node.hidden,
                node.layout,
                node.view,
                node.children.clone(),
                node.clear_inherited_effects,
            )
        };

        if hidden || layout.display == Display::None {
            return Ok(());
        }

        let Some(screen_clip) = view.outer.intersect_rect(parent_clip) else {
            return Ok(());
        };

        let saved_len = traversal.effect_stack.len();
        let (base_start, base_len) = if clear_inherited {
            (saved_len, 0)
        } else {
            (active_start, active_len)
        };

        if let Some(local) = self.core.nodes[node_id].effects.as_ref() {
            for effect in local {
                traversal.effect_stack.push(effect.box_clone());
            }
        }

        let current_len = base_len + traversal.effect_stack.len() - saved_len;

        traversal.styl.push();

        {
            let effect_slice = &traversal.effect_stack[base_start..base_start + current_len];
            self.render_node(
                traversal.dest_buf,
                traversal.styl,
                node_id,
                view,
                screen_clip,
                effect_slice,
            )?;
        }

        if let Some(children_clip) = view.content.intersect_rect(parent_clip) {
            for child in children {
                self.render_recursive(traversal, child, children_clip, base_start, current_len)?;
            }
        }

        traversal.styl.pop();
        traversal.effect_stack.truncate(saved_len);

        Ok(())
    }

    /// Post-render sweep of the tree.
    pub(crate) fn post_render(&mut self, buf: &mut TermBuf) -> Result<()> {
        let mut current = self.core.focus;
        let mut cursor_spec: Option<(NodeId, View, cursor::Cursor)> = None;
        while let Some(id) = current {
            let cursor = self.core.with_widget_view(id, |w, _| w.cursor());
            if let Some(node_cursor) = cursor
                && let Some(node) = self.core.nodes.get(id)
            {
                cursor_spec = Some((id, node.view, node_cursor));
                break;
            }
            current = self.core.nodes.get(id).and_then(|n| n.parent);
        }

        if let Some((_nid, view, c)) = cursor_spec {
            let view_rect = Rect::new(0, 0, view.content.w, view.content.h);
            if view_rect.contains_point(c.location) {
                let screen_x = view.content.tl.x + c.location.x as i32;
                let screen_y = view.content.tl.y + c.location.y as i32;
                if screen_x >= 0 && screen_y >= 0 {
                    let screen_pos = Point {
                        x: screen_x as u32,
                        y: screen_y as u32,
                    };
                    buf.overlay_cursor(screen_pos, c.shape);
                }
            }
        }

        Ok(())
    }

    /// Render the widget tree. All visible nodes are rendered.
    pub fn render<R: RenderBackend>(&mut self, be: &mut R) -> Result<()> {
        // Apply pending style change from Context::set_style
        if let Some(new_style) = self.core.pending_style.take() {
            self.style = new_style;
        }

        if let Some(root_size) = self.root_size {
            self.core.update_layout(root_size)?;

            let mut styl = StyleManager::default();
            be.reset()?;
            styl.reset();

            let def_style = styl.get(&self.style, "");
            let mut next = TermBuf::new(root_size, ' ', def_style);

            let layout_dirty = self.pre_render()?;
            if layout_dirty {
                self.core.update_layout(root_size)?;
            }

            let screen_clip = Rect::new(0, 0, root_size.w, root_size.h);
            let mut effect_stack: Vec<Box<dyn StyleEffect>> = Vec::new();
            let mut traversal = RenderTraversal {
                dest_buf: &mut next,
                styl: &mut styl,
                effect_stack: &mut effect_stack,
            };
            self.render_recursive(&mut traversal, self.core.root, screen_clip, 0, 0)?;
            self.post_render(&mut next)?;

            if let Some(prev) = &self.termbuf {
                let mut screen_buf = prev.clone();
                screen_buf.copy(&next, root_size.rect());
                screen_buf.diff(prev, be)?;
                self.termbuf = Some(screen_buf);
            } else {
                next.render(be)?;
                self.termbuf = Some(next);
            }

            self.last_render_focus_gen = self.core.focus_gen;
            self.last_focus_path = self.core.focus_path_ids();
            self.render_pending = false;
        }

        Ok(())
    }

    /// Convert a screen-space clip rect into local outer coordinates.
    fn outer_clip_to_local(outer: RectI32, clip: Rect) -> Rect {
        let dx = (clip.tl.x as i64 - outer.tl.x as i64).max(0) as u32;
        let dy = (clip.tl.y as i64 - outer.tl.y as i64).max(0) as u32;
        Rect::new(dx, dy, clip.w, clip.h)
    }

    /// Return the path for the uppermost node at a specific location.
    fn location_path(&self, location: Point) -> Result<Path> {
        if let Some(id) = self.core.locate_node(self.core.root, location)? {
            Ok(self.core.node_path(self.core.root, id))
        } else {
            Ok(Path::empty())
        }
    }

    /// Propagate a mouse event through the node under the event and all its ancestors.
    pub(crate) fn mouse(&mut self, m: mouse::MouseEvent) -> Result<()> {
        let mut action = None;
        let mut changed = false;
        let mut target = None;
        let mut path = Path::empty();

        if let Some(capture) = self.core.mouse_capture {
            if self.core.nodes.contains_key(capture) {
                path = self.core.node_path(self.core.root, capture);
                target = Some(capture);
            } else {
                self.core.mouse_capture = None;
            }
        }

        if target.is_none() {
            path = self.location_path(m.location)?;
            target = self.core.locate_node(self.core.root, m.location)?;
        }

        if let Some(nid) = target {
            let mut target = Some(nid);
            while let Some(id) = target {
                let view = self.core.nodes.get(id).map(|n| n.view).unwrap_or_default();
                let content = view.content;
                let local_location = content.to_local_point(m.location);

                let outcome = self.core.dispatch_event_on_node(
                    id,
                    &Event::Mouse(mouse::MouseEvent {
                        action: m.action,
                        button: m.button,
                        modifiers: m.modifiers,
                        location: local_location,
                    }),
                );

                match outcome {
                    EventOutcome::Handle | EventOutcome::Consume => {
                        changed = true;
                        break;
                    }
                    EventOutcome::Ignore => {
                        if let Some(binding) = self
                            .keymap
                            .resolve(&path, &inputmap::InputSpec::Mouse(m.into()))
                        {
                            action = Some((binding, id));
                            break;
                        }
                        path.pop();
                        target = self.core.nodes[id].parent;
                    }
                }
            }
        }

        if let Some((binding, nid)) = action {
            match binding {
                inputmap::BindingTarget::Script(sid) => {
                    self.run_script(nid, sid)?;
                }
                inputmap::BindingTarget::Command(cmd) => {
                    commands::dispatch(&mut self.core, nid, &cmd)?;
                }
            }
            changed = true;
        }

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
        let k = tk.into();
        if self.core.focus.is_none() {
            self.core.focus_first(self.core.root);
        }

        let start = self.core.focus.unwrap_or(self.core.root);
        let mut path = self.core.node_path(self.core.root, start);
        let mut target = Some(start);
        let mut action = None;
        let mut changed = false;

        while let Some(id) = target {
            let mut fallback_binding = None;
            if let Some((binding, m)) = self
                .keymap
                .resolve_match(&path, &inputmap::InputSpec::Key(k))
            {
                let path_len = path.to_string().len();
                if m.end == path_len && m.len > 0 {
                    action = Some((binding, id));
                    break;
                }
                fallback_binding = Some(binding);
            }

            let outcome = self.core.dispatch_event_on_node(id, &Event::Key(k));
            match outcome {
                EventOutcome::Handle | EventOutcome::Consume => {
                    changed = true;
                    break;
                }
                EventOutcome::Ignore => {
                    if let Some(binding) = fallback_binding {
                        action = Some((binding, id));
                        break;
                    }
                    path.pop();
                    target = self.core.nodes[id].parent;
                }
            }
        }

        if let Some((binding, nid)) = action {
            match binding {
                inputmap::BindingTarget::Script(sid) => {
                    self.run_script(nid, sid)?;
                }
                inputmap::BindingTarget::Command(cmd) => {
                    commands::dispatch(&mut self.core, nid, &cmd)?;
                }
            }
            changed = true;
        }

        if changed {
            self.render_pending = true;
        }

        Ok(())
    }

    /// Dispatch a focus-related event to the focused node, bubbling as needed.
    fn dispatch_focus_event(&mut self, event: &Event) -> Result<()> {
        if self.core.focus.is_none() {
            self.core.focus_first(self.core.root);
        }

        let start = self.core.focus.unwrap_or(self.core.root);
        let _ = self.core.dispatch_event(start, event);
        Ok(())
    }

    /// Handle poll events by executing callbacks on each node in the list.
    fn poll(&mut self, ids: &[NodeId]) -> Result<()> {
        for id in ids {
            if self.core.nodes.contains_key(*id) {
                let next = self.core.with_widget_mut(*id, |w, core| {
                    let mut ctx = crate::core::context::CoreContext::new(core, *id);
                    w.poll(&mut ctx)
                });
                if let Some(d) = next {
                    self.poller.schedule(*id, d);
                }
            }
        }
        Ok(())
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
    pub fn set_root_size(&mut self, size: Expanse) -> Result<()> {
        self.root_size = Some(size);
        self.render_pending = true;
        self.core.update_layout(size)?;
        Ok(())
    }
}

/// Validate a child view position against the parent canvas bounds.
/// A trait that allows widgets to perform recursive initialization of themselves and their
/// children.
pub trait Loader {
    /// Load commands or resources into the canopy instance.
    fn load(_: &mut Canopy) {}
}

#[cfg(test)]
mod tests {
    use std::{
        any::Any,
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    use super::*;
    use crate::{
        Context, ViewContext,
        commands::{CommandNode, CommandSpec},
        derive_commands,
        error::Result,
        geom::{Direction, Point, RectI32},
        layout::Layout,
        path::Path,
        state::NodeName,
        testing::{
            backend::{CanvasRender, TestRender},
            ttree::{Ba, BaLa, BaLb, OutcomeTarget, R, get_state, reset_state, run_ttree},
        },
        widget::{EventOutcome, Widget},
    };

    static POLL_COUNT: AtomicUsize = AtomicUsize::new(0);

    pub struct PollWidget;

    #[derive_commands]
    impl PollWidget {
        pub fn new() -> Self {
            Self
        }
    }

    impl Widget for PollWidget {
        fn poll(&mut self, _ctx: &mut dyn Context) -> Option<Duration> {
            POLL_COUNT.fetch_add(1, Ordering::SeqCst);
            None
        }
    }

    pub struct StaticWidget;

    #[derive_commands]
    impl StaticWidget {
        pub fn new() -> Self {
            Self
        }
    }

    impl Widget for StaticWidget {
        fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }
    }

    pub struct CaptureWidget {
        drags: usize,
    }

    #[derive_commands]
    impl CaptureWidget {
        pub fn new() -> Self {
            Self { drags: 0 }
        }
    }

    impl Widget for CaptureWidget {
        fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
            if let Event::Mouse(mouse_event) = event {
                match mouse_event.action {
                    mouse::Action::Down if mouse_event.button == mouse::Button::Left => {
                        ctx.capture_mouse();
                        return EventOutcome::Handle;
                    }
                    mouse::Action::Drag if mouse_event.button == mouse::Button::Left => {
                        self.drags = self.drags.saturating_add(1);
                        return EventOutcome::Handle;
                    }
                    mouse::Action::Up if mouse_event.button == mouse::Button::Left => {
                        ctx.release_mouse();
                        return EventOutcome::Handle;
                    }
                    _ => {}
                }
            }
            EventOutcome::Ignore
        }
    }

    fn set_outcome<T: Any + OutcomeTarget>(core: &mut Core, id: NodeId, outcome: EventOutcome) {
        core.with_widget_mut(id, |w, _| {
            let any = w as &mut dyn Any;
            if let Some(node) = any.downcast_mut::<T>() {
                node.set_outcome(outcome);
            }
        });
    }

    fn capture_drag_count(core: &mut Core, id: NodeId) -> usize {
        core.with_widget_mut(id, |w, _| {
            let any = w as &mut dyn Any;
            any.downcast_mut::<CaptureWidget>()
                .map(|widget| widget.drags)
                .unwrap_or(0)
        })
    }

    fn make_mouse_event(core: &Core, node_id: NodeId) -> mouse::MouseEvent {
        let loc = core
            .nodes
            .get(node_id)
            .map(|n| {
                let tl = n.view.outer.tl;
                Point {
                    x: tl.x.max(0) as u32,
                    y: tl.y.max(0) as u32,
                }
            })
            .unwrap_or_default();
        mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: loc,
        }
    }

    #[test]
    fn mouse_move_does_not_request_render() -> Result<()> {
        let mut canopy = Canopy::new();
        let app_id = canopy
            .core
            .add_child_to_boxed(canopy.core.root, Box::new(StaticWidget::new()))?;
        canopy
            .core
            .with_layout_of(app_id, |layout| *layout = Layout::fill())?;
        canopy.set_root_size(Expanse::new(10, 6))?;

        let (_, mut render) = TestRender::create();
        canopy.render(&mut render)?;
        assert!(!canopy.render_if_pending(&mut render)?);

        let event = mouse::MouseEvent {
            action: mouse::Action::Moved,
            button: mouse::Button::None,
            modifiers: key::Empty,
            location: Point { x: 1, y: 1 },
        };
        canopy.event(Event::Mouse(event))?;
        assert!(!canopy.render_if_pending(&mut render)?);
        Ok(())
    }

    #[test]
    fn mouse_capture_routes_drag_outside() -> Result<()> {
        let mut canopy = Canopy::new();
        let app_id = canopy
            .core
            .add_child_to_boxed(canopy.core.root, Box::new(CaptureWidget::new()))?;
        canopy
            .core
            .with_layout_of(app_id, |layout| *layout = Layout::fill())?;
        canopy.set_root_size(Expanse::new(10, 6))?;

        let (_, mut render) = TestRender::create();
        canopy.render(&mut render)?;

        let down = make_mouse_event(&canopy.core, app_id);
        canopy.event(Event::Mouse(down))?;

        let drag = mouse::MouseEvent {
            action: mouse::Action::Drag,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: Point { x: 50, y: 50 },
        };
        canopy.event(Event::Mouse(drag))?;

        assert_eq!(capture_drag_count(&mut canopy.core, app_id), 1);

        let up = mouse::MouseEvent {
            action: mouse::Action::Up,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: Point { x: 50, y: 50 },
        };
        canopy.event(Event::Mouse(up))?;

        Ok(())
    }

    #[test]
    fn set_widget_resets_initialization() -> Result<()> {
        POLL_COUNT.store(0, Ordering::SeqCst);
        let mut canopy = Canopy::new();
        let node_id = canopy
            .core
            .add_child_to_boxed(canopy.core.root, Box::new(PollWidget::new()))?;
        canopy.set_root_size(Expanse::new(10, 10))?;

        let (_, mut render) = TestRender::create();
        render.render(&mut canopy)?;
        assert_eq!(POLL_COUNT.load(Ordering::SeqCst), 1);

        canopy.core.set_widget(node_id, PollWidget::new());
        render.render(&mut canopy)?;
        assert_eq!(POLL_COUNT.load(Ordering::SeqCst), 2);
        Ok(())
    }

    #[test]
    fn tbindings() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.keymap.bind(
                "",
                inputmap::InputSpec::Key('a'.into()),
                "",
                c.script_host.compile(r#"ba_la::c_leaf()"#)?,
            )?;
            c.keymap.bind(
                "",
                inputmap::InputSpec::Key('r'.into()),
                "",
                c.script_host.compile(r#"r::c_root()"#)?,
            )?;
            c.keymap.bind(
                "",
                inputmap::InputSpec::Key('x'.into()),
                "ba/",
                c.script_host.compile(r#"r::c_root()"#)?,
            )?;

            c.core.set_focus(tree.a_a);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->ignore", "ba_la.c_leaf()"]);

            reset_state();
            c.key('r')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->ignore", "r.c_root()"]);

            reset_state();
            c.core.set_focus(tree.a);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->ignore", "ba_la.c_leaf()"]);

            reset_state();
            c.core.set_focus(tree.a_a);
            c.key('x')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->ignore", "r.c_root()"]);

            reset_state();
            c.core.set_focus(tree.root);
            c.key('x')?;
            let s = get_state();
            assert_eq!(s.path, vec!["r@key->ignore"]);

            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tkey() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.root);
            set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["r@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_a);
            set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_a);
            set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->ignore", "ba@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_a);
            set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_la@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a);
            set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a);
            set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->ignore", "r@key->handle"]);
            c.key('a')?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec![
                    "ba@key->ignore",
                    "r@key->handle",
                    "ba@key->ignore",
                    "r@key->ignore"
                ]
            );
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_b);
            set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Ignore);
            set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_lb@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_a);
            set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_b);
            set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->ignore", "ba@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_b);
            set_outcome::<BaLb>(&mut c.core, tree.a_b, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->handle"]);
            Ok(())
        })?;

        run_ttree(|c, _, tree| {
            c.core.set_focus(tree.a_b);
            set_outcome::<BaLb>(&mut c.core, tree.a_b, EventOutcome::Handle);
            set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Handle);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->handle"]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tmouse() -> Result<()> {
        run_ttree(|c, mut tr, tree| {
            c.core.set_focus(tree.root);
            set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
            tr.render(c)?;
            let evt = make_mouse_event(&c.core, tree.a_a);
            c.mouse(evt)?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_la@mouse->ignore", "ba@mouse->ignore", "r@mouse->handle"]
            );
            Ok(())
        })?;

        run_ttree(|c, mut tr, tree| {
            set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
            tr.render(c)?;
            let evt = make_mouse_event(&c.core, tree.a_a);
            c.mouse(evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
            Ok(())
        })?;

        run_ttree(|c, mut tr, tree| {
            set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
            tr.render(c)?;
            let evt = make_mouse_event(&c.core, tree.a_a);
            c.mouse(evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
            Ok(())
        })?;

        run_ttree(|c, mut tr, tree| {
            set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
            tr.render(c)?;
            let evt = make_mouse_event(&c.core, tree.a_a);
            c.mouse(evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tresize() -> Result<()> {
        run_ttree(|c, mut tr, tree| {
            let size: u32 = 100;
            let half = i32::try_from(size / 2).expect("size fits i32");
            tr.render(c)?;
            assert_eq!(
                c.core.nodes[tree.root].view.outer,
                RectI32::new(0, 0, size, size)
            );
            assert_eq!(
                c.core.nodes[tree.a].view.outer,
                RectI32::new(0, 0, size / 2, size)
            );
            assert_eq!(
                c.core.nodes[tree.b].view.outer,
                RectI32::new(half, 0, size / 2, size)
            );

            c.set_root_size(Expanse::new(50, 50))?;
            tr.render(c)?;
            assert_eq!(c.core.nodes[tree.b].view.outer, RectI32::new(25, 0, 25, 50));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn trender() -> Result<()> {
        run_ttree(|c, mut tr, tree| {
            tr.render(c)?;
            assert!(!tr.buf_empty());

            tr.render(c)?;
            assert!(tr.buf_empty());
            tr.render(c)?;
            tr.render(c)?;
            tr.render(c)?;

            tr.render(c)?;
            assert!(tr.buf_empty());

            c.core.set_focus(tree.a_a);
            tr.render(c)?;
            assert!(tr.buf_empty());

            c.core.focus_next(c.core.root);
            tr.render(c)?;
            assert!(tr.buf_empty());

            c.core.focus_prev(c.core.root);
            tr.render(c)?;
            assert!(tr.buf_empty());

            tr.render(c)?;
            assert!(tr.buf_empty());

            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn focus_path() -> Result<()> {
        run_ttree(|c, _, _tree| {
            assert_eq!(c.core.focus_path(c.core.root), Path::empty());
            c.core.focus_next(c.core.root);
            assert_eq!(c.core.focus_path(c.core.root), Path::new(&["r"]));
            c.core.focus_next(c.core.root);
            assert_eq!(c.core.focus_path(c.core.root), Path::new(&["r", "ba"]));
            c.core.focus_next(c.core.root);
            assert_eq!(
                c.core.focus_path(c.core.root),
                Path::new(&["r", "ba", "ba_la"])
            );
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn focus_next() -> Result<()> {
        run_ttree(|c, _, tree| {
            assert!(!c.core.is_focused(tree.root));
            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.root));

            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.a));

            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.a_a));
            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.a_b));
            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.b));

            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.b_a));
            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.b_b));

            c.core.focus_next(c.core.root);
            assert!(c.core.is_focused(tree.root));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn focus_prev() -> Result<()> {
        run_ttree(|c, _, tree| {
            assert!(!c.core.is_focused(tree.root));
            c.core.focus_prev(c.core.root);
            assert!(c.core.is_focused(tree.b_b));

            c.core.focus_prev(c.core.root);
            assert!(c.core.is_focused(tree.b_a));

            c.core.focus_prev(c.core.root);
            assert!(c.core.is_focused(tree.b));

            c.core.set_focus(tree.root);
            c.core.focus_prev(c.core.root);
            assert!(c.core.is_focused(tree.b_b));

            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tshift_right() -> Result<()> {
        run_ttree(|c, mut tr, tree| {
            tr.render(c)?;
            c.core.set_focus(tree.a_a);
            c.core.focus_dir(c.core.root, Direction::Right);
            assert!(c.core.is_focused(tree.b_a));
            c.core.focus_dir(c.core.root, Direction::Right);
            assert!(c.core.is_focused(tree.b_a));

            c.core.set_focus(tree.a_b);
            c.core.focus_dir(c.core.root, Direction::Right);
            assert!(c.core.is_focused(tree.b_b));
            c.core.focus_dir(c.core.root, Direction::Right);
            assert!(c.core.is_focused(tree.b_b));
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tfoci() -> Result<()> {
        run_ttree(|c, _, tree| {
            assert_eq!(c.core.focus_path(c.core.root), Path::empty());

            assert!(!c.core.is_on_focus_path(tree.root));
            assert!(!c.core.is_on_focus_path(tree.a));

            c.core.set_focus(tree.a_a);
            assert!(c.core.is_on_focus_path(tree.root));
            assert!(c.core.is_on_focus_path(tree.a));
            assert!(!c.core.is_on_focus_path(tree.b));
            assert_eq!(
                c.core.focus_path(c.core.root),
                Path::new(&["r", "ba", "ba_la"])
            );

            c.core.set_focus(tree.a);
            assert_eq!(c.core.focus_path(c.core.root), Path::new(&["r", "ba"]));

            c.core.set_focus(tree.root);
            assert_eq!(c.core.focus_path(c.core.root), Path::new(&["r"]));

            c.core.set_focus(tree.b_a);
            assert_eq!(
                c.core.focus_path(c.core.root),
                Path::new(&["r", "bb", "bb_la"])
            );
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tkey_no_render() -> Result<()> {
        struct N;

        impl CommandNode for N {
            fn commands() -> &'static [&'static CommandSpec] {
                &[]
            }
        }

        impl Widget for N {
            fn layout(&self) -> Layout {
                Layout::fill()
            }

            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
                true
            }

            fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
                r.text("any", ctx.view().outer_rect_local().line(0), "<n>")
            }

            fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
                match event {
                    Event::Key(_) => EventOutcome::Consume,
                    _ => EventOutcome::Ignore,
                }
            }

            fn name(&self) -> NodeName {
                NodeName::convert("n")
            }
        }

        let (_, mut tr) = TestRender::create();
        let mut canopy = Canopy::new();
        canopy.add_commands::<N>();
        canopy.core.set_widget(canopy.core.root, N);

        canopy.set_root_size(Expanse::new(10, 1))?;
        canopy.core.set_focus(canopy.core.root);
        canopy.render(&mut tr)?;
        assert!(!tr.buf_empty());
        let prev_buf = canopy.termbuf.clone().expect("missing termbuf");
        tr.text.lock().unwrap().text.clear();

        canopy.key('a')?;
        canopy.render(&mut tr)?;
        let next_buf = canopy.termbuf.clone().expect("missing termbuf");
        assert_eq!(prev_buf.cells, next_buf.cells);
        Ok(())
    }

    #[test]
    fn zero_size_child_ok() -> Result<()> {
        struct Child;

        #[derive_commands]
        impl Child {}

        impl Widget for Child {
            fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
                Ok(())
            }

            fn name(&self) -> NodeName {
                NodeName::convert("child")
            }
        }

        struct Parent;

        #[derive_commands]
        impl Parent {
            fn new() -> Self {
                Self
            }
        }

        impl Widget for Parent {
            fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
                Ok(())
            }

            fn name(&self) -> NodeName {
                NodeName::convert("parent")
            }
        }

        let size = Expanse::new(5, 1);
        let (_, mut cr) = CanvasRender::create(size);
        let mut canopy = Canopy::new();
        canopy.core.set_widget(canopy.core.root, Parent::new());
        let child = canopy
            .core
            .add_child_to_boxed(canopy.core.root, Box::new(Child))?;
        canopy.core.with_layout_of(child, |layout| {
            *layout = Layout::column().fixed_width(0).fixed_height(0);
        })?;

        canopy.set_root_size(size)?;
        canopy.render(&mut cr)?;
        Ok(())
    }
}
