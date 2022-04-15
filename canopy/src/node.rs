use std::time::Duration;

use crate::{
    cursor,
    event::{key, mouse},
    geom::{Expanse, Frame, Rect},
    BackendControl, Outcome, Render, Result, StatefulNode, ViewPort,
};

/// Walker is implemented for the return values of tree operations.
pub trait Walker {
    /// Join this item with another instance, returning a new value. This is
    /// done to accumulate return values returned from node operations.
    fn join(&self, rhs: Self) -> Self;
    /// If skip is true, we skip further node processing and return.
    fn skip(&self) -> bool;
}

impl Walker for () {
    fn join(&self, _: Self) -> Self {}
    fn skip(&self) -> bool {
        false
    }
}

/// Nodes are the basic building-blocks of a Canopy UI. They are composed in a
/// tree, with each node responsible for managing its own children.
#[allow(unused_variables)]
pub trait Node: StatefulNode {
    /// The name of this node, if it has one, for debugging and testing.
    fn name(&self) -> Option<String> {
        None
    }

    /// Should the node render in the next sweep? This checks if the node is
    /// currently hidden, and if not, checks if the node is tainted or its focus
    /// status has changed. Over-riding this method should only be needed
    /// rarely, for instance when a container node needs to redraw if a sub-node
    /// changes.
    fn should_render(&self) -> bool {
        !self.is_hidden() && (self.is_tainted() || self.focus_changed())
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
    fn handle_key(&mut self, c: &mut dyn BackendControl, k: key::Key) -> Result<Outcome> {
        Ok(Outcome::ignore())
    }

    /// Handle a mouse input event. The default implementation ignores mouse
    /// input.
    fn handle_mouse(&mut self, c: &mut dyn BackendControl, k: mouse::Mouse) -> Result<Outcome> {
        Ok(Outcome::ignore())
    }

    /// Call a closure mutably on this node's children. The default
    /// implementation assumes this node has no children, and just returns.
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        Ok(())
    }

