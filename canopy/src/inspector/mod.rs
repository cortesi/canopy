mod statusbar;
mod view;
use std::marker::PhantomData;

use crate as canopy;
use crate::{
    event::key, widgets::frame, Actions, Canopy, ControlBackend, Node, NodeState, Outcome, Render,
    Result, StatefulNode, ViewPort,
};

#[derive(Debug, PartialEq, Clone)]
struct InspectorState {}

#[derive(StatefulNode)]

pub struct Content<S, A: Actions, N>
where
    N: Node<S, A>,
{
    state: NodeState,
    statusbar: statusbar::StatusBar<S, A, N>,
    view: frame::Frame<S, A, view::View<S, A, N>>,
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
            view: frame::Frame::new(view::View::new()),
        }
    }
}

impl<S, A: Actions, N> Node<S, A> for Content<S, A, N>
where
    N: Node<S, A>,
{
    fn render(&mut self, app: &mut Canopy<S, A>, _r: &mut Render, vp: ViewPort) -> Result<()> {
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
    fn handle_key(
        &mut self,
        app: &mut Canopy<S, A>,
        _ctrl: &mut dyn ControlBackend,
        _: &mut S,
        k: key::Key,
    ) -> Result<Outcome<A>> {
        if self.active {
            match k {
                c if c == 'a' => {
                    app.focus_first(&mut self.root)?;
                }
                c if c == self.activate => {
                    if app.on_focus_path(&self.content) {
                        self.active = false;
                        app.taint_tree(self)?;
                        app.focus_first(&mut self.root)?;
                    } else {
                        app.focus_first(self)?;
                    }
                }
                _ => return Ok(Outcome::ignore()),
            };
        } else if k == self.activate {
            self.active = true;
            app.taint_tree(self)?;
            app.focus_first(self)?;
        };
        Ok(Outcome::handle())
    }

    fn render(&mut self, app: &mut Canopy<S, A>, r: &mut Render, vp: ViewPort) -> Result<()> {
        r.style.push_layer("inspector");
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
