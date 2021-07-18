use super::Backend;
use crate::{geom, Result};
use std::io::Write;

use crossterm::{
    cursor::MoveTo,
    style::Print,
    style::{Color, SetBackgroundColor, SetForegroundColor},
    QueueableCommand,
};

pub struct Term<'a> {
    fp: &'a mut dyn Write,
}

impl<'a> Backend for Term<'a> {
    fn fg(&mut self, c: Color) -> Result<()> {
        self.fp.queue(SetForegroundColor(c))?;
        Ok(())
    }

    fn bg(&mut self, c: Color) -> Result<()> {
        self.fp.queue(SetBackgroundColor(c))?;
        Ok(())
    }

    fn fill(&mut self, r: geom::Rect, c: char) -> Result<()> {
        let line = c.to_string().repeat(r.w as usize);
        for n in 0..r.h {
            self.fp.queue(MoveTo(r.tl.x, r.tl.y + n))?;
            self.fp.queue(Print(&line))?;
        }
        Ok(())
    }

    fn text(&mut self, loc: geom::Point, txt: &str) -> Result<()> {
        self.fp.queue(MoveTo(loc.x, loc.y))?;
        self.fp.queue(Print(txt))?;
        Ok(())
    }
}
