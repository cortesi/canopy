use std::{io::Write, sync::mpsc};

use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL};

use super::{inputmap, poll::Poller, termbuf::TermBuf, viewport::ViewPort, viewstack::ViewStack};
use crate::{
    backend::BackendControl,
    commands,
    core::{Core, NodeId, context::CoreViewContext},
    cursor,
    error::{self, Result},
    event::{Event, key, mouse},
    geom::{Expanse, Point, Rect},
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
    /// Registered command set.
    pub(crate) commands: commands::CommandSet,

    /// Cached terminal buffer.
    termbuf: Option<TermBuf>,

    /// Event sender channel.
    pub(crate) event_tx: mpsc::Sender<Event>,
    /// Event receiver channel.
    pub(crate) event_rx: Option<mpsc::Receiver<Event>>,

    /// Style map used for rendering.
    pub style: StyleMap,
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
            commands: commands::CommandSet::new(),
            script_host: script::ScriptHost::new(),
            style: solarized::solarized_dark(),
            root_size: None,
            termbuf: None,
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

    /// Load the commands from a command node using the default node name.
    pub fn add_commands<T: commands::CommandNode>(&mut self) {
        let cmds = <T>::commands();
        self.script_host.load_commands(&cmds);
        self.commands.add(&cmds);
    }

    /// Output a formatted table of commands to a writer.
    pub fn print_command_table(&self, w: &mut dyn Write) -> Result<()> {
        let mut cmds: Vec<&commands::CommandSpec> = self.commands.iter().map(|(_, v)| v).collect();

        cmds.sort_by_key(|a| a.fullname());

        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.load_preset(UTF8_FULL);
        for i in cmds {
            table.add_row(vec![
                comfy_table::Cell::new(i.fullname()).fg(comfy_table::Color::Green),
                comfy_table::Cell::new(i.docs.clone()),
            ]);
        }
        writeln!(w, "{table}").map_err(|x| error::Error::Internal(x.to_string()))
    }

    /// Has the focus changed since the last render sweep?
    pub(crate) fn focus_changed(&self) -> bool {
        self.core.focus_gen != self.last_render_focus_gen
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

    /// Render a node subtree into the destination buffer.
    fn render_traversal(
        &mut self,
        dest_buf: &mut TermBuf,
        styl: &mut StyleManager,
        view_stack: &mut ViewStack,
        node_id: NodeId,
    ) -> Result<()> {
        let (hidden, vp, viewport, children) = {
            let node = &self.core.nodes[node_id];
            (node.hidden, node.vp, node.viewport, node.children.clone())
        };

        if hidden {
            return Ok(());
        }

        styl.push();

        let mut pushed_viewport = false;
        if !vp.view().is_zero() {
            view_stack.push(vp);
            pushed_viewport = true;
        }

        if pushed_viewport {
            let (clip, screen_origin) = match view_stack.projection() {
                Some((canvas_rect, screen_rect)) => (canvas_rect, screen_rect.tl),
                None => (Rect::zero(), Point::zero()),
            };
            {
                let mut rndr = Render::new_shared(&self.style, styl, dest_buf, clip, screen_origin);

                let render_result = self.core.with_widget_view(node_id, |widget, core| {
                    let ctx = CoreViewContext::new(core, node_id);
                    widget.render(&mut rndr, viewport, &ctx)
                });
                render_result?;
            }

            let canvas = vp.canvas().rect();
            for child in children {
                let (hidden, child_vp) = {
                    let node = &self.core.nodes[child];
                    (node.hidden, node.vp)
                };
                if !hidden && !position_within_canvas(canvas, child_vp.view(), child_vp.position())
                {
                    return Err(error::Error::Render(format!(
                        "Child node '{child:?}' has position {:?} outside parent canvas {:?}",
                        child_vp.position(),
                        canvas
                    )));
                }
                self.render_traversal(dest_buf, styl, view_stack, child)?;
            }
        }

        if pushed_viewport {
            view_stack.pop()?;
        }
        styl.pop();

        Ok(())
    }

    /// Post-render sweep of the tree.
    pub(crate) fn post_render(&mut self, buf: &mut TermBuf) -> Result<()> {
        let mut current = self.core.focus;
        let mut cursor_spec: Option<(NodeId, Rect, cursor::Cursor)> = None;
        while let Some(id) = current {
            let cursor = self.core.with_widget_view(id, |w, _| w.cursor());
            if let Some(node_cursor) = cursor
                && let Some(node) = self.core.nodes.get(id)
            {
                cursor_spec = Some((id, node.viewport, node_cursor));
                break;
            }
            current = self.core.nodes.get(id).and_then(|n| n.parent);
        }

        if let Some((_nid, viewport, c)) = cursor_spec {
            let view_rect = Rect::new(0, 0, viewport.w, viewport.h);
            if view_rect.contains_point(c.location) {
                let screen_pos = Point {
                    x: viewport.tl.x.saturating_add(c.location.x),
                    y: viewport.tl.y.saturating_add(c.location.y),
                };
                buf.overlay_cursor(screen_pos, c.shape);
            }
        }

        Ok(())
    }

    /// Render the widget tree. All visible nodes are rendered.
    pub(crate) fn render<R: RenderBackend>(&mut self, be: &mut R) -> Result<()> {
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

            let screen_vp = ViewPort::new(root_size, root_size.rect(), (0, 0))?;
            let mut view_stack = ViewStack::new(screen_vp);

            self.render_traversal(&mut next, &mut styl, &mut view_stack, self.core.root)?;
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
        }

        Ok(())
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
        let mut path = self.location_path(m.location)?;
        let mut script = None;

        if let Some(nid) = self.core.locate_node(self.core.root, m.location)? {
            let mut target = Some(nid);
            while let Some(id) = target {
                let screen_rect = self
                    .core
                    .nodes
                    .get(id)
                    .map(|n| n.viewport)
                    .unwrap_or_default();
                let local_location = if screen_rect.contains_point(m.location) {
                    screen_rect.rebase_point(m.location)?
                } else {
                    Point {
                        x: m.location.x.saturating_sub(screen_rect.tl.x),
                        y: m.location.y.saturating_sub(screen_rect.tl.y),
                    }
                };

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
                        break;
                    }
                    EventOutcome::Ignore => {
                        if let Some(s) = self
                            .keymap
                            .resolve(&path, &inputmap::InputSpec::Mouse(m.into()))
                        {
                            script = Some((s, id));
                            break;
                        }
                        path.pop();
                        target = self.core.nodes[id].parent;
                    }
                }
            }
        }

        if let Some((sid, nid)) = script {
            self.run_script(nid, sid)?;
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
        let mut script = None;

        while let Some(id) = target {
            if let Some(s) = self.keymap.resolve(&path, &inputmap::InputSpec::Key(k)) {
                script = Some((s, id));
                break;
            }

            let outcome = self.core.dispatch_event_on_node(id, &Event::Key(k));
            match outcome {
                EventOutcome::Handle | EventOutcome::Consume => {
                    break;
                }
                EventOutcome::Ignore => {
                    path.pop();
                    target = self.core.nodes[id].parent;
                }
            }
        }

        if let Some((sid, nid)) = script {
            self.run_script(nid, sid)?;
        }

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
            Event::Key(k) => {
                self.key(k)?;
            }
            Event::Mouse(m) => {
                self.mouse(m)?;
            }
            Event::Resize(s) => {
                self.set_root_size(s)?;
            }
            Event::Poll(ids) => {
                self.poll(&ids)?;
            }
            _ => {}
        };
        Ok(())
    }

    /// Set the size on the root node.
    pub fn set_root_size(&mut self, size: Expanse) -> Result<()> {
        self.root_size = Some(size);
        self.core.update_layout(size)?;
        Ok(())
    }
}

