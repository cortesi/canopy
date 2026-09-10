//! Padding container widget.

use canopy::{
    Widget, derive_commands,
    layout::{Edges, Layout},
    state::NodeName,
};

/// Container that adds padding around its child.
pub struct Pad {
    /// Padding applied around the child.
    padding: Edges,
}

#[derive_commands]
impl Pad {
    /// Create a pad with the provided edge padding.
    pub fn new(padding: Edges) -> Self {
        Self { padding }
    }

    /// Create a pad with uniform padding on all sides.
    pub fn uniform(padding: u32) -> Self {
        Self::new(Edges::all(padding))
    }
}

impl Default for Pad {
    fn default() -> Self {
        Self::new(Edges::all(0))
    }
}

impl Widget for Pad {
    fn layout(&self) -> Layout {
        Layout::fill().padding(self.padding)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("pad")
    }
}
