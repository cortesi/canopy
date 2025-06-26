use std::{io::Write, process, sync::mpsc};

use comfy_table::{ContentArrangement, Table};

use crate::{
    backend::BackendControl,
    commands, cursor, error,
    event::{key, mouse, Event},
    geom::{Coverage, Direction, Expanse, Point, Rect},
    inputmap,
    node::Node,
    path::*,
    poll::Poller,
    render::{show_cursor, RenderBackend},
    script,
    style::{solarized, StyleManager, StyleMap},
    tree::*,
    EventOutcome, Layout, NodeId, Render, Result, ViewPort,
};

/// The API exposed to nodes by Canopy.
pub trait Context {
    /// Does the node need to render in the next sweep? This checks if the node is currently hidden, and if not, signals
    /// that we should render if the node is tainted, its focus status has changed, or if it is forcing a render.
    fn needs_render(&self, n: &dyn Node) -> bool;

    /// Is the specified node on the focus path? A node is on the focus path if it
    /// has focus, or if it's the ancestor of a node with focus.
    fn is_on_focus_path(&self, n: &mut dyn Node) -> bool;

    /// Does the node have focus?
    fn is_focused(&self, n: &dyn Node) -> bool;

    /// Get the Rect of the screen area that currently has focus.
    fn focus_area(&self, root: &mut dyn Node) -> Option<Rect>;

    /// Move focus downward of the currently focused node within the subtree at root.
    fn focus_down(&mut self, root: &mut dyn Node);

    /// Focus the first node that accepts focus in the pre-order traversal of the subtree at root.
    fn focus_first(&mut self, root: &mut dyn Node);

    /// Move focus to the left of the currently focused node within the subtree at root.
    fn focus_left(&mut self, root: &mut dyn Node);

    /// Focus the next node in the pre-order traversal of root. If no node with focus is found, we focus the first node
    /// we can find instead.
    fn focus_next(&mut self, root: &mut dyn Node);

    /// Return the focus path for the subtree under `root`.
    fn focus_path(&self, root: &mut dyn Node) -> Path;

    /// Focus the previous node in the pre-order traversal of `root`. If no node with focus is found, we focus the first
    /// node we can find instead.
    fn focus_prev(&mut self, root: &mut dyn Node);

    /// Move focus to  right of the currently focused node within the subtree at root.
    fn focus_right(&mut self, root: &mut dyn Node);

    /// Move focus upward of the currently focused node within the subtree at root.
    fn focus_up(&mut self, root: &mut dyn Node);

    /// Focus a node. Returns `true` if focus changed.
    fn set_focus(&mut self, n: &mut dyn Node) -> bool;

    /// Move focus in a specified direction within the subtree at root.
    fn focus_dir(&mut self, root: &mut dyn Node, dir: Direction);

