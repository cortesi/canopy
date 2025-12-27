use unicode_width::UnicodeWidthStr;

use crate::{
    Context, ViewContext, command, derive_commands,
    error::Result,
    geom::Line,
    layout::{Constraint, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    widget::Widget,
};

/// Multiline text widget with wrapping and scrolling.
pub struct Text {
    /// Raw text content.
    raw: String,
    /// Optional fixed width for wrapping.
    fixed_width: Option<u32>,
}

#[derive_commands]
impl Text {
    /// Construct a text widget with raw content.
    pub fn new(raw: impl Into<String>) -> Self {
        Self {
            raw: raw.into(),
            fixed_width: None,
        }
    }

    /// Add a fixed width, ignoring fit parameters.
    pub fn with_fixed_width(mut self, width: u32) -> Self {
        self.fixed_width = Some(width);
        self
    }

    /// Return the raw text content.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Replace the raw text content.
    pub fn set_raw(&mut self, raw: impl Into<String>) {
        self.raw = raw.into();
    }

    #[command]
    /// Scroll to the top-left corner.
    pub fn scroll_to_top(&mut self, c: &mut dyn Context) {
        c.scroll_to(0, 0);
    }

    #[command]
    /// Scroll down by one line.
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        c.scroll_down();
    }

    #[command]
    /// Scroll up by one line.
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        c.scroll_up();
    }

    #[command]
    /// Scroll left by one column.
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        c.scroll_left();
    }

    #[command]
    /// Scroll right by one column.
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        c.scroll_right();
    }

    #[command]
    /// Page down in the view.
    pub fn page_down(&mut self, c: &mut dyn Context) {
        c.page_down();
    }

    #[command]
    /// Page up in the view.
    pub fn page_up(&mut self, c: &mut dyn Context) {
        c.page_up();
    }

    /// Determine the wrapping width for the given available space.
    fn wrap_width(&self, available_width: u32) -> usize {
        let width = self.fixed_width.unwrap_or(available_width).max(1);
        width as usize
    }

    /// Wrap and pad lines to the provided width.
    fn lines_for_width(&self, width: usize) -> Vec<String> {
        textwrap::wrap(&self.raw, width)
            .into_iter()
            .map(|line| format!("{:width$}", line, width = width))
            .collect()
    }
}

impl Widget for Text {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let content_origin = view.content_origin();
        let width = self.wrap_width(view_rect.w);
        let lines = self.lines_for_width(width);

        for i in 0..view_rect.h {
            let line_idx = (view_rect.tl.y + i) as usize;
            if line_idx < lines.len() {
                let line = &lines[line_idx];
                let start_char = view_rect.tl.x as usize;
                let start_byte = line
                    .char_indices()
                    .nth(start_char)
                    .map(|(i, _)| i)
                    .unwrap_or(line.len());
                let out = &line[start_byte..];
                let line_rect = Line::new(
                    content_origin.x,
                    content_origin.y.saturating_add(i),
                    view_rect.w,
                );
                rndr.text("text", line_rect, out)?;
            }
        }
        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let raw_width = self
            .raw
            .lines()
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0) as u32;

        let max_width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n,
            Constraint::Unbounded => self.fixed_width.unwrap_or(raw_width),
        };

        let wrap_width = match c.width {
            Constraint::Unbounded => self.fixed_width.unwrap_or(raw_width),
            _ => self
                .fixed_width
                .map(|w| w.min(max_width))
                .unwrap_or(max_width),
        }
        .max(1);

        let lines = textwrap::wrap(&self.raw, wrap_width as usize);
        let height = lines.len() as u32;
        c.clamp(Size::new(wrap_width, height))
    }

    fn canvas(&self, view: Size<u32>, _ctx: &crate::layout::CanvasContext) -> Size<u32> {
        let wrap_width = view.width.max(1);
        let lines = textwrap::wrap(&self.raw, wrap_width as usize);
        Size::new(wrap_width, lines.len() as u32)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("text")
    }
}
