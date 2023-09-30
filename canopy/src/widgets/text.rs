use crate as canopy;
use crate::{
    geom::Expanse,
    state::{NodeState, StatefulNode},
    *,
};

use textwrap;

#[derive(StatefulNode)]
pub struct Text {
    pub state: NodeState,
    pub raw: String,
    lines: Option<Vec<String>>,
    fixed_width: Option<u16>,
    current_size: Expanse,
}

#[derive_commands]
impl Text {
    pub fn new(raw: &str) -> Self {
        Text {
            state: NodeState::default(),

            raw: raw.to_owned(),
            lines: None,
            fixed_width: None,
            current_size: Expanse::default(),
        }
    }
    /// Add a fixed width, ignoring fit parameters
    pub fn with_fixed_width(mut self, width: u16) -> Self {
        self.fixed_width = Some(width);
        self
    }

    #[command]
    pub fn scroll_to_top(&mut self, c: &mut dyn Context) {
        c.scroll_to(self, 0, 0);
    }

    #[command]
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        c.scroll_down(self);
    }

    #[command]
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        c.scroll_up(self);
    }

    #[command]
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        c.scroll_left(self);
    }

    #[command]
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        c.scroll_right(self);
    }

    #[command]
    pub fn page_down(&mut self, c: &mut dyn Context) {
        c.page_down(self);
    }

    #[command]
    pub fn page_up(&mut self, c: &mut dyn Context) {
        c.page_up(self);
    }
}

impl Node for Text {
    fn fit(&mut self, s: Expanse) -> Result<()> {
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
            self.current_size = Expanse {
                w,
                h: split.len() as u16,
            };
            self.lines = Some(split);
        }
        let cs = self.current_size.clone();
        self.vp_mut().fit_size(cs, s);
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, rndr: &mut Render) -> Result<()> {
        let vo = self.vp().view;
        if let Some(lines) = self.lines.as_ref() {
            for i in 0..vo.h {
                let out = &lines[(vo.tl.y + i) as usize]
                    .chars()
                    .skip(vo.tl.x as usize)
                    .collect::<String>();
                rndr.text("text", vo.line(i), out)?;
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
        t.fit(Expanse::new(7, 10))?;
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
