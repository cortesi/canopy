use crate::{
    geom::{Rect, Size},
    ViewPort,
};

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
    pub viewport: ViewPort,
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
            viewport: ViewPort::default(),
        }
    }
}

/// The interface implemented by all nodes that track state.
pub trait StatefulNode {
    /// Get a reference to the node's state object.
    fn state(&self) -> &NodeState;

    /// Get a mutable reference to the node's state object.
    fn state_mut(&mut self) -> &mut NodeState;

    /// Hides the element and all its descendants from rendering. The nodes are
    /// still included in the tree.
    fn hide(&mut self) {
        self.state_mut().hidden = true;
    }

    /// Hides the element
    fn unhide(&mut self) {
        self.state_mut().hidden = false;
    }

    /// Is this element hidden?
    fn is_hidden(&self) -> bool {
        self.state().hidden
    }

    /// Get the screen rect from the viewport.
    fn screen(&self) -> Rect {
        self.state().viewport.screen_rect()
    }

    /// Get the view rect from the viewport.
    fn view(&self) -> Rect {
        self.state().viewport.view_rect()
    }

    /// Get the outer rect from the viewport.
    fn size(&self) -> Size {
        self.state().viewport.size()
    }

    fn update_viewport(&mut self, fun: &dyn Fn(ViewPort) -> ViewPort) {
        self.set_viewport(fun(self.state().viewport))
    }

    fn set_viewport(&mut self, view: ViewPort) {
        self.state_mut().viewport = view;
    }
}
