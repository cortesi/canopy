//! Padding container widget.

use canopy::{
    Context, NodeId, ReadContext, Widget, derive_commands,
    error::Result,
    layout::{Edges, Layout},
    render::Render,
    state::NodeName,
};

/// Container that adds padding around its child.
pub struct Pad {
    /// Padding applied around the child.
    padding: Edges<u32>,
}

#[derive_commands]
impl Pad {
    /// Create a pad with the provided edge padding.
    pub fn new(padding: Edges<u32>) -> Self {
        Self { padding }
    }

    /// Create a pad with uniform padding on all sides.
    pub fn uniform(padding: u32) -> Self {
        Self::new(Edges::all(padding))
    }

    /// Wrap an existing child node in a new pad and return the pad node ID.
    pub fn wrap(
        c: &mut dyn Context,
        child: impl Into<NodeId>,
        padding: Edges<u32>,
    ) -> Result<NodeId> {
        Self::wrap_with(c, child, Self::new(padding))
    }

    /// Wrap an existing child node in a configured pad and return the pad node ID.
    pub fn wrap_with(c: &mut dyn Context, child: impl Into<NodeId>, pad: Self) -> Result<NodeId> {
        let child = child.into();
        let pad_id = NodeId::from(c.create_detached(pad));
        c.detach(child)?;
        c.attach(pad_id, child)?;
        Ok(pad_id)
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

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("pad")
    }
}
