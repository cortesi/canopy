use std::time::Duration;

use crate::{
    commands::CommandNode,
    cursor,
    event::{key, mouse},
    geom::Expanse,
    state::StatefulNode,
    Context, Layout, Render, Result,
};

/// Outcome returned by event handlers.
///
/// * `Handle` - The event was handled and the node should be tainted so a render
///   sweep will occur.
/// * `Consume` - The event was handled but did not change any visual state. The
///   event should not be propagated further and the node is **not** tainted.
/// * `Ignore` - The event was ignored and should be propagated to ancestors.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EventOutcome {
    Handle,
    Consume,
    Ignore,
}


#[allow(unused_variables)]
/// Nodes are the basic building-blocks of a Canopy UI. They are composed in a tree, with each node responsible for
/// managing its own children.
pub trait Node: StatefulNode + CommandNode {
    /// Force the node to render in the next sweep. Over-riding this method should only be needed rarely, for instance
    /// when a container node (e.g. a frame) needs to redraw if a child node changes.
    fn force_render(&self, c: &dyn Context) -> bool {
        false
    }

    /// Called for each node on the focus path, after each render sweep. The first node that returns a
    /// ``cursor::Cursor`` specification controls the cursor. If no node returns a cursor, cursor display is disabled.
    fn cursor(&self) -> Option<cursor::Cursor> {
        None
    }

    /// Attempt to focus this node. If the node accepts focus, it should return true, and if not return false. The
    /// default implementation returns false.
    fn accept_focus(&mut self) -> bool {
        false
    }

    /// Call a closure on this node's children. If any child handler returns an error, processing terminates without
    /// visiting the remaining children. The order in which nodes are processed should match intuitive next/previous
    /// relationships. The default implementation assumes this node has no children, and just returns.
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        Ok(())
    }

    /// Re-compute the size and view of the node if it had to be displayed in the target area. In practice, nodes will
    /// usually either constrain themselves based on the width or the height of the target area, or neither, but not
    /// both. The resulting size may be smaller or larger than the target. If non-trivial computation is done to compute
    /// the size (e.g. reflowing text), it should be cached for use by future calls. This method may be called multiple
    /// times for a given node during a render sweep, so re-fitting to the same size should be cheap and return
    /// consistent results.
    fn layout(&mut self, l: &Layout, target: Expanse) -> Result<()> {
        Ok(())
    }

    /// Handle a key input event. This event is only called for nodes that are on the focus path. The default
    /// implementation ignores input.
    fn handle_key(&mut self, c: &mut dyn Context, k: key::Key) -> Result<EventOutcome> {
        Ok(EventOutcome::Ignore)
    }

    /// Handle a mouse input event. The default implementation ignores mouse input.
    fn handle_mouse(&mut self, c: &mut dyn Context, k: mouse::MouseEvent) -> Result<EventOutcome> {
        Ok(EventOutcome::Ignore)
    }

    /// The scheduled poll endpoint. This function is called for every node the first time it is seen during the
    /// pre-render sweep. Each time the function returns a duration, a subsequent call is scheduled. If the function
    /// returns None, the `poll` function is never called again. The default implementation returns `None`.
    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        None
    }

    /// Render this widget. The render method should:
    ///
    /// - Lay out any child nodes by manipulating their viewports. This will often involve calling the `fit` method on
    ///   the child nodes to get their dimensions.
    /// - Render itself to screen. This node's viewport will already have been set by a parent.
    ///
    /// The default implementation does nothing.
    fn render(&mut self, c: &dyn Context, r: &mut Render) -> Result<()> {
        Ok(())
    }
}
