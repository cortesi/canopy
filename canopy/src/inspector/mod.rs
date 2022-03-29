mod statusbar;
mod view;
use std::marker::PhantomData;

use crate as canopy;
use crate::{
    event::key, graft::Graft, widgets::frame, Actions, BackendControl, Node, NodeState, Outcome,
    Render, Result, StatefulNode, ViewPort,
};

#[derive(Debug, PartialEq, Clone)]
struct InspectorState {}

#[derive(StatefulNode)]

pub struct Content<S, A: Actions> {
    state: NodeState,
    statusbar: statusbar::StatusBar<S, A>,
    view: frame::Frame<S, A, view::View<S, A>>,
    _marker: PhantomData<(S, A)>,
}

impl<A: Actions> Content<InspectorState, A> {
    pub fn new() -> Self {
        Content {
            _marker: PhantomData,
            state: NodeState::default(),
            statusbar: statusbar::StatusBar::new(),
            view: frame::Frame::new(view::View::new()),
        }
    }
}

impl<A: Actions> Default for Content<InspectorState, A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Actions> Node<InspectorState, A> for Content<InspectorState, A> {
    fn render(&mut self, _r: &mut Render, vp: ViewPort) -> Result<()> {
        let parts = vp.carve_vend(1);
        self.statusbar.wrap(parts.1)?;
        self.view.wrap(parts.0)?;
        Ok(())
    }

    fn children(
        &self,
        f: &mut dyn FnMut(&dyn Node<InspectorState, A>) -> Result<()>,
    ) -> Result<()> {
        f(&self.statusbar)?;
        f(&self.view)
    }

    fn children_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Node<InspectorState, A>) -> Result<()>,
    ) -> Result<()> {
        f(&mut self.statusbar)?;
        f(&mut self.view)
    }
}

#[derive(StatefulNode)]
pub struct Inspector<S, A: Actions, N>
where
    N: Node<S, A>,
{
    _marker: PhantomData<(S, A)>,
    state: NodeState,
    root: N,
    active: bool,
    activate: key::Key,
    content: Graft<S, A, InspectorState, (), Content<InspectorState, ()>>,
}

impl<S, A: Actions, N> Inspector<S, A, N>
where
    N: Node<S, A>,
{
    pub fn new(activate: key::Key, root: N) -> Self {
        Inspector {
            _marker: PhantomData,
            state: NodeState::default(),
            active: false,
            content: Graft::new(InspectorState {}, Content::new()),
            root,
            activate,
        }
    }
}

impl<S, A: Actions, N> Node<S, A> for Inspector<S, A, N>
where
    N: Node<S, A>,
{
    fn handle_key(
        &mut self,
        _ctrl: &mut dyn BackendControl,
        _: &mut S,
        k: key::Key,
    ) -> Result<Outcome<A>> {
        if self.active {
            match k {
                c if c == 'a' => {
                    canopy::focus_first(&mut self.root)?;
                }
                c if c == self.activate => {
                    if canopy::on_focus_path(&self.content) {
                        self.active = false;
                        self.taint_tree()?;
                        canopy::focus_first(&mut self.root)?;
                    } else {
                        canopy::focus_first(self)?;
                    }
                }
                _ => return Ok(Outcome::ignore()),
            };
        } else if k == self.activate {
            self.active = true;
            self.taint_tree()?;
            canopy::focus_first(self)?;
        };
        Ok(Outcome::handle())
    }

    fn render(&mut self, r: &mut Render, vp: ViewPort) -> Result<()> {
        r.style.push_layer("inspector");
        if self.active {
            let parts = vp.split_horizontal(2)?;
            self.content.wrap(parts[0])?;
            self.root.wrap(parts[1])?;
        } else {
            self.root.wrap(vp)?;
        };
        Ok(())
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node<S, A>) -> Result<()>) -> Result<()> {
        if self.active {
            f(&self.content)?;
        }
        f(&self.root)
    }

    fn children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Node<S, A>) -> Result<()>) -> Result<()> {
        if self.active {
            f(&mut self.content)?;
        }
        f(&mut self.root)
    }
}
