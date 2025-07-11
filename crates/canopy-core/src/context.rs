use crate::{Direction, Node, Result, path::Path};

/// The API exposed to nodes by Canopy.
pub trait Context {
    /// Does the node need to render in the next sweep? Returns true for all visible nodes.
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
    /// the outer rectangle. Returns `true` if movement occurred.
    fn scroll_to(&mut self, n: &mut dyn Node, x: u32, y: u32) -> bool {
        let before = n.vp().view();
        n.scroll_to(x, y);

        before != n.vp().view()
    }

    /// Scroll the view by the given offsets. The view rectangle is clamped
    /// within the outer rectangle. Returns `true` if movement occurred.
    fn scroll_by(&mut self, n: &mut dyn Node, x: i32, y: i32) -> bool {
        let before = n.vp().view();
        n.scroll_by(x, y);

        before != n.vp().view()
    }

    /// Scroll the view up by one page. Returns `true` if movement occurred.
    fn page_up(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.page_up();

        before != n.vp().view()
    }

    /// Scroll the view down by one page. Returns `true` if movement occurred.
    fn page_down(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.page_down();

        before != n.vp().view()
    }

    /// Scroll the view up by one line. Returns `true` if movement occurred.
    fn scroll_up(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.scroll_up();

        before != n.vp().view()
    }

    /// Scroll the view down by one line. Returns `true` if movement occurred.
    fn scroll_down(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.scroll_down();

        before != n.vp().view()
    }

    /// Scroll the view left by one line. Returns `true` if movement occurred.
    fn scroll_left(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.scroll_left();

        before != n.vp().view()
    }

    /// Scroll the view right by one line. Returns `true` if movement occurred.
    fn scroll_right(&mut self, n: &mut dyn Node) -> bool {
        let before = n.vp().view();
        n.scroll_right();

        before != n.vp().view()
    }

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