    /// Compute the outer size of the node, if it had to be displayed in the
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
    fn poll(&mut self) -> Option<Duration> {
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
    /// Nodes with no children should always make sure they redraw all of
    /// `self.screen_area()`. The default implementation does nothing.
    fn render(&mut self, r: &mut Render, vp: ViewPort) -> Result<()> {
        Ok(())
    }

    /// Adjust this node so that the parent wraps it completely. This fits the
    /// node to the parent's virtual node size, then adjusts the node's view to
    /// place as much of it within the paren'ts screen rectangle as possible.
    /// Usually, this method would be used by a node that also passes the
    /// child's fit back through it's own `fit` method.
    fn wrap(&mut self, parent_vp: ViewPort) -> Result<()> {
        let fit = self.fit(parent_vp.size())?;
        self.set_viewport(self.vp().update(fit, parent_vp.screen_rect()));
        Ok(())
    }

    /// Adjust this node so that the parent's screen rectangle frames it with a
    /// given margin. Fits the child to the parent screen rect minus the border
    /// margin, then adjusts the child's view to place as much of of it on
    /// screen as possible.
    fn frame(&mut self, parent_vp: ViewPort, border: u16) -> Result<Frame> {
        let fit = self.fit(parent_vp.screen_rect().inner(border).into())?;
        let screen = parent_vp.screen_rect().inner(border);
        self.update_viewport(&|vp| vp.update(fit, screen));
        // Return a frame for drawing the screen boundary, but in the view
        // rect's co-ordinate system.
        Ok(Frame::new(
            parent_vp.screen_rect().at(parent_vp.view_rect().tl),
            border,
        ))
    }

    /// Place a node in a given screen rectangle. This fits the node to the
    /// region, and updates its viewport.
    fn place(&mut self, screen: Rect) -> Result<()> {
        let fit = self.fit(screen.expanse())?;
        self.update_viewport(&|vp| vp.update(fit, screen));
        Ok(())
    }
}

/// A postorder traversal of the nodes under e. Enabling skipping in the Walker
/// results in all the nodes in a route straight back to the root being visited
/// before exiting.
pub fn postorder<R: Walker + Default>(
    e: &mut dyn Node,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<R>,
) -> Result<R> {
    let mut v = R::default();
    e.children(&mut |x| {
        if !v.skip() {
            v = v.join(postorder(x, f)?);
        }
        Ok(())
    })?;
    Ok(v.join(f(e)?))
}

// A preorder traversal of the nodes under e. Enabling skipping in the walker
// prunes all children of the currently visited node out of the traversal.
pub fn preorder<W: Walker>(
    e: &mut dyn Node,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<W>,
) -> Result<W> {
    let mut v = f(e)?;
    if !v.skip() {
        e.children(&mut |x| {
            v = v.join(preorder(x, f)?);
            Ok(())
        })?;
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canopy::SkipWalker,
        geom::{Expanse, Rect},
        tutils::utils,
    };

    fn skipper(x: &mut dyn Node, skipname: String, v: &mut Vec<String>) -> Result<SkipWalker> {
        let mut ret = SkipWalker::default();
        let n = x.name().unwrap();
        if n == skipname {
            ret.has_skip = true
        }
        v.push(n);
        Ok(ret)
    }

    #[test]
    fn tpostorder() -> Result<()> {
        fn skipon(root: &mut utils::TRoot, skipname: String) -> Result<Vec<String>> {
            let mut v: Vec<String> = vec![];
            postorder(root, &mut |x| -> Result<SkipWalker> {
                skipper(x, skipname.clone(), &mut v)
            })?;
            Ok(v)
        }

        let mut root = utils::TRoot::new();
        assert_eq!(skipon(&mut root, "ba:la".into())?, ["ba:la", "ba", "r"]);
        assert_eq!(
            skipon(&mut root, "ba:lb".into())?,
            ["ba:la", "ba:lb", "ba", "r"]
        );
        assert_eq!(
            skipon(&mut root, "r".into())?,
            ["ba:la", "ba:lb", "ba", "bb:la", "bb:lb", "bb", "r"]
        );
        assert_eq!(
            skipon(&mut root, "bb".into())?,
            ["ba:la", "ba:lb", "ba", "bb:la", "bb:lb", "bb", "r"]
        );
        assert_eq!(
            skipon(&mut root, "ba".into())?,
            ["ba:la", "ba:lb", "ba", "r"]
        );
        Ok(())
    }

    #[test]
    fn tpreorder() -> Result<()> {
        fn skipon(root: &mut utils::TRoot, skipname: String) -> Result<Vec<String>> {
            let mut v = vec![];
            preorder(root, &mut |x| -> Result<SkipWalker> {
                skipper(x, skipname.clone(), &mut v)
            })?;
            Ok(v)
        }

        let mut root = utils::TRoot::new();
        assert_eq!(
            skipon(&mut root, "never".into())?,
            ["r", "ba", "ba:la", "ba:lb", "bb", "bb:la", "bb:lb"]
        );
        assert_eq!(skipon(&mut root, "r".into())?, ["r"]);
        assert_eq!(
            skipon(&mut root, "ba".into())?,
            ["r", "ba", "bb", "bb:la", "bb:lb"]
        );
        assert_eq!(
            skipon(&mut root, "bb".into())?,
            ["r", "ba", "ba:la", "ba:lb", "bb"]
        );
        Ok(())
    }

    #[test]
    fn node_wrap() -> Result<()> {
        // If the child is the same size as the parent, then wrap just produces
        // the same viewport
        let mut n = utils::TFixed::new(10, 10);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        n.wrap(vp)?;
        assert_eq!(n.state().viewport, vp);

        // If the child is smaller than parent, then wrap places the viewport at
        // (0, 0)
        let mut n = utils::TFixed::new(5, 5);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        let expected = ViewPort::new(Expanse::new(5, 5), Rect::new(0, 0, 5, 5), (10, 10))?;
        n.wrap(vp)?;
        assert_eq!(n.state().viewport, expected,);
        n.update_viewport(&|vp| vp.right().down());
        assert_eq!(n.state().viewport, expected,);

        // If the child is larger than parent, then wrap places the viewport at
        // (0, 0).
        let mut n = utils::TFixed::new(20, 20);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        n.wrap(vp)?;
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
        n.wrap(vp)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(1, 1, 10, 10), (10, 10))?
        );
        // When the parent viewport shrinks, we maintain position and resize
        let shrink = ViewPort::new(Expanse::new(3, 3), Rect::new(0, 0, 2, 2), (10, 10))?;
        n.wrap(shrink)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(1, 1, 2, 2), (10, 10))?
        );

        Ok(())
    }

    #[test]
    fn node_frame() -> Result<()> {
        // If we have room, the adjustment just shifts the child node relative to the screen.
        let mut n = utils::TFixed::new(5, 5);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        n.frame(vp, 1)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(5, 5), Rect::new(0, 0, 5, 5), (11, 11))?
        );

        // If if the child node is too large, it is clipped to the bottom and left
        let mut n = utils::TFixed::new(10, 10);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        n.frame(vp, 1)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 8, 8), (11, 11))?
        );

        // If if the parent is smaller than the frame would require, we get a zero view
        let mut n = utils::TFixed::new(10, 10);
        let vp = ViewPort::new(Expanse::new(0, 0), Rect::new(0, 0, 0, 0), (10, 10))?;
        n.frame(vp, 1)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 0, 0), (0, 0))?
        );

        Ok(())
    }
}
