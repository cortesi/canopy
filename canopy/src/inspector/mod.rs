mod logs;
mod view;

use crate as canopy;
use crate::{widgets::frame, *};

#[derive(StatefulNode)]

pub struct Inspector {
    state: NodeState,
    view: frame::Frame<view::View>,
}

#[derive_commands]
impl Inspector {
    pub fn new() -> Self {
        Inspector {
            state: NodeState::default(),
            view: frame::Frame::new(view::View::new()),
        }
    }
}

impl Default for Inspector {
    fn default() -> Self {
        Self::new()
    }
}

impl Node for Inspector {
    fn render(&mut self, _c: &dyn Core, r: &mut Render) -> Result<()> {
        r.style.push_layer("inspector");
        let vp = self.vp();
        fit(&mut self.view, vp)?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.view)
    }
}
