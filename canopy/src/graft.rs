use crate as canopy;
use crate::{
    event::{key, mouse},
    geom::Size,
    BackendControl, Node, NodeState, Outcome, Render, Result, StatefulNode, ViewPort,
};

/// Graft is a node that can contain a complete sub-application. This lets us
/// write re-usable, fully self-contained complex apps that can be embedded.
#[derive(StatefulNode)]
pub struct Graft<N>
where
    N: Node,
{
    state: NodeState,
    root: N,
}

impl<N> Graft<N>
where
    N: Node,
{
    pub fn new(root: N) -> Self {
        Graft {
            state: NodeState::default(),
            root,
        }
    }
}

impl<N> Node for Graft<N>
where
    N: Node,
{
    fn name(&self) -> Option<String> {
        Some("graft".into())
    }

    // We make an assumption that some node below us can hold terminal focus, so
    // we must too.
    fn handle_focus(&mut self) -> Result<Outcome> {
        self.set_focus();
        Ok(Outcome::handle())
    }

    /// Handle a key event. This event is only called for nodes that are on the
    /// focus path. The default implementation ignores input.
    fn handle_key(&mut self, ctrl: &mut dyn BackendControl, k: key::Key) -> Result<Outcome> {
        Ok(match canopy::key(ctrl, &mut self.root, k)? {
            Outcome::Handle(_) => Outcome::handle(),
            Outcome::Ignore(_) => Outcome::ignore(),
        })
    }

    /// Handle a mouse event.The default implementation ignores mouse input.
    fn handle_mouse(&mut self, ctrl: &mut dyn BackendControl, k: mouse::Mouse) -> Result<Outcome> {
        Ok(match canopy::mouse(ctrl, &mut self.root, k)? {
            Outcome::Handle(_) => Outcome::handle(),
            Outcome::Ignore(_) => Outcome::ignore(),
        })
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
