use taffy::{geometry::Size, style::AvailableSpace};

use crate::{
    Context, ViewContext, command, cursor, derive_commands,
    error::Result,
    event::{Event, key},
    geom::{LineSegment, Point, Rect},
    render::Render,
    state::NodeName,
    widget::{EventOutcome, Widget},
};

/// A text buffer that exposes edit functionality for a single line. It also
/// keeps track of a display window that slides within the line, responding
/// naturally to cursor movements.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TextBuf {
    /// Raw input text value.
    value: String,

    /// Cursor position in bytes within value.
    cursor_pos: u32,
    /// Visible window into the value.
    window: LineSegment,
}

impl TextBuf {
    /// Construct a new text buffer with initial content.
    fn new(start: impl Into<String>) -> Self {
        let value = start.into();
        let cursor_pos = value.len() as u32;
        Self {
            value,
            cursor_pos,
            window: LineSegment { off: 0, len: 0 },
        }
    }

    /// The location of the displayed cursor along the x axis.
    fn cursor_display(&self) -> u32 {
        self.cursor_pos.saturating_sub(self.window.off)
    }

    /// Return the visible text slice.
    fn text(&self) -> &str {
        let end = self.window.far().min(self.value.len() as u32) as usize;
        &self.value[self.window.off as usize..end]
    }

    /// Clamp cursor and window state to valid bounds.
    fn fix_window(&mut self) {
        if self.cursor_pos > self.value.len() as u32 {
            self.cursor_pos = self.value.len() as u32
        }
        if self.cursor_pos < self.window.off {
            self.window.off = self.cursor_pos;
        } else if self.cursor_pos >= self.window.far() {
            let mut off = self.cursor_pos.saturating_sub(self.window.len);
            if self.cursor_pos == self.value.len() as u32 {
                off = off.saturating_add(1)
            }
            self.window.off = off;
        }

        if self.cursor_display() >= self.window.len {
            let delta = self
                .cursor_display()
                .saturating_sub(self.window.len)
                .saturating_add(1);
            self.window.off = self.window.off.saturating_add(delta);
        }
    }

    /// Set the visible window width.
    fn set_display_width(&mut self, val: usize) {
        self.window = LineSegment {
            off: self.window.off,
            len: val as u32,
        };
        self.fix_window();
    }

    /// Insert a character at the cursor position.
    pub fn insert(&mut self, c: char) -> bool {
        self.value.insert(self.cursor_pos as usize, c);
        self.cursor_pos = self.cursor_pos.saturating_add(1);
        self.fix_window();
        true
    }
    /// Delete the character before the cursor.
    pub fn backspace(&mut self) -> bool {
        if !self.value.is_empty() && self.cursor_pos > 0 {
            self.value.remove(self.cursor_pos as usize - 1);
            self.cursor_pos = self.cursor_pos.saturating_sub(1);
            self.fix_window();
            true
        } else {
            false
        }
    }
    /// Move the cursor left by one character.
    pub fn left(&mut self) -> bool {
        if self.cursor_pos > 0 {
            self.cursor_pos = self.cursor_pos.saturating_sub(1);
            self.fix_window();
            true
        } else {
            false
        }
    }
    /// Move the cursor right by one character.
    pub fn right(&mut self) -> bool {
        if self.cursor_pos < self.value.len() as u32 {
            self.cursor_pos = self.cursor_pos.saturating_add(1);
            self.fix_window();
            true
        } else {
            false
        }
    }
}

/// Single-line text input widget.
pub struct Input {
    /// Text buffer for the input.
    textbuf: TextBuf,
}

#[derive_commands]
impl Input {
    /// Construct a new input with initial text.
    pub fn new(txt: impl Into<String>) -> Self {
        Self {
            textbuf: TextBuf::new(txt),
        }
    }
    /// Return the current input text.
    pub fn text(&self) -> &str {
        self.textbuf.text()
    }

    /// Return the raw input value without padding.
    pub fn value(&self) -> &str {
        &self.textbuf.value
    }

    /// Replace the input value and reset the cursor.
    pub fn set_value(&mut self, value: impl Into<String>) {
        self.textbuf = TextBuf::new(value);
    }

    /// Move the cursor left.
    #[command]
    fn left(&mut self, _c: &mut dyn Context) {
        let _ = self.textbuf.left();
    }

    /// Move the cursor right.
    #[command]
    fn right(&mut self, _c: &mut dyn Context) {
        let _ = self.textbuf.right();
    }

    /// Delete a character at the input location.
    #[command]
    fn backspace(&mut self, _c: &mut dyn Context) {
        let _ = self.textbuf.backspace();
    }
}

impl Widget for Input {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn cursor(&self) -> Option<cursor::Cursor> {
        Some(cursor::Cursor {
            location: Point {
                x: self.textbuf.cursor_display(),
                y: 0,
            },
            shape: cursor::CursorShape::Block,
            blink: true,
        })
    }

    fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
        self.textbuf.set_display_width(ctx.view().w as usize);
        r.text("text", ctx.view().line(0), self.textbuf.text())
    }

    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        match event {
            Event::Key(key::Key {
                key: key::KeyCode::Char(c),
                ..
            }) => {
                self.textbuf.insert(*c);
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
        }
    }

    fn measure(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        let width = known_dimensions
            .width
            .or_else(|| available_space.width.into_option())
            .unwrap_or(self.value().len() as f32);
        let text_len = self.value().len() as f32;
        Size {
            width: width.max(text_len),
            height: 1.0,
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("input")
    }
}
