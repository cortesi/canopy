use std::sync::mpsc;

use crate::{
    control::BackendControl,
    event::{key, mouse, Event},
    geom::{Coverage, Direction, Expanse, Point, Rect},
    node::{postorder, preorder, Node, Walk},
    poll::Poller,
    render::{show_cursor, RenderBackend},
    style::StyleManager,
    KeyMap, NodeId, Outcome, Render, Result, ViewPort,
};

/// Call a closure on the currently focused node and all its ancestors to the
/// root. If the closure returns Walk::Handle, traversal stops. Handle::Skip is
/// ignored.
fn walk_focus_path_e<R>(
    focus_gen: u64,
    root: &mut dyn Node,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<Walk<R>>,
) -> Result<Option<R>> {
    let mut focus_seen = false;
    Ok(postorder(root, &mut |x| -> Result<Walk<R>> {
        Ok(if focus_seen {
            f(x)?
        } else if x.is_hidden() {
            // Hidden nodes don't hold focus
            Walk::Continue
        } else if x.state().focus_gen == focus_gen {
            focus_seen = true;
            // Force skip on continue so we trigger skipping in the postorder
            // traversal.
            match f(x)? {
                Walk::Skip => Walk::Skip,
                Walk::Continue => Walk::Skip,
                Walk::Handle(t) => Walk::Handle(t),
            }
        } else {
            Walk::Continue
        })
    })?
    .value())
}

pub trait Core {
    fn is_on_focus_path(&self, n: &mut dyn Node) -> bool;
    fn is_focused(&self, n: &dyn Node) -> bool;
    fn is_focus_ancestor(&self, n: &mut dyn Node) -> bool;
    fn focus_area(&self, root: &mut dyn Node) -> Option<Rect>;
    fn focus_depth(&self, n: &mut dyn Node) -> usize;
    fn focus_down(&mut self, root: &mut dyn Node) -> Result<Outcome>;
    fn focus_first(&mut self, root: &mut dyn Node) -> Result<Outcome>;
    fn focus_left(&mut self, root: &mut dyn Node) -> Result<Outcome>;
    fn focus_next(&mut self, root: &mut dyn Node) -> Result<Outcome>;
    fn focus_path(&self, root: &mut dyn Node) -> String;
    fn focus_prev(&mut self, root: &mut dyn Node) -> Result<Outcome>;
    fn focus_right(&mut self, root: &mut dyn Node) -> Result<Outcome>;
    fn focus_up(&mut self, root: &mut dyn Node) -> Result<Outcome>;
    fn needs_render(&self, n: &dyn Node) -> bool;
    fn set_focus(&mut self, n: &mut dyn Node);
    fn shift_focus(&mut self, root: &mut dyn Node, dir: Direction) -> Result<Outcome>;
    fn taint(&mut self, n: &mut dyn Node);
    fn taint_tree(&mut self, e: &mut dyn Node);
}

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
    /// Has the tree been tainted? This reset to false before every event sweep.
    pub taint: bool,

    pub keymap: KeyMap,

    pub(crate) event_tx: mpsc::Sender<Event>,
    pub(crate) event_rx: Option<mpsc::Receiver<Event>>,
}

