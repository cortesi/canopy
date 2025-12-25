use std::process;

use super::{id::NodeId, viewport::ViewPort, world::Core};
use crate::{
    error::{Error, Result},
    geom::{Direction, Expanse, Rect},
    layout::Style,
    path::Path,
    widget::Widget,
};

/// Read-only context available to widgets during render and measure.
pub trait ViewContext {
    /// The node currently being rendered.
    fn node_id(&self) -> NodeId;

    /// The root node of the tree.
    fn root_id(&self) -> NodeId;

    /// Screen-space rectangle for the current node's visible view.
    fn viewport(&self) -> Rect;

    /// View rectangle for the current node, relative to its canvas.
    fn view(&self) -> Rect;

    /// Canvas size for the current node.
    fn canvas(&self) -> Expanse;

    /// Screen-space rectangle for a specific node.
    fn node_viewport(&self, node: NodeId) -> Option<Rect>;

    /// View rectangle for a specific node.
    fn node_view(&self, node: NodeId) -> Option<Rect>;

    /// Canvas size for a specific node.
    fn node_canvas(&self, node: NodeId) -> Option<Expanse>;

    /// Full viewport state for a specific node.
    fn node_vp(&self, node: NodeId) -> Option<ViewPort>;

    /// Children of a node in tree order.
    fn children(&self, node: NodeId) -> Vec<NodeId>;

    /// Does the current node have focus?
    fn is_focused(&self) -> bool;

    /// Does the specified node have focus?
    fn node_is_focused(&self, node: NodeId) -> bool;

    /// Is the current node on the focus path?
    fn is_on_focus_path(&self) -> bool;

    /// Is the specified node on the focus path?
    fn node_is_on_focus_path(&self, node: NodeId) -> bool;

    /// Return the focus path for the subtree under `root`.
    fn focus_path(&self, root: NodeId) -> Path;
}

/// Mutable context available to widgets during event handling.
pub trait Context: ViewContext {
    /// Focus a node. Returns `true` if focus changed.
    fn set_focus(&mut self, node: NodeId) -> bool;

    /// Move focus in a specified direction within the subtree at root.
    fn focus_dir(&mut self, root: NodeId, dir: Direction);

    /// Focus the first node that accepts focus in the pre-order traversal of the subtree at root.
    fn focus_first(&mut self, root: NodeId);

    /// Focus the next node in the pre-order traversal of root.
    fn focus_next(&mut self, root: NodeId);

    /// Focus the previous node in the pre-order traversal of root.
    fn focus_prev(&mut self, root: NodeId);

    /// Move focus to the right of the currently focused node within the subtree at root.
    fn focus_right(&mut self, root: NodeId) {
        self.focus_dir(root, Direction::Right)
    }

    /// Move focus to the left of the currently focused node within the subtree at root.
    fn focus_left(&mut self, root: NodeId) {
        self.focus_dir(root, Direction::Left)
    }

    /// Move focus upward of the currently focused node within the subtree at root.
    fn focus_up(&mut self, root: NodeId) {
        self.focus_dir(root, Direction::Up)
    }

    /// Move focus downward of the currently focused node within the subtree at root.
    fn focus_down(&mut self, root: NodeId) {
        self.focus_dir(root, Direction::Down)
    }

    /// Scroll the view to the specified position. Returns `true` if movement occurred.
    fn scroll_to(&mut self, x: u32, y: u32) -> bool;

    /// Scroll the view by the given offsets. Returns `true` if movement occurred.
    fn scroll_by(&mut self, x: i32, y: i32) -> bool;

    /// Scroll the view up by one page. Returns `true` if movement occurred.
    fn page_up(&mut self) -> bool;

    /// Scroll the view down by one page. Returns `true` if movement occurred.
    fn page_down(&mut self) -> bool;

    /// Scroll the view up by one line. Returns `true` if movement occurred.
    fn scroll_up(&mut self) -> bool;

    /// Scroll the view down by one line. Returns `true` if movement occurred.
    fn scroll_down(&mut self) -> bool;

    /// Scroll the view left by one line. Returns `true` if movement occurred.
    fn scroll_left(&mut self) -> bool;

    /// Scroll the view right by one line. Returns `true` if movement occurred.
    fn scroll_right(&mut self) -> bool;

    /// Update the layout style for a node.
    fn with_style(&mut self, node: NodeId, f: &mut dyn FnMut(&mut Style)) -> Result<()>;

    /// Add a new widget node to the core.
    fn add(&mut self, widget: Box<dyn Widget>) -> NodeId;

    /// Execute a closure with mutable access to a widget and its node-bound context.
    fn with_widget_mut(
        &mut self,
        node: NodeId,
        f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
    ) -> Result<()>;

