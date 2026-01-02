//! Center widget for centering content.

use canopy::{
    ReadContext, Widget, derive_commands,
    error::Result,
    layout::{Align, Direction, Layout},
    render::Render,
    state::NodeName,
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

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("center")
    }
}
