use crate::{
    Context, Layout, command, cursor, derive_commands,
    error::Result,
    event::key,
    geom::{Expanse, LineSegment, Point},
    node::{EventOutcome, Node},
    render::Render,
    state::{NodeState, StatefulNode},
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
        self.cursor_pos - self.window.off
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
            let mut off = self.cursor_pos - self.window.len;
            // When we're right at the end of the sequence, we need one extra
            // character for the cursor.
            if self.cursor_pos == self.value.len() as u32 {
                off += 1
            }
            self.window.off = off;
        }

        if self.cursor_display() >= self.window.len {
            let delta = self.cursor_display() - self.window.len + 1;
            self.window.off += delta;
        }
    }

    /// Set the visible window width during layout.
    fn set_display_width(&mut self, val: usize) {
        self.window = LineSegment {
            off: self.window.off,
            len: val as u32,
        };
    }

    /// Insert a character at the cursor position.
    pub fn insert(&mut self, c: char) -> bool {
        self.value.insert(self.cursor_pos as usize, c);
        self.cursor_pos += 1;
        self.fix_window();
        true
    }
    /// Delete the character before the cursor.
    pub fn backspace(&mut self) -> bool {
        if !self.value.is_empty() && self.cursor_pos > 0 {
            self.value.remove(self.cursor_pos as usize - 1);
            self.cursor_pos -= 1;
            self.fix_window();
            true
        } else {
            false
        }
    }
    /// Move the cursor left by one character.
    pub fn left(&mut self) -> bool {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.fix_window();
            true
        } else {
            false
        }
    }
    /// Move the cursor right by one character.
    pub fn right(&mut self) -> bool {
        if self.cursor_pos < self.value.len() as u32 {
            self.cursor_pos += 1;
            self.fix_window();
            true
        } else {
            false
        }
    }
}

/// A single input line, one character high.
#[derive(canopy::StatefulNode)]
/// Single-line text input widget.
pub struct Input {
    /// Node state.
    state: NodeState,
    /// Text buffer for the input.
    textbuf: TextBuf,
}

#[derive_commands]
impl Input {
    /// Construct a new input with initial text.
    pub fn new(txt: impl Into<String>) -> Self {
        Self {
            state: NodeState::default(),
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

    /// Move the cursor left.
    #[command]
    fn left(&mut self, _c: &mut dyn Context) {
        if self.textbuf.left() {}
    }

    /// Move the cursor right.
    #[command]
    fn right(&mut self, _c: &mut dyn Context) {
        if self.textbuf.right() {}
    }

    /// Delete a character at the input location.
    #[command]
    fn backspace(&mut self, _c: &mut dyn Context) {
        if self.textbuf.backspace() {}
    }
}

// DefaultBindings is part of canopy, not canopy-core
// impl DefaultBindings for Input {
//     fn defaults(b: Binder) -> Binder {
//         b.key(key::KeyCode::Left, "input::left()")
//             .key(key::KeyCode::Right, "input::right()")
//             .key(key::KeyCode::Backspace, "input::backspace()")
//     }
// }

impl Node for Input {
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

    fn render(&mut self, _: &dyn Context, r: &mut Render) -> Result<()> {
        r.text("text", self.vp().view().line(0), self.textbuf.text())
    }

    fn handle_key(&mut self, _c: &mut dyn Context, k: key::Key) -> Result<EventOutcome> {
        match k {
            key::Key {
                mods: _,
                key: key::KeyCode::Char(c),
            } => {
                self.textbuf.insert(c);
                Ok(EventOutcome::Handle)
            }
            _ => Ok(EventOutcome::Ignore),
        }
    }

    fn layout(&mut self, _l: &Layout, sz: Expanse) -> Result<()> {
        self.textbuf.set_display_width(sz.w as usize);
        let tbl = self.textbuf.value.len() as u32;
        let expanse = if self.textbuf.window.len >= tbl {
            sz
        } else {
            Expanse::new(tbl, 1)
        };
        self.fit_size(expanse, sz);
        Ok(())
    }
}
