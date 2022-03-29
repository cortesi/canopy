use duplicate::duplicate_item;
use std::marker::PhantomData;
use std::process::exit;

use crate::geom::{Direction, Rect};
use crate::{
    control::BackendControl,
    event::{key, mouse, Event},
    geom::{Point, Size},
    global::STATE,
    node::{postorder, postorder_mut, preorder, Node, Walker},
    Actions, Outcome, Render, Result, StatefulNode, ViewPort,
};

#[derive(Default)]
pub(crate) struct SkipWalker {
    pub has_skip: bool,
}

impl SkipWalker {
    pub fn new(skip: bool) -> Self {
        SkipWalker { has_skip: skip }
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
#[derive(Default)]
pub struct Canopy<S, A: Actions> {
    _marker: PhantomData<(S, A)>,
}

impl<'a, S, A: Actions> Canopy<S, A> {
    pub fn new() -> Self {
        Canopy {
            _marker: PhantomData,
        }
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
                    if !seen && x.handle_focus(self)?.is_handled() {
                        seen = true;
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
            Ok(if !focus_set && x.handle_focus(self)?.is_handled() {
                focus_set = true;
                SkipWalker::new(true)
            } else {
                SkipWalker::new(false)
            })
        })?;
        Ok(Outcome::handle())
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
        if e.is_focused() {
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
                    if x.handle_focus(self)?.is_handled() {
                        focus_set = true;
                    }
                } else if x.is_focused() {
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
        let current = STATE.with(|global_state| -> u64 { global_state.borrow().focus_gen });
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
                    x.handle_focus(self)?.is_handled();
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
        let focus_gen = STATE.with(|global_state| -> u64 { global_state.borrow().focus_gen });
        focus_path(focus_gen, e, f)
    }

    /// Call a closure mutably on every node in the current focus path, from the
    /// focused leaf to the root.
    pub fn focus_path_mut<R: Walker + Default>(
        &self,
        e: &mut dyn Node<S, A>,
        f: &mut dyn FnMut(&mut dyn Node<S, A>) -> Result<R>,
    ) -> Result<R> {
        let focus_gen = STATE.with(|global_state| -> u64 { global_state.borrow().focus_gen });
        focus_path_mut(focus_gen, e, f)
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
        let focus_gen = STATE.with(|global_state| -> u64 { global_state.borrow().focus_gen });
        focus_path(focus_gen, e, &mut |n| -> Result<()> {
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
            r.render_gen = STATE.with(|global_state| -> u64 { global_state.borrow().render_gen });
            Ok(())
        })?;
        Ok(())
    }

    /// Mark a single node for render.
    pub fn taint(&self, e: &mut dyn Node<S, A>) {
        let r = e.state_mut();
        r.render_gen = STATE.with(|global_state| -> u64 { global_state.borrow().render_gen });
    }

    /// Mark that a node should skip the next render sweep.
    pub fn skip_taint(&self, e: &mut dyn Node<S, A>) {
        let r = e.state_mut();
        r.render_skip_gen = STATE.with(|global_state| -> u64 { global_state.borrow().render_gen });
    }

    fn render_traversal(&mut self, r: &mut Render, e: &mut dyn Node<S, A>) -> Result<()> {
        if !e.is_hidden() {
            r.push();
            if e.should_render() {
                if e.is_focused() {
                    let s = &mut e.state_mut();
                    s.rendered_focus_gen =
                        STATE.with(|global_state| -> u64 { global_state.borrow().focus_gen });
                }
                r.viewport = e.state().viewport;
                e.render(self, r, e.state().viewport)?;
            }
            // This is a new node - we don't want it perpetually stuck in
            // render, so we need to update its render_gen.
            if e.state().render_gen == 0 {
                e.state_mut().render_gen =
                    STATE.with(|global_state| -> u64 { global_state.borrow().render_gen });
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
        STATE.with(|global_state| {
            let mut gs = global_state.borrow_mut();
            gs.render_gen += 1;
            gs.last_focus_gen = gs.focus_gen;
        });
        Ok(())
    }

    /// Propagate a mouse event through the node under the event and all its
    /// ancestors. Events are handled only once, and then ignored.
    pub fn mouse(
        &mut self,
        ctrl: &mut dyn BackendControl,
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
        ctrl: &mut dyn BackendControl,
        root: &mut dyn Node<S, A>,
        s: &mut S,
        k: key::Key,
    ) -> Result<Outcome<A>> {
        let mut handled = false;
        let mut halt = false;
        let mut actions: Vec<A> = vec![];
        let focus_gen = STATE.with(|global_state| -> u64 { global_state.borrow().focus_gen });
        focus_path_mut(focus_gen, root, &mut move |x| -> Result<Outcome<A>> {
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
        ctrl: &mut dyn BackendControl,
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
        ctrl: &mut dyn BackendControl,
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
    pub fn exit(&mut self, c: &mut dyn BackendControl, code: i32) -> ! {
        let _ = c.exit();
        exit(code)
    }
}

/// Calls a closure on the currently focused node and all its parents to the
/// root.
#[duplicate_item(
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

/// Calls a closure on the leaf node under (x, y), then all its parents to the
/// root.
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

    pub fn focvec(root: &mut TRoot) -> Result<Vec<String>> {
        let mut v = vec![];
        let focus_gen = STATE.with(|global_state| -> u64 { global_state.borrow().focus_gen });
        focus_path_mut(focus_gen, root, &mut |x| -> Result<()> {
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
            &mut dyn BackendControl,
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
            assert!(!root.is_focused());
            app.focus_next(&mut root)?;
            assert!(root.is_focused());

            app.focus_next(&mut root)?;
            assert!(root.a.is_focused());
            assert!(app.is_focus_ancestor(&root));
            assert!(!app.is_focus_ancestor(&root.a));

            app.focus_next(&mut root)?;
            assert!(root.a.a.is_focused());
            assert!(app.is_focus_ancestor(&root.a));
            app.focus_next(&mut root)?;
            assert!(root.a.b.is_focused());
            assert!(app.is_focus_ancestor(&root.a));
            app.focus_next(&mut root)?;
            assert!(root.b.is_focused());

            root.b.b.set_focus();
            assert!(app.is_focus_ancestor(&root.b));
            app.focus_next(&mut root)?;
            assert!(root.is_focused());
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tfocus_prev() -> Result<()> {
        run_test(|_, mut app, _, _, mut root, _| {
            assert!(!root.is_focused());
            app.focus_prev(&mut root)?;
            assert!(root.b.b.is_focused());

            app.focus_prev(&mut root)?;
            assert!(root.b.a.is_focused());

            app.focus_prev(&mut root)?;
            assert!(root.b.is_focused());

            root.set_focus();
            app.focus_prev(&mut root)?;
            assert!(root.b.b.is_focused());

            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tfoci() -> Result<()> {
        run_test(|_, app, _, _, mut root, _| {
            assert_eq!(focvec(&mut root)?.len(), 0);

            assert!(!app.on_focus_path(&root));
            assert!(!app.on_focus_path(&root.a));

            root.a.a.set_focus();
            assert!(app.on_focus_path(&root));
            assert!(app.on_focus_path(&root.a));
            assert!(!app.on_focus_path(&root.b));

            assert_eq!(focvec(&mut root)?, vec!["ba:la", "ba", "r"]);

            root.a.set_focus();
            assert_eq!(focvec(&mut root)?, vec!["ba", "r"]);

            root.set_focus();
            assert_eq!(focvec(&mut root)?, vec!["r"]);

            root.b.a.set_focus();
            assert_eq!(focvec(&mut root)?, vec!["bb:la", "bb", "r"]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tfocus_right() -> Result<()> {
        run_test(|_, mut app, mut r, _, mut root, _| {
            app.render(&mut r, &mut root)?;
            root.a.a.set_focus();
            app.focus_right(&mut root)?;
            assert!(root.b.a.is_focused());
            app.focus_right(&mut root)?;
            assert!(root.b.a.is_focused());

            root.a.b.set_focus();
            app.focus_right(&mut root)?;
            assert!(root.b.b.is_focused());
            app.focus_right(&mut root)?;
            assert!(root.b.b.is_focused());
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn taction() -> Result<()> {
        run_test(|_, mut app, _, c, mut root, mut s| {
            root.set_focus();
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
            root.set_focus();
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
            root.set_focus();
            root.next_outcome = Some(Outcome::handle());
            assert!(app.key(ctrl, &mut root, &mut s, K_ANY)?.is_handled());
            assert_eq!(s.path, vec!["r@key->handle"]);
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            root.a.a.set_focus();
            root.a.a.next_outcome = Some(Outcome::handle());
            assert!(app.key(ctrl, &mut root, &mut s, K_ANY)?.is_handled());
            assert_eq!(s.path, vec!["ba:la@key->handle"]);
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            root.a.a.set_focus();
            root.a.next_outcome = Some(Outcome::handle());
            assert!(app.key(ctrl, &mut root, &mut s, K_ANY)?.is_handled());
            assert_eq!(s.path, vec!["ba:la@key->ignore", "ba@key->handle"]);
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            root.a.a.set_focus();
            root.next_outcome = Some(Outcome::handle());
            assert!(app.key(ctrl, &mut root, &mut s, K_ANY)?.is_handled());
            assert_eq!(
                s.path,
                vec!["ba:la@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            root.a.set_focus();
            root.a.next_outcome = Some(Outcome::handle());
            assert!(app.key(ctrl, &mut root, &mut s, K_ANY)?.is_handled());
            assert_eq!(s.path, vec!["ba@key->handle"]);
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            root.a.set_focus();
            root.next_outcome = Some(Outcome::handle());
            assert!(app.key(ctrl, &mut root, &mut s, K_ANY)?.is_handled());
            assert_eq!(s.path, vec!["ba@key->ignore", "r@key->handle"]);
            assert_eq!(app.key(ctrl, &mut root, &mut s, K_ANY)?, Outcome::ignore());
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            root.a.b.set_focus();
            root.a.next_outcome = Some(Outcome::Ignore(Ignore::default().with_skip()));
            root.next_outcome = Some(Outcome::handle());
            app.key(ctrl, &mut root, &mut s, K_ANY)?;
            assert_eq!(s.path, vec!["ba:lb@key->ignore", "ba@key->ignore"]);
            Ok(())
        })?;

        run_test(|_, mut app, _, ctrl, mut root, mut s| {
            root.a.a.set_focus();
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
            root.a.b.set_focus();
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
            root.a.b.set_focus();
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
            root.a.b.set_focus();
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
            root.a.b.set_focus();
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
            root.set_focus();
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

            root.a.a.set_focus();
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
            root.set_focus();
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
