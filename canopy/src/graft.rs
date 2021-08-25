use crate as canopy;
use crate::{
    event::{key, mouse},
    geom::Size,
    Actions, Canopy, ControlBackend, Node, NodeState, Outcome, Render, Result, StatefulNode,
    ViewPort,
};

#[derive(StatefulNode)]
pub struct Graft<'a, S, A: Actions> {
    state: NodeState,
    appstate: S,
    core: Canopy<S, A>,
    root: &'a mut dyn Node<S, A>,
}

impl<'a, S, A: Actions> Graft<'a, S, A> {
    pub fn new(appstate: S, root: &'a mut dyn Node<S, A>) -> Self {
        Graft {
            state: NodeState::default(),
            appstate,
            core: Canopy::new(),
            root,
        }
    }
}

impl<'a, SO, AO: Actions, S, A: Actions> Node<SO, AO> for Graft<'a, S, A> {
    fn name(&self) -> Option<String> {
        Some("graft".into())
    }

    // We make an assumption that some node below us can hold terminal focus, so
    // we must too.
    fn focus(&mut self, app: &mut Canopy<SO, AO>) -> Result<Outcome<AO>> {
        app.set_focus(self);
        Ok(Outcome::handle())
    }

    /// Handle a key event. This event is only called for nodes that are on the
    /// focus path. The default implementation ignores input.
    fn handle_key(
        &mut self,
        _app: &mut Canopy<SO, AO>,
        ctrl: &mut dyn ControlBackend,
        _s: &mut SO,
        k: key::Key,
    ) -> Result<Outcome<AO>> {
        Ok(
            match self.core.key(ctrl, self.root, &mut self.appstate, k)? {
                Outcome::Handle(_) => Outcome::<AO>::handle(),
                Outcome::Ignore(_) => Outcome::ignore(),
            },
        )
    }

    /// Handle a mouse event.The default implementation ignores mouse input.
    fn handle_mouse(
        &mut self,
        _app: &mut Canopy<SO, AO>,
        ctrl: &mut dyn ControlBackend,
        _s: &mut SO,
        k: mouse::Mouse,
    ) -> Result<Outcome<AO>> {
        Ok(
            match self.core.mouse(ctrl, self.root, &mut self.appstate, k)? {
                Outcome::Handle(_) => Outcome::<AO>::handle(),
                Outcome::Ignore(_) => Outcome::ignore(),
            },
        )
    }

    // Just reflect the fit from our root node
    fn fit(&mut self, _app: &mut Canopy<SO, AO>, target: Size) -> Result<Size> {
        self.root.fit(&mut self.core, target)
    }

    fn render(&mut self, _app: &mut Canopy<SO, AO>, _: &mut Render, vp: ViewPort) -> Result<()> {
        self.root.wrap(&mut self.core, vp)
    }
}
