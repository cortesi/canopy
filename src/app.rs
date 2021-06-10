use std::{fmt::Debug, io::Write};

use crate::geom::{Direction, Point, Rect};
use crate::{
    key, layout::FixedLayout, locate, mouse, postorder, preorder, Event, EventResult, Joiner, Node,
    SkipWalker,
};
use anyhow::{format_err, Result};

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Tick {}

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct Canopy {
    pub focus_gen: u64,
    pub render_gen: u64,
    pub last_focus_gen: u64,
}

impl Default for Canopy {
    fn default() -> Self {
        Self::new()
    }
}

impl Canopy {
    pub fn new() -> Self {
        Canopy {
            focus_gen: 1,
            render_gen: 1,
            last_focus_gen: 1,
        }
    }

    /// Should the node render in the next sweep? This checks if the node is
    /// tainted, if the focus of the node has changed, or if the node's
    /// Node::should_render method is active.
    pub fn should_render<S>(&mut self, e: &mut dyn Node<S>) -> bool {
        if let Some(r) = e.should_render(self) {
            r
        } else {
            self.is_tainted(e) || self.focus_changed(e)
        }
    }

    /// Is this node render tainted?
    pub fn is_tainted<S>(&self, e: &mut dyn Node<S>) -> bool {
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
    pub fn focus_changed<S>(&self, e: &mut dyn Node<S>) -> bool {
        let s = e.state();
        if s.is_focused(self) {
            if s.focus_gen != s.rendered_focus_gen {
                return true;
            }
        } else if s.rendered_focus_gen == self.last_focus_gen {
            return true;
        }
        false
    }

    /// Focus the specified node.
    pub fn set_focus<S>(&mut self, e: &mut dyn Node<S>) -> Result<EventResult> {
        if e.can_focus() {
            self.focus_gen += 1;
            e.state().focus_gen = self.focus_gen;
            return Ok(EventResult::Handle { skip: false });
        }
        Err(format_err!("node does not accept focus"))
    }

    fn focus_dir<S>(&mut self, e: &mut dyn Node<S>, dir: Direction) -> Result<EventResult> {
        let mut seen = false;
        if let Some(bounds) = e.rect() {
            if let Some(start) = self.get_focus_area(e) {
                start.search(dir, &mut |p| -> Result<bool> {
                    if !bounds.contains_point(p) {
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
        }
        Ok(EventResult::Handle { skip: false })
    }

    /// Move focus to the right of the currently focused node within the subtree.
    pub fn focus_right<S>(&mut self, e: &mut dyn Node<S>) -> Result<EventResult> {
        self.focus_dir(e, Direction::Right)
    }

    /// Move focus to the left of the currently focused node within the subtree.
    pub fn focus_left<S>(&mut self, e: &mut dyn Node<S>) -> Result<EventResult> {
        self.focus_dir(e, Direction::Left)
    }

    /// Move focus upward of the currently focused node within the subtree.
    pub fn focus_up<S>(&mut self, e: &mut dyn Node<S>) -> Result<EventResult> {
        self.focus_dir(e, Direction::Up)
    }

    /// Move focus downward of the currently focused node within the subtree.
    pub fn focus_down<S>(&mut self, e: &mut dyn Node<S>) -> Result<EventResult> {
        self.focus_dir(e, Direction::Down)
    }

    /// Focus the first node that accepts focus in the pre-order traversal of
    /// the subtree.
    pub fn focus_first<S>(&mut self, e: &mut dyn Node<S>) -> Result<EventResult> {
        let mut focus_set = false;
        preorder(e, &mut |x| -> Result<SkipWalker> {
            Ok(if !focus_set && x.can_focus() {
                self.set_focus(x)?;
                focus_set = true;
                SkipWalker { skip: true }
            } else {
                SkipWalker::default()
            })
        })?;
        Ok(EventResult::Handle { skip: false })
    }

    /// Does the node have terminal focus?
    pub fn is_focused<S>(&self, e: &mut dyn Node<S>) -> bool {
        e.state().is_focused(self)
    }

    /// A node is on the focus path if it or any of its descendants have focus.
    pub fn on_focus_path<S>(&self, e: &mut dyn Node<S>) -> bool {
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
    pub fn is_focus_ancestor<S>(&self, e: &mut dyn Node<S>) -> bool {
        if e.state().is_focused(self) {
            false
        } else {
            let mut onpath = false;
            self.focus_path(e, &mut |_| -> Result<()> {
                onpath = true;
                Ok(())
            })
            .unwrap();
            onpath
        }
    }

    /// Focus the next node in the pre-order traversal of a node. If no node
    /// with focus is found, we focus the first node we can find instead.
    pub fn focus_next<S>(&mut self, e: &mut dyn Node<S>) -> Result<EventResult> {
        let mut focus_set = false;
        let mut focus_seen = false;
        preorder(e, &mut |x| -> Result<()> {
            if !focus_set {
                if focus_seen {
                    if x.can_focus() {
                        self.set_focus(x)?;
                        focus_set = true;
                    }
                } else if x.state().is_focused(self) {
                    focus_seen = true;
                }
            }
            Ok(())
        })?;
        if !focus_set {
            self.focus_first(e)
        } else {
            Ok(EventResult::Handle { skip: false })
        }
    }

    /// Focus the previous node in the pre-order traversal of a node. If no
    /// node with focus is found, we focus the first node we can find instead.
    pub fn focus_prev<S>(&mut self, e: &mut dyn Node<S>) -> Result<EventResult> {
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
        Ok(EventResult::Handle { skip: false })
    }

    /// Find the area of the current focus, if any.
    pub fn get_focus_area<S>(&self, e: &mut dyn Node<S>) -> Option<Rect> {
        let mut ret = None;
        self.focus_path(e, &mut |x| -> Result<()> {
            if ret == None {
                ret = x.rect();
            }
            Ok(())
        })
        .unwrap();
        ret
    }

    // Calls a closure on the currently focused node and all its parents to the
    // root.
    pub fn focus_path<S, R: Joiner + Default>(
        &self,
        e: &mut dyn Node<S>,
        f: &mut dyn FnMut(&mut dyn Node<S>) -> Result<R>,
    ) -> Result<R> {
        let mut focus_seen = false;
        let mut ret = R::default();
        postorder(e, &mut |x| -> Result<SkipWalker> {
            Ok(if focus_seen {
                ret = ret.join(f(x)?);
                SkipWalker::default()
            } else if self.is_focused(x) {
                focus_seen = true;
                ret = ret.join(f(x)?);
                SkipWalker { skip: true }
            } else {
                SkipWalker::default()
            })
        })?;
        Ok(ret)
    }
    /// Returns the focal depth of the specified node. If the node is not part
    /// of the focus chain, the depth is 0. If the node is a leaf focus, the
    /// depth is 1.
    pub fn focus_depth<S>(&self, e: &mut dyn Node<S>) -> usize {
        let mut total = 0;
        self.focus_path(e, &mut |_| -> Result<()> {
            total += 1;
            Ok(())
        })
        .unwrap();
        total
    }

    /// Mark a tree of nodes for render.
    pub fn taint_tree<S>(&self, e: &mut dyn Node<S>) -> Result<()> {
        let v = postorder(e, &mut |x| -> Result<()> {
            let r = x.state();
            r.render_gen = self.render_gen;
            Ok(())
        })?;
        Ok(v)
    }

    /// Mark a single node for render.
    pub fn taint<S>(&self, e: &mut dyn Node<S>) {
        let r = e.state();
        r.render_gen = self.render_gen;
    }

    /// Mark that a node should skip the next render sweep.
    pub fn skip_taint<S>(&self, e: &mut dyn Node<S>) {
        let r = e.state();
        r.render_skip_gen = self.render_gen;
    }

    /// Render a tree of nodes. If force is true, all visible nodes are
    /// rendered, otherwise we check the taint state.
    pub fn render<S>(&mut self, e: &mut dyn Node<S>, w: &mut dyn Write) -> Result<()> {
        let r = preorder(e, &mut |x| -> Result<()> {
            if self.should_render(x) {
                if self.is_focused(x) {
                    let s = &mut x.state();
                    s.rendered_focus_gen = self.focus_gen
                }
                x.render(self, w)
            } else {
                Ok(())
            }
        })?;
        self.render_gen += 1;
        self.last_focus_gen = self.focus_gen;
        Ok(r)
    }

    /// Propagate a mouse event through the node under the event and all its
    /// ancestors. Events are handled only once, and then ignored.
    pub fn mouse<S>(
        &mut self,
        root: &mut dyn Node<S>,
        s: &mut S,
        m: mouse::Mouse,
    ) -> Result<EventResult> {
        let mut handled = false;
        locate(root, m.loc, &mut |x| {
            Ok(if handled {
                EventResult::default()
            } else {
                let m = mouse::Mouse {
                    action: m.action,
                    button: m.button,
                    modifiers: m.modifiers,
                    loc: x.rect().unwrap().rebase(m.loc)?,
                };
                match x.handle_mouse(self, s, m)? {
                    EventResult::Ignore { skip } => {
                        if skip {
                            handled = true;
                        }
                        EventResult::Ignore { skip: false }
                    }
                    EventResult::Handle { .. } => {
                        self.taint(x);
                        handled = true;
                        EventResult::Handle { skip: false }
                    }
                    itm => itm,
                }
            })
        })
    }

    /// Propagate a key event through the focus and all its ancestors. Keys handled
    /// only once, and then ignored.
    pub fn key<S>(
        &mut self,
        root: &mut dyn Node<S>,
        s: &mut S,
        k: key::Key,
    ) -> Result<EventResult> {
        let mut handled = false;
        self.clone()
            .focus_path(root, &mut |x| -> Result<EventResult> {
                Ok(if handled {
                    EventResult::default()
                } else {
                    match x.handle_key(self, s, k)? {
                        EventResult::Ignore { skip } => {
                            if skip {
                                handled = true;
                            }
                            EventResult::Ignore { skip: false }
                        }
                        EventResult::Handle { .. } => {
                            self.taint(x);
                            handled = true;
                            EventResult::Handle { skip: false }
                        }
                        itm => itm,
                    }
                })
            })
    }

    /// Propagate a resize event through the tree of nodes.
    pub fn resize<S, N>(&mut self, e: &mut N, rect: Rect) -> Result<()>
    where
        N: Node<S> + FixedLayout,
    {
        if let Some(old) = e.rect() {
            if old == rect {
                return Ok(());
            }
        }
        e.layout(self, Some(rect))?;
        self.taint_tree(e)?;
        Ok(())
    }

    /// Propagate a tick event through the tree.
    pub fn tick<S>(&mut self, root: &mut dyn Node<S>, s: &mut S, t: Tick) -> Result<EventResult> {
        let mut ret = EventResult::default();
        preorder(root, &mut |x| -> Result<SkipWalker> {
            let v = x.handle_tick(self, s, t)?;
            ret = ret.join(v);
            Ok(match v {
                EventResult::Handle { skip } => {
                    self.taint(x);
                    if skip {
                        SkipWalker { skip: true }
                    } else {
                        SkipWalker { skip: false }
                    }
                }
                EventResult::Ignore { skip } => {
                    if skip {
                        SkipWalker { skip: true }
                    } else {
                        SkipWalker { skip: false }
                    }
                }
                EventResult::Exit => SkipWalker { skip: true },
            })
        })?;
        Ok(ret)
    }

    /// Propagate an event through the tree.
    pub fn event<S, N>(&mut self, root: &mut N, s: &mut S, e: Event) -> Result<EventResult>
    where
        N: Node<S> + FixedLayout,
    {
        match e {
            Event::Key(k) => self.key(root, s, k),
            Event::Mouse(m) => self.mouse(root, s, m),
            Event::Resize(r) => {
                self.resize(root, r)?;
                Ok(EventResult::Handle { skip: false })
            }
            Event::Tick() => self.tick(root, s, Tick {}),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::geom::Point;
    use crate::tutils::utils;
    use crate::*;
    use anyhow::Result;

    pub fn focvec(app: &mut Canopy, root: &mut utils::TRoot) -> Result<Vec<String>> {
        let mut v = vec![];
        app.clone().focus_path(root, &mut |x| -> Result<()> {
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

        assert!(!root.state().is_focused(&app));
        app.focus_next(&mut root)?;
        assert!(root.state().is_focused(&app));

        app.focus_next(&mut root)?;
        assert!(root.a.state().is_focused(&app));
        assert!(app.is_focus_ancestor(&mut root));
        assert!(!app.is_focus_ancestor(&mut root.a));

        app.focus_next(&mut root)?;
        assert!(root.a.a.state().is_focused(&app));
        assert!(app.is_focus_ancestor(&mut root.a));
        app.focus_next(&mut root)?;
        assert!(root.a.b.state().is_focused(&app));
        assert!(app.is_focus_ancestor(&mut root.a));
        app.focus_next(&mut root)?;
        assert!(root.b.state().is_focused(&app));

        app.set_focus(&mut root.b.b)?;
        assert!(app.is_focus_ancestor(&mut root.b));
        app.focus_next(&mut root)?;
        assert!(root.state().is_focused(&app));

        Ok(())
    }

    #[test]
    fn tfocus_prev() -> Result<()> {
        let mut app = Canopy::new();
        let mut root = utils::TRoot::new();

        assert!(!root.state().is_focused(&app));
        app.focus_prev(&mut root)?;
        assert!(root.b.b.state().is_focused(&app));

        app.focus_prev(&mut root)?;
        assert!(root.b.a.state().is_focused(&app));

        app.focus_prev(&mut root)?;
        assert!(root.b.state().is_focused(&app));

        app.set_focus(&mut root)?;
        app.focus_prev(&mut root)?;
        assert!(root.b.b.state().is_focused(&app));

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
        assert!(root.b.a.state().is_focused(&app));
        // app.focus_right(&mut root)?;
        // assert!(root.b.a.state().is_focused(&app));

        // app.set_focus(&mut root.a.b)?;
        // app.focus_right(&mut root)?;
        // assert!(root.b.b.state().is_focused(&app));
        // app.focus_right(&mut root)?;
        // assert!(root.b.b.state().is_focused(&app));

        Ok(())
    }

    #[test]
    fn ttick() -> Result<()> {
        let mut app = Canopy::new();
        let mut root = utils::TRoot::new();

        let handled = EventResult::Handle { skip: false };
        let ignore = EventResult::Ignore { skip: false };

        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.next_event = Some(handled);
        assert_eq!(app.tick(&mut root, &mut s, app::Tick {})?, handled);
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
        root.a.next_event = Some(EventResult::Ignore { skip: true });
        assert_eq!(app.tick(&mut root, &mut s, app::Tick {})?, ignore);
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
        root.a.next_event = Some(EventResult::Ignore { skip: true });
        root.b.next_event = Some(EventResult::Handle { skip: true });
        assert_eq!(app.tick(&mut root, &mut s, app::Tick {})?, handled);
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

        let handled = EventResult::Handle { skip: false };

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
            EventResult::Ignore { skip: false }
        );

        let mut s = utils::State::new();
        app.set_focus(&mut root.a.b)?;
        root.a.next_event = Some(EventResult::Ignore { skip: true });
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

        let acted = EventResult::Handle { skip: false };

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
            Some(Rect {
                tl: Point { x: 0, y: 0 },
                w: SIZE,
                h: SIZE
            })
        );
        assert_eq!(
            root.a.rect(),
            Some(Rect {
                tl: Point { x: 0, y: 0 },
                w: SIZE / 2,
                h: SIZE
            })
        );
        assert_eq!(
            root.b.rect(),
            Some(Rect {
                tl: Point { x: SIZE / 2, y: 0 },
                w: SIZE / 2,
                h: SIZE
            })
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
            Some(Rect {
                tl: Point { x: 25, y: 0 },
                w: 25,
                h: 50
            })
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

        let handled = EventResult::Handle { skip: false };

        let mut s = utils::State::new();
        app.set_focus(&mut root)?;
        root.next_event = Some(handled);
        root.a.a.next_event = Some(handled);
        root.b.b.next_event = Some(handled);
        app.skip_taint(&mut root.a.a);
        assert_eq!(app.tick(&mut root, &mut s, app::Tick {})?, handled);
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
