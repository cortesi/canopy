use std::time::Duration;

use crate::{
    cursor,
    event::{key, mouse},
    geom::{Expanse, Frame, Rect},
    CommandNode, Core, Render, Result, StatefulNode, ViewPort,
};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Outcome {
    Handle,
    Ignore,
}

/// Walk is the return value from traversal closures.
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Walk<T> {
    /// Skip this node and continue walking. The meaning of Skip depends on the
    /// traversal function being used.
    Skip,
    /// Handle an event with a possible return value and stop walking.
    Handle(T),
    /// Continue walking, but don't mark the event as handled.
    Continue,
}

impl<T> Walk<T> {
    /// The handle value of the traversal, if any.
    pub fn value(self) -> Option<T> {
        match self {
            Walk::Handle(v) => Some(v),
            _ => None,
        }
    }
    /// Did the traversal return Handle?
    pub fn is_handled(&self) -> bool {
        match self {
            Walk::Handle(_) => true,
            _ => false,
        }
    }
    /// Did the traversal return Continue?
    pub fn is_continue(&self) -> bool {
        match self {
            Walk::Skip | Walk::Handle(_) => false,
            Walk::Continue => true,
        }
    }
}

/// Nodes are the basic building-blocks of a Canopy UI. They are composed in a
/// tree, with each node responsible for managing its own children.
#[allow(unused_variables)]
pub trait Node: StatefulNode + CommandNode {
    /// Force the node to render in the next sweep. Over-riding this method
    /// should only be needed rarely, for instance when a container node (e.g. a
    /// frame) needs to redraw if a child node changes.
    fn force_render(&self, c: &dyn Core) -> bool {
        false
    }

    /// Called for each node on the focus path, after each render sweep. The
    /// first node that returns a ``cursor::Cursor`` specification controls the
    /// cursor. If no node returns a cursor, cursor display is disabled.
    fn cursor(&self) -> Option<cursor::Cursor> {
        None
    }

    /// Attempt to focus this node. If the node accepts focus, it should return
    /// true, and if not return false. The default implementation returns false.
    fn accept_focus(&mut self) -> bool {
        false
    }

    /// Handle a key input event. This event is only called for nodes that are
    /// on the focus path. The default implementation ignores input.
    fn handle_key(&mut self, c: &mut dyn Core, k: key::Key) -> Result<Outcome> {
        Ok(Outcome::Ignore)
    }

    /// Handle a mouse input event. The default implementation ignores mouse
    /// input.
    fn handle_mouse(&mut self, c: &mut dyn Core, k: mouse::MouseEvent) -> Result<Outcome> {
        Ok(Outcome::Ignore)
    }

    /// Call a closure on this node's children. If any child handler returns an
    /// error, processing terminates without visiting the remaining children.
    /// The order in which nodes are processed should match intuitive
    /// next/previous relationships. The default implementation assumes this
    /// node has no children, and just returns.
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        Ok(())
    }

    /// Compute the outer size of the node if it had to be displayed in the
    /// target area. In practice, nodes will usually either constrain themselves
    /// based on the width or the height of the target area, or neither, but not
    /// both. The resulting size may be smaller or larger than the target. If
    /// non-trivial computation is done to compute the size (e.g. reflowing
    /// text), it should be cached for use by future calls. This method may be
    /// called multiple times for a given node during a render sweep, so
    /// re-fitting to the same size should be cheap and return consistent
    /// results. This function should not change the node's viewport parameters
    /// itself.
    ///
    /// The default implementation just returns the target value.
    fn fit(&mut self, target: Expanse) -> Result<Expanse> {
        Ok(target)
    }

    /// The scheduled poll endpoint. This function is called for every node the
    /// first time it is seen during the pre-render sweep. Each time the
    /// function returns a duration, a subsequent call is scheduled. If the
    /// function returns None, the `poll` function is never called again. The
    /// default implementation returns `None`.
    fn poll(&mut self, c: &mut dyn Core) -> Option<Duration> {
        None
    }

    /// Render this widget. The render method should:
    ///
    /// - Lay out any child nodes by manipulating their viewports. This will
    ///   often involve calling the `fit` method on the child nodes to get their
    ///   dimensions.
    /// - Render itself to screen. This node's viewport will already have been
    ///   set by a parent.
    ///
    /// The default implementation does nothing.
    fn render(&mut self, c: &dyn Core, r: &mut Render) -> Result<()> {
        Ok(())
    }
}

/// Adjust a node so that it fits a viewport. This fits the node to the
/// viewport's virtual size, then adjusts the node's view to place as much of it
/// within the viewport's screen rectangle as possible.
pub fn fit(n: &mut dyn Node, parent_vp: ViewPort) -> Result<()> {
    let fit = n.fit(parent_vp.size())?;
    n.set_viewport(n.vp().update(fit, parent_vp.screen_rect()));
    Ok(())
}

