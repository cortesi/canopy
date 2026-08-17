use canopy::{
    Context, NodeId, ViewContext, Widget, derive_commands, error::Result, layout::Layout,
    state::NodeName,
};

use crate::tabs::Tabs;

/// View contains the body of the inspector.
pub struct View;

impl Widget for View {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, _rndr: &mut canopy::render::Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("view")
    }
}

#[derive_commands]
impl View {
    /// Construct a new inspector view with child node IDs.
    pub fn new() -> Self {
        Self
    }

    /// Construct a new inspector view.
    pub fn install(context: &mut dyn Context) -> Result<(NodeId, NodeId, NodeId)> {
        let tabs = context.create_detached(Tabs::new(vec!["Stats", "Logs"]))?;
        let logs = context.create_detached(super::logs::Logs::new())?;
        let view_id = context.create_detached(Self::new())?;
        context.set_children_of(view_id.into(), vec![tabs.into(), logs.into()])?;
        context.set_layout_of(tabs, Layout::column().flex_horizontal(1).fixed_height(1))?;
        context.set_layout_of(logs, Layout::fill())?;
        Ok((view_id.into(), tabs.into(), logs.into()))
    }
}
