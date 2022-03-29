use std::marker::PhantomData;

use crate as canopy;
use crate::{
    event::{key, mouse},
    geom::Size,
    Actions, BackendControl, Node, NodeState, Outcome, Render, Result, StatefulNode, ViewPort,
};

/// Graft is a node that can contain a complete sub-application. This lets us
/// write re-usable, fully self-contained complex apps that can be embedded.
#[derive(StatefulNode)]
pub struct Graft<SO, AO, S, A: Actions, N>
where
    N: Node<S, A>,
{
    _marker: PhantomData<(SO, AO, A)>,
    state: NodeState,
    appstate: S,
    root: N,
}

impl<SO, AO, S, A: Actions, N> Graft<SO, AO, S, A, N>
where
    N: Node<S, A>,
{
    pub fn new(appstate: S, root: N) -> Self {
        Graft {
            _marker: PhantomData,
            state: NodeState::default(),
            appstate,
            root,
        }
    }
}

impl<SO, AO: Actions, S, A: Actions, N> Node<SO, AO> for Graft<SO, AO, S, A, N>
where
    N: Node<S, A>,
{
    fn name(&self) -> Option<String> {
        Some("graft".into())
    }

    // We make an assumption that some node below us can hold terminal focus, so
    // we must too.
    fn handle_focus(&mut self) -> Result<Outcome<AO>> {
        self.set_focus();
        Ok(Outcome::handle())
    }

    /// Handle a key event. This event is only called for nodes that are on the
    /// focus path. The default implementation ignores input.
    fn handle_key(
        &mut self,
        ctrl: &mut dyn BackendControl,
        _s: &mut SO,
        k: key::Key,
    ) -> Result<Outcome<AO>> {
        Ok(
            match canopy::key(ctrl, &mut self.root, &mut self.appstate, k)? {
                Outcome::Handle(_) => Outcome::<AO>::handle(),
                Outcome::Ignore(_) => Outcome::ignore(),
            },
        )
    }

    /// Handle a mouse event.The default implementation ignores mouse input.
    fn handle_mouse(
        &mut self,
        ctrl: &mut dyn BackendControl,
        _s: &mut SO,
        k: mouse::Mouse,
    ) -> Result<Outcome<AO>> {
        Ok(
            match canopy::mouse(ctrl, &mut self.root, &mut self.appstate, k)? {
                Outcome::Handle(_) => Outcome::<AO>::handle(),
                Outcome::Ignore(_) => Outcome::ignore(),
            },
        )
    }

    // Just reflect the fit from our root node
    fn fit(&mut self, target: Size) -> Result<Size> {
        self.root.fit(target)
    }

    fn render(&mut self, rndr: &mut Render, vp: ViewPort) -> Result<()> {
        self.root.wrap(vp)?;
        self.root.taint_tree()?;
        canopy::pre_render(rndr, &mut self.root)?;
        canopy::render(rndr, &mut self.root)?;
        canopy::post_render(rndr, &self.root)
    }
}
