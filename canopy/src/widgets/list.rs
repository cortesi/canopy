use crate::{layout::ConstrainedLayout, node::Node};
use std::marker::PhantomData;

pub struct List<S, N: Node<S> + ConstrainedLayout<S>> {
    _marker: PhantomData<S>,
    pub items: Vec<N>,
    pub offset: u32,
    pub focus: u32,
}

impl<S, N> List<S, N>
where
    N: Node<S> + ConstrainedLayout<S>,
{
    pub fn new(c: Vec<N>) -> Self {
        List {
            _marker: PhantomData,
            items: c,
            offset: 0,
            focus: 0,
        }
    }
}
