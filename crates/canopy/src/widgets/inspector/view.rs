use crate::{
    NodeId, ViewContext, core::Core, derive_commands, error::Result, layout::Layout,
    state::NodeName, widget::Widget, widgets::tabs::Tabs,
};

/// View contains the body of the inspector.
pub struct View;

impl Widget for View {
    fn render(&mut self, _rndr: &mut crate::render::Render, _ctx: &dyn ViewContext) -> Result<()> {
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
    pub fn install(core: &mut Core) -> Result<(NodeId, NodeId, NodeId)> {
        let tabs = core.add(Tabs::new(vec!["Stats", "Logs"]));
        let logs = core.add(super::logs::Logs::new());
        let view_id = core.add(Self::new());
        core.set_children(view_id, vec![tabs, logs])?;
        core.with_layout_of(view_id, |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;
        core.with_layout_of(tabs, |layout| {
            *layout = Layout::column().flex_horizontal(1).fixed_height(1);
        })?;
        core.with_layout_of(logs, |layout| {
            *layout = Layout::fill();
        })?;
        Ok((view_id, tabs, logs))
    }
}
