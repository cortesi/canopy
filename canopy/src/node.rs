use crate::{
    cursor,
    event::{key, mouse},
    geom::{Frame, Size},
    Actions, Canopy, Outcome, Result, StatefulNode, ViewPort,
};
use duplicate::duplicate;

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

/// Nodes are the basic building-blocks of a Canopy UI. Nodes are composed in a
/// tree structure, with each node responsible for managing its own children.
/// Nodes keep track of the area of the screen that they are responsible for
/// through the resize event.
///
/// The type paramter `S` is the application backing store object that is passed
/// to all events.
#[allow(unused_variables)]
pub trait Node<S, A: Actions>: StatefulNode {
    /// The name of this node, if it has one, for debugging and testing
    /// purposes.
    fn name(&self) -> Option<String> {
        None
    }

    /// Over-ride Canopy's usual render checking. If this function returns
    /// `Some(true)` or `Some(false)`, the response takes precedence over the
    /// taint and focus change checking that usually determines rendering
    /// behaviour. Implementing this method should only be needed in rare
    /// circumstances, like container nodes that need to respond to changes in
    /// sub-nodes. The default implementation returns `None`.
    fn should_render(&self, app: &Canopy<S, A>) -> Option<bool> {
        None
    }

    /// Can this node accept leaf focus? The default implementation returns
    /// `false`.
    fn can_focus(&self) -> bool {
        false
    }

    /// Called for each node on the focus path, after each render sweep. The
    /// first node that returns a ``cursor::Cursor`` specification controls the
    /// cursor. If no node returns a cursor, cursor display is disabled.
    fn cursor(&self) -> Option<cursor::Cursor> {
        None
    }

    /// Handle a key event. This event is only called for nodes that are on the
    /// focus path. The default implementation ignores input.
    fn handle_key(&mut self, app: &mut Canopy<S, A>, s: &mut S, k: key::Key) -> Result<Outcome<A>> {
        Ok(Outcome::ignore())
    }

    /// Handle a mouse event.The default implementation ignores mouse input.
    fn handle_mouse(
        &mut self,
        app: &mut Canopy<S, A>,
        s: &mut S,
        k: mouse::Mouse,
    ) -> Result<Outcome<A>> {
        Ok(Outcome::ignore())
    }

    /// Handle a an action returned by an event handler on one of our
    /// descendents.
    fn handle_event_action(
        &mut self,
        app: &mut Canopy<S, A>,
        s: &mut S,
        k: A,
    ) -> Result<Outcome<A>> {
        Ok(Outcome::ignore())
    }

    /// Handle a broadcast action.
    fn handle_broadcast(&mut self, app: &mut Canopy<S, A>, s: &mut S, k: A) -> Result<Outcome<A>> {
        Ok(Outcome::ignore())
    }

    /// Call a closure on this node's children. The order in which children are
    /// processed must match `children_mut`. The default implementation assumes
    /// this node has no children, and just returns.
    fn children(&self, f: &mut dyn FnMut(&dyn Node<S, A>) -> Result<()>) -> Result<()> {
        Ok(())
    }

    /// Call a closure mutably on this node's children. The order in which
    /// children are processed must match `children`. The default implementation
    /// assumes this node has no children, and just returns.
    fn children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Node<S, A>) -> Result<()>) -> Result<()> {
        Ok(())
    }

    /// Compute the outer size of the node, if it had to be displayed in the
    /// target area. In practice, nodes will usually either constrain themselves
    /// based on the width or the height of the target area, or neither, but not
    /// both. The resulting size may be smaller or larger than the target. If
    /// non-trivial computation is done to compute the size (e.g. reflowing
    /// text), it should be saved for use by future calls. This method may be
    /// called multiple times for a given node during a render sweep, so
    /// re-fitting to the same size should be cheap and return consistent
    /// results. This function should not change the node's viewport parameters
    /// itself.
    ///
    /// The default implementation just returns the target value.
    fn fit(&mut self, app: &mut Canopy<S, A>, target: Size) -> Result<Size> {
        Ok(target)
    }

    /// Render this widget. The render method should:
    ///
    /// - Lay out any child nodes by manipulating their viewports. This will
    ///   often involve calling the .fit method on the child nodes to get their
    ///   dimensions.
    /// - Render itself to screen. This node's viewport will already have been
    ///   set by a parent.
    ///
    /// Nodes with no children should always make sure they redraw all of
    /// `self.screen_area()`. The default implementation does nothing.
    fn render(&mut self, app: &mut Canopy<S, A>, vp: ViewPort) -> Result<()> {
        Ok(())
    }

    /// Adjust this node so that the specified parent viewport wraps it. Fits
    /// this node to the parent viewport, then adjusts its view to place as much
    /// of of it on screen as possible. Usually, this method would be used by a
    /// node that also passes the child's fit back through it's own `fit`
    /// method.
    fn wrap(&mut self, app: &mut Canopy<S, A>, parent_vp: ViewPort) -> Result<()> {
        let fit = self.fit(app, parent_vp.size())?;
        self.set_viewport(self.vp().update(fit, parent_vp.screen_rect()));
        Ok(())
    }

    /// Adjust this node so that the parent's screen rectangle frames it with a
    /// given margin. Fits the child to the parent screen rect minus the border
    /// margin, then adjusts the child's view to place as much of of it on
    /// screen as possible.
    fn frame(&mut self, app: &mut Canopy<S, A>, parent_vp: ViewPort, border: u16) -> Result<Frame> {
        let fit = self.fit(app, parent_vp.screen_rect().inner(border).into())?;
        let screen = parent_vp.screen_rect().inner(border);
        self.update_viewport(&|vp| vp.update(fit, screen));
        // Return a frame for drawing the screen boundary, but in the view
        // rect's co-ordinate system.
        Frame::new(parent_vp.screen_rect().at(parent_vp.view_rect().tl), border)
    }
}

