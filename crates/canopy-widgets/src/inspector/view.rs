use canopy::{
    Context, NodeId, Widget, derive_commands, error::Result, layout::Layout, state::NodeName,
};

/// View contains the body of the inspector.
pub struct View;

impl Widget for View {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn name(&self) -> NodeName {
        NodeName::convert("view")
    }
}

#[derive_commands]
impl View {
    /// Construct an empty inspector view.
    pub fn new() -> Self {
        Self
    }

    /// Build the inspector view subtree and return its node id.
    pub fn install(context: &mut dyn Context) -> Result<NodeId> {
        let logs = context.create_detached(super::logs::Logs::new())?;
        let view_id = context.create_detached(Self::new())?;
        context.set_children_of(view_id.into(), vec![logs.into()])?;
        context.set_layout_of(logs, Layout::fill())?;
        Ok(view_id.into())
    }
}
