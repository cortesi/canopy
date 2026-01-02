use std::iter::repeat_n;

use canopy::{
    Context, EventOutcome, ReadContext, Widget, command, cursor, derive_commands,
    error::Result,
    event::{Event, key},
    geom::{Line, Point},
    layout::{MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    text,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::editor::{TextBuffer, TextPosition, tab_width};

/// Default tab stop width for single-line inputs.
const DEFAULT_TAB_STOP: usize = 4;

/// A single-line text buffer with horizontal scrolling.
#[derive(Debug, Clone)]
struct InputBuffer {
    /// Rope-backed text storage.
    buffer: TextBuffer,
    /// Cached buffer contents for easy slicing.
    value: String,
    /// Column offset of the visible window.
    scroll: usize,
    /// Visible window width in columns.
    view_width: usize,
    /// Tab stop width in columns.
    tab_stop: usize,
}

impl InputBuffer {
    /// Construct a new input buffer with initial content.
    fn new(start: impl Into<String>) -> Self {
        let raw = start.into();
        let value = sanitize_single_line(&raw);
        let buffer = TextBuffer::new(value.clone());
        let mut out = Self {
            buffer,
            value,
            scroll: 0,
            view_width: 0,
            tab_stop: DEFAULT_TAB_STOP,
        };
        out.ensure_cursor_visible();
        out
    }

    /// Set the visible window width.
    fn set_display_width(&mut self, width: usize) {
        self.view_width = width;
        self.ensure_cursor_visible();
    }

    /// The location of the displayed cursor along the x axis.
    fn cursor_display(&self) -> u32 {
        let cursor_col = self.cursor_column();
        cursor_col.saturating_sub(self.scroll) as u32
    }

    /// Return the visible text slice.
    fn text(&self) -> &str {
        if self.view_width == 0 {
            return "";
        }
        let (start, end) = self.visible_range();
        &self.value[start..end]
    }

    /// Return the raw input value.
    fn value(&self) -> &str {
        &self.value
    }

    /// Return the visible text for rendering.
    fn render_text(&self) -> String {
        if self.view_width == 0 {
            return String::new();
        }
        let expanded = expand_tabs(&self.value, self.tab_stop);
        let (out, _) = text::slice_by_columns(&expanded, self.scroll, self.view_width);
        out.to_string()
    }

    /// Insert a character at the cursor position.
    fn insert(&mut self, c: char) -> bool {
        let insert = match c {
            '\n' | '\r' => ' ',
            _ => c,
        };
        self.buffer.insert_text(&insert.to_string());
        self.sync_value();
        self.ensure_cursor_visible();
        true
    }

    /// Delete the character before the cursor.
    fn backspace(&mut self) -> bool {
        if self.buffer.delete_backward(false) {
            self.sync_value();
            self.ensure_cursor_visible();
            true
        } else {
            false
        }
    }

    /// Move the cursor left by one character.
    fn left(&mut self) -> bool {
        if self.buffer.move_left(false) {
            self.ensure_cursor_visible();
            true
        } else {
            false
        }
    }

    /// Move the cursor right by one character.
    fn right(&mut self) -> bool {
        if self.buffer.move_right(false) {
            self.ensure_cursor_visible();
            true
        } else {
            false
        }
    }

    /// Return the display width of the full buffer.
    fn display_width(&self) -> u32 {
        self.line_width() as u32
    }

    /// Update the cached value string from the rope.
    fn sync_value(&mut self) {
        self.value = self.buffer.line_text(0);
    }

    /// Compute the cursor column in display coordinates.
    fn cursor_column(&self) -> usize {
        self.buffer
            .column_for_position(self.buffer.cursor(), self.tab_stop)
    }

    /// Compute the display width of the line.
    fn line_width(&self) -> usize {
        let len = self.buffer.line_char_len(0);
        self.buffer
            .column_for_position(TextPosition::new(0, len), self.tab_stop)
    }

    /// Compute the visible byte range for the current scroll state.
    fn visible_range(&self) -> (usize, usize) {
        if self.value.is_empty() || self.view_width == 0 {
            return (0, 0);
        }
        let start_pos = self
            .buffer
            .position_for_column(0, self.scroll, self.tab_stop);
        let end_col = self.scroll.saturating_add(self.view_width);
        let end_pos = self.buffer.position_for_column(0, end_col, self.tab_stop);
        let start = byte_index_for_char(&self.value, start_pos.column);
        let end = byte_index_for_char(&self.value, end_pos.column);
        if start >= end {
            (start, start)
        } else {
            (start, end)
        }
    }

    /// Ensure the cursor stays within the visible window.
    fn ensure_cursor_visible(&mut self) {
        if self.view_width == 0 {
            self.scroll = 0;
            return;
        }
        let cursor_col = self.cursor_column();
        if cursor_col < self.scroll {
            self.scroll = cursor_col;
        } else {
            let window_end = self.scroll.saturating_add(self.view_width);
            if cursor_col >= window_end {
                let delta = cursor_col.saturating_sub(window_end).saturating_add(1);
                self.scroll = self.scroll.saturating_add(delta);
            }
        }

        let text_width = self.line_width();
        if text_width <= self.view_width {
            self.scroll = 0;
        } else if self.scroll > text_width.saturating_sub(1) {
            self.scroll = text_width.saturating_sub(1);
        }
    }
}

/// Single-line text input widget.
pub struct Input {
    /// Text buffer for the input.
    buffer: InputBuffer,
}

#[derive_commands]
impl Input {
    /// Construct a new input with initial text.
    pub fn new(txt: impl Into<String>) -> Self {
        Self {
            buffer: InputBuffer::new(txt),
        }
    }

    /// Return the current input text.
    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    /// Return the raw input value without padding.
    pub fn value(&self) -> &str {
        self.buffer.value()
    }

    /// Replace the input value and reset the cursor.
    pub fn set_value(&mut self, value: impl Into<String>) {
        self.buffer = InputBuffer::new(value);
    }

    /// Move the cursor left.
    #[command]
    fn left(&mut self, _c: &mut dyn Context) {
        let _ = self.buffer.left();
    }

    /// Move the cursor right.
    #[command]
    fn right(&mut self, _c: &mut dyn Context) {
        let _ = self.buffer.right();
    }

    /// Delete a character at the input location.
    #[command]
    fn backspace(&mut self, _c: &mut dyn Context) {
        let _ = self.buffer.backspace();
    }
}

impl Widget for Input {
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn cursor(&self) -> Option<cursor::Cursor> {
        Some(cursor::Cursor {
            location: Point {
                x: self.buffer.cursor_display(),
                y: 0,
            },
            shape: cursor::CursorShape::Block,
            blink: true,
        })
    }

    fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let content_origin = view.content_origin();
        self.buffer.set_display_width(view_rect.w as usize);
        let line = Line::new(content_origin.x, content_origin.y, view_rect.w);
        let content = self.buffer.render_text();
        r.text("text", line, &content)
    }

    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        match event {
            Event::Key(key::Key {
                key: key::KeyCode::Char(c),
                ..
            }) => {
                self.buffer.insert(*c);
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
        }
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let text_len = self.buffer.display_width().max(1);
        c.clamp(Size::new(text_len, 1))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("input")
    }
}

/// Replace newlines in single-line input values.
fn sanitize_single_line(value: &str) -> String {
    value.replace(['\n', '\r'], " ")
}

/// Expand tabs into spaces using the configured tab stop.
fn expand_tabs(text: &str, tab_stop: usize) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    for grapheme in text.graphemes(true) {
        if grapheme == "\t" {
            let width = tab_width(col, tab_stop);
            out.extend(repeat_n(' ', width));
            col = col.saturating_add(width);
        } else {
            out.push_str(grapheme);
            col = col.saturating_add(text::grapheme_width(grapheme));
        }
    }
    out
}

