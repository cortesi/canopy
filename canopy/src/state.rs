use crate::geom::Rect;

pub use canopy_derive::StatefulNode;

/// An opaque structure that Canopy uses to track node state. Each Node has to
/// keep a NodeState structure, and offer it up through the `Node::state()`
/// method on request.
#[derive(Debug, PartialEq)]
pub struct NodeState {
    // If this is equal to the global render_gen, we render during the current
    // sweep.
    pub(crate) render_gen: u64,
    pub(crate) render_skip_gen: u64,
    pub(crate) focus_gen: u64,
    // The focus generation if this node held focus during the last rendering
    // phase.
    pub(crate) rendered_focus_gen: u64,
    pub rect: Rect,
}

/// The node state object - each node needs to keep one of these, and offer it
/// up by implementing the StatefulNode trait.
impl NodeState {
    pub fn default() -> Self {
        NodeState {
            render_gen: 0,
            focus_gen: 0,
            rendered_focus_gen: 0,
            render_skip_gen: 0,
            rect: Rect::default(),
        }
    }
}

/// The interface implemented by all nodes that track state.
pub trait StatefulNode {
    fn state(&self) -> &NodeState;

    fn state_mut(&mut self) -> &mut NodeState;

    fn rect(&self) -> Rect;

    fn set_rect(&mut self, r: Rect);
}
