use crate::app::Canopy;
use crate::geom::Rect;

pub use canopy_derive::StatefulNode;

/// An opaque structure that Canopy uses to track node state. Each Node has to
/// keep a NodeState structure, and offer it up through the `Node::state()`
/// method on request.
#[derive(Debug, PartialEq)]
pub struct NodeState {
    pub render_gen: u64,
    pub render_skip_gen: u64,
    pub focus_gen: u64,
    // The focus generation if this node held focus during the last rendering
    // phase.
    pub rendered_focus_gen: u64,
    pub rect: Option<Rect>,
}

impl NodeState {
    /// Does this node currently hold focus?
    pub fn is_focused(&self, appstate: &Canopy) -> bool {
        self.focus_gen == appstate.focus_gen
    }
}

impl NodeState {
    pub fn default() -> Self {
        NodeState {
            render_gen: 0,
            focus_gen: 0,
            rendered_focus_gen: 0,
            render_skip_gen: 0,
            rect: None,
        }
    }
}

pub trait StatefulNode {
    fn state(&self) -> &NodeState;

    fn state_mut(&mut self) -> &mut NodeState;

    fn rect(&self) -> Option<Rect>;

    fn set_rect(&mut self, r: Option<Rect>);
}