/// Adjust a node so that viewport's screen rectangle frames it with a given
/// margin. Fits the child to the viewport screen rect minus the border margin,
/// then adjusts the node's view to place as much of of it on screen as
/// possible. This function returns a `Frame` object that can be used to draw a
/// border around the node.
pub fn frame(n: &mut dyn Node, parent_vp: ViewPort, border: u16) -> Result<Frame> {
    let fit = n.fit(parent_vp.screen_rect().inner(border).into())?;
    let screen = parent_vp.screen_rect().inner(border);
    n.update_viewport(&|vp| vp.update(fit, screen));
    // Return a frame for drawing the screen boundary, but in the view
    // rect's co-ordinate system.
    Ok(Frame::new(
        parent_vp.screen_rect().at(parent_vp.view_rect().tl),
        border,
    ))
}

/// Place a node in a given screen rectangle. This fits the node to the
/// region, and updates its viewport.
pub fn place(n: &mut dyn Node, screen: Rect) -> Result<()> {
    let fit = n.fit(screen.expanse())?;
    n.update_viewport(&|vp| vp.update(fit, screen));
    Ok(())
}

/// A postorder traversal of the nodes under e.
///
/// - Walk::Skip causes stops further traversal of children, and all the nodes
/// in a path back to the root are visited.
/// - Walk::Handle stops the traversal and the contained value is returned.
/// - Any error return stops the traversal and the error is returned.
pub fn postorder<T>(
    e: &mut dyn Node,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<Walk<T>>,
) -> Result<Walk<T>> {
    let mut stop = None;
    e.children(&mut |x| {
        if stop.is_none() {
            let v = postorder(x, f)?;
            if !v.is_continue() {
                stop = Some(v)
            }
        }
        Ok(())
    })?;
    match stop {
        None => f(e),
        Some(v) => match v {
            Walk::Skip => {
                let v = f(e)?;
                if v.is_continue() {
                    Ok(Walk::Skip)
                } else {
                    Ok(v)
                }
            }
            Walk::Handle(t) => Ok(Walk::Handle(t)),
            _ => panic!("impossible"),
        },
    }
}

