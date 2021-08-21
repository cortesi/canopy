mod statusbar;
mod view;
use std::marker::PhantomData;

use crate as canopy;
use crate::{
    event::key, Actions, Canopy, Node, NodeState, Outcome, Result, StatefulNode, ViewPort,
};

#[derive(StatefulNode)]

pub struct Content<S, A: Actions, N>
where
    N: Node<S, A>,
{
    state: NodeState,
    statusbar: statusbar::StatusBar<S, A, N>,
    view: view::View<S, A, N>,
    _marker: PhantomData<(S, A, N)>,
}

impl<S, A: Actions, N> Content<S, A, N>
where
    N: Node<S, A>,
{
    pub fn new() -> Self {
        Content {
            _marker: PhantomData,
            state: NodeState::default(),
            statusbar: statusbar::StatusBar::new(),
            view: view::View::new(),
        }
    }
}

impl<S, A: Actions, N> Node<S, A> for Content<S, A, N>
where
    N: Node<S, A>,
{
    fn render(&mut self, app: &mut Canopy<S, A>, vp: ViewPort) -> Result<()> {
        let parts = vp.carve_vend(1)?;
        self.statusbar.wrap(app, parts[1])?;
        self.view.wrap(app, parts[0])?;
        Ok(())
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node<S, A>) -> Result<()>) -> Result<()> {
        f(&self.statusbar)?;
        f(&self.view)
    }

    fn children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Node<S, A>) -> Result<()>) -> Result<()> {
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
    content: Content<S, A, N>,
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
            content: Content::new(),
            root,
            activate,
        }
    }
}

impl<S, A: Actions, N> Node<S, A> for Inspector<S, A, N>
where
    N: Node<S, A>,
{
    fn handle_key(&mut self, app: &mut Canopy<S, A>, _: &mut S, k: key::Key) -> Result<Outcome<A>> {
        if k == self.activate {
            self.active = !self.active;
            app.taint_tree(self)?;
        }
        Ok(Outcome::handle())
    }

    fn render(&mut self, app: &mut Canopy<S, A>, vp: ViewPort) -> Result<()> {
        app.render.style.push_layer("inspector");
        if self.active {
            let parts = vp.split_horizontal(2)?;
            self.content.wrap(app, parts[0])?;
            self.root.wrap(app, parts[1])?;
        } else {
            self.root.wrap(app, vp)?;
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
