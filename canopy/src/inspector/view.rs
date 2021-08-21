use std::marker::PhantomData;

use crate as canopy;
use crate::{Actions, Canopy, Node, NodeState, Result, StatefulNode, ViewPort};

impl<S, A: Actions, N> View<S, A, N>
where
    N: Node<S, A>,
{
    pub fn new() -> Self {
        View {
            state: NodeState::default(),
            _marker: PhantomData,
        }
    }
}

#[derive(StatefulNode)]

pub struct View<S, A: Actions, N>
where
    N: Node<S, A>,
{
    state: NodeState,
    _marker: PhantomData<(S, A, N)>,
}

impl<S, A: Actions, N> Node<S, A> for View<S, A, N>
where
    N: Node<S, A>,
{
    fn render(&mut self, _app: &mut Canopy<S, A>, _vp: ViewPort) -> Result<()> {
        Ok(())
    }
}
