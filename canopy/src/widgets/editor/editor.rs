use crate as canopy;
use crate::{
    event::key,
    geom::{Expanse, LineSegment, Point},
    state::{NodeState, StatefulNode},
    *,
};

use super::core;

/// A single input line, one character high.
#[derive(StatefulNode)]
pub struct Editor {
    state: NodeState,
    core: core::Core,
}

#[derive_commands]
impl Editor {
    pub fn new(txt: &str) -> Self {
        Editor {
            state: NodeState::default(),
            core: core::Core::new(txt),
        }
    }
}

impl Node for Editor {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn render(&mut self, _: &dyn Core, r: &mut Render) -> Result<()> {
        // r.text(
        //     "text",
        //     self.vp().view_rect().first_line(),
        //     &self.textbuf.text(),
        // )
        Ok(())
    }

    fn fit(&mut self, sz: Expanse) -> Result<Expanse> {
        self.core.set_width(sz.w as usize);
        Ok(sz)
    }
}
