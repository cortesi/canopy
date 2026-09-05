/// Log panel widget.
pub mod logs;
/// Inspector view layout.
mod view;

use canopy::{
    Canopy, Context, Loader, NodeId, ViewContext, Widget, derive_commands, error::Result,
    layout::Layout, render::Render, state::NodeName,
};
use logs::Logs;

use crate::frame;

/// Default inspector bindings exposed through `inspector.default_bindings()`.
const DEFAULT_BINDINGS: &str = r#"
canopy.bind_command("C", { phase = "after_widget", path = "logs", description = "Clear log entry" }, "logs::clear")
canopy.bind_command("d", { phase = "after_widget", path = "logs", description = "Delete selected log entry" }, "logs::delete_selected")
canopy.bind_command("j", { phase = "after_widget", path = "logs", description = "Next log entry" }, "logs::select_by", 1)
canopy.bind_command("k", { phase = "after_widget", path = "logs", description = "Previous log entry" }, "logs::select_by", -1)
canopy.bind_command("g", { phase = "after_widget", path = "logs", description = "First log entry" }, "logs::select_first")
canopy.bind_command("G", { phase = "after_widget", path = "logs", description = "Last log entry" }, "logs::select_last")
canopy.bind_command("Space", { phase = "after_widget", path = "logs", description = "Page down" }, "logs::page", 1)
canopy.bind_command("PageDown", { phase = "after_widget", path = "logs", description = "Page down" }, "logs::page", 1)
canopy.bind_command("PageUp", { phase = "after_widget", path = "logs", description = "Page up" }, "logs::page", -1)
canopy.bind_command("Down", { phase = "after_widget", path = "logs", description = "Next log entry" }, "logs::select_by", 1)
canopy.bind_command("Up", { phase = "after_widget", path = "logs", description = "Previous log entry" }, "logs::select_by", -1)
"#;

/// Inspector overlay widget.
pub struct Inspector;

#[derive_commands]
impl Inspector {
    /// Construct a new inspector.
    fn new() -> Self {
        Self
    }

    /// Build the inspector subtree and return its node id.
    pub(crate) fn install(context: &mut dyn Context) -> Result<NodeId> {
        let view_id = view::View::install(context)?;
        let frame_id = context.create_detached(frame::Frame::new())?;
        context.set_children_of(frame_id.into(), vec![view_id])?;

        let inspector_id = context.create_detached(Self::new())?;
        context.set_children_of(inspector_id.into(), vec![frame_id.into()])?;

        Ok(inspector_id.into())
    }
}

impl Default for Inspector {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Inspector {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

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
        c.register_default_bindings("inspector", DEFAULT_BINDINGS)?;
        Logs::load(c)?;
        Ok(())
    }
}
