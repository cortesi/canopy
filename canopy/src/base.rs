use duplicate::duplicate;
use std::marker::PhantomData;
use std::{fmt::Debug, io::Write};

use crate::geom::{Direction, Rect};
use crate::{
    colorscheme::ColorScheme,
    cursor,
    event::{key, mouse, tick, Event},
    layout::FillLayout,
    node::{postorder, postorder_mut, preorder, EventOutcome, Node, Walker},
    Error, Point, Result,
};
use crossterm::{
    cursor::{CursorShape, DisableBlinking, EnableBlinking, Hide, MoveTo, SetCursorShape, Show},
    QueueableCommand,
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
#[derive(Debug, PartialEq, Copy)]
pub struct Canopy<S> {
    // A counter that is incremented every time focus changes. The current focus
    // will have a state `focus_gen` equal to this.
    pub focus_gen: u64,
    // A counter that is incremented every time we render. All items that
    // require rendering during the current sweep will have a state `render_gen`
    // equal to this.
    pub render_gen: u64,
    pub last_focus_gen: u64,
    _marker: PhantomData<S>,
}

// Derive isn't smart enough to notice that the type argument to Canopy doesn't
// need to be Clone, so we manually implement.
impl<S> Clone for Canopy<S> {
    fn clone(&self) -> Canopy<S> {
        Canopy {
            focus_gen: self.focus_gen,
            render_gen: self.render_gen,
            last_focus_gen: self.last_focus_gen,
            _marker: PhantomData,
        }
    }
}

impl<S> Default for Canopy<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Canopy<S> {
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
    pub fn should_render(&self, e: &dyn Node<S>) -> bool {
        if let Some(r) = e.should_render(self) {
            r
        } else {
            self.is_tainted(e) || self.focus_changed(e)
        }
    }

    /// Is this node render tainted?
    pub fn is_tainted(&self, e: &dyn Node<S>) -> bool {
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
    pub fn focus_changed(&self, e: &dyn Node<S>) -> bool {
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
    pub fn set_focus(&mut self, e: &mut dyn Node<S>) -> Result<EventOutcome> {
        if e.can_focus() {
            self.focus_gen += 1;
            e.state_mut().focus_gen = self.focus_gen;
            return Ok(EventOutcome::Handle { skip: false });
        }
        Err(Error::Focus("node does not accept focus".into()))
    }

    fn focus_dir(&mut self, e: &mut dyn Node<S>, dir: Direction) -> Result<EventOutcome> {
        let mut seen = false;
        if let Some(start) = self.get_focus_area(e) {
            start.search(dir, &mut |p| -> Result<bool> {
                if !e.rect().contains_point(p) {
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
    pub fn focus_right(&mut self, e: &mut dyn Node<S>) -> Result<EventOutcome> {
        self.focus_dir(e, Direction::Right)
    }

    /// Move focus to the left of the currently focused node within the subtree.
    pub fn focus_left(&mut self, e: &mut dyn Node<S>) -> Result<EventOutcome> {
        self.focus_dir(e, Direction::Left)
    }

    /// Move focus upward of the currently focused node within the subtree.
    pub fn focus_up(&mut self, e: &mut dyn Node<S>) -> Result<EventOutcome> {
        self.focus_dir(e, Direction::Up)
    }

    /// Move focus downward of the currently focused node within the subtree.
    pub fn focus_down(&mut self, e: &mut dyn Node<S>) -> Result<EventOutcome> {
        self.focus_dir(e, Direction::Down)
    }

    /// Focus the first node that accepts focus in the pre-order traversal of
    /// the subtree.
    pub fn focus_first(&mut self, e: &mut dyn Node<S>) -> Result<EventOutcome> {
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
    pub fn is_focused(&self, e: &dyn Node<S>) -> bool {
        let s = e.state();
        self.focus_gen == s.focus_gen
    }

    /// A node is on the focus path if it or any of its descendants have focus.
    pub fn on_focus_path(&self, e: &dyn Node<S>) -> bool {
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
    pub fn is_focus_ancestor(&self, e: &dyn Node<S>) -> bool {
        if self.is_focused(e) {
            false
        } else {
            self.on_focus_path(e)
        }
    }

    /// Focus the next node in the pre-order traversal of a node. If no node
    /// with focus is found, we focus the first node we can find instead.
    pub fn focus_next(&mut self, e: &mut dyn Node<S>) -> Result<EventOutcome> {
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
    pub fn focus_prev(&mut self, e: &mut dyn Node<S>) -> Result<EventOutcome> {
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
    pub fn get_focus_area(&self, e: &dyn Node<S>) -> Option<Rect> {
        let mut ret = None;
        self.focus_path(e, &mut |x| -> Result<()> {
            if ret == None {
                ret = Some(x.rect());
            }
            Ok(())
        })
        .unwrap();
        ret
    }

    /// Calls a closure on the currently focused node and all its parents to the
    /// root.
    #[duplicate(
        method              reference(type)    traversal;
        [focus_path]        [& type]           [postorder];
        [focus_path_mut]    [&mut type]        [postorder_mut];
    )]
    pub fn method<R: Walker + Default>(
        &self,
        e: reference([dyn Node<S>]),
        f: &mut dyn FnMut(reference([dyn Node<S>])) -> Result<R>,
    ) -> Result<R> {
        let mut focus_seen = false;
        let mut ret = R::default();
        traversal(e, &mut |x| -> Result<SkipWalker> {
            Ok(if focus_seen {
                ret = ret.join(f(x)?);
                SkipWalker::default()
            } else if self.is_focused(x) {
                focus_seen = true;
                ret = ret.join(f(x)?);
                SkipWalker { has_skip: true }
            } else {
                SkipWalker::default()
            })
        })?;
        Ok(ret)
    }

    /// Returns the focal depth of the specified node. If the node is not part
    /// of the focus chain, the depth is 0. If the node is a leaf focus, the
    /// depth is 1.
    pub fn focus_depth(&self, e: &dyn Node<S>) -> usize {
        let mut total = 0;
        self.focus_path(e, &mut |_| -> Result<()> {
            total += 1;
            Ok(())
        })
        .unwrap();
        total
    }

    /// Pre-render sweep of the tree.
    pub(crate) fn pre_render(&mut self, e: &mut dyn Node<S>, w: &mut dyn Write) -> Result<()> {
        let mut seen = false;
        self.focus_path_mut(e, &mut |_| -> Result<()> {
            seen = true;
            Ok(())
        })?;
        if !seen {
            self.focus_first(e)?;
        }
        // FIXME: Maybe only hide if we know the cursor is visible?
        w.queue(Hide {})?;
        Ok(())
    }

    /// Post-render sweep of the tree.
    pub(crate) fn post_render(&mut self, e: &mut dyn Node<S>, w: &mut dyn Write) -> Result<()> {
        let mut seen = false;
        self.focus_path_mut(e, &mut |n| -> Result<()> {
            if !seen {
                if let Some(c) = n.cursor() {
                    let r = n.rect();
                    w.queue(MoveTo(r.tl.x + c.location.x, r.tl.y + c.location.y))?;
                    w.queue(Show)?;
                    if c.blink {
                        w.queue(EnableBlinking)?;
                    } else {
                        w.queue(DisableBlinking)?;
                    }
                    w.queue(SetCursorShape(match c.shape {
                        cursor::CursorShape::Block => CursorShape::Block,
                        cursor::CursorShape::Line => CursorShape::Line,
                        cursor::CursorShape::Underscore => CursorShape::UnderScore,
                    }))?;
                    seen = true;
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    /// Mark a tree of nodes for render.
    pub fn taint_tree(&self, e: &mut dyn Node<S>) -> Result<()> {
        postorder_mut(e, &mut |x| -> Result<()> {
            let r = x.state_mut();
            r.render_gen = self.render_gen;
            Ok(())
        })?;
        Ok(())
    }

    /// Mark a single node for render.
    pub fn taint(&self, e: &mut dyn Node<S>) {
        let r = e.state_mut();
        r.render_gen = self.render_gen;
    }

    /// Mark that a node should skip the next render sweep.
    pub fn skip_taint(&self, e: &mut dyn Node<S>) {
        let r = e.state_mut();
        r.render_skip_gen = self.render_gen;
    }

    fn render_traversal(
        &mut self,
        colors: &mut ColorScheme,
        e: &mut dyn Node<S>,
        w: &mut dyn Write,
    ) -> Result<()> {
        if self.should_render(e) {
            if self.is_focused(e) {
                let s = &mut e.state_mut();
                s.rendered_focus_gen = self.focus_gen
            }
            e.render(self, colors, w)?;
        }
        colors.inc();
        e.children_mut(&mut |x| self.render_traversal(colors, x, w))?;
        colors.dec();
        Ok(())
    }

    /// Render a tree of nodes. If force is true, all visible nodes are
    /// rendered, otherwise we check the taint state.
    pub fn render(
        &mut self,
        e: &mut dyn Node<S>,
        colors: &mut ColorScheme,
        w: &mut dyn Write,
    ) -> Result<()> {
        colors.reset();
        self.render_traversal(colors, e, w)?;
        self.render_gen += 1;
        self.last_focus_gen = self.focus_gen;
        Ok(())
    }

    /// Propagate a mouse event through the node under the event and all its
    /// ancestors. Events are handled only once, and then ignored.
    pub fn mouse(
        &mut self,
        root: &mut dyn Node<S>,
        s: &mut S,
        m: mouse::Mouse,
    ) -> Result<EventOutcome> {
        let mut handled = false;
        locate(root, m.loc, &mut |x| {
            Ok(if handled {
                EventOutcome::default()
            } else {
                let m = mouse::Mouse {
                    action: m.action,
                    button: m.button,
                    modifiers: m.modifiers,
                    loc: x.rect().rebase(m.loc)?,
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
                    itm => itm,
                }
            })
        })
    }

    /// Propagate a key event through the focus and all its ancestors. Keys
    /// handled only once, and then ignored.
    pub fn key(&mut self, root: &mut dyn Node<S>, s: &mut S, k: key::Key) -> Result<EventOutcome> {
        let mut handled = false;
        self.clone()
            .focus_path_mut(root, &mut |x| -> Result<EventOutcome> {
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
                        itm => itm,
                    }
                })
            })
    }

    /// Propagate a resize event through the tree of nodes.
    pub fn resize<N>(&mut self, e: &mut N, rect: Rect) -> Result<()>
    where
        N: Node<S> + FillLayout<S>,
    {
        if e.rect() == rect {
            return Ok(());
        }
        e.layout(self, rect)?;
        self.taint_tree(e)?;
        Ok(())
    }

    /// Propagate a tick event through the tree.
    pub fn tick(
        &mut self,
        root: &mut dyn Node<S>,
        s: &mut S,
        t: tick::Tick,
    ) -> Result<EventOutcome> {
        let mut ret = EventOutcome::default();
        preorder(root, &mut |x| -> Result<SkipWalker> {
            let v = x.handle_tick(self, s, t)?;
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
                EventOutcome::Exit => SkipWalker { has_skip: true },
            })
        })?;
        Ok(ret)
    }

    /// Propagate an event through the tree.
    pub fn event<N>(&mut self, root: &mut N, s: &mut S, e: Event) -> Result<EventOutcome>
    where
        N: Node<S> + FillLayout<S>,
    {
        match e {
            Event::Key(k) => self.key(root, s, k),
            Event::Mouse(m) => self.mouse(root, s, m),
            Event::Resize(r) => {
                self.resize(root, r)?;
                Ok(EventOutcome::Handle { skip: false })
            }
            Event::Tick(t) => self.tick(root, s, t),
        }
    }
}

// Calls a closure on the leaf node under (x, y), then all its parents to the
// root.
pub fn locate<S, R: Walker + Default>(
    e: &mut dyn Node<S>,
    p: Point,
    f: &mut dyn FnMut(&mut dyn Node<S>) -> Result<R>,
) -> Result<R> {
    let mut seen = false;
    let mut ret = R::default();
    postorder_mut(e, &mut |inner| -> Result<SkipWalker> {
        Ok(if seen {
            ret = ret.join(f(inner)?);
            SkipWalker::default()
        } else {
            let a = inner.rect();
            if a.contains_point(p) {
                seen = true;
                ret = ret.join(f(inner)?);
                SkipWalker { has_skip: true }
            } else {
                SkipWalker::default()
            }
        })
    })?;
    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        geom::{Point, Rect},
        tutils::utils,
        StatefulNode,
    };

    pub fn focvec(app: &mut Canopy<utils::State>, root: &mut utils::TRoot) -> Result<Vec<String>> {
        let mut v = vec![];
        app.clone().focus_path_mut(root, &mut |x| -> Result<()> {
            let n = utils::get_name(app, x)?;
            v.push(n);
            Ok(())
        })?;
        Ok(v)
    }

    #[test]
    fn tfocus_next() -> Result<()> {
        let mut app = Canopy::new();
        let mut root = utils::TRoot::new();

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
        let mut app = Canopy::new();
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
        let mut app = Canopy::new();
        let mut root = utils::TRoot::new();

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
        let mut app = Canopy::new();
        let mut root = utils::TRoot::new();
        const SIZE: u16 = 100;
        app.resize(
            &mut root,
            Rect {
                tl: Point { x: 0, y: 0 },
                w: SIZE,
                h: SIZE,
            },
        )?;

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
    fn ttick() -> Result<()> {
        let mut app = Canopy::new();
        let mut root = utils::TRoot::new();

        let handled = EventOutcome::Handle { skip: false };
        let ignore = EventOutcome::Ignore { skip: false };

        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.next_event = Some(handled);
        assert_eq!(app.tick(&mut root, &mut s, tick::Tick {})?, handled);
        assert_eq!(
            s.path,
            vec![
                "r@tick->handle",
                "ba@tick->ignore",
                "ba:la@tick->ignore",
                "ba:lb@tick->ignore",
                "bb@tick->ignore",
                "bb:la@tick->ignore",
                "bb:lb@tick->ignore"
            ]
        );

        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.a.next_event = Some(EventOutcome::Ignore { skip: true });
        assert_eq!(app.tick(&mut root, &mut s, tick::Tick {})?, ignore);
        assert_eq!(
            s.path,
            vec![
                "r@tick->ignore",
                "ba@tick->ignore",
                "bb@tick->ignore",
                "bb:la@tick->ignore",
                "bb:lb@tick->ignore"
            ]
        );

        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.a.next_event = Some(EventOutcome::Ignore { skip: true });
        root.b.next_event = Some(EventOutcome::Handle { skip: true });
        assert_eq!(app.tick(&mut root, &mut s, tick::Tick {})?, handled);
        assert_eq!(
            s.path,
            vec!["r@tick->ignore", "ba@tick->ignore", "bb@tick->handle",]
        );

        Ok(())
    }

    #[test]
    fn tkey() -> Result<()> {
        let mut app = Canopy::new();
        let mut root = utils::TRoot::new();

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
        let mut app = Canopy::new();
        const SIZE: u16 = 100;
        let mut root = utils::TRoot::new();
        app.resize(
            &mut root,
            Rect {
                tl: Point { x: 0, y: 0 },
                w: SIZE,
                h: SIZE,
            },
        )?;

        let acted = EventOutcome::Handle { skip: false };

        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.next_event = Some(acted);
        let evt = root.a.a.mouse_event()?;
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
        let mut app = Canopy::new();
        let mut root = utils::TRoot::new();
        app.resize(
            &mut root,
            Rect {
                tl: Point { x: 0, y: 0 },
                w: SIZE,
                h: SIZE,
            },
        )?;
        assert_eq!(
            root.rect(),
            Rect {
                tl: Point { x: 0, y: 0 },
                w: SIZE,
                h: SIZE
            }
        );
        assert_eq!(
            root.a.rect(),
            Rect {
                tl: Point { x: 0, y: 0 },
                w: SIZE / 2,
                h: SIZE
            }
        );
        assert_eq!(
            root.b.rect(),
            Rect {
                tl: Point { x: SIZE / 2, y: 0 },
                w: SIZE / 2,
                h: SIZE
            }
        );

        app.resize(
            &mut root,
            Rect {
                tl: Point { x: 0, y: 0 },
                w: 50,
                h: 50,
            },
        )?;

        assert_eq!(
            root.b.rect(),
            Rect {
                tl: Point { x: 25, y: 0 },
                w: 25,
                h: 50
            }
        );

        Ok(())
    }
    #[test]
    fn trender() -> Result<()> {
        let mut app = Canopy::new();
        let mut root = utils::TRoot::new();

        const SIZE: u16 = 100;
        app.resize(
            &mut root,
            Rect {
                tl: Point { x: 0, y: 0 },
                w: SIZE,
                h: SIZE,
            },
        )?;

        assert_eq!(
            utils::trender(&mut app, &mut root)?,
            "<r><ba><ba:la><ba:lb><bb><bb:la><bb:lb>"
        );
        assert_eq!(utils::trender(&mut app, &mut root)?, "");
        app.taint(&mut root.a);
        assert_eq!(utils::trender(&mut app, &mut root)?, "<ba>");
        app.taint(&mut root.a.b);
        assert_eq!(utils::trender(&mut app, &mut root)?, "<ba:lb>");
        app.taint_tree(&mut root.a)?;
        assert_eq!(utils::trender(&mut app, &mut root)?, "<ba><ba:la><ba:lb>");
        assert_eq!(utils::trender(&mut app, &mut root)?, "");

        app.set_focus(&mut root.a.a)?;
        assert_eq!(utils::trender(&mut app, &mut root)?, "<ba:la>");
        app.focus_next(&mut root)?;
        assert_eq!(utils::trender(&mut app, &mut root)?, "<ba:la><ba:lb>");
        assert_eq!(utils::trender(&mut app, &mut root)?, "");
        app.focus_prev(&mut root)?;
        assert_eq!(utils::trender(&mut app, &mut root)?, "<ba:la><ba:lb>");
        assert_eq!(utils::trender(&mut app, &mut root)?, "");

        Ok(())
    }

    #[test]
    fn ttaintskip() -> Result<()> {
        let mut app = Canopy::new();
        let mut root = utils::TRoot::new();
        const SIZE: u16 = 100;
        app.resize(
            &mut root,
            Rect {
                tl: Point { x: 0, y: 0 },
                w: SIZE,
                h: SIZE,
            },
        )?;
        utils::trender(&mut app, &mut root)?;

        let handled = EventOutcome::Handle { skip: false };

        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.next_event = Some(handled);
        root.a.a.next_event = Some(handled);
        root.b.b.next_event = Some(handled);
        app.skip_taint(&mut root.a.a);
        assert_eq!(app.tick(&mut root, &mut s, tick::Tick {})?, handled);
        assert_eq!(
            s.path,
            vec![
                "r@tick->handle",
                "ba@tick->ignore",
                "ba:la@tick->handle",
                "ba:lb@tick->ignore",
                "bb@tick->ignore",
                "bb:la@tick->ignore",
                "bb:lb@tick->handle"
            ]
        );
        assert_eq!(utils::trender(&mut app, &mut root)?, "<r><bb:lb>");
        Ok(())
    }
}
