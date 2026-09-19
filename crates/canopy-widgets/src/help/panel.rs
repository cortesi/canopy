//! List container and control footer for contextual help.

use canopy::{
    EventOutcome, NodeName, Render, ViewContext, Widget,
    error::Result,
    event::key,
    geom::{Line, Size},
    layout::{Constraint, Direction, Edges, Layout, MeasureConstraints, Measurement, Sizing},
};

/// Column container for the scrolling list and fixed footer.
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
        // The list starts at the frame's top edge and the footer ends at its
        // bottom edge, so only the sides keep a margin.
        Layout::fill()
            .direction(Direction::Column)
            .padding(Edges::symmetric(0, 1))
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        render.fill("help/panel", context.view().outer_rect_local(), ' ')
    }

    fn name(&self) -> NodeName {
        NodeName::convert("help_panel")
    }
}

/// Fixed help-control summary below the binding list.
pub struct ControlFooter;

impl ControlFooter {
    /// Close key and what it does. Every other guide only repeats a control
    /// the list above already names, so the bar carries this one alone.
    const CLOSE: (&'static str, &'static str) = ("esc", ": close");

    /// Construct the control footer.
    pub(crate) const fn new() -> Self {
        Self
    }

    /// Return the terminal-cell width of text.
    fn text_width(text: &str) -> u32 {
        unicode_width::UnicodeWidthStr::width(text) as u32
    }

    /// Return the terminal-cell width of one key and action group.
    fn group_width(group: (&str, &str)) -> u32 {
        Self::text_width(group.0) + Self::text_width(group.1)
    }

    /// Render one styled text fragment and advance the cursor.
    fn render_text(
        render: &mut Render,
        style: &str,
        text: &str,
        x: &mut u32,
        y: u32,
    ) -> Result<()> {
        let width = Self::text_width(text);
        render.text(style, Line::new(*x, y, width), text)?;
        *x += width;
        Ok(())
    }
}

impl Widget for ControlFooter {
    fn key_outcome(&self, _key: key::Key, _context: &dyn ViewContext) -> Option<EventOutcome> {
        Some(EventOutcome::Ignore)
    }

    fn layout(&self) -> Layout {
        Layout::row().height(Sizing::Measure)
    }

    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        let width = match constraints.width {
            Constraint::Exact(width) | Constraint::AtMost(width) => width,
            Constraint::Unbounded => Self::group_width(Self::CLOSE),
        };
        constraints.clamp(Size::new(width, 1))
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        let rect = context.view().outer_rect_local();
        if rect.h == 0 || rect.w == 0 {
            return Ok(());
        }

        render.fill("help/footer", rect, ' ')?;
        let y = rect.h - 1;
        let (key, label) = Self::CLOSE;
        let mut x = rect.w.saturating_sub(Self::group_width(Self::CLOSE));
        Self::render_text(render, "help/footer/key", key, &mut x, y)?;
        Self::render_text(render, "help/footer/label", label, &mut x, y)?;
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("help_footer")
    }
}
