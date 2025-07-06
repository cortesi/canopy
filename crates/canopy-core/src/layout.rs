//! Helper functions for `Node::layout` implementations.

use crate::{Node, Result, geom::Rect};

/// The Layout struct provides operations that a node can perform on children during its layout
/// phase.
pub struct Layout {}

impl Layout {
    /// Lay the child out and place it in a given sub-rectangle of a parent's canvas.
    pub fn place(&self, child: &mut dyn Node, loc: Rect) -> Result<()> {
        child.layout(self, loc.into())?;
        child.state_mut().viewport.set_position(loc.tl);
        Ok(())
    }
}
