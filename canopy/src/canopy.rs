use duplicate::duplicate_item;

use crate::geom::{Direction, Rect};
use crate::{
    control::BackendControl,
    event::{key, mouse, Event},
    geom::{Point, Size},
    global::STATE,
    node::{postorder, postorder_mut, preorder, Node, Walker},
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
                if !seen && x.handle_focus()?.is_handled() {
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
        Ok(if !focus_set && x.handle_focus()?.is_handled() {
            focus_set = true;
            SkipWalker::new(true)
        } else {
            SkipWalker::new(false)
        })
    })?;
    Ok(Outcome::handle())
}

/// A node is on the focus path if it or any of its descendants have focus.
pub fn on_focus_path(e: &dyn Node) -> bool {
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
pub fn is_focus_ancestor(e: &dyn Node) -> bool {
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
                if x.handle_focus()?.is_handled() {
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
                x.handle_focus()?.is_handled();
            }
        }
        Ok(())
    })?;
    Ok(Outcome::handle())
}

/// Find the area of the current terminal focus node.
pub fn get_focus_area(e: &dyn Node) -> Option<Rect> {
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
pub fn focus_depth(e: &dyn Node) -> usize {
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
pub(crate) fn pre_render(r: &mut Render, e: &mut dyn Node) -> Result<()> {
    let mut seen = false;
    focus_path(e, &mut |_| -> Result<()> {
        seen = true;
        Ok(())
    })?;
    if !seen {
        focus_first(e)?;
    }
    // The cursor is disabled before every render sweep, otherwise we would
    // see it visibly on screen during redraws.
    r.hide_cursor()?;
    Ok(())
}

/// Post-render sweep of the tree.
pub(crate) fn post_render(r: &mut Render, e: &dyn Node) -> Result<()> {
    let mut seen = false;
    focus_path(e, &mut |n| -> Result<()> {
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

fn render_traversal(r: &mut Render, e: &mut dyn Node) -> Result<()> {
    if !e.is_hidden() {
        r.push();
        if e.should_render() {
            if e.is_focused() {
                let s = &mut e.state_mut();
                s.rendered_focus_gen =
                    STATE.with(|global_state| -> u64 { global_state.borrow().focus_gen });
            }
            r.viewport = e.state().viewport;
            e.render(r, e.state().viewport)?;
        }
        // This is a new node - we don't want it perpetually stuck in
        // render, so we need to update its render_gen.
        if e.state().render_gen == 0 {
            e.state_mut().render_gen =
                STATE.with(|global_state| -> u64 { global_state.borrow().render_gen });
        }
        e.children_mut(&mut |x| render_traversal(r, x))?;
        r.pop();
    }
    Ok(())
}

/// Render a tree of nodes. If force is true, all visible nodes are
/// rendered, otherwise we check the taint state. Hidden nodes and their
/// children are ignored.
pub fn render(r: &mut Render, e: &mut dyn Node) -> Result<()> {
    r.reset()?;
    render_traversal(r, e)?;
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
    focus_path_mut(root, &mut move |x| -> Result<Outcome> {
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
pub fn set_root_size<N>(size: Size, n: &mut N) -> Result<()>
where
    N: Node,
{
    let fit = n.fit(size)?;
    let vp = ViewPort::new(fit, fit, Point::default())?;
    n.set_viewport(vp);
    taint_tree(n)?;
    Ok(())
}

/// Propagate an event through the tree.
pub fn event<N>(ctrl: &mut dyn BackendControl, root: &mut N, e: Event) -> Result<Outcome>
where
    N: Node,
{
    match e {
        Event::Key(k) => key(ctrl, root, k),
        Event::Mouse(m) => mouse(ctrl, root, m),
        Event::Resize(s) => {
            set_root_size(s, root)?;
            Ok(Outcome::handle())
        }
    }
}

/// Calls a closure on the currently focused node and all its parents to the
/// root.
#[duplicate_item(
        method              reference(type)    traversal;
        [focus_path]        [& type]           [postorder];
        [focus_path_mut]    [&mut type]        [postorder_mut];
    )]
fn method<R: Walker + Default>(
    e: reference([dyn Node]),
    f: &mut dyn FnMut(reference([dyn Node])) -> Result<R>,
) -> Result<R> {
    let mut focus_seen = false;
    let mut ret = R::default();
    let focus_gen = STATE.with(|global_state| -> u64 { global_state.borrow().focus_gen });
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
pub fn locate<R: Walker + Default>(
    e: &mut dyn Node,
    p: impl Into<Point>,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<R>,
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

/// Mark a tree of nodes for render.
pub fn taint_tree(e: &mut dyn Node) -> Result<()> {
    postorder_mut(e, &mut |x| -> Result<()> {
        x.taint();
        Ok(())
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::test::{TestBuf, TestRender},
        geom::Rect,
        outcome::{Handle, Ignore},
        tutils::utils::*,
        StatefulNode,
    };
    use std::sync::{Arc, Mutex};

    pub fn focvec(root: &mut TRoot) -> Result<Vec<String>> {
        let mut v = vec![];
        focus_path_mut(root, &mut |x| -> Result<()> {
            let n = x.name().unwrap();
            v.push(n);
            Ok(())
        })?;
        Ok(v)
    }

    fn run_test(
        func: impl FnOnce(Arc<Mutex<TestBuf>>, Render, &mut dyn BackendControl, TRoot) -> Result<()>,
    ) -> Result<()> {
        let (buf, mut tr) = TestRender::create();
        let (r, mut c) = tcanopy(&mut tr);
        let mut root = TRoot::new();
        set_root_size(Size::new(100, 100), &mut root)?;
        reset_state();
        func(buf, r, &mut c, root)
    }

    #[test]
    fn tfocus_next() -> Result<()> {
        run_test(|_, _, _, mut root| {
            assert!(!root.is_focused());
            focus_next(&mut root)?;
            assert!(root.is_focused());

            focus_next(&mut root)?;
            assert!(root.a.is_focused());
            assert!(is_focus_ancestor(&root));
            assert!(!is_focus_ancestor(&root.a));

            focus_next(&mut root)?;
            assert!(root.a.a.is_focused());
            assert!(is_focus_ancestor(&root.a));
            focus_next(&mut root)?;
            assert!(root.a.b.is_focused());
            assert!(is_focus_ancestor(&root.a));
            focus_next(&mut root)?;
            assert!(root.b.is_focused());

            root.b.b.set_focus();
            assert!(is_focus_ancestor(&root.b));
            focus_next(&mut root)?;
            assert!(root.is_focused());
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tfocus_prev() -> Result<()> {
        run_test(|_, _, _, mut root| {
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
        run_test(|_, _, _, mut root| {
            assert_eq!(focvec(&mut root)?.len(), 0);

            assert!(!on_focus_path(&root));
            assert!(!on_focus_path(&root.a));

            root.a.a.set_focus();
            assert!(on_focus_path(&root));
            assert!(on_focus_path(&root.a));
            assert!(!on_focus_path(&root.b));

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
        run_test(|_, mut r, _, mut root| {
            render(&mut r, &mut root)?;
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
        run_test(|_, _, ctrl, mut root| {
            root.set_focus();
            root.next_outcome = Some(Outcome::handle());
            assert!(key(ctrl, &mut root, K_ANY)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["r@key->handle"]);
            Ok(())
        })?;

        run_test(|_, _, ctrl, mut root| {
            root.a.a.set_focus();
            root.a.a.next_outcome = Some(Outcome::handle());
            assert!(key(ctrl, &mut root, K_ANY)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba:la@key->handle"]);
            Ok(())
        })?;

        run_test(|_, _, ctrl, mut root| {
            root.a.a.set_focus();
            root.a.next_outcome = Some(Outcome::handle());
            assert!(key(ctrl, &mut root, K_ANY)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba:la@key->ignore", "ba@key->handle"]);
            Ok(())
        })?;

        run_test(|_, _, ctrl, mut root| {
            root.a.a.set_focus();
            root.next_outcome = Some(Outcome::handle());
            assert!(key(ctrl, &mut root, K_ANY)?.is_handled());
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba:la@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run_test(|_, _, ctrl, mut root| {
            root.a.set_focus();
            root.a.next_outcome = Some(Outcome::handle());
            assert!(key(ctrl, &mut root, K_ANY)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->handle"]);
            Ok(())
        })?;

        run_test(|_, _, ctrl, mut root| {
            root.a.set_focus();
            root.next_outcome = Some(Outcome::handle());
            assert!(key(ctrl, &mut root, K_ANY)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->ignore", "r@key->handle"]);
            assert_eq!(key(ctrl, &mut root, K_ANY)?, Outcome::ignore());
            Ok(())
        })?;

        run_test(|_, _, ctrl, mut root| {
            root.a.b.set_focus();
            root.a.next_outcome = Some(Outcome::Ignore(Ignore::default().with_skip()));
            root.next_outcome = Some(Outcome::handle());
            key(ctrl, &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba:lb@key->ignore", "ba@key->ignore"]);
            Ok(())
        })?;

        run_test(|_, _, ctrl, mut root| {
            root.a.a.set_focus();
            root.a.a.next_outcome = Some(Outcome::handle());
            key(ctrl, &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba:la@key->handle",]);
            Ok(())
        })?;

        run_test(|_, _, ctrl, mut root| {
            root.a.b.set_focus();
            root.a.next_outcome = Some(Outcome::handle());
            key(ctrl, &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba:lb@key->ignore", "ba@key->handle",]);
            Ok(())
        })?;

        run_test(|_, _, ctrl, mut root| {
            root.a.b.set_focus();
            root.a.b.next_outcome = Some(Outcome::Handle(Handle::default()));
            key(ctrl, &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba:lb@key->handle",]);
            Ok(())
        })?;

        run_test(|_, _, ctrl, mut root| {
            root.a.b.set_focus();
            root.a.b.next_outcome = Some(Outcome::handle());
            root.a.next_outcome = Some(Outcome::handle());
            key(ctrl, &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba:lb@key->handle",]);
            Ok(())
        })?;

        run_test(|_, _, ctrl, mut root| {
            root.a.b.set_focus();
            root.a.b.next_outcome = Some(Outcome::handle());
            root.a.next_outcome = Some(Outcome::ignore_and_skip());
            key(ctrl, &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba:lb@key->handle"]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tmouse() -> Result<()> {
        run_test(|_, mut r, ctrl, mut root| {
            root.set_focus();
            root.next_outcome = Some(Outcome::handle());
            let evt = root.a.a.make_mouse_event()?;
            render(&mut r, &mut root)?;
            assert!(mouse(ctrl, &mut root, evt)?.is_handled());
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba:la@mouse->ignore", "ba@mouse->ignore", "r@mouse->handle"]
            );
            Ok(())
        })?;

        run_test(|_, mut r, ctrl, mut root| {
            root.a.a.next_outcome = Some(Outcome::handle());
            let evt = root.a.a.make_mouse_event()?;
            render(&mut r, &mut root)?;
            assert!(mouse(ctrl, &mut root, evt)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba:la@mouse->handle"]);
            Ok(())
        })?;

        run_test(|_, mut r, ctrl, mut root| {
            root.a.a.next_outcome = Some(Outcome::handle());
            let evt = root.a.a.make_mouse_event()?;
            render(&mut r, &mut root)?;
            assert!(mouse(ctrl, &mut root, evt)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba:la@mouse->handle"]);
            Ok(())
        })?;

        run_test(|_, mut r, ctrl, mut root| {
            root.a.a.next_outcome = Some(Outcome::handle());
            let evt = root.a.a.make_mouse_event()?;
            render(&mut r, &mut root)?;
            assert!(mouse(ctrl, &mut root, evt)?.is_handled());
            let s = get_state();
            assert_eq!(s.path, vec!["ba:la@mouse->handle",]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tresize() -> Result<()> {
        run_test(|_, mut r, _, mut root| {
            let size = 100;
            assert_eq!(root.vp().screen_rect(), Rect::new(0, 0, size, size));
            render(&mut r, &mut root)?;
            assert_eq!(root.a.vp().screen_rect(), Rect::new(0, 0, size / 2, size));
            assert_eq!(
                root.b.vp().screen_rect(),
                Rect::new(size / 2, 0, size / 2, size)
            );

            set_root_size(Size::new(50, 50), &mut root)?;
            render(&mut r, &mut root)?;
            assert_eq!(root.b.vp().screen_rect(), Rect::new(25, 0, 25, 50));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn trender() -> Result<()> {
        run_test(|buf, mut r, _, mut root| {
            render(&mut r, &mut root)?;
            assert_eq!(
                buf.lock()?.text,
                vec!["<r>", "<ba>", "<ba:la>", "<ba:lb>", "<bb>", "<bb:la>", "<bb:lb>"]
            );

            render(&mut r, &mut root)?;
            assert!(buf.lock()?.is_empty());

            root.a.taint();
            render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<ba>"]);

            root.a.b.taint();
            render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<ba:lb>"]);

            taint_tree(&mut root.a)?;
            render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<ba>", "<ba:la>", "<ba:lb>"]);

            render(&mut r, &mut root)?;
            assert!(buf.lock()?.text.is_empty());

            root.a.a.set_focus();
            render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<ba:la>"]);

            focus_next(&mut root)?;
            render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<ba:la>", "<ba:lb>"]);

            focus_prev(&mut root)?;
            render(&mut r, &mut root)?;
            assert_eq!(buf.lock()?.text, vec!["<ba:la>", "<ba:lb>"]);

            render(&mut r, &mut root)?;
            assert!(buf.lock()?.text.is_empty());

            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn ttaintskip() -> Result<()> {
        run_test(|buf, mut r, _, mut root| {
            render(&mut r, &mut root)?;
            root.set_focus();
            taint_tree(&mut root)?;
            root.a.skip_taint();
            render(&mut r, &mut root)?;

            assert_eq!(
                buf.lock()?.text,
                vec!["<r>", "<ba:la>", "<ba:lb>", "<bb>", "<bb:la>", "<bb:lb>"]
            );
            Ok(())
        })?;

        Ok(())
    }
}