    /// Attach a child to a parent node.
    fn mount_child(&mut self, parent: NodeId, child: NodeId) -> Result<()>;

    /// Detach a child from a parent node.
    fn detach_child(&mut self, parent: NodeId, child: NodeId) -> Result<()>;

    /// Replace the children list for a parent node.
    fn set_children(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()>;

    /// Set node visibility. Returns `true` if visibility changed.
    fn set_hidden(&mut self, node: NodeId, hidden: bool) -> bool;

    /// Hide a node. Returns `true` if visibility changed.
    fn hide(&mut self, node: NodeId) -> bool {
        self.set_hidden(node, true)
    }

    /// Show a node. Returns `true` if visibility changed.
    fn show(&mut self, node: NodeId) -> bool {
        self.set_hidden(node, false)
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

/// Context implementation bound to a specific node.
pub struct CoreContext<'a> {
    /// Core state reference.
    core: &'a mut Core,
    /// Node bound to this context.
    node_id: NodeId,
}

impl<'a> CoreContext<'a> {
    /// Create a new context for a node.
    pub fn new(core: &'a mut Core, node_id: NodeId) -> Self {
        Self { core, node_id }
    }
}

impl<'a> ViewContext for CoreContext<'a> {
    fn node_id(&self) -> NodeId {
        self.node_id
    }

    fn root_id(&self) -> NodeId {
        self.core.root
    }

    fn viewport(&self) -> Rect {
        self.core
            .nodes
            .get(self.node_id)
            .map(|n| n.viewport)
            .unwrap_or_default()
    }

    fn view(&self) -> Rect {
        self.core
            .nodes
            .get(self.node_id)
            .map(|n| n.vp.view())
            .unwrap_or_default()
    }

    fn canvas(&self) -> Expanse {
        self.core
            .nodes
            .get(self.node_id)
            .map(|n| n.vp.canvas())
            .unwrap_or_default()
    }

    fn node_viewport(&self, node: NodeId) -> Option<Rect> {
        self.core.nodes.get(node).map(|n| n.viewport)
    }

    fn node_view(&self, node: NodeId) -> Option<Rect> {
        self.core.nodes.get(node).map(|n| n.vp.view())
    }

    fn node_canvas(&self, node: NodeId) -> Option<Expanse> {
        self.core.nodes.get(node).map(|n| n.vp.canvas())
    }

    fn node_vp(&self, node: NodeId) -> Option<ViewPort> {
        self.core.nodes.get(node).map(|n| n.vp)
    }

    fn children(&self, node: NodeId) -> Vec<NodeId> {
        self.core
            .nodes
            .get(node)
            .map(|n| n.children.clone())
            .unwrap_or_default()
    }

    fn is_focused(&self) -> bool {
        self.core.is_focused(self.node_id)
    }

    fn node_is_focused(&self, node: NodeId) -> bool {
        self.core.is_focused(node)
    }

    fn is_on_focus_path(&self) -> bool {
        self.core.is_on_focus_path(self.node_id)
    }

    fn node_is_on_focus_path(&self, node: NodeId) -> bool {
        self.core.is_on_focus_path(node)
    }

    fn focus_path(&self, root: NodeId) -> Path {
        self.core.focus_path(root)
    }
}

impl<'a> Context for CoreContext<'a> {
    fn set_focus(&mut self, node: NodeId) -> bool {
        self.core.set_focus(node)
    }

    fn focus_dir(&mut self, root: NodeId, dir: Direction) {
        self.core.focus_dir(root, dir);
    }

    fn focus_first(&mut self, root: NodeId) {
        self.core.focus_first(root);
    }

    fn focus_next(&mut self, root: NodeId) {
        self.core.focus_next(root);
    }

    fn focus_prev(&mut self, root: NodeId) {
        self.core.focus_prev(root);
    }

    fn scroll_to(&mut self, x: u32, y: u32) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.vp.view();
            node.vp.scroll_to(x, y);
            before != node.vp.view()
        } else {
            false
        }
    }

