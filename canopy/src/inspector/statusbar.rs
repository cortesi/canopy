use crate as canopy;
use crate::{Node, NodeState, Render, Result, StatefulNode};

#[derive(StatefulNode)]

pub struct StatusBar {
    state: NodeState,
}

impl StatusBar {
    pub fn new() -> Self {
        StatusBar {
            state: NodeState::default(),
        }
    }
}

impl Node for StatusBar {
    fn render(&mut self, r: &mut Render) -> Result<()> {
        r.style.push_layer("statusbar");
        r.text(
            "statusbar/text",
            self.vp().view_rect().first_line(),
            "inspector",
        )?;
        Ok(())
    }
}
