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

#[derive(Debug, PartialEq, Clone)]
pub struct TextBuf {
    pub value: String,
    cursor_pos: usize,
}

impl TextBuf {
    fn new(start: &str) -> Self {
        TextBuf {
            value: start.to_owned(),
            cursor_pos: start.len(),
        }
    }
    fn insert(&mut self, c: char) {
        self.value.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }
    fn backspace(&mut self) {
        if self.value.len() > 0 {
            self.value.remove(self.cursor_pos - 1);
            self.cursor_pos -= 1;
        }
    }
    fn left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }
    fn right(&mut self) {
        if self.cursor_pos < self.value.len() {
            self.cursor_pos += 1;
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
        }
        self.set_rect(rect);
        Ok(())
    }
}

impl<S> frame::FrameContent for InputLine<S> {
    fn bounds(&self) -> Option<(Rect, Rect)> {
        if let Some(r) = self.rect() {
            let vr = Rect {
                tl: Point { x: 0, y: 0 },
                w: r.w,
                h: r.h,
            };
            Some((vr, vr))
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
                x: self.textbuf.cursor_pos as u16,
                y: 0,
            },
            shape: cursor::CursorShape::Block,
            blink: true,
        })
    }
    fn render(&mut self, _app: &mut Canopy<S>, w: &mut dyn Write) -> Result<()> {
        if let Some(r) = self.rect() {
            w.queue(MoveTo(r.tl.x, r.tl.y))?;
            w.queue(Print(&self.textbuf.value))?;
            w.queue(Print(" ".repeat(r.w as usize - self.textbuf.value.len())))?;
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
    fn textbuf() -> Result<()> {
        let mut t = TextBuf::new("");
        t.left();
        assert_eq!(
            t,
            TextBuf {
                cursor_pos: 0,
                value: "".into()
            }
        );
        t.right();
        assert_eq!(
            t,
            TextBuf {
                cursor_pos: 0,
                value: "".into()
            }
        );
        t.backspace();
        assert_eq!(
            t,
            TextBuf {
                cursor_pos: 0,
                value: "".into()
            }
        );
        t.insert('c');
        assert_eq!(
            t,
            TextBuf {
                cursor_pos: 1,
                value: "c".into()
            }
        );
        t.left();
        assert_eq!(
            t,
            TextBuf {
                cursor_pos: 0,
                value: "c".into()
            }
        );
        t.right();
        assert_eq!(
            t,
            TextBuf {
                cursor_pos: 1,
                value: "c".into()
            }
        );
        t.insert('a');
        assert_eq!(
            t,
            TextBuf {
                cursor_pos: 2,
                value: "ca".into()
            }
        );

        Ok(())
    }
}
