use std::marker::PhantomData;

use crate as canopy;
use crate::{
    state::{NodeState, StatefulNode},
    Actions, Canopy, Node, Render, Result, ViewPort,
};

/// A tab control managing a set of nodes with titles.
#[derive(StatefulNode)]
pub struct Tabs<S, A: Actions> {
    _marker: PhantomData<(S, A)>,
    pub state: NodeState,
    pub tabs: Vec<String>,
    pub active: usize,
}

impl<S, A: Actions> Tabs<S, A> {
    pub fn new(tabs: Vec<String>) -> Self {
        Tabs {
            _marker: PhantomData,
            state: NodeState::default(),
            active: 0,
            tabs,
        }
    }
    pub fn next(&mut self) {
        self.active = (self.active + 1) % self.tabs.len();
        self.taint();
    }
    pub fn prev(&mut self) {
        self.active = (self.active.wrapping_sub(1)) % self.tabs.len();
        self.taint();
    }
}

impl<S, A: Actions> Node<S, A> for Tabs<S, A> {
    fn render(&mut self, _: &mut Canopy<S, A>, r: &mut Render, vp: ViewPort) -> Result<()> {
        for (i, rect) in vp
            .view_rect()
            .split_horizontal(self.tabs.len() as u16)?
            .iter()
            .enumerate()
        {
            let styl = if i == self.active {
                "tab/active"
            } else {
                "tab/inactive"
            };
            let (text, end) = rect.carve_hend(1);
            r.text(styl, text.first_line(), &self.tabs[i])?;
            r.text("", end.first_line(), " ")?;
        }
        Ok(())
    }
}
