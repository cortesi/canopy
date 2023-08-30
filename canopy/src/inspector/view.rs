use super::logs::Logs;
use crate as canopy;
use crate::{layout, widgets::tabs, *};

/// View contains the body of the inspector.
#[derive(StatefulNode)]
pub struct View {
    tabs: tabs::Tabs,
    logs: Logs,
    state: NodeState,
}

impl Node for View {
    fn render(&mut self, _c: &dyn Core, _r: &mut Render) -> Result<()> {
        let (a, b) = self.vp().carve_vstart(1);
        layout::fit(&mut self.tabs, a)?;
        layout::fit(&mut self.logs, b)?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.tabs)?;
        f(&mut self.logs)
    }
}

#[derive_commands]
impl View {
    pub fn new() -> Self {
        View {
            state: NodeState::default(),
            tabs: tabs::Tabs::new(vec!["Stats".into(), "Logs".into()]),
            logs: Logs::new(),
        }
    }
}
