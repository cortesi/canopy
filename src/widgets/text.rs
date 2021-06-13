use crate as canopy;
use crate::{
    app::Canopy,
    geom::{Point, Rect},
    layout::ConstrainedLayout,
    Node, NodeState,
};
use std::io::Write;
use std::marker::PhantomData;

use crossterm::{
    cursor::MoveTo,
    style::{Color, Print, SetForegroundColor},
    QueueableCommand,
};

use anyhow::{format_err, Result};
use textwrap;

pub struct Text<S> {
    pub state: NodeState,
    pub rect: Option<Rect>,
    pub virt_origin: Option<Point>,
    pub raw: String,
    _marker: PhantomData<S>,

    lines: Option<Vec<String>>,
}

impl<S> Text<S> {
    pub fn new(raw: &str) -> Self {
        Text {
            state: canopy::NodeState::default(),

            rect: None,
            virt_origin: None,

            raw: raw.to_owned(),
            lines: None,
            _marker: PhantomData,
        }
    }
}

impl<'a, S> ConstrainedLayout for Text<S> {
    fn constrain(
        &mut self,
        _app: &mut Canopy,
        width: Option<u16>,
        _height: Option<u16>,
    ) -> Result<Rect> {
        if let Some(w) = width {
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
        } else {
            Err(format_err!("Text requires a width constraint"))
        }
    }
    fn layout(&mut self, _app: &mut Canopy, virt_origin: Point, rect: Rect) -> Result<()> {
        self.rect = Some(rect);
        self.virt_origin = Some(virt_origin);
        Ok(())
    }
}

impl<'a, S> Node<S> for Text<S> {
    fn state(&mut self) -> &mut canopy::NodeState {
        &mut self.state
    }
    fn rect(&self) -> Option<Rect> {
        self.rect
    }
    fn render(&mut self, _app: &mut Canopy, w: &mut dyn Write) -> Result<()> {
        w.queue(SetForegroundColor(Color::White))?;
        if let (Some(lines), Some(rect), Some(vo)) =
            (self.lines.as_ref(), self.rect, self.virt_origin)
        {
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
        t.constrain(&mut app, Some(7), None)?;
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
