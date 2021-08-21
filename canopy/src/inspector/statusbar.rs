use std::marker::PhantomData;

use crate as canopy;
use crate::{Actions, Canopy, Node, NodeState, Result, StatefulNode, ViewPort};

#[derive(StatefulNode)]

pub struct StatusBar<S, A: Actions, N>
where
    N: Node<S, A>,
{
    state: NodeState,
    _marker: PhantomData<(S, A, N)>,
}

impl<S, A: Actions, N> StatusBar<S, A, N>
where
    N: Node<S, A>,
{
    pub fn new() -> Self {
        StatusBar {
            state: NodeState::default(),
            _marker: PhantomData,
        }
    }
}

impl<S, A: Actions, N> Node<S, A> for StatusBar<S, A, N>
where
    N: Node<S, A>,
{
    fn render(&mut self, app: &mut Canopy<S, A>, vp: ViewPort) -> Result<()> {
        app.render.style.push_layer("statusbar");
        app.render
            .text("statusbar/text", vp.view_rect().first_line(), "inspector")?;
        Ok(())
    }
}
