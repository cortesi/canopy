//! Center widget for centering content.

use crate::{
    ViewContext, derive_commands,
    error::Result,
    layout::{Align, Direction, Layout},
    render::Render,
    state::NodeName,
    widget::Widget,
};

/// Container that centers its child within available space.
pub struct Center;

#[derive_commands]
impl Center {
    /// Create a new Center widget.
    pub fn new() -> Self {
        Self
    }
}

impl Default for Center {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Center {
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
        NodeName::convert("center")
    }
}
