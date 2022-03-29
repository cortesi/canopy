use std::marker::PhantomData;

use crate as canopy;
use crate::{Actions, Node, NodeState, Render, Result, StatefulNode, ViewPort};

#[derive(StatefulNode)]

pub struct StatusBar<S, A: Actions> {
    state: NodeState,
    _marker: PhantomData<(S, A)>,
}

impl<S, A: Actions> StatusBar<S, A> {
    pub fn new() -> Self {
        StatusBar {
            state: NodeState::default(),
            _marker: PhantomData,
        }
    }
}

impl<S, A: Actions> Node<S, A> for StatusBar<S, A> {
    fn render(&mut self, r: &mut Render, vp: ViewPort) -> Result<()> {
        r.style.push_layer("statusbar");
        r.text("statusbar/text", vp.view_rect().first_line(), "inspector")?;
        Ok(())
    }
}
