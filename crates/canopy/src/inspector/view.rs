use super::logs::Logs;
use crate as canopy;
use crate::widgets::tabs;
use canopy_core::*;

/// View contains the body of the inspector.
#[derive(canopy_core::StatefulNode)]
pub struct View {
    tabs: tabs::Tabs,
    logs: Logs,
    state: NodeState,
}

impl Node for View {
    fn layout(&mut self, l: &Layout, _: Expanse) -> Result<()> {
        let vp = self.vp();
        let (a, b) = vp.view().carve_vstart(1);
        l.place(&mut self.tabs, a)?;
        l.place(&mut self.logs, b)?;
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
