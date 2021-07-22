use crate as canopy;
use crate::{
    geom::Rect,
    layout::ConstrainedWidthLayout,
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

impl<'a, S> ConstrainedWidthLayout<S> for Text<S> {
    fn constrain(&mut self, _app: &mut Canopy<S>, w: u16) -> Result<()> {
        // Only resize if the width has changed
        if self.state_mut().view.outer().w != w {
            let mut split: Vec<String> = vec![];
            for i in textwrap::wrap(&self.raw, w as usize) {
                split.push(format!("{:width$}", i, width = w as usize))
            }
            self.state_mut()
                .view
                .resize_outer(Rect::new(0, 0, w, split.len() as u16));
            self.lines = Some(split);
        }
        Ok(())
    }
}

impl<'a, S> Node<S> for Text<S> {
    fn render(&self, app: &mut Canopy<S>) -> Result<()> {
        let area = self.state().view.screen();
        let vo = self.state.view.view();
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
