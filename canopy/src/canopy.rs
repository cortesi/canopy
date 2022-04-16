use crate::geom::{Direction, Rect};
use crate::{
    control::BackendControl,
    event::{key, mouse, Event},
    geom::{Coverage, Expanse, Point},
    global::{self, STATE},
    node::{postorder, preorder, Node, Walker},
    render::{show_cursor, RenderBackend},
    style::StyleManager,
    Outcome, Render, Result, ViewPort,
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

macro_rules! process_event(
    (
        $slf:expr,
        $ctrl:expr,
        $handled:expr,
        $halt:expr,
        $node:expr,
        $proc:expr
    ) => {
        {
            let oc = if *$halt {
                Outcome::default()
            } else if *$handled {
                let hdl = Outcome::default();
                hdl
            } else {
                let hdl = $proc?;
                if hdl.has_skip() {
                    *$halt = true;
                }
                if hdl.is_handled() {
                    $node.taint();
                    *$handled = true;
                }
                hdl.clone()
            };
            Ok(oc)

        }
    };
);

/// Move focus in a specified direction within the subtree.
pub fn focus_dir(e: &mut dyn Node, dir: Direction) -> Result<Outcome> {
    let mut seen = false;
    if let Some(start) = get_focus_area(e) {
        start.search(dir, &mut |p| -> Result<bool> {
            if !e.vp().screen_rect().contains_point(p) {
                return Ok(true);
            }
            locate(e, p, &mut |x| {
                if !seen && x.accept_focus() {
                    x.set_focus();
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
pub fn focus_right(e: &mut dyn Node) -> Result<Outcome> {
    focus_dir(e, Direction::Right)
}

/// Move focus to the left of the currently focused node within the subtree.
pub fn focus_left(e: &mut dyn Node) -> Result<Outcome> {
    focus_dir(e, Direction::Left)
}

/// Move focus upward of the currently focused node within the subtree.
pub fn focus_up(e: &mut dyn Node) -> Result<Outcome> {
    focus_dir(e, Direction::Up)
}

/// Move focus downward of the currently focused node within the subtree.
pub fn focus_down(e: &mut dyn Node) -> Result<Outcome> {
    focus_dir(e, Direction::Down)
}

/// Focus the first node that accepts focus in the pre-order traversal of
/// the subtree.
pub fn focus_first(e: &mut dyn Node) -> Result<Outcome> {
    let mut focus_set = false;
    preorder(e, &mut |x| -> Result<SkipWalker> {
        Ok(if !focus_set && x.accept_focus() {
            x.set_focus();
            focus_set = true;
            SkipWalker::new(true)
        } else {
            SkipWalker::new(false)
        })
    })?;
    Ok(Outcome::handle())
}

/// A node is on the focus path if it or any of its descendants have focus.
pub fn on_focus_path(e: &mut dyn Node) -> bool {
    let mut onpath = false;
    focus_path(e, &mut |_| -> Result<()> {
        onpath = true;
        Ok(())
    })
    // We're safe to unwrap, because our closure can't return an error.
    .unwrap();
    onpath
}

/// A node is on the focus path if it does not have focus itself, but some
/// node below it does.
pub fn is_focus_ancestor(e: &mut dyn Node) -> bool {
    if e.is_focused() {
        false
    } else {
        on_focus_path(e)
    }
}

/// Focus the next node in the pre-order traversal of a node. If no node
/// with focus is found, we focus the first node we can find instead.
pub fn focus_next(e: &mut dyn Node) -> Result<Outcome> {
    let mut focus_set = false;
    let mut focus_seen = false;
    preorder(e, &mut |x| -> Result<()> {
        if !focus_set {
            if focus_seen {
                if x.accept_focus() {
                    x.set_focus();
                    focus_set = true;
                }
            } else if x.is_focused() {
                focus_seen = true;
            }
        }
        Ok(())
    })?;
    if !focus_set {
        focus_first(e)
    } else {
        Ok(Outcome::handle())
    }
}

/// Focus the previous node in the pre-order traversal of a node. If no
/// node with focus is found, we focus the first node we can find instead.
pub fn focus_prev(e: &mut dyn Node) -> Result<Outcome> {
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
                if x.accept_focus() {
                    x.set_focus();
                }
            }
        }
        Ok(())
    })?;
    Ok(Outcome::handle())
}

/// Find the area of the current terminal focus node.
pub fn get_focus_area(e: &mut dyn Node) -> Option<Rect> {
    let mut ret = None;
    focus_path(e, &mut |x| -> Result<()> {
        if ret == None {
            ret = Some(x.vp().screen_rect());
        }
        Ok(())
    })
    // We're safe to unwrap, because our closure can't return an error.
    .unwrap();
    ret
}

/// Returns the focal depth of the specified node. If the node is not part
/// of the focus chain, the depth is 0. If the node is a leaf focus, the
/// depth is 1.
pub fn focus_depth(e: &mut dyn Node) -> usize {
    let mut total = 0;
    focus_path(e, &mut |_| -> Result<()> {
        total += 1;
        Ok(())
    })
    // We're safe to unwrap, because our closure can't return an error.
    .unwrap();
    total
}

/// Pre-render sweep of the tree.
pub(crate) fn pre_render<R: RenderBackend>(r: &mut R, e: &mut dyn Node) -> Result<()> {
    let mut seen = false;
    preorder(e, &mut |x| -> Result<()> {
        if x.is_focused() {
            seen = true;
        }
        if !x.is_initialized() {
            if let Some(d) = x.poll() {
                STATE.with(|global_state| global_state.borrow_mut().poller.schedule(x.id(), d));
            }
            x.state_mut().initialized = true;
        }
        Ok(())
    })?;
    if !seen {
        focus_first(e)?;
    }

    if global::focus_changed() {
        let fg = STATE.with(|global_state| global_state.borrow().focus_gen);
        focus_path(e, &mut |n| -> Result<()> {
            n.state_mut().focus_path_gen = fg;
            Ok(())
        })?;
    }

    // The cursor is disabled before every render sweep, otherwise we would
    // see it visibly on screen during redraws.
    r.hide_cursor()?;
    Ok(())
}

/// Post-render sweep of the tree.
pub(crate) fn post_render<R: RenderBackend>(
    r: &mut R,
    styl: &mut StyleManager,
    e: &mut dyn Node,
) -> Result<()> {
    let mut seen = false;
    focus_path(e, &mut |n| -> Result<()> {
        if !seen {
            if let Some(c) = n.cursor() {
                show_cursor(r, styl, n.vp(), "cursor", c)?;
                seen = true;
            }
        }
        Ok(())
    })?;
    Ok(())
}

fn render_traversal<R: RenderBackend>(
    r: &mut R,
    styl: &mut StyleManager,
    e: &mut dyn Node,
) -> Result<()> {
    if !e.is_hidden() {
        styl.push();
        if e.should_render() {
            if e.is_focused() {
                let s = &mut e.state_mut();
                s.rendered_focus_gen =
                    STATE.with(|global_state| -> u64 { global_state.borrow().focus_gen });
            }

            let mut c = Coverage::new(e.vp().screen_rect().expanse());
            let mut rndr = Render::new(r, styl, e.vp(), &mut c);

            e.render(&mut rndr)?;

            // Now add regions managed by children to coverage
            let escreen = e.vp().screen_rect();
            e.children(&mut |n| {
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
            let sr = e.vp().view_rect();
            for l in rndr.coverage.uncovered() {
                rndr.fill("", l.rect().shift(sr.tl.x as i16, sr.tl.y as i16), ' ')?;
            }
        }
        // This is a new node - we don't want it perpetually stuck in
        // render, so we need to update its render_gen.
        if e.state().render_gen == 0 {
            e.state_mut().render_gen =
                STATE.with(|global_state| -> u64 { global_state.borrow().render_gen });
        }
        e.children(&mut |x| render_traversal(r, styl, x))?;
        styl.pop();
    }
    Ok(())
}

/// Render a tree of nodes. If force is true, all visible nodes are
/// rendered, otherwise we check the taint state. Hidden nodes and their
/// children are ignored.
pub fn render<R: RenderBackend>(
    be: &mut R,
    styl: &mut StyleManager,
    e: &mut dyn Node,
) -> Result<()> {
    be.reset()?;
    styl.reset();
    render_traversal(be, styl, e)?;
    STATE.with(|global_state| {
        let mut gs = global_state.borrow_mut();
        gs.render_gen += 1;
        gs.last_render_focus_gen = gs.focus_gen;
    });
    Ok(())
}

/// Propagate a mouse event through the node under the event and all its
/// ancestors. Events are handled only once, and then ignored.
pub fn mouse(
    ctrl: &mut dyn BackendControl,
    root: &mut dyn Node,
    m: mouse::Mouse,
) -> Result<Outcome> {
    let mut handled = false;
    let mut halt = false;
    locate(root, m.loc, &mut |x| {
        process_event!(
            self,
            ctrl,
            &mut handled,
            &mut halt,
            x,
            x.handle_mouse(
                ctrl,
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
pub fn key(ctrl: &mut dyn BackendControl, root: &mut dyn Node, k: key::Key) -> Result<Outcome> {
    let mut handled = false;
    let mut halt = false;
    focus_path(root, &mut move |x| -> Result<Outcome> {
        process_event!(
            self,
            ctrl,
            &mut handled,
            &mut halt,
            x,
            x.handle_key(ctrl, k)
        )
    })
}

/// Set the size on the root node, and taint the tree.
pub fn set_root_size(size: Expanse, n: &mut dyn Node) -> Result<()> {
    let fit = n.fit(size)?;
    let vp = ViewPort::new(fit, fit, Point::default())?;
    n.set_viewport(vp);
    taint_tree(n);
    Ok(())
}

/// Handle a poll event by traversing the complete node tree, and triggering
/// poll on each ID in the poll set.
fn poll(ids: Vec<u64>, root: &mut dyn Node) -> Result<Outcome> {
    preorder(root, &mut |x| -> Result<SkipWalker> {
        if ids.contains(&x.id()) {
            if let Some(d) = x.poll() {
                STATE.with(|global_state| global_state.borrow_mut().poller.schedule(x.id(), d));
            }
        };
        Ok(SkipWalker::new(false))
    })?;
    Ok(Outcome::handle())
}

/// Propagate an event through the tree.
pub(crate) fn event(
    ctrl: &mut dyn BackendControl,
    root: &mut dyn Node,
    e: Event,
) -> Result<Outcome> {
    match e {
        Event::Key(k) => key(ctrl, root, k),
        Event::Mouse(m) => mouse(ctrl, root, m),
        Event::Resize(s) => {
            set_root_size(s, root)?;
            Ok(Outcome::handle())
        }
        Event::Poll(ids) => poll(ids, root),
    }
}

/// Calls a closure on the currently focused node and all its parents to the
/// root.
fn focus_path<R: Walker + Default>(
    e: &mut dyn Node,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<R>,
) -> Result<R> {
    let mut focus_seen = false;
    let mut ret = R::default();
    let focus_gen = STATE.with(|global_state| -> u64 { global_state.borrow().focus_gen });
    postorder(e, &mut |x| -> Result<SkipWalker> {
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
pub fn locate<R: Walker + Default>(
    e: &mut dyn Node,
    p: impl Into<Point>,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<R>,
) -> Result<R> {
    let mut seen = false;
    let mut ret = R::default();
    let p = p.into();
    postorder(e, &mut |inner| -> Result<SkipWalker> {
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

/// Mark a tree of nodes for render.
pub fn taint_tree(e: &mut dyn Node) {
    postorder(e, &mut |x| -> Result<()> {
        x.taint();
        Ok(())
    })
    // Unwrap is safe, because no operations in the closure can fail.
    .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::test::TestRender,
        geom::Rect,
        outcome::{Handle, Ignore},
        tutils::utils::*,
        StatefulNode,
    };

    pub fn focvec(root: &mut TRoot) -> Result<Vec<String>> {
        let mut v = vec![];
        focus_path(root, &mut |x| -> Result<()> {
            let n = x.name().unwrap();
            v.push(n);
            Ok(())
        })?;
        Ok(v)
    }

    fn run_test(func: impl FnOnce(TestRender, TRoot) -> Result<()>) -> Result<()> {
        let (_, tr) = TestRender::create();
        let mut root = TRoot::new();
        set_root_size(Expanse::new(100, 100), &mut root)?;
        reset_state();
        func(tr, root)
    }

    #[test]
    fn tfocus_next() -> Result<()> {
        run_test(|_, mut root| {
            assert!(!root.is_focused());
            focus_next(&mut root)?;
            assert!(root.is_focused());

            focus_next(&mut root)?;
            assert!(root.a.is_focused());
            assert!(is_focus_ancestor(&mut root));
            assert!(!is_focus_ancestor(&mut root.a));

            focus_next(&mut root)?;
            assert!(root.a.a.is_focused());
            assert!(is_focus_ancestor(&mut root.a));
            focus_next(&mut root)?;
            assert!(root.a.b.is_focused());
            assert!(is_focus_ancestor(&mut root.a));
            focus_next(&mut root)?;
            assert!(root.b.is_focused());

            root.b.b.set_focus();
            assert!(is_focus_ancestor(&mut root.b));
            focus_next(&mut root)?;
            assert!(root.is_focused());
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tfocus_prev() -> Result<()> {
        run_test(|_, mut root| {
            assert!(!root.is_focused());
            focus_prev(&mut root)?;
            assert!(root.b.b.is_focused());

            focus_prev(&mut root)?;
            assert!(root.b.a.is_focused());

            focus_prev(&mut root)?;
            assert!(root.b.is_focused());

            root.set_focus();
            focus_prev(&mut root)?;
            assert!(root.b.b.is_focused());

            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tfoci() -> Result<()> {
        run_test(|_, mut root| {
            assert_eq!(focvec(&mut root)?.len(), 0);

            assert!(!on_focus_path(&mut root));
            assert!(!on_focus_path(&mut root.a));

            root.a.a.set_focus();
            assert!(on_focus_path(&mut root));
            assert!(on_focus_path(&mut root.a));
            assert!(!on_focus_path(&mut root.b));

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
        run_test(|mut tr, mut root| {
            tr.render(&mut root)?;
            root.a.a.set_focus();
            focus_right(&mut root)?;
            assert!(root.b.a.is_focused());
            focus_right(&mut root)?;
            assert!(root.b.a.is_focused());

            root.a.b.set_focus();
            focus_right(&mut root)?;
            assert!(root.b.b.is_focused());
            focus_right(&mut root)?;
            assert!(root.b.b.is_focused());
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tkey() -> Result<()> {
        run_test(|tr, mut root| {
            root.set_focus();
            root.next_outcome = Some(Outcome::handle());
            assert!(key(&mut tr.control(), &mut root, K_ANY)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["r@key->handle"]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.a.set_focus();
            root.a.a.next_outcome = Some(Outcome::handle());
            assert!(key(&mut tr.control(), &mut root, K_ANY)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba:la@key->handle"]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.a.set_focus();
            root.a.next_outcome = Some(Outcome::handle());
            assert!(key(&mut tr.control(), &mut root, K_ANY)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba:la@key->ignore", "ba@key->handle"]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.a.set_focus();
            root.next_outcome = Some(Outcome::handle());
            assert!(key(&mut tr.control(), &mut root, K_ANY)?.is_handled());
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba:la@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.set_focus();
            root.a.next_outcome = Some(Outcome::handle());
            assert!(key(&mut tr.control(), &mut root, K_ANY)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->handle"]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.set_focus();
            root.next_outcome = Some(Outcome::handle());
            assert!(key(&mut tr.control(), &mut root, K_ANY)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->ignore", "r@key->handle"]);
            assert_eq!(key(&mut tr.control(), &mut root, K_ANY)?, Outcome::ignore());
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.b.set_focus();
            root.a.next_outcome = Some(Outcome::Ignore(Ignore::default().with_skip()));
            root.next_outcome = Some(Outcome::handle());
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba:lb@key->ignore", "ba@key->ignore"]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.a.set_focus();
            root.a.a.next_outcome = Some(Outcome::handle());
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba:la@key->handle",]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.b.set_focus();
            root.a.next_outcome = Some(Outcome::handle());
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba:lb@key->ignore", "ba@key->handle",]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.b.set_focus();
            root.a.b.next_outcome = Some(Outcome::Handle(Handle::default()));
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba:lb@key->handle",]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.b.set_focus();
            root.a.b.next_outcome = Some(Outcome::handle());
            root.a.next_outcome = Some(Outcome::handle());
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba:lb@key->handle",]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.b.set_focus();
            root.a.b.next_outcome = Some(Outcome::handle());
            root.a.next_outcome = Some(Outcome::ignore_and_skip());
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba:lb@key->handle"]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tmouse() -> Result<()> {
        run_test(|mut tr, mut root| {
            root.set_focus();
            root.next_outcome = Some(Outcome::handle());
            let evt = root.a.a.make_mouse_event()?;
            tr.render(&mut root)?;
            assert!(mouse(&mut tr.control(), &mut root, evt)?.is_handled());
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba:la@mouse->ignore", "ba@mouse->ignore", "r@mouse->handle"]
            );
            Ok(())
        })?;

        run_test(|mut tr, mut root| {
            root.a.a.next_outcome = Some(Outcome::handle());
            let evt = root.a.a.make_mouse_event()?;
            tr.render(&mut root)?;
            assert!(mouse(&mut tr.control(), &mut root, evt)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba:la@mouse->handle"]);
            Ok(())
        })?;

        run_test(|mut tr, mut root| {
            root.a.a.next_outcome = Some(Outcome::handle());
            let evt = root.a.a.make_mouse_event()?;
            tr.render(&mut root)?;
            assert!(mouse(&mut tr.control(), &mut root, evt)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba:la@mouse->handle"]);
            Ok(())
        })?;

        run_test(|mut tr, mut root| {
            root.a.a.next_outcome = Some(Outcome::handle());
            let evt = root.a.a.make_mouse_event()?;
            tr.render(&mut root)?;
            assert!(mouse(&mut tr.control(), &mut root, evt)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba:la@mouse->handle",]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tresize() -> Result<()> {
        run_test(|mut tr, mut root| {
            let size = 100;
            assert_eq!(root.vp().screen_rect(), Rect::new(0, 0, size, size));
            tr.render(&mut root)?;
            assert_eq!(root.a.vp().screen_rect(), Rect::new(0, 0, size / 2, size));
            assert_eq!(
                root.b.vp().screen_rect(),
                Rect::new(size / 2, 0, size / 2, size)
            );

            set_root_size(Expanse::new(50, 50), &mut root)?;
            tr.render(&mut root)?;
            assert_eq!(root.b.vp().screen_rect(), Rect::new(25, 0, 25, 50));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn trender() -> Result<()> {
        run_test(|mut tr, mut root| {
            tr.render(&mut root)?;
            assert_eq!(
                tr.buf_text(),
                vec!["<r>", "<ba>", "<ba:la>", "<ba:lb>", "<bb>", "<bb:la>", "<bb:lb>"]
            );

            tr.render(&mut root)?;
            assert!(tr.buf_empty());

            root.a.taint();
            tr.render(&mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba>"]);

            root.a.b.taint();
            tr.render(&mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba:lb>"]);

            taint_tree(&mut root.a);
            tr.render(&mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba>", "<ba:la>", "<ba:lb>"]);

            tr.render(&mut root)?;
            assert!(tr.buf_empty());

            root.a.a.set_focus();
            tr.render(&mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba:la>"]);

            focus_next(&mut root)?;
            tr.render(&mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba:la>", "<ba:lb>"]);

            focus_prev(&mut root)?;
            tr.render(&mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba:la>", "<ba:lb>"]);

            tr.render(&mut root)?;
            assert!(tr.buf_empty());

            Ok(())
        })?;

        Ok(())
    }
}
