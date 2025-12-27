use super::core;
use crate::{
    Context, ViewContext, command, cursor, derive_commands,
    error::Result,
    geom::Line,
    layout::{Constraint, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    widget::Widget,
};

/// A simple editor widget.
pub struct Editor {
    /// Core editor state and logic.
    core: core::Core,
}

#[derive_commands]
impl Editor {
    /// Construct a new editor with the provided text.
    pub fn new(txt: &str) -> Self {
        Self {
            core: core::Core::new(txt),
        }
    }

    /// Move the cursor left or right.
    #[command]
    fn cursor_shift(&mut self, _: &mut dyn Context, n: isize) {
        self.core.cursor_shift(n);
    }

    /// Move the cursor up or down in the chunk list.
    #[command]
    fn cursor_shift_chunk(&mut self, _: &mut dyn Context, n: isize) {
        self.core.cursor_shift_chunk(n);
    }

    /// Move the cursor up or down by visual line.
    #[command]
    fn cursor_shift_lines(&mut self, _: &mut dyn Context, n: isize) {
        self.core.cursor_shift_lines(n);
    }
}

impl Widget for Editor {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn cursor(&self) -> Option<cursor::Cursor> {
        self.core.cursor_position().map(|p| cursor::Cursor {
            location: p,
            shape: cursor::CursorShape::Block,
            blink: true,
        })
    }

    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let content_origin = view.content_origin();
        self.core
            .resize_window(view_rect.w as usize, view_rect.h as usize);
        for (i, line) in self.core.window_text().iter().enumerate() {
            if let Some(text) = line {
                let line_rect = Line::new(
                    content_origin.x,
                    content_origin.y.saturating_add(i as u32),
                    view_rect.w,
                );
                r.text("text", line_rect, text)?;
            }
        }
        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n.max(1),
            Constraint::Unbounded => 1,
        };
        let wrap_width = width as usize;
        let mut core = self.core.clone();
        core.resize_window(wrap_width, 0);
        let height = core.wrapped_height() as u32;
        c.clamp(Size::new(width, height))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("editor")
    }
}
