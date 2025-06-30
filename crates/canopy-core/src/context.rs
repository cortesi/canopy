use crate::{Direction, Node, Result, path::Path};

/// The API exposed to nodes by Canopy.
pub trait Context {
    /// Does the node need to render in the next sweep? This checks if the node is currently hidden, and if not, signals
    /// that we should render if the node is tainted, its focus status has changed, or if it is forcing a render.
    fn needs_render(&self, n: &dyn Node) -> bool;

    /// Is the specified node on the focus path? A node is on the focus path if it
    /// has focus, or if it's the ancestor of a node with focus.
    fn is_on_focus_path(&self, n: &mut dyn Node) -> bool;

    /// Does the node have focus?
    fn is_focused(&self, n: &dyn Node) -> bool;

    /// Move focus downward of the currently focused node within the subtree at root.
    fn focus_down(&mut self, root: &mut dyn Node);

    /// Focus the first node that accepts focus in the pre-order traversal of the subtree at root.
    fn focus_first(&mut self, root: &mut dyn Node);

    /// Move focus to the left of the currently focused node within the subtree at root.
    fn focus_left(&mut self, root: &mut dyn Node);

    /// Focus the next node in the pre-order traversal of root. If no node with focus is found, we focus the first node
    /// we can find instead.
    fn focus_next(&mut self, root: &mut dyn Node);

    /// Return the focus path for the subtree under `root`.
    fn focus_path(&self, root: &mut dyn Node) -> Path;

    /// Focus the previous node in the pre-order traversal of `root`. If no node with focus is found, we focus the first
    /// node we can find instead.
    fn focus_prev(&mut self, root: &mut dyn Node);

    /// Move focus to  right of the currently focused node within the subtree at root.
    fn focus_right(&mut self, root: &mut dyn Node);

    /// Move focus upward of the currently focused node within the subtree at root.
    fn focus_up(&mut self, root: &mut dyn Node);

    /// Focus a node. Returns `true` if focus changed.
    fn set_focus(&mut self, n: &mut dyn Node) -> bool;

    /// Move focus in a specified direction within the subtree at root.
    fn focus_dir(&mut self, root: &mut dyn Node, dir: Direction);

    /// Scroll the view to the specified position. The view is clamped within
    /// the outer rectangle. Returns `true` if movement occurred and taints the
    /// subtree on change.
    fn scroll_to(&mut self, n: &mut dyn Node, x: u16, y: u16) -> bool {
        let before = n.vp().view();
        n.state_mut().scroll_to(x, y);
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view by the given offsets. The view rectangle is clamped
    /// within the outer rectangle. Returns `true` if movement occurred and
    /// taints the subtree on change.
    fn scroll_by(&mut self, n: &mut dyn Node, x: i16, y: i16) -> bool {
        let before = n.vp().view();
        n.state_mut().scroll_by(x, y);
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view up by one page. Returns `true` if movement occurred and
    /// taints the subtree on change.
    fn page_up(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.state_mut().page_up();
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view down by one page. Returns `true` if movement occurred
    /// and taints the subtree on change.
    fn page_down(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.state_mut().page_down();
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view up by one line. Returns `true` if movement occurred and
    /// taints the subtree on change.
    fn scroll_up(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.state_mut().scroll_up();
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view down by one line. Returns `true` if movement occurred
    /// and taints the subtree on change.
    fn scroll_down(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.state_mut().scroll_down();
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view left by one line. Returns `true` if movement occurred
    /// and taints the subtree on change.
    fn scroll_left(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.state_mut().scroll_left();
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Scroll the view right by one line. Returns `true` if movement occurred
    /// and taints the subtree on change.
    fn scroll_right(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.state_mut().scroll_right();
        let changed = before != n.vp().view();
        if changed {
            self.taint_tree(n);
        }
        changed
    }

    /// Taint a node to signal that it should be re-rendered.
    fn taint(&mut self, n: &mut dyn Node);

    /// Taint the entire subtree under a node.
    fn taint_tree(&mut self, e: &mut dyn Node);

    /// Start the backend renderer.
    fn start(&mut self) -> Result<()>;

    /// Stop the backend renderer, releasing control of the terminal.
    fn stop(&mut self) -> Result<()>;

    /// Stop the render backend and exit the process.
    fn exit(&mut self, code: i32) -> !;

    /// Current focus generation counter.
    fn current_focus_gen(&self) -> u64 {
        0
    }
}
