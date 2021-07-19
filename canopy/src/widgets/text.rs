use crate as canopy;
use crate::{
    geom::{Point, Rect},
    layout::ConstrainedWidthLayout,
    state::{NodeState, StatefulNode},
    Canopy, Node, Render, Result,
};
use std::marker::PhantomData;

use textwrap;

#[derive(StatefulNode)]
pub struct Text<S> {
    pub state: NodeState,
    pub raw: String,
    lines: Option<Vec<String>>,

    _marker: PhantomData<S>,
}

impl<S> Text<S> {
    pub fn new(raw: &str) -> Self {
        Text {
            state: NodeState::default(),

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
}

impl<'a, S> Node<S> for Text<S> {
    fn render(&self, _app: &Canopy<S>, rndr: &mut Render) -> Result<()> {
        let area = self.screen_area();
        let vo = self.virt_area();
        if let Some(lines) = self.lines.as_ref() {
            for i in 0..area.h {
                let r = Rect {
                    tl: Point {
                        x: area.tl.x,
                        y: area.tl.y + i,
                    },
                    w: area.w,
                    h: 1,
                };
                if (vo.tl.y + i) < lines.len() as u16 {
                    rndr.text("text", r, &lines[(vo.tl.y + i) as usize])?;
                } else {
                    rndr.fill("text", r, ' ')?;
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