/// Validate a child viewport position against the parent canvas bounds.
fn position_within_canvas(canvas: Rect, view: Rect, position: Point) -> bool {
    let max_x = canvas.tl.x.saturating_add(canvas.w);
    let max_y = canvas.tl.y.saturating_add(canvas.h);

    let x_ok = if view.w == 0 {
        position.x >= canvas.tl.x && position.x <= max_x
    } else {
        position.x >= canvas.tl.x && position.x < max_x
    };
    let y_ok = if view.h == 0 {
        position.y >= canvas.tl.y && position.y <= max_y
    } else {
        position.y >= canvas.tl.y && position.y < max_y
    };

    x_ok && y_ok
}

/// A trait that allows widgets to perform recursive initialization of themselves and their children.
pub trait Loader {
    /// Load commands or resources into the canopy instance.
    fn load(_: &mut Canopy) {}
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use taffy::style::Dimension;

    use super::*;
    use crate::{
        Context, ViewContext,
        commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
        derive_commands,
        error::{Error, Result},
        geom::{Direction, Rect},
        path::Path,
        state::NodeName,
        testing::{
            backend::{CanvasRender, TestRender},
            ttree::{Ba, BaLa, BaLb, OutcomeTarget, R, get_state, reset_state, run_ttree},
        },
        widget::{EventOutcome, Widget},
    };

