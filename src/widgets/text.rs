use crate as canopy;
use crate::{app::Canopy, geom::Rect, Node, NodeState};
use std::io::Write;
use std::marker::PhantomData;

use crossterm::{
    cursor::MoveTo,
    style::{Color, Print, SetForegroundColor},
    QueueableCommand,
};

use anyhow::Result;
use textwrap;

pub struct Text<S> {
    pub state: NodeState,
    pub rect: Option<Rect>,
    pub view: Option<Rect>,
    pub virt: Option<Rect>,
    pub raw: String,
    _marker: PhantomData<S>,

    lines: Option<Vec<String>>,
}

impl<S> Text<S> {
    pub fn new(raw: &str) -> Self {
        Text {
            state: canopy::NodeState::default(),
            rect: None,
            view: None,
            virt: None,
            raw: raw.to_owned(),
            lines: None,
            _marker: PhantomData,
        }
    }
}

impl<'a, S> Node<S> for Text<S> {
    fn state(&mut self) -> &mut canopy::NodeState {
        &mut self.state
    }
    fn rect(&self) -> Option<Rect> {
        self.rect
    }
    fn layout(&mut self, _app: &mut Canopy, rect: Option<Rect>, view: Option<Rect>) -> Result<()> {
        self.rect = rect;
        self.view = view;
        Ok(())
    }
    fn virt_size(
        &mut self,
        _app: &mut Canopy,
        width: Option<u16>,
        _height: Option<u16>,
    ) -> Option<Rect> {
        if let Some(w) = width {
            let mut split: Vec<String> = vec![];
            for i in textwrap::wrap(&self.raw, w as usize) {
                split.push(format!("{:width$}", i, width = w as usize))
            }
            self.virt = Some(Rect {
                x: 0,
                y: 0,
                w,
                h: split.len() as u16,
            });
            self.lines = Some(split);
            self.virt
        } else {
            None
        }
    }
    fn render(&mut self, _app: &mut Canopy, w: &mut dyn Write) -> Result<()> {
        w.queue(SetForegroundColor(Color::White))?;
        if let (Some(lines), Some(v), Some(r)) = (self.lines.as_ref(), self.view, self.rect) {
            for i in 0..v.h {
                w.queue(MoveTo(r.x, r.y + i))?;
                let l = &lines[(v.y + i) as usize];
                w.queue(Print(&l[(v.x) as usize..(v.x + v.w) as usize]))?;
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
        t.virt_size(&mut app, Some(7), None);

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