/// Convert a char index to a byte index in a string.
fn byte_index_for_char(text: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    if char_index >= text.chars().count() {
        return text.len();
    }
    text.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use unicode_width::UnicodeWidthStr;

    use super::InputBuffer;

    #[test]
    fn input_buffer_handles_multibyte_chars() {
        let mut buf = InputBuffer::new("a");
        buf.set_display_width(10);
        let accent = '\u{00e9}';
        buf.insert(accent);
        let expected = format!("a{accent}");
        assert_eq!(buf.value(), expected);
        assert_eq!(
            buf.cursor_display(),
            UnicodeWidthStr::width(expected.as_str()) as u32
        );
        buf.left();
        assert_eq!(buf.cursor_display(), UnicodeWidthStr::width("a") as u32);
        buf.backspace();
        assert_eq!(buf.value(), accent.to_string());
    }

    #[test]
    fn input_buffer_handles_grapheme_clusters() {
        let astronaut = "\u{1f469}\u{200d}\u{1f680}";
        let mut buf = InputBuffer::new(format!("a{astronaut}b"));
        buf.set_display_width(10);
        let expected = format!("a{astronaut}b");
        assert_eq!(
            buf.cursor_display(),
            UnicodeWidthStr::width(expected.as_str()) as u32
        );
        buf.left();
        let expected = format!("a{astronaut}");
        assert_eq!(
            buf.cursor_display(),
            UnicodeWidthStr::width(expected.as_str()) as u32
        );
        buf.backspace();
        assert_eq!(buf.value(), "ab");
    }
}
