//! Helper functions for `Node::layout` implementations.

use crate::{
    Node, Result, ViewPort,
    geom::{Expanse, Rect},
};

/// The Layout struct provides operations that a node can perform on children during its layout
/// phase.
pub struct Layout {}

impl Layout {
    /// Hides the element and all its descendants from rendering. The nodes are still included in
    /// the tree.
    pub fn hide(&self, child: &mut dyn Node) {
        child.state_mut().hidden = true;
    }

    /// Unhides the element and all its descendants, allowing them to be rendered again.
    pub fn unhide(&self, child: &mut dyn Node) {
        child.state_mut().hidden = false;
    }

    /// Fill a node to occupy the given size, resetting its view to start at (0,0).
    /// This is typically used for root nodes or nodes that should always show
    /// their content from the top-left corner.
    pub fn fill(&self, n: &mut dyn Node, sz: Expanse) -> Result<()> {
        n.state_mut().set_canvas(sz);
        n.state_mut().set_view(sz.rect());
        Ok(())
    }

    /// Lay the child out and place it in a given sub-rectangle of a parent's canvas.
    pub fn place(&self, child: &mut dyn Node, loc: Rect) -> Result<()> {
        child.layout(self, loc.into())?;
        child.state_mut().set_position(loc.tl);
        Ok(())
    }

    /// Set the view rectangle for a node.
    pub fn set_view(&self, node: &mut dyn Node, view: Rect) {
        node.state_mut().set_view(view);
    }

    /// Adjust a child node so that it fits a viewport. This lays the node out to the parent's view
    /// rectangle, then adjusts the node's position to match.
    // FIXME: this shoudl just take a rect
    pub fn fit(&self, n: &mut dyn Node, parent_vp: ViewPort) -> Result<()> {
        n.layout(self, parent_vp.view().into())?;
        n.state_mut().set_position(parent_vp.position());
        Ok(())
    }
}
