use canopy_core as canopy;

use canopy_core::{
    Context, Layout, Node, NodeState, Render, Result, StatefulNode, command, derive_commands,
    event::key,
    geom::{Expanse, LineSegment, Point},
    *,
};

/// A text buffer that exposes edit functionality for a single line. It also
/// keeps track of a display window that slides within the line, responding
/// naturally to cursor movements.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TextBuf {
    pub value: String,

    cursor_pos: u32,
    window: LineSegment,
}

impl TextBuf {
    fn new(start: &str) -> Self {
        TextBuf {
            value: start.to_owned(),
            cursor_pos: start.len() as u32,
            window: LineSegment { off: 0, len: 0 },
        }
    }

    /// The location of the displayed cursor along the x axis
    fn cursor_display(&self) -> u32 {
        self.cursor_pos - self.window.off
    }

    fn text(&self) -> String {
        let end = self.window.far().min(self.value.len() as u32) as usize;
        let v = self.value[self.window.off as usize..end].to_owned();
        let extra = self.window.len as usize - v.len();
        format!("{}{}", v, " ".repeat(extra))
    }

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

    /// Should be called during layout
    fn set_display_width(&mut self, val: usize) {
        self.window = LineSegment {
            off: self.window.off,
            len: val as u32,
        };
    }

    pub fn goto(&mut self, loc: u32) -> bool {
        let changed = self.cursor_pos != loc;
        self.cursor_pos = loc;
        self.fix_window();
        changed
    }
    pub fn insert(&mut self, c: char) -> bool {
        self.value.insert(self.cursor_pos as usize, c);
        self.cursor_pos += 1;
        self.fix_window();
        true
    }
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
    pub fn left(&mut self) -> bool {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.fix_window();
            true
        } else {
            false
        }
    }
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
#[derive(canopy_core::StatefulNode)]
pub struct Input {
    state: NodeState,
    pub textbuf: TextBuf,
}

#[derive_commands]
impl Input {
    pub fn new(txt: &str) -> Self {
        Input {
            state: NodeState::default(),
            textbuf: TextBuf::new(txt),
        }
    }
    pub fn text(&self) -> String {
        self.textbuf.text()
    }

    /// Move the cursor left.
    #[command]
    fn left(&mut self, c: &mut dyn Context) {
        if self.textbuf.left() {
            c.taint(self);
        }
    }

    /// Move the cursor right.
    #[command]
    fn right(&mut self, c: &mut dyn Context) {
        if self.textbuf.right() {
            c.taint(self);
        }
    }

    /// Delete a character at the input location.
    #[command]
    fn backspace(&mut self, c: &mut dyn Context) {
        if self.textbuf.backspace() {
            c.taint(self);
        }
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
        r.text("text", self.vp().view().line(0), &self.textbuf.text())
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

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        self.textbuf.set_display_width(sz.w as usize);
        let tbl = self.textbuf.value.len() as u32;
        let expanse = if self.textbuf.window.len >= tbl {
            sz
        } else {
            Expanse::new(tbl, 1)
        };
        l.size(self, expanse, sz)?;
        Ok(())
    }
}
