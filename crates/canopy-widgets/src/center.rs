//! Center widget for centering content.

use canopy::{
    EventOutcome, NodeName, ViewContext, Widget, derive_commands,
    event::key,
    layout::{Align, Direction, Layout},
};

/// Container that centers its child within available space.
///
/// For a dimmed overlay, push an effect on the background content with
/// `c.push_effect(background_id, effects::brightness(0.5))`. This container
/// stays at full brightness because it is a sibling of the dimmed content, not
/// a descendant. Insert it as a sibling inside a parent that uses `Stack`
/// layout so it can overlay the existing view.
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
    fn key_outcome(&self, _key: key::Key, _context: &dyn ViewContext) -> Option<EventOutcome> {
        Some(EventOutcome::Ignore)
    }

    fn layout(&self) -> Layout {
        Layout::fill()
            .direction(Direction::Stack)
            .align_horizontal(Align::Center)
            .align_vertical(Align::Center)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("center")
    }
}
