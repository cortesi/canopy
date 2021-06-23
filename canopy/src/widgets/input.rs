use std::io::Write;
use std::marker::PhantomData;

use anyhow::{format_err, Result};

use crate as canopy;
use crate::{
    cursor,
    event::key,
    geom::{Point, Rect},
    layout::FixedLayout,
    state::{NodeState, StatefulNode},
    widgets::frame,
    Canopy, EventResult, Node,
};

use crossterm::{cursor::MoveTo, style::Print, QueueableCommand};

// A text buffer that exposes edit functionality for a single line. It also
// keeps track of a display window that slides within the line, responding
// naturally to cursor movements.
#[derive(Debug, PartialEq, Clone)]
pub struct TextBuf {
    pub value: String,

    cursor_pos: usize,
    display_width: usize,
    display_loc: usize,
}

impl TextBuf {
    fn new(start: &str) -> Self {
        TextBuf {
            value: start.to_owned(),
            cursor_pos: start.len(),
            display_width: 0,
            display_loc: 0,
        }
    }
    /// The location of the displayed cursor along the x axis
    fn cursor_display(&self) -> usize {
        self.cursor_pos - self.display_loc
    }
    fn text(&self) -> String {
        let end = (self.display_loc + self.display_width).min(self.value.len());
        let mut v = self.value[self.display_loc..end].to_owned();
        let extra = self.display_width - v.len();
        v = v + &" ".repeat(extra);
        v
    }
    fn fix_window(&mut self) {
        if self.cursor_pos > self.value.len() {
            self.cursor_pos = self.value.len()
        }
        if self.cursor_pos < self.display_loc {
            self.display_loc = self.cursor_pos;
        } else if self.cursor_pos >= self.display_loc + self.display_width {
            self.display_loc = self.cursor_pos - self.display_width;
            // When we're right at the end of the sequence, we need one extra
            // character for the cursor.
            if self.cursor_pos == self.value.len() {
                self.display_loc += 1
            }
        }

        if self.cursor_display() >= self.display_width {
            let delta = self.cursor_display() - self.display_width + 1;
            self.display_loc += delta;
        }
    }
    /// Should be called during layout
    fn set_display_width(&mut self, val: usize) {
        self.display_width = val;
    }

    fn goto(&mut self, loc: usize) {
        self.cursor_pos = loc;
        self.fix_window();
    }
    fn insert(&mut self, c: char) {
        self.value.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
        self.fix_window();
    }
    fn backspace(&mut self) {
        if self.value.len() > 0 {
            self.value.remove(self.cursor_pos - 1);
            self.cursor_pos -= 1;
            self.fix_window();
        }
    }
    fn left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.fix_window();
        }
    }
    fn right(&mut self) {
        if self.cursor_pos < self.value.len() {
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
}

impl<S> FixedLayout<S> for InputLine<S> {
    fn layout(&mut self, _app: &mut Canopy<S>, rect: Option<Rect>) -> Result<()> {
        if let Some(r) = rect {
            if r.h != 1 {
                return Err(format_err!("InputLine height must be exactly 1."));
            }
            self.textbuf.set_display_width(r.w as usize);
        }
        self.set_rect(rect);
        Ok(())
    }
}

impl<S> frame::FrameContent for InputLine<S> {
    fn bounds(&self) -> Option<(Rect, Rect)> {
        if let Some(r) = self.rect() {
            if self.textbuf.display_width >= self.textbuf.value.len() {
                let r = Rect {
                    tl: Point { x: 0, y: 0 },
                    w: r.w,
                    h: 1,
                };
                Some((r, r))
            } else {
                Some((
                    Rect {
                        tl: Point {
                            x: self.textbuf.display_loc as u16,
                            y: 0,
                        },
                        w: self.textbuf.display_width as u16,
                        h: 1,
                    },
                    Rect {
                        tl: Point { x: 0, y: 0 },
                        w: self.textbuf.value.len() as u16,
                        h: 1,
                    },
                ))
            }
        } else {
            None
        }
    }
}

impl<'a, S> Node<S> for InputLine<S> {
    fn can_focus(&self) -> bool {
        true
    }
    fn cursor(&mut self) -> Option<cursor::Cursor> {
        Some(cursor::Cursor {
            location: Point {
                x: self.textbuf.cursor_display() as u16,
                y: 0,
            },
            shape: cursor::CursorShape::Block,
            blink: true,
        })
    }
    fn render(&mut self, _app: &mut Canopy<S>, w: &mut dyn Write) -> Result<()> {
        if let Some(r) = self.rect() {
            w.queue(MoveTo(r.tl.x, r.tl.y))?;
            w.queue(Print(&self.textbuf.text()))?;
        }
        Ok(())
    }
    fn handle_key(&mut self, _app: &mut Canopy<S>, _: &mut S, k: key::Key) -> Result<EventResult> {
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
            _ => return Ok(EventResult::Ignore { skip: false }),
        };
        Ok(EventResult::Handle { skip: false })
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
