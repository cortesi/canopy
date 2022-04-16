use crate::{global, ViewPort};
use std::sync::atomic::AtomicU64;

pub use canopy_derive::StatefulNode;

static CURRENT_ID: AtomicU64 = AtomicU64::new(0);

/// An opaque structure that Canopy uses to track node state. Each Node has to
/// keep a NodeState structure, and offer it up through the `Node::state()`
/// method on request.
#[derive(Debug, PartialEq)]
pub struct NodeState {
    // Is this node hidden?
    pub(crate) id: u64,
    /// If this is equal to the global render_gen, we render during the current
    /// sweep.
    pub(crate) render_gen: u64,
    /// This node's focus generation. We increment the global focus counter when
    /// focus changes, invalidating the current focus generation without having
    /// to update all node states.
    pub(crate) focus_gen: u64,
    /// Set to be equal to the focus_gen during a pre-render sweep, if focus has
    /// changed.
    pub(crate) focus_path_gen: u64,
    // The last render sweep during which this node held focus.
    pub(crate) rendered_focus_gen: u64,
    /// Set to the `render_gen` during the pre-render sweep if focus has
    /// changed, and this node was either on the old focus path, or is on the
    /// new focus path.
    pub(crate) focus_path_render_gen: u64,

    /// The view for this node. The inner rectangle always has the same size as
    /// the screen_area.
    pub(crate) viewport: ViewPort,
    // Is this node hidden?
    pub(crate) hidden: bool,
    // Has this node been initialized? This is used to determine if we need to
    // call the poll function during the pre-render sweep.
    pub(crate) initialized: bool,
}

/// The node state object - each node needs to keep one of these, and offer it
/// up by implementing the StatefulNode trait.
impl NodeState {
    pub fn default() -> Self {
        let id = CURRENT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        NodeState {
            id,
            render_gen: 0,
            focus_gen: 0,
            focus_path_gen: 0,
            focus_path_render_gen: 0,
            rendered_focus_gen: 0,
            hidden: false,
            viewport: ViewPort::default(),
            initialized: false,
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

    /// Get the node's `ViewPort`.
    fn vp(&self) -> ViewPort {
        self.state().viewport
    }

    /// Execute a closure that gets a mutable reference to the node's `ViewPort`
    /// for modification.
    fn update_viewport(&mut self, fun: &dyn Fn(ViewPort) -> ViewPort) {
        self.set_viewport(fun(self.state().viewport))
    }

    /// Replace the current `ViewPort`.
    fn set_viewport(&mut self, view: ViewPort) {
        self.state_mut().viewport = view;
    }

    /// A unique ID for this node.
    fn id(&self) -> u64 {
        self.state().id
    }

    /// Has this node been initialized? That is, has its poll function been
    /// called for the first time to schedule future polls.
    fn is_initialized(&self) -> bool {
        self.state().initialized
    }

    /// Focus this node.
    fn set_focus(&mut self) {
        global::STATE.with(|global_state| {
            global_state.borrow_mut().focus_gen += 1;
            self.state_mut().focus_gen = global_state.borrow().focus_gen;
        });
    }

    /// Is this node render tainted?
    fn is_tainted(&self) -> bool {
        global::STATE.with(|global_state| {
            let s = self.state();
            let rg = global_state.borrow().render_gen;
            // Tainting if render_gen is 0 lets us initialize a nodestate
            // without knowing about the app state
            rg == s.render_gen || s.render_gen == 0
        })
    }

    /// Does the node have terminal focus?
    fn is_focused(&self) -> bool {
        global::STATE.with(|global_state| -> bool {
            let s = self.state();
            global_state.borrow_mut().focus_gen == s.focus_gen
        })
    }

    /// Mark a this node for render.
    fn taint(&mut self) {
        let r = self.state_mut();
        r.render_gen = global::STATE.with(|global_state| -> u64 {
            let mut s = global_state.borrow_mut();
            s.taint = true;
            s.render_gen
        });
    }

    /// Has the focus status of this node changed since the last render
    /// sweep?
    fn focus_changed(&self) -> bool {
        if global::focus_changed() {
            global::STATE.with(|global_state| -> bool {
                let gs = global_state.borrow();
                let s = self.state();
                // Our focus has changed if we're the currently focused node, or
                // if we were previously focused during the last sweep.
                s.focus_gen == gs.focus_gen || s.focus_gen == gs.last_render_focus_gen
            })
        } else {
            false
        }
    }

    /// Has the focus path status of this node changed since the last render
    /// sweep?
    fn focus_path_changed(&self) -> bool {
        if global::focus_changed() {
            global::STATE.with(|global_state| -> bool {
                let gs = global_state.borrow();
                let s = self.state();
                // Our focus has changed if we're the currently on the focus path, or
                // if we were previously focused during the last sweep.
                s.focus_path_gen == gs.focus_gen || s.focus_path_gen == gs.last_render_focus_gen
            })
        } else {
            false
        }
    }
}
