//! List container for contextual help.

use canopy::{
    EventOutcome, NodeName, Render, ViewContext, Widget,
    error::Result,
    event::key,
    layout::{Edges, Layout},
};

/// Container for the scrolling help list.
pub struct HelpPanel;

impl HelpPanel {
    /// Construct an empty help panel.
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Widget for HelpPanel {
    fn key_outcome(&self, _key: key::Key, _context: &dyn ViewContext) -> Option<EventOutcome> {
        Some(EventOutcome::Ignore)
    }

    fn layout(&self) -> Layout {
        // The list starts at the frame's top edge, so only the sides keep a
        // margin.
        Layout::fill().padding(Edges::symmetric(0, 1))
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        render.fill("help/panel", context.view().outer_rect_local(), ' ')
    }

    fn name(&self) -> NodeName {
        NodeName::convert("help_panel")
    }
}