// A preorder traversal of the nodes under e.
///
/// - Walk::Skip prunes all children of the current node from the traversal.
/// - Walk::Handle stops the traversal and the contained value is returned.
/// - Any error return stops the traversal and the error is returned.
pub fn preorder<T>(
    e: &mut dyn Node,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<Walk<T>>,
) -> Result<Walk<T>> {
    let mut res = f(e)?;
    if res.is_continue() {
        e.children(&mut |x| {
            if res.is_continue() {
                match preorder(x, f)? {
                    Walk::Skip => panic!("impossible"),
                    Walk::Continue => {}
                    Walk::Handle(t) => res = Walk::Handle(t),
                };
            }
            Ok(())
        })?;
    }
    // Skip is not propagated upwards, so we translate it to continue.
    Ok(match res {
        Walk::Skip => Walk::Continue,
        _ => res,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        geom::{Expanse, Rect},
        tutils::*,
        Error,
    };

    /// Tiny helper to turn arrays into owned String vecs to ease comparison.
    fn vc(a: &[&str]) -> Vec<String> {
        a.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn tpreorder() -> Result<()> {
        fn trigger(name: &str, func: Result<Walk<()>>) -> (Vec<String>, Result<Walk<()>>) {
            let mut v: Vec<String> = vec![];
            let mut root = R::new();
            let res = preorder(&mut root, &mut |x| -> Result<Walk<()>> {
                v.push(x.name().to_string());
                if x.name() == name {
                    func.clone()
                } else {
                    Ok(Walk::Continue)
                }
            });
            (v, res)
        }

        assert_eq!(
            trigger("never", Ok(Walk::Skip)),
            (
                vc(&["r", "ba", "ba_la", "ba_lb", "bb", "bb_la", "bb_lb"]),
                Ok(Walk::Continue)
            )
        );

        // Skip
        assert_eq!(
            trigger("ba", Ok(Walk::Skip)),
            (vc(&["r", "ba", "bb", "bb_la", "bb_lb"]), Ok(Walk::Continue))
        );
        assert_eq!(
            trigger("r", Ok(Walk::Skip)),
            (vc(&["r"]), Ok(Walk::Continue))
        );

        // Handle
        assert_eq!(
            trigger("ba", Ok(Walk::Handle(()))),
            (vc(&["r", "ba"]), Ok(Walk::Handle(())))
        );
        assert_eq!(
            trigger("ba_la", Ok(Walk::Handle(()))),
            (vc(&["r", "ba", "ba_la"]), Ok(Walk::Handle(())))
        );

        // Error
        assert_eq!(
            trigger("ba_la".into(), Err(Error::NoResult)),
            (vc(&["r", "ba", "ba_la"]), Err(Error::NoResult))
        );
        assert_eq!(
            trigger("r".into(), Err(Error::NoResult)),
            (vc(&["r"]), Err(Error::NoResult))
        );

        Ok(())
    }

    #[test]
    fn tpostorder() -> Result<()> {
        fn trigger(name: &str, func: Result<Walk<()>>) -> (Vec<String>, Result<Walk<()>>) {
            let mut v: Vec<String> = vec![];
            let mut root = R::new();
            let res = postorder(&mut root, &mut |x| -> Result<Walk<()>> {
                v.push(x.name().to_string());
                if x.name() == name {
                    func.clone()
                } else {
                    Ok(Walk::Continue)
                }
            });
            (v, res)
        }

        // Skip
        assert_eq!(
            trigger("ba_la", Ok(Walk::Skip)),
            (vc(&["ba_la", "ba", "r"]), Ok(Walk::Skip))
        );

        assert_eq!(
            trigger("ba_lb", Ok(Walk::Skip)),
            (vc(&["ba_la", "ba_lb", "ba", "r"]), Ok(Walk::Skip))
        );
        assert_eq!(
            trigger("r", Ok(Walk::Skip)),
            (
                vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb", "r"]),
                Ok(Walk::Skip)
            )
        );
        assert_eq!(
            trigger("bb", Ok(Walk::Skip)),
            (
                vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb", "r"]),
                Ok(Walk::Skip)
            )
        );
        assert_eq!(
            trigger("ba", Ok(Walk::Skip)),
            (vc(&["ba_la", "ba_lb", "ba", "r"]), Ok(Walk::Skip))
        );

        // Handle
        assert_eq!(
            trigger("ba_la".into(), Ok(Walk::Handle(()))),
            (vc(&["ba_la"]), Ok(Walk::Handle(())))
        );
        assert_eq!(
            trigger("bb".into(), Ok(Walk::Handle(()))),
            (
                vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb"]),
                Ok(Walk::Handle(()))
            )
        );

        // Error
        assert_eq!(
            trigger("ba_la".into(), Err(Error::NoResult)),
            (vc(&["ba_la"]), Err(Error::NoResult))
        );
        assert_eq!(
            trigger("bb".into(), Err(Error::NoResult)),
            (
                vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb"]),
                Err(Error::NoResult)
            )
        );

        Ok(())
    }

    #[test]
    fn node_fit() -> Result<()> {
        // If the child is the same size as the parent, then wrap just produces
        // the same viewport
        let mut n = TFixed::new(10, 10);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        fit(&mut n, vp)?;
        assert_eq!(n.state().viewport, vp);

        // If the child is smaller than parent, then wrap places the viewport at
        // (0, 0)
        let mut n = TFixed::new(5, 5);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        let expected = ViewPort::new(Expanse::new(5, 5), Rect::new(0, 0, 5, 5), (10, 10))?;
        fit(&mut n, vp)?;
        assert_eq!(n.state().viewport, expected,);
        n.update_viewport(&|vp| vp.right().down());
        assert_eq!(n.state().viewport, expected,);

        // If the child is larger than parent, then wrap places the viewport at
        // (0, 0).
        let mut n = TFixed::new(20, 20);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        fit(&mut n, vp)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(0, 0, 10, 10), (10, 10))?
        );
        // The child can shift its view freely
        n.update_viewport(&|x| x.right().down());
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(1, 1, 10, 10), (10, 10))?
        );
        // And subsequent wraps maintain the child view position, if possible
        fit(&mut n, vp)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(1, 1, 10, 10), (10, 10))?
        );
        // When the parent viewport shrinks, we maintain position and resize
        let shrink = ViewPort::new(Expanse::new(3, 3), Rect::new(0, 0, 2, 2), (10, 10))?;
        fit(&mut n, shrink)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(1, 1, 2, 2), (10, 10))?
        );

        Ok(())
    }

    #[test]
    fn node_frame() -> Result<()> {
        // If we have room, the adjustment just shifts the child node relative to the screen.
        let mut n = TFixed::new(5, 5);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        frame(&mut n, vp, 1)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(5, 5), Rect::new(0, 0, 5, 5), (11, 11))?
        );

        // If if the child node is too large, it is clipped to the bottom and left
        let mut n = TFixed::new(10, 10);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        frame(&mut n, vp, 1)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 8, 8), (11, 11))?
        );

        // If if the parent is smaller than the frame would require, we get a zero view
        let mut n = TFixed::new(10, 10);
        let vp = ViewPort::new(Expanse::new(0, 0), Rect::new(0, 0, 0, 0), (10, 10))?;
        frame(&mut n, vp, 1)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 0, 0), (0, 0))?
        );

        Ok(())
    }
}
