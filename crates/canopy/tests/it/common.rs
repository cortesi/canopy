//! Helpers shared by more than one integration module.

use canopy::{Canopy, FocusScope, NodeId, error::Result, geom::Direction};

/// Return the name of the focused grid cell, if a cell holds focus.
pub fn focused_cell(canopy: &Canopy) -> Option<String> {
    canopy.with_root_view(|context| {
        let root = context.root_id();
        let focused = context.focused_leaf(root)?;
        let mut path = context.node_path(root, focused);
        path.pop().filter(|name| name.starts_with("cell_"))
    })
}

/// Focus the first focusable node in a subtree.
pub fn focus_first(canopy: &mut Canopy, root: NodeId) -> Result<()> {
    canopy.with_root_context(|context| context.focus_first(FocusScope::Node(root)).map(|_| ()))
}

/// Move focus one step in a direction within a subtree.
pub fn focus_dir(canopy: &mut Canopy, root: NodeId, direction: Direction) -> Result<()> {
    canopy.with_root_context(|context| {
        context
            .focus_dir(FocusScope::Node(root), direction)
            .map(|_| ())
    })
}
