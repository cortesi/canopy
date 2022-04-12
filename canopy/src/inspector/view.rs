use super::logs::Logs;
use crate as canopy;
use crate::{
    event::key, widgets::tabs, BackendControl, Node, NodeState, Outcome, Render, Result,
    StatefulNode, ViewPort,
};

/// View contains the body of the inspector.
#[derive(StatefulNode)]

pub struct View {
    tabs: tabs::Tabs,
    logs: Logs,
    state: NodeState,
}

impl Node for View {
    fn handle_focus(&mut self) -> Result<Outcome> {
        self.set_focus();
        Ok(Outcome::handle())
    }

    fn handle_key(&mut self, _: &mut dyn BackendControl, k: key::Key) -> Result<Outcome> {
        match k {
            c if c == key::KeyCode::Tab => self.tabs.next(),
            _ => return Ok(Outcome::ignore()),
        };
        Ok(Outcome::handle())
    }

    fn render(&mut self, _r: &mut Render, vp: ViewPort) -> Result<()> {
        let (a, b) = vp.carve_vstart(1);
        self.tabs.wrap(a)?;
        self.logs.wrap(b)?;
        Ok(())
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node) -> Result<()>) -> Result<()> {
        f(&self.tabs)?;
        f(&self.logs)
    }

    fn children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.tabs)?;
        f(&mut self.logs)
    }
}

impl View {
    pub fn new() -> Self {
        View {
            state: NodeState::default(),
            tabs: tabs::Tabs::new(vec!["Nodes".into(), "Events".into(), "Logs".into()]),
            logs: Logs::new(),
        }
    }
}
