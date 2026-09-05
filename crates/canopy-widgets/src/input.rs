use canopy::{
    Context, EventOutcome, ViewContext, Widget, WidgetSemantics, command, cursor, derive_commands,
    error::Result,
    event::{Event, key},
    geom::{Line, Point},
    layout::{MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    style::{WidgetState, roles},
    text,
};

use crate::editor::{TextBuffer, TextPosition};

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

    /// Return the raw input value.
    fn value(&self) -> &str {
        &self.value
    }

    /// Return the visible text for rendering.
    fn render_text(&self) -> String {
        if self.view_width == 0 {
            return String::new();
        }
        let expanded = text::expand_tabs(&self.value, self.tab_stop);
        let (out, _) = text::slice_by_columns(&expanded, self.scroll, self.view_width);
        out.to_string()
    }

    /// Insert a character at the cursor position.
    fn insert(&mut self, c: char) {
        let insert = match c {
            '\n' | '\r' => ' ',
            _ => c,
        };
        self.buffer.insert_text(&insert.to_string());
        self.sync_value();
        self.ensure_cursor_visible();
    }

    /// Delete the character before the cursor.
    fn backspace(&mut self) {
        if self.buffer.delete_backward(false) {
            self.sync_value();
            self.ensure_cursor_visible();
        }
    }

    /// Move the cursor left by one character.
    fn left(&mut self) {
        if self.buffer.move_left(false) {
            self.ensure_cursor_visible();
        }
    }

    /// Move the cursor right by one character.
    fn right(&mut self) {
        if self.buffer.move_right(false) {
            self.ensure_cursor_visible();
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
        let max_scroll = text_width.saturating_add(1).saturating_sub(self.view_width);
        self.scroll = self.scroll.min(max_scroll);
    }
}

/// Single-line text input widget.
pub struct Input {
    /// Text buffer for the input.
    buffer: InputBuffer,
    /// Optional semantic label independent of the text value.
    label: Option<String>,
    /// Policy for exposing the value in semantic snapshots.
    value_exposure: ValueExposure,
}

/// Policy for publishing an input value in semantic snapshots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ValueExposure {
    /// Omit the value from semantics.
    #[default]
    Omit,
    /// Publish the raw single-line value.
    Public,
    /// Mark the value as sensitive and always omit it from semantics.
    Sensitive,
}

#[derive_commands]
impl Input {
    /// Construct a new input with initial text.
    pub fn new(txt: impl Into<String>) -> Self {
        Self {
            buffer: InputBuffer::new(txt),
            label: None,
            value_exposure: ValueExposure::Omit,
        }
    }

    /// Set the semantic label without changing the displayed value.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Configure whether semantic snapshots expose this input's value.
    pub fn with_value_exposure(mut self, exposure: ValueExposure) -> Self {
        self.value_exposure = exposure;
        self
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
        self.buffer.left();
    }

    /// Move the cursor right.
    #[command]
    fn right(&mut self, _c: &mut dyn Context) {
        self.buffer.right();
    }

    /// Delete a character at the input location.
    #[command]
    fn backspace(&mut self, _c: &mut dyn Context) {
        self.buffer.backspace();
    }
}

impl Widget for Input {
    fn semantics(&self, _ctx: &dyn ViewContext) -> Result<WidgetSemantics> {
        Ok(WidgetSemantics {
            role: Some("input".into()),
            label: self.label.clone(),
            value: (self.value_exposure == ValueExposure::Public).then(|| self.value().to_owned()),
            ..WidgetSemantics::default()
        })
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn cursor(&self) -> Option<cursor::Cursor> {
        Some(cursor::Cursor {
            location: Point {
                x: self.buffer.cursor_display(),
                y: 0,
            },
            shape: cursor::CursorShape::Block,
        })
    }

    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer(roles::INPUT);
        if ctx.is_focused() {
            r.push_layer(WidgetState::Focused.layer());
        }
        let view = ctx.view();
        let view_rect = view.view_rect();
        let content_origin = view.content_origin();
        self.buffer.set_display_width(view_rect.w as usize);
        let line = Line::new(content_origin.x, content_origin.y, view_rect.w);
        let content = self.buffer.render_text();
        r.text(roles::INPUT_TEXT, line, &content)?;
        if ctx.is_focused() && self.buffer.cursor_display() < view_rect.w {
            r.restyle(
                roles::INPUT_CURSOR,
                Point {
                    x: content_origin.x + self.buffer.cursor_display(),
                    y: content_origin.y,
                },
            );
        }
        Ok(())
    }

    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        let outcome = match event {
            Event::Key(key::Key {
                key: key::KeyCode::Char(c),
                mods,
            }) if !mods.ctrl && !mods.alt => {
                self.buffer.insert(*c);
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
        };
        Ok(outcome)
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

#[cfg(test)]
mod tests {
    use canopy::{
        EventOutcome, Widget,
        event::{Event, key},
        testing::dummyctx::DummyContext,
    };
    use unicode_width::UnicodeWidthStr;

    use super::{Input, InputBuffer, ValueExposure};

    #[test]
    fn semantic_values_require_explicit_public_exposure() {
        for exposure in [
            ValueExposure::Omit,
            ValueExposure::Public,
            ValueExposure::Sensitive,
        ] {
            let mut input = Input::new("secret")
                .with_label("Account")
                .with_value_exposure(exposure);
            input.set_value("updated");
            let semantics = input
                .semantics(&DummyContext::default())
                .expect("input semantics");
            assert_eq!(semantics.role.as_deref(), Some("input"));
            assert_eq!(semantics.label.as_deref(), Some("Account"));
            assert_eq!(
                semantics.value.as_deref(),
                (exposure == ValueExposure::Public).then_some("updated")
            );
        }
        assert_eq!(
            Input::new("private")
                .semantics(&DummyContext::default())
                .expect("default semantics")
                .value,
            None
        );
    }

    #[test]
    fn input_caret_stays_inside_nonempty_viewports() {
        for width in [1, 3] {
            for text in ["", "a", "abc", "abcdef", "界a", "e\u{301}ab"] {
                let mut buf = InputBuffer::new(text);
                buf.set_display_width(width);
                assert!(
                    buf.cursor_display() < width as u32,
                    "{text:?}, width {width}"
                );
                for _ in 0..text.chars().count() {
                    buf.left();
                    assert!(buf.cursor_display() < width as u32);
                }
                assert_eq!(buf.scroll, 0);
                for _ in 0..text.chars().count() {
                    buf.right();
                    assert!(buf.cursor_display() < width as u32);
                }
                for _ in 0..text.chars().count() {
                    buf.backspace();
                    assert!(buf.cursor_display() < width as u32);
                }
                assert_eq!(buf.value(), "");
            }
        }
    }

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
    fn input_ignores_ctrl_and_alt_chords() {
        let mut input = Input::new("");
        let mut ctx = DummyContext::default();
        for mods in [key::Ctrl, key::Alt] {
            let event = Event::Key(key::Key {
                key: key::KeyCode::Char('a'),
                mods,
            });
            assert_eq!(
                input.on_event(&event, &mut ctx).unwrap(),
                EventOutcome::Ignore
            );
            assert_eq!(input.value(), "");
        }

        let event = Event::Key(key::Key {
            key: key::KeyCode::Char('a'),
            mods: key::Empty,
        });
        assert_eq!(
            input.on_event(&event, &mut ctx).unwrap(),
            EventOutcome::Handle
        );
        assert_eq!(input.value(), "a");
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
