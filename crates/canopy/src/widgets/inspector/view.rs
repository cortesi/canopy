use taffy::style::{Dimension, Display, FlexDirection};

use crate::{
    Context, NodeId, ViewContext,
    core::Core,
    derive_commands,
    error::Result,
    event::Event,
    geom::Rect,
    state::NodeName,
    widget::{EventOutcome, Widget},
    widgets::tabs::Tabs,
};

/// View contains the body of the inspector.
pub struct View;

impl Widget for View {
    fn render(
        &mut self,
        _rndr: &mut crate::render::Render,
        _area: Rect,
        _ctx: &dyn ViewContext,
    ) -> Result<()> {
        Ok(())
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
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
        core.build(view_id).style(|style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Column;
        });
        core.build(tabs).style(|style| {
            style.size.height = Dimension::Points(1.0);
            style.flex_shrink = 0.0;
        });
        core.build(logs).style(|style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        });
        Ok((view_id, tabs, logs))
    }
}
