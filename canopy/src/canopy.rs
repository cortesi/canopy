use crate::{
    control::BackendControl,
    event::{key, mouse, Event},
    focus,
    geom::{Coverage, Expanse, Point},
    global::{self, STATE},
    node::{postorder, preorder, Node, Walk},
    render::{show_cursor, RenderBackend},
    style::StyleManager,
    NodeId, Outcome, Render, Result, ViewPort,
};

/// Pre-render sweep of the tree.
pub(crate) fn pre_render<R: RenderBackend>(r: &mut R, e: &mut dyn Node) -> Result<()> {
    let mut seen = false;
    preorder(e, &mut |x| -> Result<Walk<()>> {
        if x.is_focused() {
            seen = true;
        }
        if !x.is_initialized() {
            if let Some(d) = x.poll() {
                STATE.with(|global_state| global_state.borrow_mut().poller.schedule(x.id(), d));
            }
            x.state_mut().initialized = true;
        }
        Ok(Walk::Continue)
    })?;
    if !seen {
        focus::shift_first(e)?;
    }

    if global::focus_changed() {
        let fg = STATE.with(|global_state| global_state.borrow().focus_gen);
        focus::walk(e, &mut |n| -> Result<Walk<()>> {
            n.state_mut().focus_path_gen = fg;
            Ok(Walk::Continue)
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
    focus::walk(e, &mut |n| -> Result<Walk<()>> {
        Ok(if let Some(c) = n.cursor() {
            show_cursor(r, styl, n.vp(), "cursor", c)?;
            Walk::Handle(())
        } else {
            Walk::Continue
        })
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
pub fn mouse(ctrl: &mut dyn BackendControl, root: &mut dyn Node, m: mouse::Mouse) -> Result<()> {
    locate(root, m.loc, &mut |x| {
        let hdl = x.handle_mouse(
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
                x.taint();
                Walk::Handle(())
            }
            Outcome::Ignore => Walk::Continue,
        })
    })?;
    Ok(())
}

/// Propagate a key event through the focus and all its ancestors.
pub fn key(ctrl: &mut dyn BackendControl, root: &mut dyn Node, k: key::Key) -> Result<()> {
    focus::walk(root, &mut move |x| -> Result<Walk<()>> {
        Ok(match x.handle_key(ctrl, k)? {
            Outcome::Handle => {
                x.taint();
                Walk::Handle(())
            }
            Outcome::Ignore => Walk::Continue,
        })
    })?;
    Ok(())
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
fn poll(ids: Vec<u64>, root: &mut dyn Node) -> Result<()> {
    preorder(root, &mut |x| -> Result<Walk<()>> {
        if ids.contains(&x.id()) {
            if let Some(d) = x.poll() {
                STATE.with(|global_state| global_state.borrow_mut().poller.schedule(x.id(), d));
            }
        };
        Ok(Walk::Continue)
    })?;
    Ok(())
}

/// Propagate an event through the tree.
pub(crate) fn event(ctrl: &mut dyn BackendControl, root: &mut dyn Node, e: Event) -> Result<()> {
    match e {
        Event::Key(k) => {
            key(ctrl, root, k)?;
        }
        Event::Mouse(m) => {
            mouse(ctrl, root, m)?;
        }
        Event::Resize(s) => {
            set_root_size(s, root)?;
        }
        Event::Poll(ids) => {
            poll(ids, root)?;
        }
    };
    Ok(())
}

/// Calls a closure on the leaf node under (x, y), then all its parents to the
/// root.
pub fn locate<R>(
    e: &mut dyn Node,
    p: impl Into<Point>,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<Walk<R>>,
) -> Result<Walk<R>> {
    let mut seen = false;
    let p = p.into();
    postorder(e, &mut |inner| -> Result<Walk<R>> {
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

/// Mark a tree of nodes for render.
pub fn taint_tree(e: &mut dyn Node) {
    postorder(e, &mut |x| -> Result<Walk<()>> {
        x.taint();
        Ok(Walk::Continue)
    })
    // Unwrap is safe, because no operations in the closure can fail.
    .unwrap();
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
    "/".to_string() + &path.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backend::test::TestRender, geom::Rect, tutils::utils::*, StatefulNode};

    fn run_test(func: impl FnOnce(TestRender, TRoot) -> Result<()>) -> Result<()> {
        let (_, tr) = TestRender::create();
        let mut root = TRoot::new();
        set_root_size(Expanse::new(100, 100), &mut root)?;
        reset_state();
        func(tr, root)
    }

    #[test]
    fn tkey() -> Result<()> {
        run_test(|tr, mut root| {
            root.set_focus();
            root.next_outcome = Some(Outcome::Handle);
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["r@key->handle"]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.a.set_focus();
            root.a.a.next_outcome = Some(Outcome::Handle);
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->handle"]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.a.set_focus();
            root.a.next_outcome = Some(Outcome::Handle);
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->ignore", "ba@key->handle"]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.a.set_focus();
            root.next_outcome = Some(Outcome::Handle);
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_la@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.set_focus();
            root.a.next_outcome = Some(Outcome::Handle);
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->handle"]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.set_focus();
            root.next_outcome = Some(Outcome::Handle);
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba@key->ignore", "r@key->handle"]);
            key(&mut tr.control(), &mut root, K_ANY)?;
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

        run_test(|tr, mut root| {
            root.a.b.set_focus();
            root.a.next_outcome = Some(Outcome::Ignore);
            root.next_outcome = Some(Outcome::Handle);
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_lb@key->ignore", "ba@key->ignore", "r@key->handle"]
            );
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.a.set_focus();
            root.a.a.next_outcome = Some(Outcome::Handle);
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@key->handle",]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.b.set_focus();
            root.a.next_outcome = Some(Outcome::Handle);
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->ignore", "ba@key->handle",]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.b.set_focus();
            root.a.b.next_outcome = Some(Outcome::Handle);
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->handle",]);
            Ok(())
        })?;

        run_test(|tr, mut root| {
            root.a.b.set_focus();
            root.a.b.next_outcome = Some(Outcome::Handle);
            root.a.next_outcome = Some(Outcome::Handle);
            key(&mut tr.control(), &mut root, K_ANY)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_lb@key->handle",]);
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn tnode_path() -> Result<()> {
        run_test(|_, mut root| {
            println!("HEREA: {}", node_path(root.a.a.id(), &mut root));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn tmouse() -> Result<()> {
        run_test(|mut tr, mut root| {
            root.set_focus();
            root.next_outcome = Some(Outcome::Handle);
            let evt = root.a.a.make_mouse_event()?;
            tr.render(&mut root)?;
            mouse(&mut tr.control(), &mut root, evt)?;
            let s = get_state();
            assert_eq!(
                s.path,
                vec!["ba_la@mouse->ignore", "ba@mouse->ignore", "r@mouse->handle"]
            );
            Ok(())
        })?;

        run_test(|mut tr, mut root| {
            root.a.a.next_outcome = Some(Outcome::Handle);
            let evt = root.a.a.make_mouse_event()?;
            tr.render(&mut root)?;
            mouse(&mut tr.control(), &mut root, evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
            Ok(())
        })?;

        run_test(|mut tr, mut root| {
            root.a.a.next_outcome = Some(Outcome::Handle);
            let evt = root.a.a.make_mouse_event()?;
            tr.render(&mut root)?;
            mouse(&mut tr.control(), &mut root, evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
            Ok(())
        })?;

        run_test(|mut tr, mut root| {
            root.a.a.next_outcome = Some(Outcome::Handle);
            let evt = root.a.a.make_mouse_event()?;
            tr.render(&mut root)?;
            mouse(&mut tr.control(), &mut root, evt)?;
            let s = get_state();
            assert_eq!(s.path, vec!["ba_la@mouse->handle",]);
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
                vec!["<r>", "<ba>", "<ba_la>", "<ba_lb>", "<bb>", "<bb_la>", "<bb_lb>"]
            );

            tr.render(&mut root)?;
            assert!(tr.buf_empty());

            root.a.taint();
            tr.render(&mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba>"]);

            root.a.b.taint();
            tr.render(&mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba_lb>"]);

            taint_tree(&mut root.a);
            tr.render(&mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba>", "<ba_la>", "<ba_lb>"]);

            tr.render(&mut root)?;
            assert!(tr.buf_empty());

            root.a.a.set_focus();
            tr.render(&mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba_la>"]);

            focus::shift_next(&mut root)?;
            tr.render(&mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba_la>", "<ba_lb>"]);

            focus::shift_prev(&mut root)?;
            tr.render(&mut root)?;
            assert_eq!(tr.buf_text(), vec!["<ba_la>", "<ba_lb>"]);

            tr.render(&mut root)?;
            assert!(tr.buf_empty());

            Ok(())
        })?;

        Ok(())
    }
}