impl Core for Canopy {
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
    fn focus_path(&self, root: &mut dyn Node) -> String {
        let mut path = Vec::new();
        self.walk_focus_path(root, &mut |n| -> Result<Walk<()>> {
            path.insert(0, n.name().to_string());
            Ok(Walk::Continue)
        })
        // We're safe to unwrap because our closure can't return an error.
        .unwrap();
        format!("/{}", path.join("/"))
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
    fn shift_focus(&mut self, root: &mut dyn Node, dir: Direction) -> Result<Outcome> {
        let mut seen = false;
        if let Some(start) = self.focus_area(root) {
            start.search(dir, &mut |p| -> Result<bool> {
                if !root.vp().screen_rect().contains_point(p) {
                    return Ok(true);
                }
                locate(root, p, &mut |x| -> Result<Walk<()>> {
                    if !seen && x.accept_focus() {
                        self.set_focus(x);
                        seen = true;
                    };
                    Ok(Walk::Continue)
                })?;
                Ok(seen)
            })?
        }
        Ok(Outcome::Handle)
    }

    /// Move focus to the right of the currently focused node within the subtree at root.
    fn focus_right(&mut self, root: &mut dyn Node) -> Result<Outcome> {
        self.shift_focus(root, Direction::Right)
    }

    /// Move focus to the left of the currently focused node within the subtree at root.
    fn focus_left(&mut self, root: &mut dyn Node) -> Result<Outcome> {
        self.shift_focus(root, Direction::Left)
    }

    /// Move focus upward of the currently focused node within the subtree at root.
    fn focus_up(&mut self, root: &mut dyn Node) -> Result<Outcome> {
        self.shift_focus(root, Direction::Up)
    }

    /// Move focus downward of the currently focused node within the subtree at root.
    fn focus_down(&mut self, root: &mut dyn Node) -> Result<Outcome> {
        self.shift_focus(root, Direction::Down)
    }

    /// Focus the first node that accepts focus in the pre-order traversal of
    /// the subtree at root.
    fn focus_first(&mut self, root: &mut dyn Node) -> Result<Outcome> {
        let mut focus_set = false;
        preorder(root, &mut |x| -> Result<Walk<()>> {
            Ok(if !focus_set && x.accept_focus() {
                self.set_focus(x);
                focus_set = true;
                Walk::Skip
            } else {
                Walk::Continue
            })
        })?;
        Ok(Outcome::Handle)
    }

    /// A node is on the focus path if it does not have focus itself, but some
    /// node below it does.
    fn is_focus_ancestor(&self, n: &mut dyn Node) -> bool {
        if self.is_focused(n) {
            false
        } else {
            self.is_on_focus_path(n)
        }
    }

    /// Focus the next node in the pre-order traversal of root. If no node with
    /// focus is found, we focus the first node we can find instead.
    fn focus_next(&mut self, root: &mut dyn Node) -> Result<Outcome> {
        let mut focus_set = false;
        let mut focus_seen = false;
        preorder(root, &mut |x| -> Result<Walk<()>> {
            if !focus_set {
                if focus_seen {
                    if x.accept_focus() {
                        self.set_focus(x);
                        focus_set = true;
                    }
                } else if self.is_focused(x) {
                    focus_seen = true;
                }
            }
            Ok(Walk::Continue)
        })?;
        if !focus_set {
            self.focus_first(root)
        } else {
            Ok(Outcome::Handle)
        }
    }

    /// Focus the previous node in the pre-order traversal of `root`. If no node
    /// with focus is found, we focus the first node we can find instead.
    fn focus_prev(&mut self, root: &mut dyn Node) -> Result<Outcome> {
        let current = self.focus_gen;
        let mut focus_seen = false;
        let mut first = true;
        preorder(root, &mut |x| -> Result<Walk<()>> {
            // We skip the first node in the traversal
            if first {
                first = false
            } else if !focus_seen {
                if x.state().focus_gen == current {
                    focus_seen = true;
                } else if x.accept_focus() {
                    self.set_focus(x);
                }
            }
            Ok(Walk::Continue)
        })?;
        Ok(Outcome::Handle)
    }

    /// Returns the focal depth of the specified node. If the node is not part
    /// of the focus chain, the depth is 0. If the node is a leaf focus, the
    /// depth is 1.
    fn focus_depth(&self, n: &mut dyn Node) -> usize {
        let mut total = 0;
        self.walk_focus_path(n, &mut |_| -> Result<Walk<()>> {
            total += 1;
            Ok(Walk::Continue)
        })
        // We're safe to unwrap, because our closure can't return an error.
        .unwrap();
        total
    }

    /// Focus a node.
    fn set_focus(&mut self, n: &mut dyn Node) {
        self.focus_gen += 1;
        n.state_mut().focus_gen = self.focus_gen;
    }

    /// Does the node have terminal focus?
    fn is_focused(&self, n: &dyn Node) -> bool {
        n.state().focus_gen == self.focus_gen
    }
}

impl Canopy {
    pub(crate) fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Canopy {
            focus_gen: 1,
            last_render_focus_gen: 1,
            render_gen: 1,
            taint: false,
            poller: Poller::new(tx.clone()),
            event_tx: tx,
            event_rx: Some(rx),
            keymap: KeyMap::new(),
        }
    }

