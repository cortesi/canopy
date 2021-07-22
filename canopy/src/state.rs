use crate::geom::{Rect, ViewPort};

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
    // The last render sweep during which this node held focus.
    pub(crate) rendered_focus_gen: u64,

    // The view for this node. The inner rectangle always has the same size as
    // the screen_area.
    pub view: ViewPort,
    pub(crate) hidden: bool,
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
            hidden: false,
            view: ViewPort::default(),
        }
    }
}

/// The interface implemented by all nodes that track state.
pub trait StatefulNode {
    /// Get a reference to the node's state object.
    fn state(&self) -> &NodeState;

    /// Get a mutable reference to the node's state object.
    fn state_mut(&mut self) -> &mut NodeState;

    fn screen(&self) -> Rect {
        self.state().view.screen()
    }

    fn view(&self) -> Rect {
        self.state().view.view()
    }

    fn outer(&self) -> Rect {
        self.state().view.outer()
    }

    /// Hides the element
    fn hide(&mut self) {
        self.state_mut().hidden = true;
    }

    /// Is this element hidden?
    fn is_hidden(&self) -> bool {
        self.state().hidden
    }
}