/// A postorder traversal of the nodes under e. Enabling skipping in the Walker
/// results in all the nodes in a route straight back to the root being visited
/// before exiting.
#[duplicate(
    method             reference(type)  children;
    [postorder]        [& type]         [children];
    [postorder_mut]    [&mut type]      [children_mut];
)]
pub fn method<S, A: Actions, R: Walker + Default>(
    e: reference([dyn Node<S, A>]),
    f: &mut dyn FnMut(reference([dyn Node<S, A>])) -> Result<R>,
) -> Result<R> {
    let mut v = R::default();
    e.children(&mut |x| {
        if !v.skip() {
            v = v.join(method(x, f)?);
        }
        Ok(())
    })?;
    Ok(v.join(f(e)?))
}

// A preorder traversal of the nodes under e. Enabling skipping in the walker
// prunes all children of the currently visited node out of the traversal.
pub fn preorder<S, A: Actions, W: Walker>(
    e: &mut dyn Node<S, A>,
    f: &mut dyn FnMut(&mut dyn Node<S, A>) -> Result<W>,
) -> Result<W> {
    let mut v = f(e)?;
    if !v.skip() {
        e.children_mut(&mut |x| {
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
        geom::{Rect, Size},
        render::test::TestRender,
        tutils::utils,
    };

    fn skipper(
        x: &mut dyn Node<utils::State, utils::TActions>,
        skipname: String,
        v: &mut Vec<String>,
    ) -> Result<SkipWalker> {
        let mut ret = SkipWalker::default();
        let n = x.name().unwrap();
        if n == skipname {
            ret.has_skip = true
        }
        v.push(n.into());
        Ok(ret)
    }

    #[test]
    fn tpostorder() -> Result<()> {
        fn skipon(root: &mut utils::TRoot, skipname: String) -> Result<Vec<String>> {
            let mut v: Vec<String> = vec![];
            postorder_mut(root, &mut |x| -> Result<SkipWalker> {
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
    fn twrap() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);

        // If the child is the same size as the parent, then wrap just produces
        // the same viewport
        let mut n = utils::TFixed::new(10, 10);
        let vp = ViewPort::new(Size::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        n.wrap(&mut app, vp)?;
        assert_eq!(n.state().viewport, vp);

        // If the child is smaller than parent, then wrap places the viewport at
        // (0, 0)
        let mut n = utils::TFixed::new(5, 5);
        let vp = ViewPort::new(Size::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        let expected = ViewPort::new(Size::new(5, 5), Rect::new(0, 0, 5, 5), (10, 10))?;
        n.wrap(&mut app, vp)?;
        assert_eq!(n.state().viewport, expected,);
        n.update_viewport(&|vp| vp.right().down());
        assert_eq!(n.state().viewport, expected,);

        // If the child is larger than parent, then wrap places the viewport at
        // (0, 0).
        let mut n = utils::TFixed::new(20, 20);
        let vp = ViewPort::new(Size::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        n.wrap(&mut app, vp)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Size::new(20, 20), Rect::new(0, 0, 10, 10), (10, 10))?
        );
        // The child can shift its view freely
        n.update_viewport(&|x| x.right().down());
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Size::new(20, 20), Rect::new(1, 1, 10, 10), (10, 10))?
        );
        // And subsequent wraps maintain the child view position, if possible
        n.wrap(&mut app, vp)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Size::new(20, 20), Rect::new(1, 1, 10, 10), (10, 10))?
        );
        // When the parent viewport shrinks, we maintain position and resize
        let shrink = ViewPort::new(Size::new(3, 3), Rect::new(0, 0, 2, 2), (10, 10))?;
        n.wrap(&mut app, shrink)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Size::new(20, 20), Rect::new(1, 1, 2, 2), (10, 10))?
        );

        Ok(())
    }
}
