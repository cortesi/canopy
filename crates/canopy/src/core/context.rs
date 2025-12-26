use std::{
    any::{Any, type_name},
    process,
};

use super::{
    builder::NodeBuilder,
    id::{NodeId, TypedId},
    viewport::ViewPort,
    world::Core,
};
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

    /// Cached layout style for the current node.
    fn style(&self) -> Style;

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

    /// Return the focused leaf under the subtree rooted at `root`.
    fn focused_leaf(&self, root: NodeId) -> Option<NodeId>;

    /// Return focusable leaves in pre-order under the subtree rooted at `root`.
    fn focusable_leaves(&self, root: NodeId) -> Vec<NodeId>;
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

impl dyn Context + '_ {
    /// Execute a closure with mutable access to a widget of type `W`.
    pub fn with_widget<W, R>(
        &mut self,
        node: NodeId,
        mut f: impl FnMut(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R>
    where
        W: Widget + 'static,
    {
        let mut output = None;
        self.with_widget_mut(node, &mut |widget, ctx| {
            let any = widget as &mut dyn Any;
            let widget = any.downcast_mut::<W>().ok_or_else(|| {
                Error::Invalid(format!("expected widget type {}", type_name::<W>()))
            })?;
            output = Some(f(widget, ctx)?);
            Ok(())
        })?;
        output.ok_or_else(|| Error::Internal("missing widget result".into()))
    }

    /// Execute a closure with mutable access to a widget using a typed node ID.
    pub fn with_typed<W, R>(
        &mut self,
        node: TypedId<W>,
        f: impl FnMut(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R>
    where
        W: Widget + 'static,
    {
        self.with_widget(node.into(), f)
    }

    /// Execute a closure with mutable access to a widget of type `W` if it matches.
    pub fn try_with_widget<W, R>(
        &mut self,
        node: NodeId,
        mut f: impl FnMut(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<Option<R>>
    where
        W: Widget + 'static,
    {
        let mut output = None;
        let mut matched = false;
        self.with_widget_mut(node, &mut |widget, ctx| {
            let any = widget as &mut dyn Any;
            if let Some(widget) = any.downcast_mut::<W>() {
                matched = true;
                output = Some(f(widget, ctx)?);
            }
            Ok(())
        })?;
        if matched {
            output
                .ok_or_else(|| Error::Internal("missing widget result".into()))
                .map(Some)
        } else {
            Ok(None)
        }
    }

    /// Execute a closure with mutable access to a widget using a typed node ID if it matches.
    pub fn try_with_typed<W, R>(
        &mut self,
        node: TypedId<W>,
        f: impl FnMut(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<Option<R>>
    where
        W: Widget + 'static,
    {
        self.try_with_widget(node.into(), f)
    }

    /// Add a widget to the core and return the new node ID.
    pub fn add_widget<W: Widget + 'static>(&mut self, widget: W) -> NodeId {
        self.add(widget.into())
    }

    /// Add a widget to the core and return a typed node identifier.
    pub fn add_typed<W: Widget + 'static>(&mut self, widget: W) -> TypedId<W> {
        let node = self.add_widget(widget);
        TypedId::new(node)
    }

    /// Add a widget under `parent` and return the new node ID.
    pub fn add_child<W: Widget + 'static>(&mut self, parent: NodeId, widget: W) -> Result<NodeId> {
        let child = self.add_widget(widget);
        self.mount_child(parent, child)?;
        Ok(child)
    }

    /// Add multiple boxed widgets under `parent` and return their node IDs.
    pub fn add_children<I>(&mut self, parent: NodeId, widgets: I) -> Result<Vec<NodeId>>
    where
        I: IntoIterator<Item = Box<dyn Widget>>,
    {
        let mut ids = Vec::new();
        for widget in widgets {
            let child = self.add(widget);
            self.mount_child(parent, child)?;
            ids.push(child);
        }
        Ok(ids)
    }

    /// Return the only child of this node, or `None` if there are no children.
    ///
    /// # Panics
    ///
    /// Panics if there is more than one child.
    pub fn only_child(&self) -> Option<NodeId> {
        let children = self.children(self.node_id());
        match children.len() {
            0 => None,
            1 => children.into_iter().next(),
            _ => panic!("expected a single child for node {:?}", self.node_id()),
        }
    }

    /// Return a builder for the specified node.
    pub fn build(&mut self, node: NodeId) -> NodeBuilder<'_, dyn Context + '_> {
        NodeBuilder {
            ctx: self,
            id: node,
        }
    }

    /// Suggest a focus target after removing `removed` from the subtree rooted at `root`.
    pub fn suggest_focus_after_remove(&mut self, root: NodeId, removed: NodeId) -> Option<NodeId> {
        let focusables = self.focusable_leaves(root);
        if let Some(index) = focusables.iter().position(|id| *id == removed) {
            if let Some(next) = focusables.get(index + 1).copied() {
                return Some(next);
            }
            if index > 0 {
                return Some(focusables[index - 1]);
            }
            return None;
        }
        self.focused_leaf(root)
    }
}

/// Return whether a node's widget reports it accepts focus.
fn node_accepts_focus(core: &Core, node_id: NodeId) -> bool {
    core.nodes
        .get(node_id)
        .and_then(|node| node.widget.as_ref())
        .is_some_and(|widget| widget.accept_focus())
}

/// Return true if `node` is within the subtree rooted at `root`.
fn is_descendant(core: &Core, root: NodeId, node: NodeId) -> bool {
    let mut current = Some(node);
    while let Some(id) = current {
        if id == root {
            return true;
        }
        current = core.nodes.get(id).and_then(|n| n.parent);
    }
    false
}

/// Collect focusable leaves in pre-order for a core subtree.
fn focusable_leaves_for(core: &Core, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let Some(node) = core.nodes.get(id) else {
            continue;
        };
        if node.hidden {
            continue;
        }
        if node_accepts_focus(core, id) {
            out.push(id);
        }
        for child in node.children.iter().rev() {
            stack.push(*child);
        }
    }
    out
}

/// Return the focused leaf within the core subtree rooted at `root`.
fn focused_leaf_for(core: &Core, root: NodeId) -> Option<NodeId> {
    let focused = core.focus?;
    let node = core.nodes.get(focused)?;
    if node.hidden || !is_descendant(core, root, focused) {
        return None;
    }
    if node_accepts_focus(core, focused) {
        Some(focused)
    } else {
        None
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

    fn style(&self) -> Style {
        self.core
            .nodes
            .get(self.node_id)
            .map(|n| n.style.clone())
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

    fn focused_leaf(&self, root: NodeId) -> Option<NodeId> {
        focused_leaf_for(self.core, root)
    }

    fn focusable_leaves(&self, root: NodeId) -> Vec<NodeId> {
        focusable_leaves_for(self.core, root)
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
        self.core.with_style(node, f)
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

    fn style(&self) -> Style {
        self.core
            .nodes
            .get(self.node_id)
            .map(|n| n.style.clone())
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

    fn focused_leaf(&self, root: NodeId) -> Option<NodeId> {
        focused_leaf_for(self.core, root)
    }

    fn focusable_leaves(&self, root: NodeId) -> Vec<NodeId> {
        focusable_leaves_for(self.core, root)
    }
}