    /// Has the focus status of this node changed since the last render
    /// sweep?
    pub fn node_focus_changed(&self, n: &dyn Node) -> bool {
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
    pub fn is_tainted(&self, n: &dyn Node) -> bool {
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
            self.focus_first(root)?;
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
    ) -> Result<()> {
        if !n.is_hidden() {
            styl.push();
            if self.needs_render(n) {
                if self.is_focused(n) {
                    let s = &mut n.state_mut();
                    s.rendered_focus_gen = self.focus_gen;
                }

                let mut c = Coverage::new(n.vp().screen_rect().expanse());
                let mut rndr = Render::new(r, styl, n.vp(), &mut c);

                n.render(self, &mut rndr)?;

                // Now add regions managed by children to coverage
                let escreen = n.vp().screen_rect();
                n.children(&mut |n| {
                    if !n.is_hidden() {
                        let s = n.vp().screen_rect();
                        if !s.is_zero() {
                            rndr.coverage.add(escreen.rebase_rect(&s)?);
                        }
                    }
                    Ok(())
                })?;

                // We now have coverage, relative to this node's screen rectange. We
                // rebase each rect back down to our virtual co-ordinates.
                let sr = n.vp().view_rect();
                for l in rndr.coverage.uncovered() {
                    rndr.fill("", l.rect().shift(sr.tl.x as i16, sr.tl.y as i16), ' ')?;
                }
            }
            // This is a new node - we don't want it perpetually stuck in
            // render, so we need to update its render_gen.
            if n.state().render_gen == 0 {
                n.state_mut().render_gen = self.render_gen;
            }
            n.children(&mut |x| self.render_traversal(r, styl, x))?;
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
        self.walk_focus_path(root, &mut |n| -> Result<Walk<()>> {
            Ok(if let Some(c) = n.cursor() {
                show_cursor(r, styl, n.vp(), "cursor", c)?;
                Walk::Handle(())
            } else {
                Walk::Continue
            })
        })?;
        Ok(())
    }

    /// Render a tree of nodes. If force is true, all visible nodes are
    /// rendered, otherwise we check the taint state. Hidden nodes and their
    /// children are ignored.
    pub fn render<R: RenderBackend>(
        &mut self,
        be: &mut R,
        styl: &mut StyleManager,
        root: &mut dyn Node,
    ) -> Result<()> {
        be.reset()?;
        styl.reset();
        self.render_traversal(be, styl, root)?;
        self.render_gen += 1;
        self.last_render_focus_gen = self.focus_gen;
        Ok(())
    }

    /// Propagate a mouse event through the node under the event and all its
    /// ancestors. Events are handled only once, and then ignored.
    pub fn mouse(
        &mut self,
        ctrl: &mut dyn BackendControl,
        root: &mut dyn Node,
        m: mouse::Mouse,
    ) -> Result<()> {
        locate(root, m.loc, &mut |x| {
            let hdl = x.handle_mouse(
                self,
                ctrl,
                mouse::Mouse {
                    action: m.action,
                    button: m.button,
                    modifiers: m.modifiers,
                    loc: x.vp().screen_rect().rebase_point(m.loc)?,
                },
            )?;
            Ok(match hdl {
                Outcome::Handle => {
                    self.taint(x);
                    Walk::Handle(())
                }
                Outcome::Ignore => Walk::Continue,
            })
        })?;
        Ok(())
    }

    /// Propagate a key event through the focus and all its ancestors.
    pub fn key<T>(
        &mut self,
        ctrl: &mut dyn BackendControl,
        root: &mut dyn Node,
        tk: T,
    ) -> Result<()>
    where
        T: Into<key::Key>,
    {
        let k = tk.into();
        walk_focus_path_e(self.focus_gen, root, &mut move |x| -> Result<Walk<()>> {
            Ok(match x.handle_key(self, ctrl, k)? {
                Outcome::Handle => {
                    self.taint(x);
                    Walk::Handle(())
                }
                Outcome::Ignore => Walk::Continue,
            })
        })?;
        Ok(())
    }

    /// Handle a poll event by traversing the complete node tree, and triggering
    /// poll on each ID in the poll set.
    fn poll(&mut self, ids: Vec<u64>, root: &mut dyn Node) -> Result<()> {
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
    pub(crate) fn event(
        &mut self,
        ctrl: &mut dyn BackendControl,
        root: &mut dyn Node,
        e: Event,
    ) -> Result<()> {
        match e {
            Event::Key(k) => {
                self.key(ctrl, root, k)?;
            }
            Event::Mouse(m) => {
                self.mouse(ctrl, root, m)?;
            }
            Event::Resize(s) => {
                self.set_root_size(s, root)?;
            }
            Event::Poll(ids) => {
                self.poll(ids, root)?;
            }
        };
        Ok(())
    }

    /// Set the size on the root node, and taint the tree.
    pub fn set_root_size(&mut self, size: Expanse, n: &mut dyn Node) -> Result<()> {
        let fit = n.fit(size)?;
        let vp = ViewPort::new(fit, fit, Point::default())?;
        n.set_viewport(vp);
        self.taint_tree(n);
        Ok(())
    }

    /// Call a closure on the currently focused node and all its ancestors to the
    /// root. If the closure returns Walk::Handle, traversal stops. Handle::Skip is
    /// ignored.
    pub fn walk_focus_path<R>(
        &self,
        root: &mut dyn Node,
        f: &mut dyn FnMut(&mut dyn Node) -> Result<Walk<R>>,
    ) -> Result<Option<R>> {
        walk_focus_path_e(self.focus_gen, root, f)
    }
}

/// Calls a closure on the leaf node under (x, y), then all its parents to the
/// root.
pub fn locate<R>(
    root: &mut dyn Node,
    p: impl Into<Point>,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<Walk<R>>,
) -> Result<Walk<R>> {
    let mut seen = false;
    let p = p.into();
    postorder(root, &mut |inner| -> Result<Walk<R>> {
        Ok(if seen {
            f(inner)?
        } else if !inner.is_hidden() {
            let a = inner.vp().screen_rect();
            if a.contains_point(p) {
                seen = true;
                match f(inner)? {
                    Walk::Continue => Walk::Skip,
                    Walk::Skip => Walk::Skip,
                    Walk::Handle(t) => Walk::Handle(t),
                }
            } else {
                Walk::Continue
            }
        } else {
            Walk::Skip
        })
    })
}

/// Call a closure on the node with the specified `id`, and all its ancestors to
/// the specified `root`.
pub fn walk_to_root<T>(
    root: &mut dyn Node,
    id: T,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<()>,
) -> Result<()>
where
    T: Into<NodeId>,
{
    let mut seen = false;
    let uid = id.into();
    postorder(root, &mut |x| -> Result<Walk<()>> {
        Ok(if seen {
            f(x)?;
            Walk::Continue
        } else if x.id() == uid {
            seen = true;
            f(x)?;
            Walk::Skip
        } else {
            Walk::Continue
        })
    })?;
    Ok(())
}

/// Return the node path for a specified node id, relative to the specified
///`root`.
pub fn node_path<T>(id: T, root: &mut dyn Node) -> String
where
    T: Into<NodeId>,
{
    let mut path = Vec::new();
    walk_to_root(root, id, &mut |n| -> Result<()> {
        path.insert(0, n.name().to_string());
        Ok(())
    })
    .unwrap();
    format!("/{}", path.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backend::test::TestRender, geom::Rect, tutils::utils::*, StatefulNode};

    fn run_test(func: impl FnOnce(&mut Canopy, TestRender, TRoot) -> Result<()>) -> Result<()> {
        let (_, tr) = TestRender::create();
        let mut root = TRoot::new();
        let mut c = Canopy::new();
        c.set_root_size(Expanse::new(100, 100), &mut root)?;
        reset_state();
        func(&mut c, tr, root)
    }

    #[test]
    fn tkey() -> Result<()> {
        run_test(|c, tr, mut root| {
            c.set_focus(&mut root);
            root.next_outcome = Some(Outcome::Handle);
            c.key(&mut tr.control(), &mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["r@key->handle"]);
            Ok(())
        })?;

        run_test(|c, tr, mut root| {
            c.set_focus(&mut root.a.a);
            root.a.a.next_outcome = Some(Outcome::Handle);
            c.key(&mut tr.control(), &mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->handle"]);
            Ok(())
        })?;

        run_test(|c, tr, mut root| {
            c.set_focus(&mut root.a.a);
            root.a.next_outcome = Some(Outcome::Handle);
            c.key(&mut tr.control(), &mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->ignore", "ba@key->handle"]);
            Ok(())
        })?;

        run_test(|c, tr, mut root| {
            c.set_focus(&mut root.a.a);
            root.next_outcome = Some(Outcome::Handle);
            c.key(&mut tr.control(), &mut root, 'a')?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_la@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run_test(|c, tr, mut root| {
            c.set_focus(&mut root.a);
            root.a.next_outcome = Some(Outcome::Handle);
            c.key(&mut tr.control(), &mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->handle"]);
            Ok(())
        })?;

        run_test(|c, tr, mut root| {
            c.set_focus(&mut root.a);
            root.next_outcome = Some(Outcome::Handle);
            c.key(&mut tr.control(), &mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->ignore", "r@key->handle"]);
            c.key(&mut tr.control(), &mut root, 'a')?;
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

        run_test(|c, tr, mut root| {
            c.set_focus(&mut root.a.b);
            root.a.next_outcome = Some(Outcome::Ignore);
            root.next_outcome = Some(Outcome::Handle);
            c.key(&mut tr.control(), &mut root, 'a')?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_lb@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run_test(|c, tr, mut root| {
            c.set_focus(&mut root.a.a);
            root.a.a.next_outcome = Some(Outcome::Handle);
            c.key(&mut tr.control(), &mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->handle",]);
            Ok(())
        })?;

        run_test(|c, tr, mut root| {
            c.set_focus(&mut root.a.b);
            root.a.next_outcome = Some(Outcome::Handle);
            c.key(&mut tr.control(), &mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->ignore", "ba@key->handle",]);
            Ok(())
        })?;

        run_test(|c, tr, mut root| {
            c.set_focus(&mut root.a.b);
            root.a.b.next_outcome = Some(Outcome::Handle);
            c.key(&mut tr.control(), &mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->handle",]);
            Ok(())
        })?;

        run_test(|c, tr, mut root| {
            c.set_focus(&mut root.a.b);
            root.a.b.next_outcome = Some(Outcome::Handle);
            root.a.next_outcome = Some(Outcome::Handle);
            c.key(&mut tr.control(), &mut root, 'a')?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->handle",]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tnode_path() -> Result<()> {
        run_test(|_c, _, mut root| {
            println!("HEREA: {}", node_path(root.a.a.id(), &mut root));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tmouse() -> Result<()> {
        run_test(|c, mut tr, mut root| {
            c.set_focus(&mut root);
            root.next_outcome = Some(Outcome::Handle);
            let evt = root.a.a.make_mouse_event()?;
            tr.render(c, &mut root)?;
            c.mouse(&mut tr.control(), &mut root, evt)?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_la@mouse->ignore", "ba@mouse->ignore", "r@mouse->handle"]
            );
            Ok(())
        })?;

        run_test(|c, mut tr, mut root| {
            root.a.a.next_outcome = Some(Outcome::Handle);
            let evt = root.a.a.make_mouse_event()?;
            tr.render(c, &mut root)?;
            c.mouse(&mut tr.control(), &mut root, evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
            Ok(())
        })?;

        run_test(|c, mut tr, mut root| {
            root.a.a.next_outcome = Some(Outcome::Handle);
            let evt = root.a.a.make_mouse_event()?;
            tr.render(c, &mut root)?;
            c.mouse(&mut tr.control(), &mut root, evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
            Ok(())
        })?;

        run_test(|c, mut tr, mut root| {
            root.a.a.next_outcome = Some(Outcome::Handle);
            let evt = root.a.a.make_mouse_event()?;
            tr.render(c, &mut root)?;
            c.mouse(&mut tr.control(), &mut root, evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle",]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tresize() -> Result<()> {
        run_test(|c, mut tr, mut root| {
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
        run_test(|c, mut tr, mut root| {
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
            assert_eq!(tr.buf_text(), vec!["<ba_la>"]);

            c.focus_next(&mut root)?;
            tr.render(c, &mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba_la>", "<ba_lb>"]);

            c.focus_prev(&mut root)?;
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
        run_test(|c, _, mut root| {
            assert_eq!(c.focus_path(&mut root), "/".to_string());
            c.focus_next(&mut root)?;
            assert_eq!(c.focus_path(&mut root), "/r".to_string());
            c.focus_next(&mut root)?;
            assert_eq!(c.focus_path(&mut root), "/r/ba".to_string());
            c.focus_next(&mut root)?;
            assert_eq!(c.focus_path(&mut root), "/r/ba/ba_la".to_string());
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn focus_next() -> Result<()> {
        run_test(|c, _, mut root| {
            assert!(!c.is_focused(&root));
            c.focus_next(&mut root)?;
            assert!(c.is_focused(&root));

            c.focus_next(&mut root)?;
            assert!(c.is_focused(&root.a));
            assert!(c.is_focus_ancestor(&mut root));
            assert!(!c.is_focus_ancestor(&mut root.a));

            c.focus_next(&mut root)?;
            assert!(c.is_focused(&root.a.a));
            assert!(c.is_focus_ancestor(&mut root.a));
            c.focus_next(&mut root)?;
            assert!(c.is_focused(&root.a.b));
            assert!(c.is_focus_ancestor(&mut root.a));
            c.focus_next(&mut root)?;
            assert!(c.is_focused(&root.b));

            c.set_focus(&mut root.b.b);
            assert!(c.is_focus_ancestor(&mut root.b));
            c.focus_next(&mut root)?;
            assert!(c.is_focused(&root));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn focus_prev() -> Result<()> {
        run_test(|c, _, mut root| {
            assert!(!c.is_focused(&root));
            c.focus_prev(&mut root)?;
            assert!(c.is_focused(&root.b.b));

            c.focus_prev(&mut root)?;
            assert!(c.is_focused(&root.b.a));

            c.focus_prev(&mut root)?;
            assert!(c.is_focused(&root.b));

            c.set_focus(&mut root);
            c.focus_prev(&mut root)?;
            assert!(c.is_focused(&root.b.b));

            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tshift_right() -> Result<()> {
        run_test(|c, mut tr, mut root| {
            tr.render(c, &mut root)?;
            c.set_focus(&mut root.a.a);
            c.focus_right(&mut root)?;
            assert!(c.is_focused(&root.b.a));
            c.focus_right(&mut root)?;
            assert!(c.is_focused(&root.b.a));

            c.set_focus(&mut root.a.b);
            c.focus_right(&mut root)?;
            assert!(c.is_focused(&root.b.b));
            c.focus_right(&mut root)?;
            assert!(c.is_focused(&root.b.b));
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tfoci() -> Result<()> {
        run_test(|c, _, mut root| {
            assert_eq!(c.focus_path(&mut root), "/".to_string());

            assert!(!c.is_on_focus_path(&mut root));
            assert!(!c.is_on_focus_path(&mut root.a));

            c.set_focus(&mut root.a.a);
            assert!(c.is_on_focus_path(&mut root));
            assert!(c.is_on_focus_path(&mut root.a));
            assert!(!c.is_on_focus_path(&mut root.b));
            assert_eq!(c.focus_path(&mut root), "/r/ba/ba_la".to_string());

            c.set_focus(&mut root.a);
            assert_eq!(c.focus_path(&mut root), "/r/ba".to_string());

            c.set_focus(&mut root);
            assert_eq!(c.focus_path(&mut root), "/r".to_string());

            c.set_focus(&mut root.b.a);
            assert_eq!(c.focus_path(&mut root), "/r/bb/bb_la".to_string());
            Ok(())
        })?;

        Ok(())
    }
}
