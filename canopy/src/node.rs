use crate::{
    cursor,
    event::{key, mouse},
    geom::Rect,
    geom::Size,
    Actions, Canopy, Outcome, Result, StatefulNode,
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

    /// Lay out this component's children.
    ///
    /// This method is called after the node is laid out by its parent.
    /// Implementers should call `fit` on all children, and then lay them out by
    /// changing the child's viewport.
    ///
    /// The default does nothing, which is appropriate for nodes that have no
    /// children.
    fn layout(&mut self, app: &mut Canopy<S, A>, screen_rect: Rect) -> Result<()> {
        Ok(())
    }

    /// Render this widget using the geometry that was set through the node's
    /// Layout implementation. Nodes with no children should always make sure
    /// they redraw all of `self.screen_area()`. The default implementation does
    /// nothing.
    fn render(&self, app: &mut Canopy<S, A>) -> Result<()> {
        Ok(())
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
    use crate::{base::SkipWalker, tutils::utils};

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
}
