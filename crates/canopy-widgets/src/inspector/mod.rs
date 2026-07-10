/// Log panel widget.
mod logs;
/// Inspector view layout.
mod view;

use canopy::{
    Canopy, Context, Loader, NodeId, ViewContext, Widget, derive_commands, error::Result,
    layout::Layout, render::Render, state::NodeName,
};
use logs::Logs;

use crate::{frame, tabs};

/// Default inspector bindings exposed through `inspector.default_bindings()`.
const DEFAULT_BINDINGS: &str = r#"
canopy.bind_with("Tab", { path = "inspector/", desc = "Next tab" }, function()
    tabs.select_by(1)
end)

canopy.bind_with("C", { path = "logs", desc = "Clear log entry" }, function()
    logs.clear()
end)
canopy.bind_with("d", { path = "logs", desc = "Delete selected log entry" }, function()
    logs.delete_selected()
end)
canopy.bind_with("j", { path = "logs", desc = "Next log entry" }, function()
    logs.select_by(1)
end)
canopy.bind_with("k", { path = "logs", desc = "Previous log entry" }, function()
    logs.select_by(-1)
end)
canopy.bind_with("g", { path = "logs", desc = "First log entry" }, function()
    logs.select_first()
end)
canopy.bind_with("G", { path = "logs", desc = "Last log entry" }, function()
    logs.select_last()
end)
canopy.bind_with("Space", { path = "logs", desc = "Page down" }, function()
    logs.page(1)
end)
canopy.bind_with("PageDown", { path = "logs", desc = "Page down" }, function()
    logs.page(1)
end)
canopy.bind_with("PageUp", { path = "logs", desc = "Page up" }, function()
    logs.page(-1)
end)
canopy.bind_with("Down", { path = "logs", desc = "Next log entry" }, function()
    logs.select_by(1)
end)
canopy.bind_with("Up", { path = "logs", desc = "Previous log entry" }, function()
    logs.select_by(-1)
end)
"#;

/// Inspector overlay widget.
pub struct Inspector;

#[derive_commands]
impl Inspector {
    /// Construct a new inspector.
    pub fn new() -> Self {
        Self
    }

    /// Build the inspector subtree and return its node id.
    pub fn install(context: &mut dyn Context) -> Result<NodeId> {
        let (view_id, _tabs, _logs) = view::View::install(context)?;
        let frame_id = context.create_detached(frame::Frame::new())?;
        context.set_children_of(frame_id.into(), vec![view_id])?;
        context.set_layout_of(frame_id, Layout::fill())?;

        let inspector_id = context.create_detached(Self::new())?;
        context.set_children_of(inspector_id.into(), vec![frame_id.into()])?;
        context.set_layout_of(inspector_id, Layout::fill())?;

        Ok(inspector_id.into())
    }
}

impl Default for Inspector {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Inspector {
    fn render(&mut self, r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("inspector");
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("inspector")
    }
}

impl Loader for Inspector {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        c.add_commands::<tabs::Tabs>()?;
        c.register_default_bindings("inspector", DEFAULT_BINDINGS)?;
        Logs::load(c)?;
        Ok(())
    }
}
