use duplicate::duplicate;
use std::marker::PhantomData;

use crate::geom::{Direction, Rect};
use crate::{
    event::{key, mouse, Event},
    geom::Point,
    node::{postorder, postorder_mut, preorder, EventOutcome, Node, Walker},
    Actions, Error, Render, Result, StatefulNode,
};

pub struct SkipWalker {
    pub has_skip: bool,
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

/// The core of a Canopy app - this struct keeps track of the render and focus
/// state, and provides functionality for interacting with node trees.
pub struct Canopy<'a, S, A: Actions> {
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

    /// The active render backend.
    pub render: Render<'a>,

    _marker: PhantomData<S>,
    _marker2: PhantomData<A>,
}

impl<'a, S, A: Actions> Canopy<'a, S, A> {
    pub fn new(render: Render<'a>) -> Self {
        Canopy {
            render,
            focus_gen: 1,
            render_gen: 1,
            last_focus_gen: 1,
            _marker: PhantomData,
            _marker2: PhantomData,
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
    pub fn set_focus(&mut self, e: &mut dyn Node<S, A>) -> Result<EventOutcome> {
        if e.can_focus() {
            self.focus_gen += 1;
            e.state_mut().focus_gen = self.focus_gen;
            return Ok(EventOutcome::Handle { skip: false });
        }
        Err(Error::Focus("node does not accept focus".into()))
    }

    /// Move focus in a specified direction within the subtree.
    pub fn focus_dir(&mut self, e: &mut dyn Node<S, A>, dir: Direction) -> Result<EventOutcome> {
        let mut seen = false;
        if let Some(start) = self.get_focus_area(e) {
            start.search(dir, &mut |p| -> Result<bool> {
                if !e.screen().contains_point(p) {
                    return Ok(true);
                }
                locate(e, p, &mut |x| {
                    if !seen && x.can_focus() {
                        seen = true;
                        self.set_focus(x)?;
                    }
                    Ok(())
                })?;
                Ok(seen)
            })?
        }
        Ok(EventOutcome::Handle { skip: false })
    }

    /// Move focus to the right of the currently focused node within the subtree.
    pub fn focus_right(&mut self, e: &mut dyn Node<S, A>) -> Result<EventOutcome> {
        self.focus_dir(e, Direction::Right)
    }

    /// Move focus to the left of the currently focused node within the subtree.
    pub fn focus_left(&mut self, e: &mut dyn Node<S, A>) -> Result<EventOutcome> {
        self.focus_dir(e, Direction::Left)
    }

    /// Move focus upward of the currently focused node within the subtree.
    pub fn focus_up(&mut self, e: &mut dyn Node<S, A>) -> Result<EventOutcome> {
        self.focus_dir(e, Direction::Up)
    }

    /// Move focus downward of the currently focused node within the subtree.
    pub fn focus_down(&mut self, e: &mut dyn Node<S, A>) -> Result<EventOutcome> {
        self.focus_dir(e, Direction::Down)
    }

    /// Focus the first node that accepts focus in the pre-order traversal of
    /// the subtree.
    pub fn focus_first(&mut self, e: &mut dyn Node<S, A>) -> Result<EventOutcome> {
        let mut focus_set = false;
        preorder(e, &mut |x| -> Result<SkipWalker> {
            Ok(if !focus_set && x.can_focus() {
                self.set_focus(x)?;
                focus_set = true;
                SkipWalker { has_skip: true }
            } else {
                SkipWalker::default()
            })
        })?;
        Ok(EventOutcome::Handle { skip: false })
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
    pub fn focus_next(&mut self, e: &mut dyn Node<S, A>) -> Result<EventOutcome> {
        let mut focus_set = false;
        let mut focus_seen = false;
        preorder(e, &mut |x| -> Result<()> {
            if !focus_set {
                if focus_seen {
                    if x.can_focus() {
                        self.set_focus(x)?;
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
            Ok(EventOutcome::Handle { skip: false })
        }
    }

    /// Focus the previous node in the pre-order traversal of a node. If no
    /// node with focus is found, we focus the first node we can find instead.
    pub fn focus_prev(&mut self, e: &mut dyn Node<S, A>) -> Result<EventOutcome> {
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
                } else if x.can_focus() {
                    self.set_focus(x)?;
                }
            }
            Ok(())
        })?;
        Ok(EventOutcome::Handle { skip: false })
    }

    /// Find the area of the current terminal focus node.
    pub fn get_focus_area(&self, e: &dyn Node<S, A>) -> Option<Rect> {
        let mut ret = None;
        self.focus_path(e, &mut |x| -> Result<()> {
            if ret == None {
                ret = Some(x.screen());
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
    pub(crate) fn pre_render(&mut self, e: &mut dyn Node<S, A>) -> Result<()> {
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
        self.render.hide_cursor()?;
        Ok(())
    }

    /// Post-render sweep of the tree.
    pub(crate) fn post_render(&mut self, e: &dyn Node<S, A>) -> Result<()> {
        let mut seen = false;
        focus_path(self.focus_gen, e, &mut |n| -> Result<()> {
            if !seen {
                if let Some(c) = n.cursor() {
                    self.render.show_cursor("cursor", c)?;
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

    fn render_traversal(&mut self, e: &mut dyn Node<S, A>) -> Result<()> {
        if !e.is_hidden() {
            self.render.push();
            if self.should_render(e) {
                if self.is_focused(e) {
                    let s = &mut e.state_mut();
                    s.rendered_focus_gen = self.focus_gen
                }
                self.render.viewport = e.state().viewport;
                e.render(self)?;
            }
            // This is a new node - we don't want it perpetually stuck in
            // render, so we need to update its render_gen.
            if e.state().render_gen == 0 {
                e.state_mut().render_gen = self.render_gen;
            }
            e.children_mut(&mut |x| self.render_traversal(x))?;
            self.render.pop();
        }
        Ok(())
    }

    /// Render a tree of nodes. If force is true, all visible nodes are
    /// rendered, otherwise we check the taint state. Hidden nodes and their
    /// children are ignored.
    pub fn render(&mut self, e: &mut dyn Node<S, A>) -> Result<()> {
        self.render.reset()?;
        self.render_traversal(e)?;
        self.render_gen += 1;
        self.last_focus_gen = self.focus_gen;
        Ok(())
    }

    /// Propagate a mouse event through the node under the event and all its
    /// ancestors. Events are handled only once, and then ignored.
    pub fn mouse(
        &mut self,
        root: &mut dyn Node<S, A>,
        s: &mut S,
        m: mouse::Mouse,
    ) -> Result<EventOutcome> {
        let mut handled = false;
        locate(root, m.loc, &mut |x| {
            Ok(if handled {
                EventOutcome::default()
            } else if !x.is_hidden() {
                let r = x.screen();
                let m = mouse::Mouse {
                    action: m.action,
                    button: m.button,
                    modifiers: m.modifiers,
                    loc: r.rebase_point(m.loc)?,
                };
                match x.handle_mouse(self, s, m)? {
                    EventOutcome::Ignore { skip } => {
                        if skip {
                            handled = true;
                        }
                        EventOutcome::Ignore { skip: false }
                    }
                    EventOutcome::Handle { .. } => {
                        self.taint(x);
                        handled = true;
                        EventOutcome::Handle { skip: false }
                    }
                }
            } else {
                EventOutcome::default()
            })
        })
    }

    /// Propagate a key event through the focus and all its ancestors. Keys
    /// handled only once, and then ignored.
    pub fn key(
        &mut self,
        root: &mut dyn Node<S, A>,
        s: &mut S,
        k: key::Key,
    ) -> Result<EventOutcome> {
        let mut handled = false;
        focus_path_mut(self.focus_gen, root, &mut |x| -> Result<EventOutcome> {
            Ok(if handled {
                EventOutcome::default()
            } else {
                match x.handle_key(self, s, k)? {
                    EventOutcome::Ignore { skip } => {
                        if skip {
                            handled = true;
                        }
                        EventOutcome::Ignore { skip: false }
                    }
                    EventOutcome::Handle { .. } => {
                        self.taint(x);
                        handled = true;
                        EventOutcome::Handle { skip: false }
                    }
                }
            })
        })
    }

    /// Handle a screen resize. This calls layout and taints the tree.
    pub fn resize<N>(&mut self, e: &mut N, rect: Rect) -> Result<()>
    where
        N: Node<S, A>,
    {
        fit_and_update(self, rect, e)?;
        self.taint_tree(e)?;
        Ok(())
    }

    /// Propagate a tick event through the tree. All nodes get the event, even
    /// if they are hidden.
    pub fn action(&mut self, root: &mut dyn Node<S, A>, s: &mut S, t: A) -> Result<EventOutcome> {
        let mut ret = EventOutcome::default();
        preorder(root, &mut |x| -> Result<SkipWalker> {
            let v = x.handle_action(self, s, t)?;
            ret = ret.join(v);
            Ok(match v {
                EventOutcome::Handle { skip } => {
                    self.taint(x);
                    if skip {
                        SkipWalker { has_skip: true }
                    } else {
                        SkipWalker { has_skip: false }
                    }
                }
                EventOutcome::Ignore { skip } => {
                    if skip {
                        SkipWalker { has_skip: true }
                    } else {
                        SkipWalker { has_skip: false }
                    }
                }
            })
        })?;
        Ok(ret)
    }

    /// Propagate an event through the tree.
    pub fn event<N>(&mut self, root: &mut N, s: &mut S, e: Event<A>) -> Result<EventOutcome>
    where
        N: Node<S, A>,
    {
        match e {
            Event::Key(k) => self.key(root, s, k),
            Event::Mouse(m) => self.mouse(root, s, m),
            Event::Resize(r) => {
                self.resize(root, r)?;
                Ok(EventOutcome::Handle { skip: false })
            }
            Event::Action(t) => self.action(root, s, t),
        }
    }

    /// Clean up render loop and exit the process.
    pub fn exit(&mut self, code: i32) -> ! {
        self.render.exit(code)
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
            SkipWalker::default()
        } else if x.is_hidden() {
            // Hidden nodes don't hold focus
            SkipWalker::default()
        } else if x.state().focus_gen == focus_gen {
            focus_seen = true;
            ret = ret.join(f(x)?);
            SkipWalker { has_skip: true }
        } else {
            SkipWalker::default()
        })
    })?;
    Ok(ret)
}

/// A convenience method that fits the component to the screen rect, updates its
/// view, then calls layout on it to lay out its children.
pub fn fit_and_update<S, A: Actions, N>(
    app: &mut Canopy<S, A>,
    screen: Rect,
    n: &mut N,
) -> Result<()>
where
    N: Node<S, A> + StatefulNode,
{
    let fit = n.fit(app, screen.size())?;
    n.update_view(fit, screen);
    n.layout(app, screen)?;
    Ok(())
}

// Calls a closure on the leaf node under (x, y), then all its parents to the
// root.
pub fn locate<S, A: Actions, R: Walker + Default>(
    e: &mut dyn Node<S, A>,
    p: Point,
    f: &mut dyn FnMut(&mut dyn Node<S, A>) -> Result<R>,
) -> Result<R> {
    let mut seen = false;
    let mut ret = R::default();
    postorder_mut(e, &mut |inner| -> Result<SkipWalker> {
        Ok(if seen {
            ret = ret.join(f(inner)?);
            SkipWalker::default()
        } else if !inner.is_hidden() {
            let a = inner.screen();
            if a.contains_point(p) {
                seen = true;
                ret = ret.join(f(inner)?);
                SkipWalker { has_skip: true }
            } else {
                SkipWalker::default()
            }
        } else {
            SkipWalker { has_skip: true }
        })
    })?;
    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{geom::Rect, render::test::TestRender, tutils::utils, StatefulNode};

    pub fn focvec(
        app: &mut Canopy<utils::State, ()>,
        root: &mut utils::TRoot,
    ) -> Result<Vec<String>> {
        let mut v = vec![];
        focus_path_mut(app.focus_gen, root, &mut |x| -> Result<()> {
            let n = x.name().unwrap();
            v.push(n);
            Ok(())
        })?;
        Ok(v)
    }

    #[test]
    fn tfocus_next() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);

        let mut root = utils::TRoot::new();
        root.layout(&mut app, Rect::default())?;

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

        app.set_focus(&mut root.b.b)?;
        assert!(app.is_focus_ancestor(&mut root.b));
        app.focus_next(&mut root)?;
        assert!(app.is_focused(&root));

        Ok(())
    }

    #[test]
    fn tfocus_prev() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);

        let mut root = utils::TRoot::new();

        assert!(!app.is_focused(&root));
        app.focus_prev(&mut root)?;
        assert!(app.is_focused(&root.b.b));

        app.focus_prev(&mut root)?;
        assert!(app.is_focused(&root.b.a));

        app.focus_prev(&mut root)?;
        assert!(app.is_focused(&root.b));

        app.set_focus(&mut root)?;
        app.focus_prev(&mut root)?;
        assert!(app.is_focused(&root.b.b));

        Ok(())
    }

    #[test]
    fn tfoci() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);

        let mut root = utils::TRoot::new();
        root.layout(&mut app, Rect::default())?;

        assert_eq!(focvec(&mut app, &mut root)?.len(), 0);

        assert!(!app.on_focus_path(&mut root));
        assert!(!app.on_focus_path(&mut root.a));

        app.set_focus(&mut root.a.a)?;
        assert!(app.on_focus_path(&mut root));
        assert!(app.on_focus_path(&mut root.a));
        assert!(!app.on_focus_path(&mut root.b));

        assert_eq!(focvec(&mut app, &mut root)?, vec!["ba:la", "ba", "r"]);

        app.set_focus(&mut root.a)?;
        assert_eq!(focvec(&mut app, &mut root)?, vec!["ba", "r"]);

        app.set_focus(&mut root)?;
        assert_eq!(focvec(&mut app, &mut root)?, vec!["r"]);

        app.set_focus(&mut root.b.a)?;
        assert_eq!(focvec(&mut app, &mut root)?, vec!["bb:la", "bb", "r"]);

        Ok(())
    }

    #[test]
    fn tfocus_right() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);
        let mut root = utils::TRoot::new();

        fit_and_update(&mut app, Rect::new(0, 0, 100, 100), &mut root)?;

        app.set_focus(&mut root.a.a)?;
        app.focus_right(&mut root)?;
        assert!(app.is_focused(&root.b.a));
        app.focus_right(&mut root)?;
        assert!(app.is_focused(&root.b.a));

        app.set_focus(&mut root.a.b)?;
        app.focus_right(&mut root)?;
        assert!(app.is_focused(&root.b.b));
        app.focus_right(&mut root)?;
        assert!(app.is_focused(&root.b.b));

        Ok(())
    }

    #[test]
    fn taction() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);
        let mut root = utils::TRoot::new();

