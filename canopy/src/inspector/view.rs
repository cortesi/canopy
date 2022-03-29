use std::marker::PhantomData;

use crate as canopy;
use crate::{
    event::key, widgets::tabs, Actions, BackendControl, Canopy, Node, NodeState, Outcome, Render,
    Result, StatefulNode, ViewPort,
};

#[derive(StatefulNode)]

pub struct Logs<S, A: Actions> {
    state: NodeState,
    _marker: PhantomData<(S, A)>,
}

impl<S, A: Actions> Node<S, A> for Logs<S, A> {
    fn render(&mut self, _app: &mut Canopy<S, A>, r: &mut Render, vp: ViewPort) -> Result<()> {
        r.fill("", vp.view_rect(), ' ')?;
        Ok(())
    }
}

impl<S, A: Actions> Logs<S, A> {
    pub fn new() -> Self {
        Logs {
            state: NodeState::default(),
            _marker: PhantomData,
        }
    }
}

/// View contains the body of the inspector.
#[derive(StatefulNode)]

pub struct View<S, A: Actions> {
    tabs: tabs::Tabs<S, A>,
    logs: Logs<S, A>,
    state: NodeState,
    _marker: PhantomData<(S, A)>,
}

impl<S, A: Actions> Node<S, A> for View<S, A> {
    fn handle_focus(&mut self, _app: &mut Canopy<S, A>) -> Result<Outcome<A>> {
        self.set_focus();
        Ok(Outcome::handle())
    }

    fn handle_key(
        &mut self,
        app: &mut Canopy<S, A>,
        _: &mut dyn BackendControl,
        _: &mut S,
        k: key::Key,
    ) -> Result<Outcome<A>> {
        match k {
            c if c == key::KeyCode::Tab => self.tabs.next(app),
            _ => return Ok(Outcome::ignore()),
        };
        Ok(Outcome::handle())
    }

    fn render(&mut self, app: &mut Canopy<S, A>, _r: &mut Render, vp: ViewPort) -> Result<()> {
        let (a, b) = vp.carve_vstart(1);
        self.tabs.wrap(app, a)?;
        self.logs.wrap(app, b)?;
        Ok(())
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node<S, A>) -> Result<()>) -> Result<()> {
        f(&self.tabs)?;
        f(&self.logs)
    }

    fn children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Node<S, A>) -> Result<()>) -> Result<()> {
        f(&mut self.tabs)?;
        f(&mut self.logs)
    }
}

impl<S, A: Actions> View<S, A> {
    pub fn new() -> Self {
        View {
            state: NodeState::default(),
            _marker: PhantomData,
            tabs: tabs::Tabs::new(vec!["Nodes".into(), "Events".into(), "Logs".into()]),
            logs: Logs::new(),
        }
    }
}
