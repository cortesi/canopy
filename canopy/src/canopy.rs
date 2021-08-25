use duplicate::duplicate;
use std::marker::PhantomData;
use std::process::exit;

use crate::geom::{Direction, Rect};
use crate::{
    control::ControlBackend,
    event::{key, mouse, Event},
    geom::{Point, Size},
    node::{postorder, postorder_mut, preorder, Node, Walker},
    Actions, Outcome, Render, Result, StatefulNode, ViewPort,
};

pub(crate) struct SkipWalker {
    pub has_skip: bool,
}

impl SkipWalker {
    pub fn new(skip: bool) -> Self {
        SkipWalker { has_skip: skip }
    }
}

impl Default for SkipWalker {
    fn default() -> Self {
        SkipWalker { has_skip: false }
    }
}

impl Walker for SkipWalker {
    fn skip(&self) -> bool {
        self.has_skip
    }
    fn join(&self, rhs: Self) -> Self {
        SkipWalker {
            has_skip: (self.has_skip | rhs.has_skip),
        }
    }
}

// This is extracted from the event processing functions on Canopy, because the
// code is brittle and complicated, and is identical bar a single method call.
macro_rules! process_event(
    (
        $slf:expr,
        $ctrl:expr,
        $actions:expr,
        $handled:expr,
        $halt:expr,
        $state:expr,
        $node:expr,
        $proc:expr
    ) => {
        {
            let oc = if *$halt {
                Outcome::default()
            } else if *$handled {
                let mut hdl = Outcome::default();
                for a in $actions {
                    hdl = hdl.join($node.handle_event_action($slf, $ctrl, $state, *a)?);
                    if hdl.has_skip() {
                        *$halt = true;
                        break
                    }
                }
                if let Outcome::Handle(h) = &hdl {
                    *$actions = h.actions.clone()
                }
                hdl
            } else {
                let hdl = $proc?;
                if let Outcome::Handle(h) = &hdl {
                    *$actions = h.actions.clone()
                }
                if hdl.has_skip() {
                    *$halt = true;
                }
                if hdl.is_handled() {
                    $slf.taint($node);
                    *$handled = true;
                }
                hdl.clone()
            };
            Ok(oc)

        }
    };
);

/// The core of a Canopy app - this struct keeps track of the render and focus
/// state, and provides functionality for interacting with node trees.
pub struct Canopy<S, A: Actions> {
    // A counter that is incremented every time focus changes. The current focus
    // will have a state `focus_gen` equal to this.
    focus_gen: u64,
    // Stores the focus_gen during the last render. Used to detect if focus has
    // changed.
    last_focus_gen: u64,
    // A counter that is incremented every time we render. All items that
    // require rendering during the current sweep will have a state `render_gen`
    // equal to this.
    render_gen: u64,

    _marker: PhantomData<(S, A)>,
}

