use super::logs::Logs;
use crate::{
    Layout, derive_commands,
    error::Result,
    geom::Expanse,
    node::Node,
    state::{NodeState, StatefulNode},
    widgets::tabs,
};

/// View contains the body of the inspector.
#[derive(canopy::StatefulNode)]
pub struct View {
    /// Tab strip for inspector sections.
    tabs: tabs::Tabs,
    /// Log list panel.
    logs: Logs,
    /// Node state.
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
    /// Construct a new inspector view.
    pub fn new() -> Self {
        Self {
            state: NodeState::default(),
            tabs: tabs::Tabs::new(vec!["Stats".into(), "Logs".into()]),
            logs: Logs::new(),
        }
    }
}
