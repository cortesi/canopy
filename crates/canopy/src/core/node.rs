/// This is the UI the interface that all nodes must implement in Canopy.
///
/// Some explanation is necessary to justify the split between .layout() and .render(). Why not
/// just combine these two methods into a single .draw() method? The reason is that nodes need to
/// be able to account for the size of their children when laying themselves out. Imagine, for
/// instance, a text container that wraps text and resizes itself to fit. The parent needs to be
/// aware of the resulting size BEFORE it can decide where to place the child and render itself.
/// Our solution is to split rendering and layout. During the layout phase, no drawing is done, but
/// all nodes have their size and position determined. We do this by asking each node what their
/// size would be IF they had to draw within a given target area. The result can be larger or
/// smaller than the target, but gives the node the opportunity to do things like reflow text with
/// a width constraint, and then tell the parent how much space it needs. During the render phase,
/// size and position are fixed, and nodes only draw themselves to the screen.
///
/// Another key aspect of the design is that all rendering and layout happens with respect to the
/// node's own canvas. Nodes don't have to (and can't) know where they are being drawn on the
/// physical screen.
use std::time::Duration;

use crate::{
    Context, Layout,
    commands::CommandNode,
    cursor,
    error::Result,
    event::{key, mouse},
    geom::Expanse,
    render::Render,
    state::StatefulNode,
};

/// The result of an event handler.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EventOutcome {
    /// The event was processed and the node should be rendered.
    Handle,
    /// The event was processed, but nothing changed so rendering is skipped and propagation stops.
    Consume,
    /// The event was not handled and will bubble up the tree.
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

    /// Re-compute the size and view of the node to display in the target area. In practice, nodes
    /// will either constrain themselves based on the width or the height of the target area, or
    /// neither, but not both. The resulting size may be smaller or larger than the target. If
    /// non-trivial computation is done (e.g. reflowing text), it should be cached for use by
    /// future calls.
    fn layout(&mut self, l: &Layout, target: Expanse) -> Result<()> {
        Ok(())
    }

    /// Render this widget, only drawing itself with reference to its own canvas.
    fn render(&mut self, c: &dyn Context, r: &mut Render) -> Result<()> {
        Ok(())
    }
}
