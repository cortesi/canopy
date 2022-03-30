use crate as canopy;
use crate::{
    geom::{Line, Size},
    state::{NodeState, StatefulNode},
    Node, Render, Result, ViewPort,
};

use textwrap;

#[derive(StatefulNode)]
pub struct Text {
    pub state: NodeState,
    pub raw: String,
    lines: Option<Vec<String>>,
    fixed_width: Option<u16>,
    current_size: Size,
}

impl Text {
    pub fn new(raw: &str) -> Self {
        Text {
            state: NodeState::default(),

            raw: raw.to_owned(),
            lines: None,
            fixed_width: None,
            current_size: Size::default(),
        }
    }
    /// Add a fixed width, ignoring fit parameters
    pub fn with_fixed_width(mut self, width: u16) -> Self {
        self.fixed_width = Some(width);
        self
    }
}

impl Node for Text {
    fn fit(&mut self, s: Size) -> Result<Size> {
        let w = if let Some(w) = self.fixed_width {
            w
        } else {
            s.w
        };
        // Only resize if the width has changed
        if self.current_size.w != w {
            let mut split: Vec<String> = vec![];
            for i in textwrap::wrap(&self.raw, w as usize) {
                split.push(format!("{:width$}", i, width = w as usize))
            }
            self.current_size = Size {
                w,
                h: split.len() as u16,
            };
            self.lines = Some(split);
        }
        Ok(self.current_size)
    }
    fn render(&mut self, rndr: &mut Render, vp: ViewPort) -> Result<()> {
        let vo = vp.view_rect();
        if let Some(lines) = self.lines.as_ref() {
            for i in vo.tl.y..(vo.tl.y + vo.h) {
                let out = &lines[i as usize]
                    .chars()
                    .skip(vo.tl.x as usize)
                    .collect::<String>();
                rndr.text("text", Line::new(vo.tl.x, i, vo.w), out)?;
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
        let txt = "aaa bbb ccc\nddd eee fff\nggg hhh iii";
        let mut t: Text = Text::new(txt);
        t.fit(Size::new(7, 10))?;
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
