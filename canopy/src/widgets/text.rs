use crate as canopy;
use crate::{
    geom::{Rect, Size},
    state::{NodeState, StatefulNode},
    Canopy, Node, Result,
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

impl<'a, S> Node<S> for Text<S> {
    fn fit(&mut self, _app: &mut Canopy<S>, s: Size) -> Result<Size> {
        // Only resize if the width has changed
        if self.state().viewport.outer().w != s.w {
            let mut split: Vec<String> = vec![];
            for i in textwrap::wrap(&self.raw, s.w as usize) {
                split.push(format!("{:width$}", i, width = s.w as usize))
            }
            let len = split.len() as u16;
            self.lines = Some(split);
            Ok(Size::new(s.w, len))
        } else {
            Ok(self.outer())
        }
    }
    fn render(&self, app: &mut Canopy<S>) -> Result<()> {
        let area = self.screen();
        let vo = self.state.viewport.view();
        if let Some(lines) = self.lines.as_ref() {
            for i in 0..area.h {
                let r = Rect::new(area.tl.x, area.tl.y + i, area.w, 1);
                if (vo.tl.y + i) < lines.len() as u16 {
                    app.render.text("text", r, &lines[(vo.tl.y + i) as usize])?;
                } else {
                    app.render.fill("text", r, ' ')?;
                };
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::test::TestRender;
    use crate::tutils::utils;

    #[test]
    fn text_sizing() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut app = utils::tcanopy(&mut tr);
        let txt = "aaa bbb ccc\nddd eee fff\nggg hhh iii";
        let mut t: Text<utils::State> = Text::new(txt);
        t.fit(&mut app, Size::new(7, 10))?;
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
