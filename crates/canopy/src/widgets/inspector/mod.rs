/// Log panel widget.
mod logs;
/// Inspector view layout.
mod view;

use logs::Logs;
use taffy::style::{Dimension, Display, FlexDirection};

use crate::{
    Binder, Canopy, Context, DefaultBindings, Loader, NodeId, ViewContext,
    core::Core,
    derive_commands,
    error::Result,
    event::{Event, key::*},
    geom::Rect,
    render::Render,
    state::NodeName,
    widget::{EventOutcome, Widget},
    widgets::{frame, tabs},
};

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
        let frame_id = core.add(frame::Frame::new());
        core.set_children(frame_id, vec![view_id])?;
        core.build(frame_id).style(|style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        });

        let inspector_id = core.add(Self::new());
        core.set_children(inspector_id, vec![frame_id])?;
        core.build(inspector_id).style(|style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Column;
        });

        Ok(inspector_id)
    }
}

impl Default for Inspector {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Inspector {
    fn render(&mut self, r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("inspector");
        Ok(())
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
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
            .key('j', "logs::select_next()")
            .key('k', "logs::select_prev()")
            .key('g', "logs::select_first()")
            .key('G', "logs::select_last()")
            .key(' ', "logs::page_down()")
            .key(KeyCode::PageDown, "logs::page_down()")
            .key(KeyCode::PageUp, "logs::page_up()")
            .key(KeyCode::Down, "logs::select_next()")
            .key(KeyCode::Down, "logs::select_prev()")
    }
}

impl Loader for Inspector {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<tabs::Tabs>();
        Logs::load(c);
    }
}