        let handled = EventOutcome::Handle { skip: false };
        let ignore = EventOutcome::Ignore { skip: false };

        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.next_event = Some(handled);
        assert_eq!(app.action(&mut root, &mut s, ())?, handled);
        assert_eq!(
            s.path,
            vec![
                "r@action->handle",
                "ba@action->ignore",
                "ba:la@action->ignore",
                "ba:lb@action->ignore",
                "bb@action->ignore",
                "bb:la@action->ignore",
                "bb:lb@action->ignore"
            ]
        );

        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.a.next_event = Some(EventOutcome::Ignore { skip: true });
        assert_eq!(app.action(&mut root, &mut s, ())?, ignore);
        assert_eq!(
            s.path,
            vec![
                "r@action->ignore",
                "ba@action->ignore",
                "bb@action->ignore",
                "bb:la@action->ignore",
                "bb:lb@action->ignore"
            ]
        );

        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.a.next_event = Some(EventOutcome::Ignore { skip: true });
        root.b.next_event = Some(EventOutcome::Handle { skip: true });
        assert_eq!(app.action(&mut root, &mut s, ())?, handled);
        assert_eq!(
            s.path,
            vec!["r@action->ignore", "ba@action->ignore", "bb@action->handle",]
        );

        Ok(())
    }

    #[test]
    fn tkey() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);
        let mut root = utils::TRoot::new();
        root.layout(&mut app, Rect::default())?;

        let handled = EventOutcome::Handle { skip: false };

        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.next_event = Some(handled);
        assert_eq!(app.key(&mut root, &mut s, utils::K_ANY)?, handled);
        assert_eq!(s.path, vec!["r@key->handle"]);

        let mut s = utils::State::new();
        app.set_focus(&mut root.a.a)?;
        root.a.a.next_event = Some(handled);
        assert_eq!(app.key(&mut root, &mut s, utils::K_ANY)?, handled);
        assert_eq!(s.path, vec!["ba:la@key->handle"]);

        let mut s = utils::State::new();
        root.a.next_event = Some(handled);
        assert_eq!(app.key(&mut root, &mut s, utils::K_ANY)?, handled);
        assert_eq!(s.path, vec!["ba:la@key->ignore", "ba@key->handle"]);

        let mut s = utils::State::new();
        root.next_event = Some(handled);
        assert_eq!(app.key(&mut root, &mut s, utils::K_ANY)?, handled);
        assert_eq!(
            s.path,
            vec!["ba:la@key->ignore", "ba@key->ignore", "r@key->handle"]
        );

        let mut s = utils::State::new();
        app.set_focus(&mut root.a)?;
        root.a.next_event = Some(handled);
        assert_eq!(app.key(&mut root, &mut s, utils::K_ANY)?, handled);
        assert_eq!(s.path, vec!["ba@key->handle"]);

        let mut s = utils::State::new();
        root.next_event = Some(handled);
        assert_eq!(app.key(&mut root, &mut s, utils::K_ANY)?, handled);
        assert_eq!(s.path, vec!["ba@key->ignore", "r@key->handle"]);

        assert_eq!(
            app.key(&mut root, &mut s, utils::K_ANY)?,
            EventOutcome::Ignore { skip: false }
        );

        let mut s = utils::State::new();
        app.set_focus(&mut root.a.b)?;
        root.a.next_event = Some(EventOutcome::Ignore { skip: true });
        root.next_event = Some(handled);
        app.key(&mut root, &mut s, utils::K_ANY)?;
        assert_eq!(s.path, vec!["ba:lb@key->ignore", "ba@key->ignore"]);

        Ok(())
    }

    #[test]
    fn tmouse() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);
        const SIZE: u16 = 100;
        let mut root = utils::TRoot::new();
        fit_and_update(&mut app, Rect::new(0, 0, SIZE, SIZE), &mut root)?;

        let acted = EventOutcome::Handle { skip: false };
        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.next_event = Some(acted);
        let evt = root.a.a.make_mouse_event()?;
        assert_eq!(app.mouse(&mut root, &mut s, evt)?, acted);
        assert_eq!(
            s.path,
            vec!["ba:la@mouse->ignore", "ba@mouse->ignore", "r@mouse->handle"]
        );

        root.a.a.next_event = Some(acted);
        let mut s = utils::State::new();
        assert_eq!(app.mouse(&mut root, &mut s, evt)?, acted);
        assert_eq!(s.path, vec!["ba:la@mouse->handle"]);

        Ok(())
    }

    #[test]
    fn tresize() -> Result<()> {
        const SIZE: u16 = 100;
        let (_, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);
        let mut root = utils::TRoot::new();

        fit_and_update(&mut app, Rect::new(0, 0, SIZE, SIZE), &mut root)?;

        assert_eq!(root.screen(), Rect::new(0, 0, SIZE, SIZE));
        assert_eq!(root.a.screen(), Rect::new(0, 0, SIZE / 2, SIZE));
        assert_eq!(root.b.screen(), Rect::new(SIZE / 2, 0, SIZE / 2, SIZE));

        app.resize(&mut root, Rect::new(0, 0, 50, 50))?;

        assert_eq!(root.b.screen(), Rect::new(25, 0, 25, 50));

        Ok(())
    }
    #[test]
    fn trender() -> Result<()> {
        let mut root = utils::TRoot::new();
        let (buf, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);
        fit_and_update(&mut app, Rect::new(0, 0, 100, 100), &mut root)?;

        app.render(&mut root)?;
        assert_eq!(
            buf.lock()?.text,
            vec!["<r>", "<ba>", "<ba:la>", "<ba:lb>", "<bb>", "<bb:la>", "<bb:lb>"]
        );

        app.render(&mut root)?;
        assert!(buf.lock()?.is_empty());

        app.taint(&mut root.a);
        app.render(&mut root)?;
        assert_eq!(buf.lock()?.text, vec!["<ba>"]);

        app.taint(&mut root.a.b);
        app.render(&mut root)?;
        assert_eq!(buf.lock()?.text, vec!["<ba:lb>"]);

        app.taint_tree(&mut root.a)?;
        app.render(&mut root)?;
        assert_eq!(buf.lock()?.text, vec!["<ba>", "<ba:la>", "<ba:lb>"]);

        app.render(&mut root)?;
        assert!(buf.lock()?.text.is_empty());

        app.set_focus(&mut root.a.a)?;
        app.render(&mut root)?;
        assert_eq!(buf.lock()?.text, vec!["<ba:la>"]);

        app.focus_next(&mut root)?;
        app.render(&mut root)?;
        assert_eq!(buf.lock()?.text, vec!["<ba:la>", "<ba:lb>"]);

        app.focus_prev(&mut root)?;
        app.render(&mut root)?;
        assert_eq!(buf.lock()?.text, vec!["<ba:la>", "<ba:lb>"]);

        app.render(&mut root)?;
        assert!(buf.lock()?.text.is_empty());

        Ok(())
    }

    #[test]
    fn ttaintskip() -> Result<()> {
        let (buf, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);

        let mut root = utils::TRoot::new();
        const SIZE: u16 = 100;
        fit_and_update(&mut app, Rect::new(0, 0, SIZE, SIZE), &mut root)?;

        app.render(&mut root)?;

        let handled = EventOutcome::Handle { skip: false };
        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.next_event = Some(handled);
        root.a.a.next_event = Some(handled);
        root.b.b.next_event = Some(handled);
        app.skip_taint(&mut root.a.a);
        assert_eq!(app.action(&mut root, &mut s, ())?, handled);
        assert_eq!(
            s.path,
            vec![
                "r@action->handle",
                "ba@action->ignore",
                "ba:la@action->handle",
                "ba:lb@action->ignore",
                "bb@action->ignore",
                "bb:la@action->ignore",
                "bb:lb@action->handle"
            ]
        );
        app.render(&mut root)?;
        assert_eq!(buf.lock()?.text, vec!["<r>", "<bb:lb>"]);
        Ok(())
    }
}
