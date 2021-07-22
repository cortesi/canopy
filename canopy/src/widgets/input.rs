use std::marker::PhantomData;

use crate as canopy;
use crate::{
    cursor,
    error::Error,
    event::key,
    geom::{LineSegment, Point, Rect, View},
    layout::Layout,
    state::{NodeState, StatefulNode},
    widgets::frame,
    Canopy, EventOutcome, Node, Result,
};

/// A text buffer that exposes edit functionality for a single line. It also
/// keeps track of a display window that slides within the line, responding
/// naturally to cursor movements.
#[derive(Debug, PartialEq, Clone)]
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
        let mut v = self.value[self.window.off as usize..end].to_owned();
        let extra = self.window.len as usize - v.len();
        v = v + &" ".repeat(extra);
        v
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
            self.window.off = off as u16;
        }

        if self.cursor_display() >= self.window.len {
            let delta = self.cursor_display() - self.window.len + 1;
            self.window.off += delta as u16;
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
pub struct InputLine<S> {
    state: NodeState,
    _marker: PhantomData<S>,
    pub textbuf: TextBuf,
}

impl<S> InputLine<S> {
    pub fn new(txt: &str) -> Self {
        InputLine {
            state: NodeState::default(),
            _marker: PhantomData,
            textbuf: TextBuf::new(txt),
        }
    }
    fn resize(&mut self) -> Result<()> {
        let r = self.state().view.screen();
        if self.textbuf.window.len >= self.textbuf.value.len() as u16 {
            let r = Rect::new(0, 0, r.w, 1);
            self.state_mut().view = View::new(r, r)?;
        } else {
            let view = Rect::new(0, 0, self.textbuf.value.len() as u16, 1);
            self.state_mut().view = View::new(
                Rect::new(self.textbuf.window.off, 0, self.textbuf.window.len, 1)
                    .clamp(view)
                    .unwrap(),
                view,
            )?;
        }
        Ok(())
    }
}

impl<S> Layout<S> for InputLine<S> {
    fn layout_children(&mut self, _app: &mut Canopy<S>) -> Result<()> {
        let rect = self.state().view.screen();
        if rect.h != 1 {
            return Err(Error::Layout("InputLine height must be exactly 1.".into()));
        }
        self.textbuf.set_display_width(rect.w as usize);
        Ok(())
    }
}

impl<S> frame::FrameContent for InputLine<S> {}

impl<'a, S> Node<S> for InputLine<S> {
    fn can_focus(&self) -> bool {
        true
    }
    fn cursor(&self) -> Option<cursor::Cursor> {
        Some(cursor::Cursor {
            location: Point {
                x: self.textbuf.cursor_display() as u16,
                y: 0,
            },
            shape: cursor::CursorShape::Block,
            blink: true,
        })
    }

    fn render(&self, app: &mut Canopy<S>) -> Result<()> {
        app.render
            .text("text", self.state().view.screen(), &self.textbuf.text())
    }

    fn handle_key(&mut self, _app: &mut Canopy<S>, _: &mut S, k: key::Key) -> Result<EventOutcome> {
        match k {
            key::Key(_, key::KeyCode::Left) => {
                self.textbuf.left();
            }
            key::Key(_, key::KeyCode::Right) => {
                self.textbuf.right();
            }
            key::Key(_, key::KeyCode::Backspace) => {
                self.textbuf.backspace();
            }
            key::Key(_, key::KeyCode::Char(c)) => {
                self.textbuf.insert(c);
            }
            _ => return Ok(EventOutcome::Ignore { skip: false }),
        };
        self.resize()?;
        Ok(EventOutcome::Handle { skip: false })
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
