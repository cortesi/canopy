//! List container and control footer for contextual help.

use canopy::{
    ViewContext, Widget,
    error::Result,
    geom::Line,
    layout::{Constraint, Direction, Layout, MeasureConstraints, Measurement, Size, Sizing},
    render::Render,
    state::NodeName,
};

/// Column container for the scrolling list and fixed footer.
pub struct HelpPanel;

impl HelpPanel {
    /// Construct an empty help panel.
    pub const fn new() -> Self {
        Self
    }
}

impl Default for HelpPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for HelpPanel {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("help_panel")
    }
}

/// Fixed help-control summary below the binding list.
pub struct ControlFooter;

impl ControlFooter {
    /// Navigation key groups shown from left to right when space permits.
    const NAVIGATION: [(&'static str, &'static str); 3] = [
        ("Up/k Down/j", " scroll"),
        ("PgUp/PgDn", " page"),
        ("Home/End", " jump"),
    ];
    /// Close keys, which remain visible when navigation groups do not fit.
    const CLOSE: (&'static str, &'static str) = ("?/Esc", " close");
    /// Space between adjacent guide groups.
    const GAP: &'static str = "  ";

    /// Construct the control footer.
    pub const fn new() -> Self {
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

    /// Return the width required to show every guide group.
    fn preferred_width() -> u32 {
        Self::NAVIGATION
            .iter()
            .copied()
            .map(Self::group_width)
            .sum::<u32>()
            + Self::text_width(Self::GAP) * Self::NAVIGATION.len() as u32
            + Self::group_width(Self::CLOSE)
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

    /// Render one key and action group with their respective styles.
    fn render_group(render: &mut Render, group: (&str, &str), x: &mut u32, y: u32) -> Result<()> {
        Self::render_text(render, "help/footer/key", group.0, x, y)?;
        Self::render_text(render, "help/footer/label", group.1, x, y)
    }
}

impl Default for ControlFooter {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ControlFooter {
    fn layout(&self) -> Layout {
        Layout::row().height(Sizing::Measure)
    }

    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        let width = match constraints.width {
            Constraint::Exact(width) | Constraint::AtMost(width) => width,
            Constraint::Unbounded => Self::preferred_width(),
        };
        constraints.clamp(Size::new(width, 1))
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        let rect = context.view().outer_rect_local();
        if rect.h == 0 || rect.w == 0 {
            return Ok(());
        }

        let close_width = Self::group_width(Self::CLOSE);
        let close_x = rect.w.saturating_sub(close_width);
        let navigation_limit = close_x.saturating_sub(Self::text_width(Self::GAP));
        let mut x = 0;
        for (index, group) in Self::NAVIGATION.iter().copied().enumerate() {
            let gap_width = u32::from(index > 0) * Self::text_width(Self::GAP);
            if x + gap_width + Self::group_width(group) > navigation_limit {
                break;
            }
            if index > 0 {
                Self::render_text(render, "help/footer", Self::GAP, &mut x, 0)?;
            }
            Self::render_group(render, group, &mut x, 0)?;
        }

        let mut close_x = close_x;
        Self::render_group(render, Self::CLOSE, &mut close_x, 0)?;
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("help_footer")
    }
}