    /// Scroll the view to the specified position. The view is clamped within
    /// the outer rectangle. Returns `true` if the view changed.
    fn scroll_to(&mut self, n: &mut dyn Node, x: u16, y: u16) -> bool {
        let before = n.vp().view();
        n.state_mut().scroll_to(x, y);
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view by the given offsets. The view rectangle is clamped
    /// within the outer rectangle. Returns `true` if the view changed.
    fn scroll_by(&mut self, n: &mut dyn Node, x: i16, y: i16) -> bool {
        let before = n.vp().view();
        n.state_mut().scroll_by(x, y);
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view up by the height of the view rectangle. Returns `true`
    /// if the view changed.
    fn page_up(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.state_mut().page_up();
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view down by the height of the view rectangle. Returns `true`
    /// if the view changed.
    fn page_down(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.state_mut().page_down();
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view up by one line. Returns `true` if the view changed.
    fn scroll_up(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.state_mut().scroll_up();
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view down by one line. Returns `true` if the view changed.
    fn scroll_down(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.state_mut().scroll_down();
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view left by one line. Returns `true` if the view changed.
    fn scroll_left(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.state_mut().scroll_left();
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view right by one line. Returns `true` if the view changed.
    fn scroll_right(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.state_mut().scroll_right();
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Taint a node to signal that it should be re-rendered.
    fn taint(&mut self, n: &mut dyn Node);

    /// Taint the entire subtree under a node.
    fn taint_tree(&mut self, e: &mut dyn Node);

    /// Start the backend renderer.
    fn start(&mut self) -> Result<()>;

    /// Stop the backend renderer, releasing control of the terminal.
    fn stop(&mut self) -> Result<()>;

    /// Stop the render backend and exit the process.
    fn exit(&mut self, code: i32) -> !;

    /// Current focus generation counter.
    fn current_focus_gen(&self) -> u64 {
        0
    }
}

#[derive(Debug)]
pub struct Canopy {
    /// A counter that is incremented every time focus changes. The current focus
    /// will have a state `focus_gen` equal to this.
    focus_gen: u64,
    /// Stores the focus_gen during the last render. Used to detect if focus has
    /// changed.
    last_render_focus_gen: u64,

    /// A counter that is incremented every time we render. All items that
    /// require rendering during the current sweep will have a state `render_gen`
    /// equal to this.
    render_gen: u64,
    /// The poller is responsible for tracking nodes that have pending poll
    /// events, and scheduling their execution.
    poller: Poller,
    /// Has the tree been tainted? Resets to false before every event sweep.
    pub(crate) taint: bool,
    /// Root window size
    pub(crate) root_size: Option<Expanse>,

    pub(crate) script_host: script::ScriptHost,
    pub(crate) keymap: inputmap::InputMap,
    pub(crate) commands: commands::CommandSet,
    pub(crate) backend: Option<Box<dyn BackendControl>>,

    pub(crate) event_tx: mpsc::Sender<Event>,
    pub(crate) event_rx: Option<mpsc::Receiver<Event>>,

    pub style: StyleMap,
}

impl Context for Canopy {
    /// Does the node need to render in the next sweep? This checks if the node
    /// is currently hidden, and if not, signals that we should render if:
    ///
    /// - the node is tainted
    /// - its focus status has changed
    /// - it is forcing a render
    fn needs_render(&self, n: &dyn Node) -> bool {
        !n.is_hidden() && (n.force_render(self) || self.is_tainted(n) || self.node_focus_changed(n))
    }

    /// Taint a node for render.
    fn taint(&mut self, n: &mut dyn Node) {
        let r = n.state_mut();
        r.render_gen = self.render_gen;
        self.taint = true;
    }

    /// Mark a tree of nodes for render.
    fn taint_tree(&mut self, e: &mut dyn Node) {
        postorder(e, &mut |x| -> Result<Walk<()>> {
            self.taint(x);
            Ok(Walk::Continue)
        })
        // Unwrap is safe, because no operations in the closure can fail.
        .unwrap();
    }

    /// Is the specified node on the focus path? A node is on the focus path if it
    /// has focus, or if it's the ancestor of a node with focus.
    fn is_on_focus_path(&self, n: &mut dyn Node) -> bool {
        self.walk_focus_path(n, &mut |_| -> Result<Walk<bool>> { Ok(Walk::Handle(true)) })
            // We're safe to unwrap, because our closure can't return an error.
            .unwrap()
            .unwrap_or(false)
    }

    /// Return the focus path for the subtree under `root`.
    fn focus_path(&self, root: &mut dyn Node) -> Path {
        let mut p = Vec::new();
        self.walk_focus_path(root, &mut |n| -> Result<Walk<()>> {
            p.insert(0, n.name().to_string());
            Ok(Walk::Continue)
        })
        // We're safe to unwrap because our closure can't return an error.
        .unwrap();
        Path::new(&p)
    }

    /// Find the area of the current terminal focus node under the specified `root`.
    fn focus_area(&self, root: &mut dyn Node) -> Option<Rect> {
        self.walk_focus_path(root, &mut |x| -> Result<Walk<Rect>> {
            Ok(Walk::Handle(x.vp().screen_rect()))
        })
        // We're safe to unwrap, because our closure can't return an error.
        .unwrap()
    }

    /// Move focus in a specified direction within the subtree at root.
    fn focus_dir(&mut self, root: &mut dyn Node, dir: Direction) {
        let mut seen = false;
        let mut last = None;
        if let Some(start) = self.focus_area(root) {
            let bounds = self
                .root_size
                .unwrap_or_else(|| Expanse::new(u16::MAX, u16::MAX));

            start
                .search(dir, &mut |p| -> Result<bool> {
                    if seen
                        || p.x >= bounds.w
                        || p.y >= bounds.h
                    {
                        return Ok(true);
                    }
                    let n = node_at(root, p);
                    if n != last {
                        last = n.clone();
                        if let Some(nid) = &n {
                            walk_to_root(root, nid, &mut |x| {
                                if !seen && x.accept_focus() {
                                    seen = true;
                                    self.set_focus(x);
                                }
                                Ok(())
                            })
                            // Unwrap is safe, because the closure cannot fail
                            .unwrap();
                        }
                    }
                    Ok(false)
                })
                .unwrap()
        }
    }

    /// Move focus to  right of the currently focused node within the subtree at root.
    fn focus_right(&mut self, root: &mut dyn Node) {
        self.focus_dir(root, Direction::Right)
    }

    /// Move focus to the left of the currently focused node within the subtree at root.
    fn focus_left(&mut self, root: &mut dyn Node) {
        self.focus_dir(root, Direction::Left)
    }

    /// Move focus upward of the currently focused node within the subtree at root.
    fn focus_up(&mut self, root: &mut dyn Node) {
        self.focus_dir(root, Direction::Up)
    }

    /// Move focus downward of the currently focused node within the subtree at root.
    fn focus_down(&mut self, root: &mut dyn Node) {
        self.focus_dir(root, Direction::Down)
    }

    /// Focus the first node that accepts focus in the pre-order traversal of
    /// the subtree at root.
    fn focus_first(&mut self, root: &mut dyn Node) {
        let mut focus_set = false;
        preorder(root, &mut |x| -> Result<Walk<()>> {
            Ok(if x.is_hidden() {
                Walk::Skip
            } else if !focus_set && x.accept_focus() {
                self.set_focus(x);
                focus_set = true;
                Walk::Handle(())
            } else {
                Walk::Continue
            })
        })
        // Unwrap is safe, because the closure cannot fail.
        .unwrap();
    }

    /// Focus the next node in the pre-order traversal of root. If no node with
    /// focus is found, we focus the first node we can find instead.
    fn focus_next(&mut self, root: &mut dyn Node) {
        let mut focus_seen = false;
        let ret = preorder(root, &mut |x| -> Result<Walk<()>> {
            if x.is_hidden() {
                return Ok(Walk::Skip);
            }
            if focus_seen {
                if x.accept_focus() {
                    self.set_focus(x);
                    return Ok(Walk::Handle(()));
                }
            } else if self.is_focused(x) {
                focus_seen = true;
            }
            Ok(Walk::Continue)
        })
        // Unwrap is safe, because the closure cannot fail.
        .unwrap();
        if !ret.is_handled() {
            self.focus_first(root)
        }
    }

    /// Focus the previous node in the pre-order traversal of `root`. If no node
    /// with focus is found, we focus the first node we can find instead.
    fn focus_prev(&mut self, root: &mut dyn Node) {
        let current = self.focus_gen;
        let mut first = true;
        preorder(root, &mut |x| -> Result<Walk<()>> {
            if x.is_hidden() {
                return Ok(Walk::Skip);
            }
            if first {
                // We skip the first node in the traversal
                first = false
            } else if x.state().focus_gen == current {
                // This is the node that was previously focused, so we can stop.
                return Ok(Walk::Handle(()));
            } else if x.accept_focus() {
                // Speculatively set focus on this node.
                self.set_focus(x);
            }
            Ok(Walk::Continue)
        })
        // Unwrap is safe, because the closure cannot fail.
        .unwrap();
    }

    /// Focus a node. Returns `true` if focus changed.
    fn set_focus(&mut self, n: &mut dyn Node) -> bool {
        if self.is_focused(n) {
            false
        } else {
            self.focus_gen += 1;
            n.state_mut().focus_gen = self.focus_gen;
            true
        }
    }

    /// Does the node have terminal focus?
    fn is_focused(&self, n: &dyn Node) -> bool {
        n.state().focus_gen == self.focus_gen
    }

    /// Start the backend renderer.
    fn start(&mut self) -> Result<()> {
        self.backend.as_mut().unwrap().start()
    }

    /// Stop the backend renderer, releasing control of the terminal.
    fn stop(&mut self) -> Result<()> {
        self.backend.as_mut().unwrap().stop()
    }

    /// Stop the render backend and exit the process.
    fn exit(&mut self, code: i32) -> ! {
        let _ = self.stop();
        process::exit(code)
    }

    fn current_focus_gen(&self) -> u64 {
        self.focus_gen
    }
}

impl Canopy {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Canopy {
            focus_gen: 1,
            last_render_focus_gen: 1,
            render_gen: 1,
            taint: false,
            poller: Poller::new(tx.clone()),
            event_tx: tx,
            event_rx: Some(rx),
            keymap: inputmap::InputMap::new(),
            commands: commands::CommandSet::new(),
            script_host: script::ScriptHost::new(),
            style: solarized::solarized_dark(),
            root_size: None,
            backend: None,
        }
    }

    pub fn register_backend<T: BackendControl + 'static>(&mut self, be: T) {
        self.backend = Some(Box::new(be))
    }

    pub fn run_script(
        &mut self,
        root: &mut dyn Node,
        node_id: NodeId,
        sid: script::ScriptId,
    ) -> Result<()> {
        let this: *mut dyn Context = self;
        let script_host = &mut self.script_host;
        // SAFETY: `this` is valid for the duration of this call because we have
        // a mutable reference to `self`.
        unsafe { script_host.execute(&mut *this, root, node_id, sid) }?;
        Ok(())
    }

    /// Bind a mouse action in the global mode with a given path filter to a script.
    pub fn bind_mouse<K>(&mut self, mouse: K, path_filter: &str, script: &str) -> Result<()>
    where
        mouse::Mouse: From<K>,
    {
        self.bind_mode_mouse(mouse, "", path_filter, script)
    }

    /// Bind a mouse action in a specified mode with a given path filter to a
    /// script.
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
            inputmap::Input::Mouse(mouse.into()),
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
            inputmap::Input::Key(key.into()),
            path_filter,
            self.script_host.compile(script)?,
        )
    }

    /// Load the commands from a command node using the default node name
    /// derived from the name of the struct.
    pub fn add_commands<T: commands::CommandNode>(&mut self) {
        let cmds = <T>::commands();
        self.script_host.load_commands(&cmds);
        self.commands.commands(&cmds);
    }

    /// Output a formatted table of commands to a writer.
    pub fn print_command_table(&self, w: &mut dyn Write) -> Result<()> {
        let mut cmds: Vec<&commands::CommandSpec> = self.commands.commands.values().collect();

        cmds.sort_by_key(|a| a.fullname());

        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.load_preset(comfy_table::presets::UTF8_FULL);
        for i in cmds {
            table.add_row(vec![
                comfy_table::Cell::new(i.fullname()).fg(comfy_table::Color::Green),
                comfy_table::Cell::new(i.docs.clone()),
            ]);
        }
        writeln!(w, "{table}").map_err(|x| error::Error::Internal(x.to_string()))
    }

    /// Has the focus status of this node changed since the last render
    /// sweep?
    fn node_focus_changed(&self, n: &dyn Node) -> bool {
        if self.focus_changed() {
            let s = n.state();
            // Our focus has changed if we're the currently focused node, or
            // if we were previously focused during the last sweep.
            s.focus_gen == self.focus_gen || s.focus_gen == self.last_render_focus_gen
        } else {
            false
        }
    }

    /// Has the focus path status of this node changed since the last render
    /// sweep?
    pub fn node_focus_path_changed(&self, n: &dyn Node) -> bool {
        if self.focus_changed() {
            let s = n.state();
            // Our focus has changed if we're the currently on the focus path, or
            // if we were previously focused during the last sweep.
            s.focus_path_gen == self.focus_gen || s.focus_path_gen == self.last_render_focus_gen
        } else {
            false
        }
    }

    /// Is this node render tainted?
    fn is_tainted(&self, n: &dyn Node) -> bool {
        let s = n.state();
        // Tainting if render_gen is 0 lets us initialize a nodestate
        // without knowing about the app state
        self.render_gen == s.render_gen || s.render_gen == 0
    }

    /// Has the focus changed since the last render sweep?
    pub(crate) fn focus_changed(&self) -> bool {
        self.focus_gen != self.last_render_focus_gen
    }

    /// Register the poller channel
    pub(crate) fn start_poller(&mut self, tx: mpsc::Sender<Event>) {
        self.event_tx = tx;
    }

    /// Pre-render sweep of the tree.
    pub(crate) fn pre_render<R: RenderBackend>(
        &mut self,
        r: &mut R,
        root: &mut dyn Node,
    ) -> Result<()> {
        let mut seen = false;
        preorder(root, &mut |x| -> Result<Walk<()>> {
            if self.is_focused(x) {
                seen = true;
            }
            if !x.is_initialized() {
                if let Some(d) = x.poll(self) {
                    self.poller.schedule(x.id(), d);
                }
                x.state_mut().initialized = true;
            }
            Ok(Walk::Continue)
        })?;
        if !seen {
            self.focus_first(root);
        }

        if self.focus_changed() {
            let fg = self.focus_gen;
            self.walk_focus_path(root, &mut |n| -> Result<Walk<()>> {
                n.state_mut().focus_path_gen = fg;
                Ok(Walk::Continue)
            })?;
        }

        // The cursor is disabled before every render sweep, otherwise we would
        // see it visibly on screen during redraws.
        r.hide_cursor()?;
        Ok(())
    }

    fn render_traversal<R: RenderBackend>(
        &mut self,
        r: &mut R,
        styl: &mut StyleManager,
        n: &mut dyn Node,
        base: Point,
    ) -> Result<()> {
        if !n.is_hidden() {
            styl.push();
            if self.needs_render(n) {
                if self.is_focused(n) {
                    let s = &mut n.state_mut();
                    s.rendered_focus_gen = self.focus_gen;
                }

                let mut c = Coverage::new(n.vp().screen_rect().expanse());
                let mut rndr = Render::new(r, &self.style, styl, n.vp(), &mut c, base);

                n.render(self, &mut rndr)?;

                // Now add regions managed by children to coverage
                let parent = n.vp().screen_rect();
                n.children(&mut |child| {
                    if !child.is_hidden() {
                        let child_rect = child.vp().screen_rect();
                        assert!(
                            parent.contains_rect(&child_rect),
                            "child {} viewport {:?} outside parent {:?}",
                            child.name(),
                            child_rect,
                            parent
                        );
                        if !child_rect.is_zero() {
                            rndr.coverage.add(child_rect);
                        }
                    }
                    Ok(())
                })?;

                // We now have coverage, relative to this node's screen rectange. We
                // rebase each rect back down to our virtual co-ordinates.
                let sr = n.vp().view();
                for l in rndr.coverage.uncovered() {
                    rndr.fill("", l.rect().shift(sr.tl.x as i16, sr.tl.y as i16), ' ')?;
                }
            }
            // This is a new node - we don't want it perpetually stuck in
            // render, so we need to update its render_gen.
            if n.state().render_gen == 0 {
                n.state_mut().render_gen = self.render_gen;
            }
            let parent = n.vp().screen_rect();
            n.children(&mut |child| {
                if !child.is_hidden() {
                    let child_rect = child.vp().screen_rect();
                    assert!(
                        parent.contains_rect(&child_rect),
                        "child {} viewport {:?} outside parent {:?}",
                        child.name(),
                        child_rect,
                        parent
                    );
                }
                self.render_traversal(r, styl, child, base)
            })?;
            styl.pop();
        }
        Ok(())
    }

    /// Post-render sweep of the tree.
    pub(crate) fn post_render<R: RenderBackend>(
        &self,
        r: &mut R,
        styl: &mut StyleManager,
        root: &mut dyn Node,
    ) -> Result<()> {
        let mut cn: Option<(NodeId, ViewPort, cursor::Cursor)> = None;
        self.walk_focus_path(root, &mut |n| -> Result<Walk<()>> {
            Ok(if let Some(c) = n.cursor() {
                cn = Some((n.id(), n.vp(), c));

                Walk::Handle(())
            } else {
                Walk::Continue
            })
        })?;
        if let Some((_nid, vp, c)) = cn {
            show_cursor(r, &self.style, styl, vp, "cursor", c + vp.position())?;
        }

        Ok(())
    }

    /// Render a tree of nodes. If force is true, all visible nodes are
    /// rendered, otherwise we check the taint state. Hidden nodes and their
    /// children are ignored.
    pub(crate) fn render<R: RenderBackend>(
        &mut self,
        be: &mut R,
        root: &mut dyn Node,
    ) -> Result<()> {
        if let Some(root_size) = self.root_size {
            // This calls fit recursively on the entire tree, so after this all nodes are positioned.
            let l = Layout {};
            root.layout(&l, root_size)?;

            let mut styl = StyleManager::default();
            be.reset()?;
            styl.reset();

            self.pre_render(be, root)?;
            self.render_traversal(be, &mut styl, root, (0, 0).into())?;
            self.render_gen += 1;
            self.last_render_focus_gen = self.focus_gen;
            self.post_render(be, &mut styl, root)?;
        }

        Ok(())
    }

    /// Return the path for the uppermost node at a specific location. Return an empty
    /// path if the location is outside of the node tree.
    fn location_path(&self, root: &mut dyn Node, location: Point) -> Path {
        let id = locate(root, location, &mut |x| -> Result<Locate<NodeId>> {
            Ok(Locate::Match(x.id()))
        });

        if let Some(id) = id.unwrap() {
            node_path(&id, root)
        } else {
            Path::empty()
        }
    }

    /// Propagate a mouse event through the node under the event and all its
    /// ancestors. Events are handled only once, and then ignored.
    pub(crate) fn mouse(&mut self, root: &mut dyn Node, m: mouse::MouseEvent) -> Result<()> {
        let mut path = self.location_path(root, m.location);
        let mut script = None;
        let mut handled = false;
        if let Some(nid) = node_at(root, m.location) {
            walk_to_root(root, &nid, &mut |x| {
                if handled {
                    return Ok(());
                }

                let hdl = x.handle_mouse(
                    self,
                    mouse::MouseEvent {
                        action: m.action,
                        button: m.button,
                        modifiers: m.modifiers,
                        location: x.vp().screen_rect().rebase_point(m.location)?,
                    },
                )?;
                match hdl {
                    EventOutcome::Handle => {
                        handled = true;
                        self.taint(x);
                    }
                    EventOutcome::Consume => {
                        handled = true;
                    }
                    _ => {
                        if let Some(s) =
                            self.keymap.resolve(&path, inputmap::Input::Mouse(m.into()))
                        {
                            handled = true;
                            script = Some((s, x.id()));
                        } else {
                            path.pop();
                        }
                    }
                };
                Ok(())
            })?;
        }
        if let Some((sid, nid)) = script {
            self.run_script(root, nid, sid)?;
        }

        Ok(())
    }

    /// Propagate a key event through the focus and all its ancestors.
    pub(crate) fn key<T>(&mut self, root: &mut dyn Node, tk: T) -> Result<()>
    where
        T: Into<key::Key>,
    {
        let k = tk.into();
        let mut path = self.focus_path(root);
        let v = walk_focus_path_e(self.focus_gen, root, &mut |x| -> Result<
            Walk<Option<(script::ScriptId, NodeId)>>,
        > {
            Ok(
                if let Some(s) = self.keymap.resolve(&path, inputmap::Input::Key(k)) {
                    Walk::Handle(Some((s, x.id())))
                } else {
                    match x.handle_key(self, k)? {
                        EventOutcome::Handle => {
                            self.taint(x);
                            Walk::Handle(None)
                        }
                        EventOutcome::Consume => Walk::Handle(None),
                        _ => {
                            path.pop();
                            Walk::Continue
                        }
                    }
                },
            )
        })?;
        if let Some(Some((sid, nid))) = v {
            self.run_script(root, nid, sid)?;
        }
        Ok(())
    }

    /// Handle a poll event by traversing the complete node tree, and triggering
    /// poll on each ID in the poll set.
    fn poll(&mut self, ids: Vec<NodeId>, root: &mut dyn Node) -> Result<()> {
        preorder(root, &mut |x| -> Result<Walk<()>> {
            if ids.contains(&x.id()) {
                if let Some(d) = x.poll(self) {
                    self.poller.schedule(x.id(), d);
                }
            };
            Ok(Walk::Continue)
        })?;
        Ok(())
    }

    /// Propagate an event through the tree.
    pub(crate) fn event(&mut self, root: &mut dyn Node, e: Event) -> Result<()> {
        match e {
            Event::Key(k) => {
                self.key(root, k)?;
            }
            Event::Mouse(m) => {
                self.mouse(root, m)?;
            }
            Event::Resize(s) => {
                self.set_root_size(s, root)?;
            }
            Event::Poll(ids) => {
                self.poll(ids, root)?;
            }
            // FIXME: Implement new crossterm events.
            _ => {}
        };
        Ok(())
    }

    /// Set the size on the root node, and taint the tree.
    pub(crate) fn set_root_size(&mut self, size: Expanse, root: &mut dyn Node) -> Result<()> {
        self.root_size = Some(size);
        self.taint_tree(root);
        // Apply layout immediately so viewport reflects the new size
        let l = Layout {};
        root.layout(&l, size)?;
        Ok(())
    }

    /// Call a closure on the currently focused node and all its ancestors to the
    /// root. If the closure returns Walk::Handle, traversal stops. Handle::Skip is
    /// ignored.
    pub(crate) fn walk_focus_path<R>(
        &self,
        root: &mut dyn Node,
        f: &mut dyn FnMut(&mut dyn Node) -> Result<Walk<R>>,
    ) -> Result<Option<R>> {
        walk_focus_path_e(self.focus_gen, root, f)
    }
}

/// A trait that allows widgets to perform recursive initialization of
/// themselves and their children. The most common use for this trait is to load
/// the command sets from a node tree.
pub trait Loader {
    fn load(_: &mut Canopy) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{geom::Rect, tutils::*, StatefulNode};

    #[test]
    fn tbindings() -> Result<()> {
        run(|c, _, mut root| {
            c.keymap.bind(
                "",
                inputmap::Input::Key('a'.into()),
                "",
                c.script_host.compile("ba_la::c_leaf()")?,
            )?;
            c.keymap.bind(
                "",
                inputmap::Input::Key('r'.into()),
                "",
                c.script_host.compile("r::c_root()")?,
            )?;
            c.keymap.bind(
                "",
                inputmap::Input::Key('x'.into()),
                "ba/",
                c.script_host.compile("r::c_root()")?,
            )?;

            c.set_focus(&mut root.a.a);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la.c_leaf()"]);

            reset_state();
            c.key(&mut root, 'r')?;
            let s = get_state();
            assert_eq!(s.path, vec!["r.c_root()"]);

            reset_state();
            c.set_focus(&mut root.a);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la.c_leaf()"]);

            reset_state();
            c.set_focus(&mut root.a.a);
            c.key(&mut root, 'x')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->ignore", "r.c_root()"]);

            reset_state();
            c.set_focus(&mut root);
            c.key(&mut root, 'x')?;
            let s = get_state();
            assert_eq!(s.path, vec!["r@key->ignore"]);

            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tkey() -> Result<()> {
        run(|c, _, mut root| {
            c.set_focus(&mut root);
            root.next_outcome = Some(EventOutcome::Handle);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["r@key->handle"]);
            Ok(())
        })?;

        run(|c, _, mut root| {
            c.set_focus(&mut root.a.a);
            root.a.a.next_outcome = Some(EventOutcome::Handle);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->handle"]);
            Ok(())
        })?;

        run(|c, _, mut root| {
            c.set_focus(&mut root.a.a);
            root.a.next_outcome = Some(EventOutcome::Handle);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->ignore", "ba@key->handle"]);
            Ok(())
        })?;

        run(|c, _, mut root| {
            c.set_focus(&mut root.a.a);
            root.next_outcome = Some(EventOutcome::Handle);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_la@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run(|c, _, mut root| {
            c.set_focus(&mut root.a);
            root.a.next_outcome = Some(EventOutcome::Handle);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->handle"]);
            Ok(())
        })?;

        run(|c, _, mut root| {
            c.set_focus(&mut root.a);
            root.next_outcome = Some(EventOutcome::Handle);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->ignore", "r@key->handle"]);
            c.key(&mut root, 'a')?;
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

        run(|c, _, mut root| {
            c.set_focus(&mut root.a.b);
            root.a.next_outcome = Some(EventOutcome::Ignore);
            root.next_outcome = Some(EventOutcome::Handle);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_lb@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run(|c, _, mut root| {
            c.set_focus(&mut root.a.a);
            root.a.a.next_outcome = Some(EventOutcome::Handle);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->handle",]);
            Ok(())
        })?;

        run(|c, _, mut root| {
            c.set_focus(&mut root.a.b);
            root.a.next_outcome = Some(EventOutcome::Handle);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->ignore", "ba@key->handle",]);
            Ok(())
        })?;

        run(|c, _, mut root| {
            c.set_focus(&mut root.a.b);
            root.a.b.next_outcome = Some(EventOutcome::Handle);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->handle",]);
            Ok(())
        })?;

        run(|c, _, mut root| {
            c.set_focus(&mut root.a.b);
            root.a.b.next_outcome = Some(EventOutcome::Handle);
            root.a.next_outcome = Some(EventOutcome::Handle);
            c.key(&mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->handle",]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tmouse() -> Result<()> {
        run(|c, mut tr, mut root| {
            c.set_focus(&mut root);
            root.next_outcome = Some(EventOutcome::Handle);
            let evt = root.a.a.make_mouse_event()?;
            tr.render(c, &mut root)?;
            c.mouse(&mut root, evt)?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_la@mouse->ignore", "ba@mouse->ignore", "r@mouse->handle"]
            );
            Ok(())
        })?;

        run(|c, mut tr, mut root| {
            root.a.a.next_outcome = Some(EventOutcome::Handle);
            let evt = root.a.a.make_mouse_event()?;
            tr.render(c, &mut root)?;
            c.mouse(&mut root, evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
            Ok(())
        })?;

        run(|c, mut tr, mut root| {
            root.a.a.next_outcome = Some(EventOutcome::Handle);
            let evt = root.a.a.make_mouse_event()?;
            tr.render(c, &mut root)?;
            c.mouse(&mut root, evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
            Ok(())
        })?;

        run(|c, mut tr, mut root| {
            root.a.a.next_outcome = Some(EventOutcome::Handle);
            let evt = root.a.a.make_mouse_event()?;
            tr.render(c, &mut root)?;
            c.mouse(&mut root, evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle",]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tresize() -> Result<()> {
        run(|c, mut tr, mut root| {
            let size = 100;
            assert_eq!(root.vp().screen_rect(), Rect::new(0, 0, size, size));
            tr.render(c, &mut root)?;
            assert_eq!(root.a.vp().screen_rect(), Rect::new(0, 0, size / 2, size));
            assert_eq!(
                root.b.vp().screen_rect(),
                Rect::new(size / 2, 0, size / 2, size)
            );

            c.set_root_size(Expanse::new(50, 50), &mut root)?;
            tr.render(c, &mut root)?;
            assert_eq!(root.b.vp().screen_rect(), Rect::new(25, 0, 25, 50));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn trender() -> Result<()> {
        run(|c, mut tr, mut root| {
            tr.render(c, &mut root)?;
            assert_eq!(
                tr.buf_text(),
                vec!["<r>", "<ba>", "<ba_la>", "<ba_lb>", "<bb>", "<bb_la>", "<bb_lb>"]
            );

            tr.render(c, &mut root)?;
            assert!(tr.buf_empty());

            c.taint(&mut root.a);
            tr.render(c, &mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba>"]);

            c.taint(&mut root.a.b);
            tr.render(c, &mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba_lb>"]);

            c.taint_tree(&mut root.a);
            tr.render(c, &mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba>", "<ba_la>", "<ba_lb>"]);

            tr.render(c, &mut root)?;
            assert!(tr.buf_empty());

            c.set_focus(&mut root.a.a);
            tr.render(c, &mut root)?;
            assert_eq!(tr.buf_text(), vec!["<r>", "<ba_la>"]);

            c.focus_next(&mut root);
            tr.render(c, &mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba_la>", "<ba_lb>"]);

            c.focus_prev(&mut root);
            tr.render(c, &mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba_la>", "<ba_lb>"]);

            tr.render(c, &mut root)?;
            assert!(tr.buf_empty());

            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn focus_path() -> Result<()> {
        run(|c, _, mut root| {
            assert_eq!(c.focus_path(&mut root), Path::empty());
            c.focus_next(&mut root);
            assert_eq!(c.focus_path(&mut root), Path::new(&["r"]));
            c.focus_next(&mut root);
            assert_eq!(c.focus_path(&mut root), Path::new(&["r", "ba"]));
            c.focus_next(&mut root);
            assert_eq!(c.focus_path(&mut root), Path::new(&["r", "ba", "ba_la"]));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn focus_next() -> Result<()> {
        run(|c, _, mut root| {
            assert!(!c.is_focused(&root));
            c.focus_next(&mut root);
            assert!(c.is_focused(&root));

            c.focus_next(&mut root);
            assert!(c.is_focused(&root.a));

            c.focus_next(&mut root);
            assert!(c.is_focused(&root.a.a));
            c.focus_next(&mut root);
            assert!(c.is_focused(&root.a.b));
            c.focus_next(&mut root);
            assert!(c.is_focused(&root.b));

            c.set_focus(&mut root.b.b);
            c.focus_next(&mut root);
            assert!(c.is_focused(&root));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn focus_prev() -> Result<()> {
        run(|c, _, mut root| {
            assert!(!c.is_focused(&root));
            c.focus_prev(&mut root);
            assert!(c.is_focused(&root.b.b));

            c.focus_prev(&mut root);
            assert!(c.is_focused(&root.b.a));

            c.focus_prev(&mut root);
            assert!(c.is_focused(&root.b));

            c.set_focus(&mut root);
            c.focus_prev(&mut root);
            assert!(c.is_focused(&root.b.b));

            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tshift_right() -> Result<()> {
        run(|c, mut tr, mut root| {
            tr.render(c, &mut root)?;
            c.set_focus(&mut root.a.a);
            c.focus_right(&mut root);
            assert!(c.is_focused(&root.b.a));
            c.focus_right(&mut root);
            assert!(c.is_focused(&root.b.a));

            c.set_focus(&mut root.a.b);
            c.focus_right(&mut root);
            assert!(c.is_focused(&root.b.b));
            c.focus_right(&mut root);
            assert!(c.is_focused(&root.b.b));
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tfoci() -> Result<()> {
        run(|c, _, mut root| {
            assert_eq!(c.focus_path(&mut root), Path::empty());

            assert!(!c.is_on_focus_path(&mut root));
            assert!(!c.is_on_focus_path(&mut root.a));

            c.set_focus(&mut root.a.a);
            assert!(c.is_on_focus_path(&mut root));
            assert!(c.is_on_focus_path(&mut root.a));
            assert!(!c.is_on_focus_path(&mut root.b));
            assert_eq!(c.focus_path(&mut root), Path::new(&["r", "ba", "ba_la"]));

            c.set_focus(&mut root.a);
            assert_eq!(c.focus_path(&mut root), Path::new(&["r", "ba"]));

            c.set_focus(&mut root);
            assert_eq!(c.focus_path(&mut root), Path::new(&["r"]));

            c.set_focus(&mut root.b.a);
            assert_eq!(c.focus_path(&mut root), Path::new(&["r", "bb", "bb_la"]));
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tkey_no_render() -> Result<()> {
        use crate as canopy;
        use crate::backend::test::TestRender;
        use crate::commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue};
        use crate::{Error, EventOutcome, NodeState};

        #[derive(StatefulNode)]
        struct N {
            state: NodeState,
        }

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

        impl Node for N {
            fn accept_focus(&mut self) -> bool {
                true
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)
            }

            fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
                r.text("any", self.vp().view().line(0), "<n>")
            }

            fn handle_key(&mut self, _c: &mut dyn Context, _k: key::Key) -> Result<EventOutcome> {
                Ok(EventOutcome::Consume)
            }
        }

        let (_, mut tr) = TestRender::create();
        let mut canopy = Canopy::new();
        let mut root = N {
            state: NodeState::default(),
        };
        canopy.add_commands::<N>();

        canopy.set_root_size(Expanse::new(10, 1), &mut root)?;
        canopy.set_focus(&mut root);
        canopy.render(&mut tr, &mut root)?;
        assert!(!tr.buf_empty());
        tr.text.lock().unwrap().text.clear();
        canopy.taint = false;

        canopy.key(&mut root, 'a')?;
        assert!(!canopy.taint);
        if canopy.taint || canopy.focus_changed() {
            canopy.render(&mut tr, &mut root)?;
            canopy.taint = false;
        }
        assert!(tr.buf_empty());
        Ok(())
    }
}
