use crate as canopy;
use crate::*;

#[derive(StatefulNode)]

pub struct StatusBar {
    state: NodeState,
}

#[derive_commands]
impl StatusBar {
    pub fn new() -> Self {
        StatusBar {
            state: NodeState::default(),
        }
    }
}

impl Node for StatusBar {
    fn render(&mut self, _c: &dyn Core, r: &mut Render) -> Result<()> {
        r.style.push_layer("statusbar");
        r.text(
            "statusbar/text",
            self.vp().view_rect().first_line(),
            "inspector",
        )?;
        Ok(())
    }
}