impl<'a, S, A: Actions> Canopy<S, A> {
    pub fn new() -> Self {
        Canopy {
            focus_gen: 1,
            render_gen: 1,
            last_focus_gen: 1,
            _marker: PhantomData,
        }
    }

    /// Should the node render in the next sweep? This checks if the node is
    /// tainted, if the focus of the node has changed, or if the node's
    /// Node::should_render method is active.
    pub fn should_render(&self, e: &dyn Node<S, A>) -> bool {
        if e.is_hidden() {
            false
        } else if let Some(r) = e.should_render(self) {
            r
        } else {
            self.is_tainted(e) || self.focus_changed(e)
        }
    }

    /// Is this node render tainted?
    pub fn is_tainted(&self, e: &dyn Node<S, A>) -> bool {
        let s = e.state();
        if self.render_gen == s.render_skip_gen {
            false
        } else {
            // Tainting if render_gen is 0 lets us initialize a nodestate
            // without knowing about the app state
            self.render_gen == s.render_gen || s.render_gen == 0
        }
    }

    /// Has the focus status of this node changed since the last render
    /// sweep?
    pub fn focus_changed(&self, e: &dyn Node<S, A>) -> bool {
        let s = e.state();
        if self.is_focused(e) {
            if s.focus_gen != s.rendered_focus_gen {
                return true;
            }
        } else if s.rendered_focus_gen == self.last_focus_gen {
            return true;
        }
        false
    }

    /// Focus the specified node.
    pub fn set_focus(&mut self, e: &mut dyn Node<S, A>) {
        self.focus_gen += 1;
        e.state_mut().focus_gen = self.focus_gen;
    }

    /// Move focus in a specified direction within the subtree.
    pub fn focus_dir(&mut self, e: &mut dyn Node<S, A>, dir: Direction) -> Result<Outcome<A>> {
        let mut seen = false;
        if let Some(start) = self.get_focus_area(e) {
            start.search(dir, &mut |p| -> Result<bool> {
                if !e.vp().screen_rect().contains_point(p) {
                    return Ok(true);
                }
                locate(e, p, &mut |x| {
                    if !seen {
                        if x.focus(self)?.is_handled() {
                            seen = true;
                        };
                    };
                    Ok(())
                })?;
                Ok(seen)
            })?
        }
        Ok(Outcome::handle())
    }

    /// Move focus to the right of the currently focused node within the subtree.
    pub fn focus_right(&mut self, e: &mut dyn Node<S, A>) -> Result<Outcome<A>> {
        self.focus_dir(e, Direction::Right)
    }

    /// Move focus to the left of the currently focused node within the subtree.
    pub fn focus_left(&mut self, e: &mut dyn Node<S, A>) -> Result<Outcome<A>> {
        self.focus_dir(e, Direction::Left)
    }

    /// Move focus upward of the currently focused node within the subtree.
    pub fn focus_up(&mut self, e: &mut dyn Node<S, A>) -> Result<Outcome<A>> {
        self.focus_dir(e, Direction::Up)
    }

    /// Move focus downward of the currently focused node within the subtree.
    pub fn focus_down(&mut self, e: &mut dyn Node<S, A>) -> Result<Outcome<A>> {
        self.focus_dir(e, Direction::Down)
    }

    /// Focus the first node that accepts focus in the pre-order traversal of
    /// the subtree.
    pub fn focus_first(&mut self, e: &mut dyn Node<S, A>) -> Result<Outcome<A>> {
        let mut focus_set = false;
        preorder(e, &mut |x| -> Result<SkipWalker> {
            Ok(if !focus_set && x.focus(self)?.is_handled() {
                focus_set = true;
                SkipWalker::new(true)
            } else {
                SkipWalker::new(false)
            })
        })?;
        Ok(Outcome::handle())
    }

    /// Does the node have terminal focus?
    pub fn is_focused(&self, e: &dyn Node<S, A>) -> bool {
        let s = e.state();
        self.focus_gen == s.focus_gen
    }

    /// A node is on the focus path if it or any of its descendants have focus.
    pub fn on_focus_path(&self, e: &dyn Node<S, A>) -> bool {
        let mut onpath = false;
        self.focus_path(e, &mut |_| -> Result<()> {
            onpath = true;
            Ok(())
        })
        .unwrap();
        onpath
    }

    /// A node is on the focus path if it does not have focus itself, but some
    /// node below it does.
    pub fn is_focus_ancestor(&self, e: &dyn Node<S, A>) -> bool {
        if self.is_focused(e) {
            false
        } else {
            self.on_focus_path(e)
        }
    }

    /// Focus the next node in the pre-order traversal of a node. If no node
    /// with focus is found, we focus the first node we can find instead.
    pub fn focus_next(&mut self, e: &mut dyn Node<S, A>) -> Result<Outcome<A>> {
        let mut focus_set = false;
        let mut focus_seen = false;
        preorder(e, &mut |x| -> Result<()> {
            if !focus_set {
                if focus_seen {
                    if x.focus(self)?.is_handled() {
                        focus_set = true;
                    }
                } else if self.is_focused(x) {
                    focus_seen = true;
                }
            }
            Ok(())
        })?;
        if !focus_set {
            self.focus_first(e)
        } else {
            Ok(Outcome::handle())
        }
    }

    /// Focus the previous node in the pre-order traversal of a node. If no
    /// node with focus is found, we focus the first node we can find instead.
    pub fn focus_prev(&mut self, e: &mut dyn Node<S, A>) -> Result<Outcome<A>> {
        let current = self.focus_gen;
        let mut focus_seen = false;
        let mut first = true;
        preorder(e, &mut |x| -> Result<()> {
            // We skip the first node in the traversal
            if first {
                first = false
            } else if !focus_seen {
                if x.state().focus_gen == current {
                    focus_seen = true;
                } else {
                    x.focus(self)?.is_handled();
                }
            }
            Ok(())
        })?;
        Ok(Outcome::handle())
    }

    /// Find the area of the current terminal focus node.
    pub fn get_focus_area(&self, e: &dyn Node<S, A>) -> Option<Rect> {
        let mut ret = None;
        self.focus_path(e, &mut |x| -> Result<()> {
            if ret == None {
                ret = Some(x.vp().screen_rect());
            }
            Ok(())
        })
        .unwrap();
        ret
    }

    /// Call a closure on every node in the current focus path, from the focused
    /// leaf to the root.
    pub fn focus_path<R: Walker + Default>(
        &self,
        e: &dyn Node<S, A>,
        f: &mut dyn FnMut(&dyn Node<S, A>) -> Result<R>,
    ) -> Result<R> {
        focus_path(self.focus_gen, e, f)
    }

    /// Call a closure mutably on every node in the current focus path, from the
    /// focused leaf to the root.
    pub fn focus_path_mut<R: Walker + Default>(
        &self,
        e: &mut dyn Node<S, A>,
        f: &mut dyn FnMut(&mut dyn Node<S, A>) -> Result<R>,
    ) -> Result<R> {
        focus_path_mut(self.focus_gen, e, f)
    }

    /// Returns the focal depth of the specified node. If the node is not part
    /// of the focus chain, the depth is 0. If the node is a leaf focus, the
    /// depth is 1.
    pub fn focus_depth(&self, e: &dyn Node<S, A>) -> usize {
        let mut total = 0;
        self.focus_path(e, &mut |_| -> Result<()> {
            total += 1;
            Ok(())
        })
        .unwrap();
        total
    }

    /// Pre-render sweep of the tree.
    pub(crate) fn pre_render(&mut self, r: &mut Render, e: &mut dyn Node<S, A>) -> Result<()> {
        let mut seen = false;
        self.focus_path(e, &mut |_| -> Result<()> {
            seen = true;
            Ok(())
        })?;
        if !seen {
            self.focus_first(e)?;
        }
        // The cursor is disabled before every render sweep, otherwise we would
        // see it visibly on screen during redraws.
        r.hide_cursor()?;
        Ok(())
    }

    /// Post-render sweep of the tree.
    pub(crate) fn post_render(&mut self, r: &mut Render, e: &dyn Node<S, A>) -> Result<()> {
        let mut seen = false;
        focus_path(self.focus_gen, e, &mut |n| -> Result<()> {
            if !seen {
                if let Some(c) = n.cursor() {
                    r.show_cursor("cursor", c)?;
                    seen = true;
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    /// Mark a tree of nodes for render.
    pub fn taint_tree(&self, e: &mut dyn Node<S, A>) -> Result<()> {
        postorder_mut(e, &mut |x| -> Result<()> {
            let r = x.state_mut();
            r.render_gen = self.render_gen;
            Ok(())
        })?;
        Ok(())
    }

    /// Mark a single node for render.
    pub fn taint(&self, e: &mut dyn Node<S, A>) {
        let r = e.state_mut();
        r.render_gen = self.render_gen;
    }

    /// Mark that a node should skip the next render sweep.
    pub fn skip_taint(&self, e: &mut dyn Node<S, A>) {
        let r = e.state_mut();
        r.render_skip_gen = self.render_gen;
    }

    fn render_traversal(&mut self, r: &mut Render, e: &mut dyn Node<S, A>) -> Result<()> {
        if !e.is_hidden() {
            r.push();
            if self.should_render(e) {
                if self.is_focused(e) {
                    let s = &mut e.state_mut();
                    s.rendered_focus_gen = self.focus_gen
                }
                r.viewport = e.state().viewport;
                e.render(self, r, e.state().viewport)?;
            }
            // This is a new node - we don't want it perpetually stuck in
            // render, so we need to update its render_gen.
            if e.state().render_gen == 0 {
                e.state_mut().render_gen = self.render_gen;
            }
            e.children_mut(&mut |x| self.render_traversal(r, x))?;
            r.pop();
        }
        Ok(())
    }

    /// Render a tree of nodes. If force is true, all visible nodes are
    /// rendered, otherwise we check the taint state. Hidden nodes and their
    /// children are ignored.
    pub fn render(&mut self, r: &mut Render, e: &mut dyn Node<S, A>) -> Result<()> {
        r.reset()?;
        self.render_traversal(r, e)?;
        self.render_gen += 1;
        self.last_focus_gen = self.focus_gen;
        Ok(())
    }

    /// Propagate a mouse event through the node under the event and all its
    /// ancestors. Events are handled only once, and then ignored.
    pub fn mouse(
        &mut self,
        ctrl: &mut dyn ControlBackend,
        root: &mut dyn Node<S, A>,
        s: &mut S,
        m: mouse::Mouse,
    ) -> Result<Outcome<A>> {
        let mut handled = false;
        let mut halt = false;
        let mut actions: Vec<A> = vec![];
        locate(root, m.loc, &mut |x| {
            process_event!(
                self,
                ctrl,
                &mut actions,
                &mut handled,
                &mut halt,
                s,
                x,
                x.handle_mouse(
                    self,
                    ctrl,
                    s,
                    mouse::Mouse {
                        action: m.action,
                        button: m.button,
                        modifiers: m.modifiers,
                        loc: x.vp().screen_rect().rebase_point(m.loc)?,
                    },
                )
            )
        })
    }

    /// Propagate a key event through the focus and all its ancestors. Keys
    /// handled only once, and then ignored.
    pub fn key(
        &mut self,
        ctrl: &mut dyn ControlBackend,
        root: &mut dyn Node<S, A>,
        s: &mut S,
        k: key::Key,
    ) -> Result<Outcome<A>> {
        let mut handled = false;
        let mut halt = false;
        let mut actions: Vec<A> = vec![];
        focus_path_mut(self.focus_gen, root, &mut move |x| -> Result<Outcome<A>> {
            process_event!(
                self,
                ctrl,
                &mut actions,
                &mut handled,
                &mut halt,
                s,
                x,
                x.handle_key(self, ctrl, s, k)
            )
        })
    }

    /// Set the size on the root node, and taint the tree.
    pub fn set_root_size<N>(&mut self, size: Size, n: &mut N) -> Result<()>
    where
        N: Node<S, A> + StatefulNode,
    {
        let fit = n.fit(self, size)?;
        let vp = ViewPort::new(fit, fit, Point::default())?;
        n.set_viewport(vp);
        self.taint_tree(n)?;
        Ok(())
    }

    /// Broadcast an action through the tree. All nodes get the event, even if
    /// they are hidden. Enabling skip on the returned Outcome prevents
    /// propagation to a node's children. Nodes that handle an action are
    /// automatically tainted.
    pub fn broadcast(
        &mut self,
        ctrl: &mut dyn ControlBackend,
        root: &mut dyn Node<S, A>,
        s: &mut S,
        t: A,
    ) -> Result<Outcome<A>> {
        preorder(root, &mut |x| -> Result<Outcome<A>> {
            let o = x.handle_broadcast(self, ctrl, s, t)?;
            if o.is_handled() {
                self.taint(x);
            }
            Ok(o)
        })
    }

    /// Propagate an event through the tree.
    pub fn event<N>(
        &mut self,
        ctrl: &mut dyn ControlBackend,
        root: &mut N,
        s: &mut S,
        e: Event<A>,
    ) -> Result<Outcome<A>>
    where
        N: Node<S, A>,
    {
        match e {
            Event::Key(k) => self.key(ctrl, root, s, k),
            Event::Mouse(m) => self.mouse(ctrl, root, s, m),
            Event::Resize(s) => {
                self.set_root_size(s, root)?;
                Ok(Outcome::handle())
            }
            Event::Action(t) => self.broadcast(ctrl, root, s, t),
        }
    }

    /// Clean up render loop and exit the process.
    pub fn exit(&mut self, c: &mut dyn ControlBackend, code: i32) -> ! {
        let _ = c.exit();
        exit(code)
    }
}

/// Calls a closure on the currently focused node and all its parents to the
/// root.
#[duplicate(
        method              reference(type)    traversal;
        [focus_path]        [& type]           [postorder];
        [focus_path_mut]    [&mut type]        [postorder_mut];
    )]
fn method<S, A: Actions, R: Walker + Default>(
    focus_gen: u64,
    e: reference([dyn Node<S, A>]),
    f: &mut dyn FnMut(reference([dyn Node<S, A>])) -> Result<R>,
) -> Result<R> {
    let mut focus_seen = false;
    let mut ret = R::default();
    traversal(e, &mut |x| -> Result<SkipWalker> {
        Ok(if focus_seen {
            ret = ret.join(f(x)?);
            SkipWalker::new(false)
        } else if x.is_hidden() {
            // Hidden nodes don't hold focus
            SkipWalker::new(false)
        } else if x.state().focus_gen == focus_gen {
            focus_seen = true;
            ret = ret.join(f(x)?);
            SkipWalker::new(true)
        } else {
            SkipWalker::new(false)
        })
    })?;
    Ok(ret)
}

// Calls a closure on the leaf node under (x, y), then all its parents to the
// root.
pub fn locate<S, A: Actions, R: Walker + Default>(
    e: &mut dyn Node<S, A>,
    p: impl Into<Point>,
    f: &mut dyn FnMut(&mut dyn Node<S, A>) -> Result<R>,
) -> Result<R> {
    let mut seen = false;
    let mut ret = R::default();
    let p = p.into();
    postorder_mut(e, &mut |inner| -> Result<SkipWalker> {
        Ok(if seen {
            ret = ret.join(f(inner)?);
            SkipWalker::new(false)
        } else if !inner.is_hidden() {
            let a = inner.vp().screen_rect();
            if a.contains_point(p) {
                seen = true;
                ret = ret.join(f(inner)?);
                SkipWalker::new(true)
            } else {
                SkipWalker::new(false)
            }
        } else {
            SkipWalker::new(true)
        })
    })?;
    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::test::{TestBuf, TestRender},
        geom::Rect,
        outcome::{Handle, Ignore},
        tutils::utils::*,
    };
    use std::sync::{Arc, Mutex};

    pub fn focvec(app: &mut Canopy<State, TActions>, root: &mut TRoot) -> Result<Vec<String>> {
        let mut v = vec![];
        focus_path_mut(app.focus_gen, root, &mut |x| -> Result<()> {
            let n = x.name().unwrap();
            v.push(n);
            Ok(())
        })?;
        Ok(v)
    }

    fn run_test(
        func: impl FnOnce(
            Arc<Mutex<TestBuf>>,
            Canopy<State, TActions>,
            Render,
            &mut dyn ControlBackend,
            TRoot,
            State,
        ) -> Result<()>,
    ) -> Result<()> {
        let (buf, mut tr) = TestRender::create();
        let (mut app, r, mut c) = tcanopy(&mut tr);
        let mut root = TRoot::new();
        app.set_root_size(Size::new(100, 100), &mut root)?;
        func(buf, app, r, &mut c, root, State::new())
    }

    #[test]
    fn tfocus_next() -> Result<()> {
        run_test(|_, mut app, _, _, mut root, _| {
            assert!(!app.is_focused(&root));
            app.focus_next(&mut root)?;
            assert!(app.is_focused(&root));

            app.focus_next(&mut root)?;
            assert!(app.is_focused(&root.a));
            assert!(app.is_focus_ancestor(&mut root));
            assert!(!app.is_focus_ancestor(&mut root.a));

            app.focus_next(&mut root)?;
            assert!(app.is_focused(&root.a.a));
            assert!(app.is_focus_ancestor(&mut root.a));
            app.focus_next(&mut root)?;
            assert!(app.is_focused(&root.a.b));
            assert!(app.is_focus_ancestor(&mut root.a));
            app.focus_next(&mut root)?;
            assert!(app.is_focused(&root.b));

            app.set_focus(&mut root.b.b);
            assert!(app.is_focus_ancestor(&mut root.b));
            app.focus_next(&mut root)?;
            assert!(app.is_focused(&root));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tfocus_prev() -> Result<()> {
        run_test(|_, mut app, _, _, mut root, _| {
            assert!(!app.is_focused(&root));
            app.focus_prev(&mut root)?;
            assert!(app.is_focused(&root.b.b));

            app.focus_prev(&mut root)?;
            assert!(app.is_focused(&root.b.a));

            app.focus_prev(&mut root)?;
            assert!(app.is_focused(&root.b));

            app.set_focus(&mut root);
            app.focus_prev(&mut root)?;
            assert!(app.is_focused(&root.b.b));

            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tfoci() -> Result<()> {
        run_test(|_, mut app, _, _, mut root, _| {
            assert_eq!(focvec(&mut app, &mut root)?.len(), 0);

            assert!(!app.on_focus_path(&mut root));
            assert!(!app.on_focus_path(&mut root.a));

            app.set_focus(&mut root.a.a);
            assert!(app.on_focus_path(&mut root));
            assert!(app.on_focus_path(&mut root.a));
            assert!(!app.on_focus_path(&mut root.b));

            assert_eq!(focvec(&mut app, &mut root)?, vec!["ba:la", "ba", "r"]);

            app.set_focus(&mut root.a);
            assert_eq!(focvec(&mut app, &mut root)?, vec!["ba", "r"]);

            app.set_focus(&mut root);
            assert_eq!(focvec(&mut app, &mut root)?, vec!["r"]);

            app.set_focus(&mut root.b.a);
            assert_eq!(focvec(&mut app, &mut root)?, vec!["bb:la", "bb", "r"]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tfocus_right() -> Result<()> {
        run_test(|_, mut app, mut r, _, mut root, _| {
            app.render(&mut r, &mut root)?;
            app.set_focus(&mut root.a.a);
            app.focus_right(&mut root)?;
            assert!(app.is_focused(&root.b.a));
            app.focus_right(&mut root)?;
            assert!(app.is_focused(&root.b.a));

            app.set_focus(&mut root.a.b);
            app.focus_right(&mut root)?;
            assert!(app.is_focused(&root.b.b));
            app.focus_right(&mut root)?;
            assert!(app.is_focused(&root.b.b));
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn taction() -> Result<()> {
        run_test(|_, mut app, _, c, mut root, mut s| {
            app.set_focus(&mut root);
            root.next_outcome = Some(Outcome::handle_and_continue());
            assert_eq!(
                app.broadcast(c, &mut root, &mut s, TActions::One)?,
                Outcome::handle_and_continue()
            );
            assert_eq!(
                s.path,
                vec![
                    "r@broadcast:one->handle",
                    "ba@broadcast:one->ignore",
                    "ba:la@broadcast:one->ignore",
                    "ba:lb@broadcast:one->ignore",
                    "bb@broadcast:one->ignore",
                    "bb:la@broadcast:one->ignore",
                    "bb:lb@broadcast:one->ignore"
                ]
            );
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root);
            root.a.next_outcome = Some(Outcome::Ignore(Ignore::default().with_skip()));
            assert_eq!(
                app.broadcast(ctrl, &mut root, &mut s, TActions::One)?,
                Outcome::ignore_and_skip(),
            );
            assert_eq!(
                s.path,
                vec![
                    "r@broadcast:one->ignore",
                    "ba@broadcast:one->ignore",
                    "bb@broadcast:one->ignore",
                    "bb:la@broadcast:one->ignore",
                    "bb:lb@broadcast:one->ignore"
                ]
            );
            Ok(())
        })?;

        Ok(())
    }

    // These tests double as tests for the process_event macro - no need to
    // duplicate the details in the mouse specific tests.
    #[test]
    fn tkey() -> Result<()> {
        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root);
            root.next_outcome = Some(Outcome::handle());
            assert!(app.key(ctrl, &mut root, &mut s, K_ANY)?.is_handled());
            assert_eq!(s.path, vec!["r@key->handle"]);
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root.a.a);
            root.a.a.next_outcome = Some(Outcome::handle());
            assert!(app.key(ctrl, &mut root, &mut s, K_ANY)?.is_handled());
            assert_eq!(s.path, vec!["ba:la@key->handle"]);
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root.a.a);
            root.a.next_outcome = Some(Outcome::handle());
            assert!(app.key(ctrl, &mut root, &mut s, K_ANY)?.is_handled());
            assert_eq!(s.path, vec!["ba:la@key->ignore", "ba@key->handle"]);
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root.a.a);
            root.next_outcome = Some(Outcome::handle());
            assert!(app.key(ctrl, &mut root, &mut s, K_ANY)?.is_handled());
            assert_eq!(
                s.path,
                vec!["ba:la@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root.a);
            root.a.next_outcome = Some(Outcome::handle());
            assert!(app.key(ctrl, &mut root, &mut s, K_ANY)?.is_handled());
            assert_eq!(s.path, vec!["ba@key->handle"]);
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root.a);
            root.next_outcome = Some(Outcome::handle());
            assert!(app.key(ctrl, &mut root, &mut s, K_ANY)?.is_handled());
            assert_eq!(s.path, vec!["ba@key->ignore", "r@key->handle"]);
            assert_eq!(app.key(ctrl, &mut root, &mut s, K_ANY)?, Outcome::ignore());
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root.a.b);
            root.a.next_outcome = Some(Outcome::Ignore(Ignore::default().with_skip()));
            root.next_outcome = Some(Outcome::handle());
            app.key(ctrl, &mut root, &mut s, K_ANY)?;
            assert_eq!(s.path, vec!["ba:lb@key->ignore", "ba@key->ignore"]);
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root.a.a);
            root.a.a.next_outcome = Some(Outcome::handle_with_action(TActions::One));
            app.key(ctrl, &mut root, &mut s, K_ANY)?;
            assert_eq!(
                s.path,
                vec![
                    "ba:la@key->handle",
                    "ba@eaction:one->ignore",
                    "r@eaction:one->ignore",
                ]
            );
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root.a.b);
            root.a.next_outcome = Some(Outcome::handle_with_action(TActions::One));
            app.key(ctrl, &mut root, &mut s, K_ANY)?;
            assert_eq!(
                s.path,
                vec![
                    "ba:lb@key->ignore",
                    "ba@key->handle",
                    "r@eaction:one->ignore",
                ]
            );
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root.a.b);
            root.a.b.next_outcome = Some(Outcome::Handle(
                Handle::default()
                    .with_action(TActions::One)
                    .with_action(TActions::Two),
            ));
            app.key(ctrl, &mut root, &mut s, K_ANY)?;
            assert_eq!(s.path, vec!["ba:lb@key->handle",]);
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root.a.b);
            root.a.b.next_outcome = Some(Outcome::handle_with_action(TActions::One));
            root.a.next_outcome = Some(Outcome::handle_with_action(TActions::Two));
            app.key(ctrl, &mut root, &mut s, K_ANY)?;
            assert_eq!(
                s.path,
                vec![
                    "ba:lb@key->handle",
                    "ba@eaction:one->handle",
                    "r@eaction:two->ignore",
                ]
            );
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            app.set_focus(&mut root.a.b);
            root.a.b.next_outcome = Some(Outcome::handle_with_action(TActions::One));
            root.a.next_outcome = Some(Outcome::ignore_and_skip());
            app.key(ctrl, &mut root, &mut s, K_ANY)?;
            assert_eq!(s.path, vec!["ba:lb@key->handle", "ba@eaction:one->ignore",]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tmouse() -> Result<()> {
        run_test(|_, mut app, mut r, ctrl, mut root, mut s| {
            app.set_focus(&mut root);
            root.next_outcome = Some(Outcome::handle());
            let evt = root.a.a.make_mouse_event()?;
            app.render(&mut r, &mut root)?;
            assert!(app.mouse(ctrl, &mut root, &mut s, evt)?.is_handled());
            assert_eq!(
                s.path,
                vec!["ba:la@mouse->ignore", "ba@mouse->ignore", "r@mouse->handle"]
            );
            Ok(())
        })?;

        run_test(|_, mut app, mut r, ctrl, mut root, mut s| {
            root.a.a.next_outcome = Some(Outcome::handle());
            let evt = root.a.a.make_mouse_event()?;
            app.render(&mut r, &mut root)?;
            assert!(app.mouse(ctrl, &mut root, &mut s, evt)?.is_handled());
            assert_eq!(s.path, vec!["ba:la@mouse->handle"]);
            Ok(())
        })?;

        run_test(|_, mut app, mut r, ctrl, mut root, mut s| {
            root.a.a.next_outcome = Some(Outcome::handle());
            let evt = root.a.a.make_mouse_event()?;
            app.render(&mut r, &mut root)?;
            assert!(app.mouse(ctrl, &mut root, &mut s, evt)?.is_handled());
            assert_eq!(s.path, vec!["ba:la@mouse->handle"]);
            Ok(())
        })?;

        run_test(|_, mut app, mut r, ctrl, mut root, mut s| {
            root.a.a.next_outcome = Some(Outcome::handle_with_action(TActions::One));
            let evt = root.a.a.make_mouse_event()?;
            app.render(&mut r, &mut root)?;
            assert!(app.mouse(ctrl, &mut root, &mut s, evt)?.is_handled());
            assert_eq!(
                s.path,
                vec![
                    "ba:la@mouse->handle",
                    "ba@eaction:one->ignore",
                    "r@eaction:one->ignore",
                ]
            );
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tresize() -> Result<()> {
        run_test(|_, mut app, mut r, _, mut root, _| {
            let size = 100;
            assert_eq!(root.vp().screen_rect(), Rect::new(0, 0, size, size));
            app.render(&mut r, &mut root)?;
            assert_eq!(root.a.vp().screen_rect(), Rect::new(0, 0, size / 2, size));
            assert_eq!(
                root.b.vp().screen_rect(),
                Rect::new(size / 2, 0, size / 2, size)
            );

            app.set_root_size(Size::new(50, 50), &mut root)?;
            app.render(&mut r, &mut root)?;
            assert_eq!(root.b.vp().screen_rect(), Rect::new(25, 0, 25, 50));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn trender() -> Result<()> {
        run_test(|buf, mut app, mut r, _, mut root, _| {
            app.render(&mut r, &mut root)?;
            assert_eq!(
                buf.lock()?.text,
                vec!["<r>", "<ba>", "<ba:la>", "<ba:lb>", "<bb>", "<bb:la>", "<bb:lb>"]
            );

            app.render(&mut r, &mut root)?;
            assert!(buf.lock()?.is_empty());

            app.taint(&mut root.a);
            app.render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<ba>"]);

            app.taint(&mut root.a.b);
            app.render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<ba:lb>"]);

            app.taint_tree(&mut root.a)?;
            app.render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<ba>", "<ba:la>", "<ba:lb>"]);

            app.render(&mut r, &mut root)?;
            assert!(buf.lock()?.text.is_empty());

            app.set_focus(&mut root.a.a);
            app.render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<ba:la>"]);

            app.focus_next(&mut root)?;
            app.render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<ba:la>", "<ba:lb>"]);

            app.focus_prev(&mut root)?;
            app.render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<ba:la>", "<ba:lb>"]);

            app.render(&mut r, &mut root)?;
            assert!(buf.lock()?.text.is_empty());

            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn ttaintskip() -> Result<()> {
        run_test(|buf, mut app, mut r, ctrl, mut root, _| {
            app.render(&mut r, &mut root)?;
            let mut s = State::new();
            app.set_focus(&mut root);
            root.next_outcome = Some(Outcome::handle_and_continue());
            root.a.a.next_outcome = Some(Outcome::handle());
            root.b.b.next_outcome = Some(Outcome::handle());
            app.skip_taint(&mut root.a.a);
            assert!(app
                .broadcast(ctrl, &mut root, &mut s, TActions::Two)?
                .is_handled());
            assert_eq!(
                s.path,
                vec![
                    "r@broadcast:two->handle",
                    "ba@broadcast:two->ignore",
                    "ba:la@broadcast:two->handle",
                    "ba:lb@broadcast:two->ignore",
                    "bb@broadcast:two->ignore",
                    "bb:la@broadcast:two->ignore",
                    "bb:lb@broadcast:two->handle"
                ]
            );
            app.render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<r>", "<bb:lb>"]);
            Ok(())
        })?;

        Ok(())
    }
}
