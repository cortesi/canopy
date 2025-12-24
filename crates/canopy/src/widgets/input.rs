use taffy::{geometry::Size, style::AvailableSpace};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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

    /// Cursor position in grapheme clusters.
    cursor_pos: usize,
    /// Visible window into the value, measured in display columns.
    window: LineSegment,
}

/// Metadata describing a grapheme cluster in the buffer.
#[derive(Debug, Clone, Copy)]
struct GraphemeInfo {
    /// Byte offset where the grapheme starts.
    start: usize,
    /// Byte offset where the grapheme ends.
    end: usize,
    /// Display width of the grapheme in columns.
    width: u32,
}

impl TextBuf {
    /// Construct a new text buffer with initial content.
    fn new(start: impl Into<String>) -> Self {
        let value = start.into();
        let cursor_pos = value.graphemes(true).count();
        Self {
            value,
            cursor_pos,
            window: LineSegment { off: 0, len: 0 },
        }
    }

    /// The location of the displayed cursor along the x axis.
    fn cursor_display(&self) -> u32 {
        let info = self.grapheme_info();
        let cursor_cols = self.cursor_columns(&info);
        cursor_cols.saturating_sub(self.window.off)
    }

    /// Return the visible text slice.
    fn text(&self) -> &str {
        if self.window.len == 0 {
            return "";
        }
        let info = self.grapheme_info();
        let (start, end) = self.visible_range(&info);
        &self.value[start..end]
    }

    /// Clamp cursor and window state to valid bounds.
    fn fix_window(&mut self) {
        let info = self.grapheme_info();
        if self.cursor_pos > info.len() {
            self.cursor_pos = info.len();
        }
        if self.window.len == 0 {
            self.window.off = 0;
            return;
        }
        let cursor_cols = self.cursor_columns(&info);
        let text_width = self.text_width(&info);
        if cursor_cols < self.window.off {
            self.window.off = cursor_cols;
        } else {
            let window_end = self.window.off.saturating_add(self.window.len);
            if cursor_cols >= window_end {
                let delta = cursor_cols.saturating_sub(window_end).saturating_add(1);
                self.window.off = self.window.off.saturating_add(delta);
            }
        }

        if text_width <= self.window.len {
            self.window.off = 0;
        } else if self.window.off > text_width.saturating_sub(1) {
            self.window.off = text_width.saturating_sub(1);
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
        let info = self.grapheme_info();
        let byte_index = self.cursor_byte_index(&info);
        self.value.insert(byte_index, c);
        let info = self.grapheme_info();
        self.cursor_pos = self.grapheme_index_for_byte(&info, byte_index + c.len_utf8());
        self.fix_window();
        true
    }
    /// Delete the character before the cursor.
    pub fn backspace(&mut self) -> bool {
        if !self.value.is_empty() && self.cursor_pos > 0 {
            let info = self.grapheme_info();
            let remove_index = self.cursor_pos.saturating_sub(1);
            if let Some(g) = info.get(remove_index) {
                let start = g.start;
                self.value.replace_range(g.start..g.end, "");
                let info = self.grapheme_info();
                self.cursor_pos = self.grapheme_index_for_byte(&info, start);
            } else {
                self.cursor_pos = 0;
            }
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
        let info = self.grapheme_info();
        if self.cursor_pos < info.len() {
            self.cursor_pos = self.cursor_pos.saturating_add(1);
            self.fix_window();
            true
        } else {
            false
        }
    }

    /// Collect grapheme metadata for the current buffer.
    fn grapheme_info(&self) -> Vec<GraphemeInfo> {
        self.value
            .grapheme_indices(true)
            .map(|(start, g)| GraphemeInfo {
                start,
                end: start + g.len(),
                width: UnicodeWidthStr::width(g) as u32,
            })
            .collect()
    }

    /// Compute the display width of the buffer using grapheme metadata.
    fn text_width(&self, info: &[GraphemeInfo]) -> u32 {
        info.iter().map(|g| g.width).sum()
    }

    /// Compute the cursor column position using grapheme metadata.
    fn cursor_columns(&self, info: &[GraphemeInfo]) -> u32 {
        info.iter().take(self.cursor_pos).map(|g| g.width).sum()
    }

    /// Find the byte index for the cursor position.
    fn cursor_byte_index(&self, info: &[GraphemeInfo]) -> usize {
        if self.cursor_pos >= info.len() {
            self.value.len()
        } else {
            info[self.cursor_pos].start
        }
    }

    /// Map a byte offset to a grapheme index.
    fn grapheme_index_for_byte(&self, info: &[GraphemeInfo], byte: usize) -> usize {
        info.iter().take_while(|g| g.start < byte).count()
    }

    /// Compute the byte range visible in the current window.
    fn visible_range(&self, info: &[GraphemeInfo]) -> (usize, usize) {
        if info.is_empty() {
            return (0, 0);
        }
        let window_start = self.window.off;
        let window_end = self.window.off.saturating_add(self.window.len);
        let mut col: u32 = 0;
        let mut start = None;
        let mut end = 0;
        for g in info {
            let g_start = col;
            let g_end = col.saturating_add(g.width);
            if start.is_none() && g_end > window_start {
                start = Some(g.start);
            }
            if start.is_some() {
                if g_start < window_end {
                    end = g.end;
                } else {
                    break;
                }
            }
            col = g_end;
        }
        let start = start.unwrap_or(self.value.len());
        if start == self.value.len() {
            (start, start)
        } else {
            (start, end)
        }
    }

    /// Return the display width of the full buffer.
    fn display_width(&self) -> u32 {
        let info = self.grapheme_info();
        self.text_width(&info)
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
        let text_len = self.textbuf.display_width() as f32;
        let width = known_dimensions
            .width
            .or_else(|| available_space.width.into_option())
            .unwrap_or(text_len);
        Size {
            width: width.max(text_len),
            height: 1.0,
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("input")
    }
}

#[cfg(test)]
mod tests {
    use unicode_width::UnicodeWidthStr;

    use super::TextBuf;

    #[test]
    fn textbuf_handles_multibyte_chars() {
        let mut buf = TextBuf::new("a");
        buf.set_display_width(10);
        let accent = '\u{00e9}';
        buf.insert(accent);
        let expected = format!("a{accent}");
        assert_eq!(buf.value, expected);
        assert_eq!(
            buf.cursor_display(),
            UnicodeWidthStr::width(expected.as_str()) as u32
        );
        buf.left();
        assert_eq!(buf.cursor_display(), UnicodeWidthStr::width("a") as u32);
        buf.backspace();
        assert_eq!(buf.value, accent.to_string());
    }

    #[test]
    fn textbuf_handles_grapheme_clusters() {
        let astronaut = "\u{1f469}\u{200d}\u{1f680}";
        let mut buf = TextBuf::new(format!("a{astronaut}b"));
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
        assert_eq!(buf.value, "ab");
    }
}
