use std::iter;

use crate as canopy;
use crate::{
    event::key,
    geom::{Expanse, LineSegment, Point},
    state::{NodeState, StatefulNode},
    *,
};

use super::core;

#[derive(StatefulNode)]
pub struct EditorView {
    state: NodeState,
    core: core::Core,
    window_offset: usize,
}

#[derive_commands]
impl EditorView {
    pub fn new(txt: &str) -> Self {
        EditorView {
            state: NodeState::default(),
            core: core::Core::new(txt),
            window_offset: 0,
        }
    }
}

impl Node for EditorView {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn render(&mut self, _: &dyn Core, r: &mut Render) -> Result<()> {
        let vo = self.vp().view_rect();
        for (i, s) in self
            .core
            .state
            .wrapped_window(self.window_offset, vo.h as usize)
            .iter()
            .enumerate()
        {
            r.text("text", vo.line(i as u16), s)?;
        }
        Ok(())
    }

    fn fit(&mut self, sz: Expanse) -> Result<Expanse> {
        self.core.set_width(sz.w as usize);
        Ok(Expanse::new(sz.w, self.core.state.wrapped_height() as u16))
    }
}

/// A single input line, one character high.
#[derive(StatefulNode)]
pub struct Editor {
    state: NodeState,
    view: EditorView,
}

#[derive_commands]
impl Editor {
    pub fn new(txt: &str) -> Self {
        Editor {
            state: NodeState::default(),
            view: EditorView::new(txt),
        }
    }
}

impl Node for Editor {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn render(&mut self, _c: &dyn Core, _: &mut Render) -> Result<()> {
        let vp = self.vp();
        fit(&mut self.view, vp)
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.view)
    }
}
