mod logs;

use canopy::{
    Canopy, Context, ContextExt, Loader, NodeId, NodeName, Render, ViewContext, Widget,
    derive_commands, error::Result, layout::Layout,
};
use logs::Logs;

use crate::frame;

/// Default inspector bindings exposed through `inspector.default_bindings()`.
const DEFAULT_BINDINGS: &str = r#"
canopy.keymap {
    path = "logs",
    { key = "C", description = "Clear log entry", action = command.logs.clear() },
    {
        key = "d",
        description = "Delete selected log entry",
        action = command.logs.delete_selected(),
    },
    { key = { "j", "Down" }, description = "Next log entry", action = command.logs.select_by(1) },
    {
        key = { "k", "Up" },
        description = "Previous log entry",
        action = command.logs.select_by(-1),
    },
    { key = "g", description = "First log entry", action = command.logs.select_first() },
    { key = "G", description = "Last log entry", action = command.logs.select_last() },
    { key = { "Space", "PageDown" }, description = "Page down", action = command.logs.page(1) },
    { key = "PageUp", description = "Page up", action = command.logs.page(-1) },
}
"#;

/// Inspector overlay widget.
///
/// Keeps the latest 1,000 log entries, truncating each to 4,096 UTF-8 bytes.
pub struct Inspector;

#[derive_commands]
impl Inspector {
    /// Construct a new inspector.
    fn new() -> Self {
        Self
    }

    /// Build the inspector subtree and return its node id.
    pub(crate) fn install(context: &mut dyn Context) -> Result<NodeId> {
        let logs_id = context.create_detached(Logs::new())?;
        let frame_id = context.create_detached(frame::Frame::new())?;
        context.set_children_of(frame_id.into(), vec![logs_id.into()])?;

        let inspector_id = context.create_detached(Self::new())?;
        context.set_children_of(inspector_id.into(), vec![frame_id.into()])?;

        Ok(inspector_id.into())
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
