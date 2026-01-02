/// Log panel widget.
mod logs;
/// Inspector view layout.
mod view;

use canopy::{
    Binder, Canopy, Core, DefaultBindings, Loader, NodeId, ReadContext, Widget, derive_commands,
    error::Result, event::key::*, layout::Layout, render::Render, state::NodeName,
};
use logs::Logs;

use crate::{frame, tabs};

/// Inspector overlay widget.
pub struct Inspector;

#[derive_commands]
impl Inspector {
    /// Construct a new inspector.
    pub fn new() -> Self {
        Self
    }

    /// Build the inspector subtree and return its node id.
    pub fn install(core: &mut Core) -> Result<NodeId> {
        let (view_id, _tabs, _logs) = view::View::install(core)?;
        let frame_id = core.create_detached(frame::Frame::new());
        core.set_children(frame_id, vec![view_id])?;
        core.set_layout_of(frame_id, Layout::fill())?;

        let inspector_id = core.create_detached(Self::new());
        core.set_children(inspector_id, vec![frame_id])?;
        core.set_layout_of(inspector_id, Layout::fill())?;

        Ok(inspector_id)
    }
}

impl Default for Inspector {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Inspector {
    fn render(&mut self, r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        r.push_layer("inspector");
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("inspector")
    }
}

impl DefaultBindings for Inspector {
    fn defaults(b: Binder) -> Binder {
        b.with_path("inspector/")
            .key(KeyCode::Tab, "tabs::next()")
            .with_path("logs")
            .key('C', "logs::clear()")
            .key('d', "logs::delete_selected()")
            .key('j', "logs::select_by(1)")
            .key('k', "logs::select_by(-1)")
            .key('g', "logs::select_first()")
            .key('G', "logs::select_last()")
            .key(' ', "logs::page_down()")
            .key(KeyCode::PageDown, "logs::page_down()")
            .key(KeyCode::PageUp, "logs::page_up()")
            .key(KeyCode::Down, "logs::select_by(1)")
            .key(KeyCode::Up, "logs::select_by(-1)")
    }
}

impl Loader for Inspector {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<tabs::Tabs>();
        Logs::load(c);
    }
}
