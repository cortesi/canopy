use crate as canopy;
use crate::{
    geom::{Point, Rect},
    layout::ConstrainedWidthLayout,
    state::{NodeState, StatefulNode},
    style::Style,
    Canopy, Node, Result,
};
use std::io::Write;
use std::marker::PhantomData;

use crossterm::{
    cursor::MoveTo,
    style::{Print, SetBackgroundColor, SetForegroundColor},
    QueueableCommand,
};

use textwrap;

#[derive(StatefulNode)]
pub struct Text<S> {
    pub state: NodeState,
    pub virt_origin: Option<Point>,
    pub raw: String,
    _marker: PhantomData<S>,

    lines: Option<Vec<String>>,
}

impl<S> Text<S> {
    pub fn new(raw: &str) -> Self {
        Text {
            state: NodeState::default(),

            virt_origin: None,

            raw: raw.to_owned(),
            lines: None,
            _marker: PhantomData,
        }
    }
}

impl<'a, S> ConstrainedWidthLayout<S> for Text<S> {
    fn constrain(&mut self, _app: &mut Canopy<S>, w: u16) -> Result<Rect> {
        if let Some(l) = &self.lines {
            if !l.is_empty() && l[0].len() == w as usize {
                return Ok(Rect {
                    tl: Point { x: 0, y: 0 },
                    w,
                    h: l.len() as u16,
                });
            }
        }

        let mut split: Vec<String> = vec![];
        for i in textwrap::wrap(&self.raw, w as usize) {
            split.push(format!("{:width$}", i, width = w as usize))
        }
        let ret = Rect {
            tl: Point { x: 0, y: 0 },
            w,
            h: split.len() as u16,
        };
        self.lines = Some(split);
        Ok(ret)
    }
    fn layout(&mut self, _app: &mut Canopy<S>, virt_origin: Point, rect: Rect) -> Result<()> {
        self.set_rect(rect);
        self.virt_origin = Some(virt_origin);
        Ok(())
    }
}

impl<'a, S> Node<S> for Text<S> {
    fn render(&self, _app: &Canopy<S>, style: &mut Style, w: &mut dyn Write) -> Result<()> {
        let (fg, bg) = style.get("text");
        w.queue(SetForegroundColor(fg))?;
        w.queue(SetBackgroundColor(bg))?;
        if let (Some(lines), Some(vo)) = (self.lines.as_ref(), self.virt_origin) {
            let rect = self.rect();
            for i in 0..rect.h {
                w.queue(MoveTo(rect.tl.x, rect.tl.y + i))?;
                if (vo.y + i) < lines.len() as u16 {
                    let l = &lines[(vo.y + i) as usize];
                    w.queue(Print(&l[(vo.x) as usize..(vo.x + rect.w) as usize]))?;
                } else {
                    w.queue(Print(" ".repeat(rect.w as usize)))?;
                };
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_sizing() -> Result<()> {
        let mut app = Canopy::new();
        let txt = "aaa bbb ccc\nddd eee fff\nggg hhh iii";
        let mut t: Text<()> = Text::new(txt);
        t.constrain(&mut app, 7)?;
        let expected: Vec<String> = vec![
            "aaa bbb".into(),
            "ccc    ".into(),
            "ddd eee".into(),
            "fff    ".into(),
            "ggg hhh".into(),
            "iii    ".into(),
        ];
        assert_eq!(t.lines, Some(expected));
        Ok(())
    }
}
