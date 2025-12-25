use super::core;
use crate::{
    Context, ViewContext, command, cursor, derive_commands,
    error::Result,
    geom::Rect,
    layout::{AvailableSpace, Size},
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
    fn accept_focus(&self) -> bool {
        true
    }

    fn cursor(&self) -> Option<cursor::Cursor> {
        self.core.cursor_position().map(|p| cursor::Cursor {
            location: p,
            shape: cursor::CursorShape::Block,
            blink: true,
        })
    }

    fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        self.core.resize_window(view.w as usize, view.h as usize);
        for (i, line) in self.core.window_text().iter().enumerate() {
            if let Some(text) = line {
                r.text("text", view.line(i as u32), text)?;
            }
        }
        Ok(())
    }

    fn measure(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        let width = known_dimensions
            .width
            .or_else(|| available_space.width.into_option())
            .unwrap_or(0.0);
        let wrap_width = width.max(1.0) as usize;
        let mut core = self.core.clone();
        core.resize_window(wrap_width, 0);
        let height = core.wrapped_height() as f32;
        Size { width, height }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("editor")
    }
}
