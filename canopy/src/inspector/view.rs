use super::logs::Logs;
use crate as canopy;
use crate::{
    derive_actions, event::key, fit, widgets::tabs, BackendControl, Node, NodeState, Outcome,
    Render, Result, StatefulNode,
};

/// View contains the body of the inspector.
#[derive(StatefulNode)]

pub struct View {
    tabs: tabs::Tabs,
    logs: Logs,
    state: NodeState,
}

impl Node for View {
    fn handle_key(&mut self, _: &mut dyn BackendControl, k: key::Key) -> Result<Outcome> {
        match k {
            c if c == key::KeyCode::Tab => self.tabs.next(),
            _ => return Ok(Outcome::ignore()),
        };
        Ok(Outcome::handle())
    }

    fn render(&mut self, _r: &mut Render) -> Result<()> {
        let (a, b) = self.vp().carve_vstart(1);
        fit(&mut self.tabs, a)?;
        fit(&mut self.logs, b)?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.tabs)?;
        f(&mut self.logs)
    }
}

#[derive_actions]
impl View {
    pub fn new() -> Self {
        View {
            state: NodeState::default(),
            tabs: tabs::Tabs::new(vec!["Nodes".into(), "Events".into(), "Logs".into()]),
            logs: Logs::new(),
        }
    }
}
