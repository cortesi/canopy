//! Modal widget for centered overlay content.

use crate::{
    ViewContext, derive_commands,
    error::Result,
    layout::{Align, Direction, Layout},
    render::Render,
    state::NodeName,
    widget::Widget,
};

/// A modal container that centers its content.
///
/// For the dimming effect, the parent should push an effect on the background content
/// using `c.push_effect(background_id, effects::dim(0.5))`. The Modal itself renders
/// at full brightness since it's a sibling to the dimmed content, not a descendant.
///
/// This widget is typically inserted as a sibling to the background content inside
/// a parent configured with `Stack` layout so it can overlay the existing view.
pub struct Modal;

#[derive_commands]
impl Modal {
    /// Create a new Modal widget.
    pub fn new() -> Self {
        Self
    }
}

impl Default for Modal {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Modal {
    fn layout(&self) -> Layout {
        Layout::fill()
            .direction(Direction::Stack)
            .align_horizontal(Align::Center)
            .align_vertical(Align::Center)
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("modal")
    }
}
