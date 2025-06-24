use crate as canopy;
use crate::{
    event::key,
    geom::{Expanse, LineSegment, Point},
    state::{NodeState, StatefulNode},
    *,
};

/// A text buffer that exposes edit functionality for a single line. It also
/// keeps track of a display window that slides within the line, responding
/// naturally to cursor movements.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TextBuf {
    pub value: String,

    cursor_pos: u16,
    window: LineSegment,
}

impl TextBuf {
    fn new(start: &str) -> Self {
        TextBuf {
            value: start.to_owned(),
            cursor_pos: start.len() as u16,
            window: LineSegment { off: 0, len: 0 },
        }
    }

    /// The location of the displayed cursor along the x axis
    fn cursor_display(&self) -> u16 {
        self.cursor_pos - self.window.off
    }

    fn text(&self) -> String {
        let end = self.window.far().min(self.value.len() as u16) as usize;
        let v = self.value[self.window.off as usize..end].to_owned();
        let extra = self.window.len as usize - v.len();
        format!("{}{}", v, " ".repeat(extra))
    }

    fn fix_window(&mut self) {
        if self.cursor_pos > self.value.len() as u16 {
            self.cursor_pos = self.value.len() as u16
        }
        if self.cursor_pos < self.window.off {
            self.window.off = self.cursor_pos;
        } else if self.cursor_pos >= self.window.far() {
            let mut off = self.cursor_pos - self.window.len;
            // When we're right at the end of the sequence, we need one extra
            // character for the cursor.
            if self.cursor_pos == self.value.len() as u16 {
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
            len: val as u16,
        };
    }

    pub fn goto(&mut self, loc: u16) {
        self.cursor_pos = loc;
        self.fix_window();
    }
    pub fn insert(&mut self, c: char) {
        self.value.insert(self.cursor_pos as usize, c);
        self.cursor_pos += 1;
        self.fix_window();
    }
    pub fn backspace(&mut self) {
        if !self.value.is_empty() && self.cursor_pos > 0 {
            self.value.remove(self.cursor_pos as usize - 1);
            self.cursor_pos -= 1;
            self.fix_window();
        }
    }
    pub fn left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.fix_window();
        }
    }
    pub fn right(&mut self) {
        if self.cursor_pos < self.value.len() as u16 {
            self.cursor_pos += 1;
            self.fix_window();
        }
    }
}

/// A single input line, one character high.
#[derive(StatefulNode)]
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
    fn left(&mut self, _: &dyn Context) {
        self.textbuf.left();
    }

    /// Move the cursor right.
    #[command]
    fn right(&mut self, _: &dyn Context) {
        self.textbuf.right();
    }

    /// Delete a character at the input location.
    #[command]
    fn backspace(&mut self, _: &dyn Context) {
        self.textbuf.backspace();
    }
}

impl DefaultBindings for Input {
    fn defaults(b: Binder) -> Binder {
        b.key(key::KeyCode::Left, "input::left()")
            .key(key::KeyCode::Right, "input::right()")
            .key(key::KeyCode::Backspace, "input::backspace()")
    }
}

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
        let tbl = self.textbuf.value.len() as u16;
        let expanse = if self.textbuf.window.len >= tbl {
            sz
        } else {
            Expanse::new(tbl, 1)
        };
        l.size(self, expanse, sz)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textbuf_basic() -> Result<()> {
        let mut t = TextBuf::new("");
        t.set_display_width(3);
        t.left();
        assert_eq!(t.text(), "   ");
        t.right();
        assert_eq!(t.text(), "   ");
        t.backspace();
        assert_eq!(t.text(), "   ");

        t.insert('c');
        assert_eq!(t.text(), "c  ");
        assert_eq!(t.cursor_pos, 1);

        t.left();
        assert_eq!(t.text(), "c  ");
        assert_eq!(t.cursor_pos, 0);

        t.right();
        assert_eq!(t.text(), "c  ");
        assert_eq!(t.cursor_pos, 1);

        t.right();
        assert_eq!(t.text(), "c  ");
        assert_eq!(t.cursor_pos, 1);

        t.insert('a');
        assert_eq!(t.text(), "ca ");
        assert_eq!(t.cursor_pos, 2);

        t.insert('g');
        assert_eq!(t.text(), "ag ");
        assert_eq!(t.cursor_pos, 3);

        t.insert('t');
        assert_eq!(t.text(), "gt ");
        assert_eq!(t.cursor_pos, 4);

        t.insert('f');
        assert_eq!(t.text(), "tf ");
        assert_eq!(t.cursor_pos, 5);

        t.goto(0);
        assert_eq!(t.text(), "cag");
        assert_eq!(t.cursor_pos, 0);

        t.right();
        t.right();
        t.right();
        assert_eq!(t.text(), "agt");
        assert_eq!(t.cursor_pos, 3);
        assert_eq!(t.cursor_display(), 2);

        Ok(())
    }
}