    fn set_outcome<T: Any + OutcomeTarget>(core: &mut Core, id: NodeId, outcome: EventOutcome) {
        core.with_widget_mut(id, |w, _| {
            let any = w as &mut dyn Any;
            if let Some(node) = any.downcast_mut::<T>() {
                node.set_outcome(outcome);
            }
        });
    }

    fn make_mouse_event(core: &Core, node_id: NodeId) -> mouse::MouseEvent {
        let loc = core
            .nodes
            .get(node_id)
            .map(|n| n.viewport.tl)
            .unwrap_or_default();
        mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: loc,
        }
    }

    #[test]
    fn tbindings() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.keymap.bind(
                "",
                inputmap::InputSpec::Key('a'.into()),
                "",
                c.script_host.compile("ba_la::c_leaf()")?,
            )?;
            c.keymap.bind(
                "",
                inputmap::InputSpec::Key('r'.into()),
                "",
                c.script_host.compile("r::c_root()")?,
            )?;
            c.keymap.bind(
                "",
                inputmap::InputSpec::Key('x'.into()),
                "ba/",
                c.script_host.compile("r::c_root()")?,
            )?;

            c.core.set_focus(tree.a_a);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la.c_leaf()"]);

            reset_state();
            c.key('r')?;
            let s = get_state();
            assert_eq!(s.path, vec!["r.c_root()"]);

            reset_state();
            c.core.set_focus(tree.a);
            c.key('a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la.c_leaf()"]);

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
            let size = 100;
            tr.render(c)?;
            assert_eq!(
                c.core.nodes[tree.root].viewport,
                Rect::new(0, 0, size, size)
            );
            assert_eq!(
                c.core.nodes[tree.a].viewport,
                Rect::new(0, 0, size / 2, size)
            );
            assert_eq!(
                c.core.nodes[tree.b].viewport,
                Rect::new(size / 2, 0, size / 2, size)
            );

            c.set_root_size(Expanse::new(50, 50))?;
            tr.render(c)?;
            assert_eq!(c.core.nodes[tree.b].viewport, Rect::new(25, 0, 25, 50));
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
            fn commands() -> Vec<CommandSpec> {
                vec![]
            }

            fn dispatch(
                &mut self,
                _c: &mut dyn Context,
                _cmd: &CommandInvocation,
            ) -> Result<ReturnValue> {
                Err(Error::UnknownCommand("".into()))
            }
        }

        impl Widget for N {
            fn accept_focus(&self) -> bool {
                true
            }

            fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
                r.text("any", ctx.view().line(0), "<n>")
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
            fn render(
                &mut self,
                _r: &mut Render,
                _area: Rect,
                _ctx: &dyn ViewContext,
            ) -> Result<()> {
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
            fn render(
                &mut self,
                _r: &mut Render,
                _area: Rect,
                _ctx: &dyn ViewContext,
            ) -> Result<()> {
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
        let child = canopy.core.add(Child);
        canopy.core.set_children(canopy.core.root, vec![child])?;
        canopy.core.build(child).style(|style| {
            style.size.width = Dimension::Points(0.0);
            style.size.height = Dimension::Points(0.0);
        });

        canopy.set_root_size(size)?;
        canopy.render(&mut cr)?;
        Ok(())
    }
}