    fn scroll_by(&mut self, x: i32, y: i32) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.vp.view();
            node.vp.scroll_by(x, y);
            before != node.vp.view()
        } else {
            false
        }
    }

    fn page_up(&mut self) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.vp.view();
            node.vp.page_up();
            before != node.vp.view()
        } else {
            false
        }
    }

    fn page_down(&mut self) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.vp.view();
            node.vp.page_down();
            before != node.vp.view()
        } else {
            false
        }
    }

    fn scroll_up(&mut self) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.vp.view();
            node.vp.scroll_up();
            before != node.vp.view()
        } else {
            false
        }
    }

    fn scroll_down(&mut self) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.vp.view();
            node.vp.scroll_down();
            before != node.vp.view()
        } else {
            false
        }
    }

    fn scroll_left(&mut self) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.vp.view();
            node.vp.scroll_left();
            before != node.vp.view()
        } else {
            false
        }
    }

    fn scroll_right(&mut self) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.vp.view();
            node.vp.scroll_right();
            before != node.vp.view()
        } else {
            false
        }
    }

    fn with_style(&mut self, node: NodeId, f: &mut dyn FnMut(&mut Style)) -> Result<()> {
        let t_id = self
            .core
            .nodes
            .get(node)
            .ok_or_else(|| Error::Internal("missing node".into()))?
            .taffy_id;
        let mut style = self.core.taffy.style(t_id).cloned().unwrap_or_default();
        f(&mut style);
        self.core
            .taffy
            .set_style(t_id, style.clone())
            .map_err(|e| Error::Layout(e.to_string()))?;
        if let Some(node) = self.core.nodes.get_mut(node) {
            node.style = style;
        }
        Ok(())
    }

    fn add(&mut self, widget: Box<dyn Widget>) -> NodeId {
        self.core.add_boxed(widget)
    }

    fn with_widget_mut(
        &mut self,
        node: NodeId,
        f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        self.core.with_widget_mut(node, |widget, core| {
            let mut ctx = CoreContext::new(core, node);
            f(widget, &mut ctx)
        })
    }

    fn mount_child(&mut self, parent: NodeId, child: NodeId) -> Result<()> {
        self.core.mount_child(parent, child)
    }

    fn detach_child(&mut self, parent: NodeId, child: NodeId) -> Result<()> {
        self.core.detach_child(parent, child)
    }

    fn set_children(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()> {
        self.core.set_children(parent, children)
    }

    fn set_hidden(&mut self, node: NodeId, hidden: bool) -> bool {
        self.core.set_hidden(node, hidden)
    }

    fn start(&mut self) -> Result<()> {
        self.core
            .backend
            .as_mut()
            .ok_or_else(|| Error::Internal("backend not set".into()))?
            .start()
    }

    fn stop(&mut self) -> Result<()> {
        self.core
            .backend
            .as_mut()
            .ok_or_else(|| Error::Internal("backend not set".into()))?
            .stop()
    }

    fn exit(&mut self, code: i32) -> ! {
        let _ = self.stop().ok();
        process::exit(code)
    }

    fn current_focus_gen(&self) -> u64 {
        self.core.focus_gen
    }
}

/// Read-only context bound to a specific node.
pub struct CoreViewContext<'a> {
    /// Core state reference.
    core: &'a Core,
    /// Node bound to this context.
    node_id: NodeId,
}

impl<'a> CoreViewContext<'a> {
    /// Create a new read-only context for a node.
    pub fn new(core: &'a Core, node_id: NodeId) -> Self {
        Self { core, node_id }
    }
}

impl<'a> ViewContext for CoreViewContext<'a> {
    fn node_id(&self) -> NodeId {
        self.node_id
    }

    fn root_id(&self) -> NodeId {
        self.core.root
    }

    fn viewport(&self) -> Rect {
        self.core
            .nodes
            .get(self.node_id)
            .map(|n| n.viewport)
            .unwrap_or_default()
    }

    fn view(&self) -> Rect {
        self.core
            .nodes
            .get(self.node_id)
            .map(|n| n.vp.view())
            .unwrap_or_default()
    }

    fn canvas(&self) -> Expanse {
        self.core
            .nodes
            .get(self.node_id)
            .map(|n| n.vp.canvas())
            .unwrap_or_default()
    }

    fn node_viewport(&self, node: NodeId) -> Option<Rect> {
        self.core.nodes.get(node).map(|n| n.viewport)
    }

    fn node_view(&self, node: NodeId) -> Option<Rect> {
        self.core.nodes.get(node).map(|n| n.vp.view())
    }

    fn node_canvas(&self, node: NodeId) -> Option<Expanse> {
        self.core.nodes.get(node).map(|n| n.vp.canvas())
    }

    fn node_vp(&self, node: NodeId) -> Option<ViewPort> {
        self.core.nodes.get(node).map(|n| n.vp)
    }

    fn children(&self, node: NodeId) -> Vec<NodeId> {
        self.core
            .nodes
            .get(node)
            .map(|n| n.children.clone())
            .unwrap_or_default()
    }

    fn is_focused(&self) -> bool {
        self.core.is_focused(self.node_id)
    }

    fn node_is_focused(&self, node: NodeId) -> bool {
        self.core.is_focused(node)
    }

    fn is_on_focus_path(&self) -> bool {
        self.core.is_on_focus_path(self.node_id)
    }

    fn node_is_on_focus_path(&self, node: NodeId) -> bool {
        self.core.is_on_focus_path(node)
    }

    fn focus_path(&self, root: NodeId) -> Path {
        self.core.focus_path(root)
    }
}
