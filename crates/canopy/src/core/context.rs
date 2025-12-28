use std::{
    any::{Any, type_name},
    process,
};

use super::{
    commands,
    id::{NodeId, TypedId},
    style::StyleEffect,
    view::View,
    world::Core,
};
use crate::{
    error::{Error, Result},
    geom::{Direction, Expanse, Point, PointI32, Rect, RectI32},
    layout::Layout,
    path::Path,
    style::StyleMap,
    widget::Widget,
};

/// Read-only context available to widgets during render and measure.
pub trait ViewContext {
    /// The node currently being rendered.
    fn node_id(&self) -> NodeId;

    /// The root node of the tree.
    fn root_id(&self) -> NodeId;

    /// View information for the current node.
    fn view(&self) -> &View;

    /// Cached layout configuration for the current node.
    fn layout(&self) -> Layout;

    /// View information for a specific node.
    fn node_view(&self, node: NodeId) -> Option<View>;

    /// Mark this node dirty so the next frame re-runs layout.
    fn taint(&self);

    /// Canvas size for the current node.
    fn canvas(&self) -> Expanse {
        self.view().canvas
    }

    /// Visible view rectangle in content coordinates.
    fn view_rect(&self) -> Rect {
        self.view().view_rect()
    }

    /// Visible view rectangle in local outer coordinates.
    fn view_rect_local(&self) -> Rect {
        self.view().view_rect_local()
    }

    /// Local outer rectangle for this node.
    fn outer_rect_local(&self) -> Rect {
        self.view().outer_rect_local()
    }

    /// Children of the current node in tree order.
    fn children(&self) -> Vec<NodeId> {
        self.children_of(self.node_id())
    }

    /// Children of a specific node in tree order.
    fn children_of(&self, node: NodeId) -> Vec<NodeId>;

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

    /// Return the parent of a node, or `None` if it is the root or not found.
    fn parent_of(&self, node: NodeId) -> Option<NodeId>;
}

/// Default zero-sized view used when a node lacks layout data.
const DEFAULT_VIEW: View = View {
    outer: RectI32 {
        tl: PointI32 { x: 0, y: 0 },
        w: 0,
        h: 0,
    },
    content: RectI32 {
        tl: PointI32 { x: 0, y: 0 },
        w: 0,
        h: 0,
    },
    tl: Point { x: 0, y: 0 },
    canvas: Expanse { w: 0, h: 0 },
};

/// Clamp a scroll offset so it stays within the view/canvas bounds.
fn clamp_scroll_offset(scroll: &mut Point, view: Expanse, canvas: Expanse) {
    let max_x = if view.w == 0 {
        0
    } else {
        canvas.w.saturating_sub(view.w)
    };
    let max_y = if view.h == 0 {
        0
    } else {
        canvas.h.saturating_sub(view.h)
    };
    scroll.x = scroll.x.min(max_x);
    scroll.y = scroll.y.min(max_y);
}

/// Mutable context available to widgets during event handling.
pub trait Context: ViewContext {
    /// Focus a node. Returns `true` if focus changed.
    fn set_focus(&mut self, node: NodeId) -> bool;

    /// Move focus in a specified direction within the current node's subtree.
    fn focus_dir(&mut self, dir: Direction) {
        self.focus_dir_in(self.node_id(), dir)
    }

    /// Move focus in a specified direction within the specified subtree.
    fn focus_dir_in(&mut self, root: NodeId, dir: Direction);

    /// Move focus in a specified direction within the entire tree (from root).
    fn focus_dir_global(&mut self, dir: Direction) {
        self.focus_dir_in(self.root_id(), dir)
    }

    /// Focus the first node that accepts focus in the current node's subtree.
    fn focus_first(&mut self) {
        self.focus_first_in(self.node_id())
    }

    /// Focus the first node that accepts focus in the specified subtree.
    fn focus_first_in(&mut self, root: NodeId);

    /// Focus the first node that accepts focus in the entire tree (from root).
    fn focus_first_global(&mut self) {
        self.focus_first_in(self.root_id())
    }

    /// Focus the next node in the current node's subtree.
    fn focus_next(&mut self) {
        self.focus_next_in(self.node_id())
    }

    /// Focus the next node in the specified subtree.
    fn focus_next_in(&mut self, root: NodeId);

    /// Focus the next node in the entire tree (from root).
    fn focus_next_global(&mut self) {
        self.focus_next_in(self.root_id())
    }

    /// Focus the previous node in the current node's subtree.
    fn focus_prev(&mut self) {
        self.focus_prev_in(self.node_id())
    }

    /// Focus the previous node in the specified subtree.
    fn focus_prev_in(&mut self, root: NodeId);

    /// Focus the previous node in the entire tree (from root).
    fn focus_prev_global(&mut self) {
        self.focus_prev_in(self.root_id())
    }

    /// Move focus to the right within the current node's subtree.
    fn focus_right(&mut self) {
        self.focus_dir(Direction::Right)
    }

    /// Move focus to the right within the specified subtree.
    fn focus_right_in(&mut self, root: NodeId) {
        self.focus_dir_in(root, Direction::Right)
    }

    /// Move focus to the right within the entire tree (from root).
    fn focus_right_global(&mut self) {
        self.focus_dir_global(Direction::Right)
    }

    /// Move focus to the left within the current node's subtree.
    fn focus_left(&mut self) {
        self.focus_dir(Direction::Left)
    }

    /// Move focus to the left within the specified subtree.
    fn focus_left_in(&mut self, root: NodeId) {
        self.focus_dir_in(root, Direction::Left)
    }

    /// Move focus to the left within the entire tree (from root).
    fn focus_left_global(&mut self) {
        self.focus_dir_global(Direction::Left)
    }

    /// Move focus upward within the current node's subtree.
    fn focus_up(&mut self) {
        self.focus_dir(Direction::Up)
    }

    /// Move focus upward within the specified subtree.
    fn focus_up_in(&mut self, root: NodeId) {
        self.focus_dir_in(root, Direction::Up)
    }

    /// Move focus upward within the entire tree (from root).
    fn focus_up_global(&mut self) {
        self.focus_dir_global(Direction::Up)
    }

    /// Move focus downward within the current node's subtree.
    fn focus_down(&mut self) {
        self.focus_dir(Direction::Down)
    }

    /// Move focus downward within the specified subtree.
    fn focus_down_in(&mut self, root: NodeId) {
        self.focus_dir_in(root, Direction::Down)
    }

    /// Move focus downward within the entire tree (from root).
    fn focus_down_global(&mut self) {
        self.focus_dir_global(Direction::Down)
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

    /// Update the layout for the current node.
    fn with_layout(&mut self, f: &mut dyn FnMut(&mut Layout)) -> Result<()> {
        let node = self.node_id();
        self.with_layout_of(node, f)
    }

    /// Update the layout for a specific node.
    fn with_layout_of(&mut self, node: NodeId, f: &mut dyn FnMut(&mut Layout)) -> Result<()>;

    /// Add a new widget node to the core.
    fn add(&mut self, widget: Box<dyn Widget>) -> NodeId;

    /// Execute a closure with mutable access to a widget and its node-bound context.
    fn with_widget_mut(
        &mut self,
        node: NodeId,
        f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
    ) -> Result<()>;

    /// Dispatch a command relative to this node.
    fn dispatch_command(
        &mut self,
        cmd: &commands::CommandInvocation,
    ) -> Result<Option<commands::ReturnValue>>;

    /// Attach a child to the current node.
    fn mount_child(&mut self, child: NodeId) -> Result<()> {
        self.mount_child_to(self.node_id(), child)
    }

    /// Attach a child to a specific parent node.
    fn mount_child_to(&mut self, parent: NodeId, child: NodeId) -> Result<()>;

    /// Detach a child from the current node.
    fn detach_child(&mut self, child: NodeId) -> Result<()> {
        self.detach_child_from(self.node_id(), child)
    }

    /// Detach a child from a specific parent node.
    fn detach_child_from(&mut self, parent: NodeId, child: NodeId) -> Result<()>;

    /// Replace the children list for the current node.
    fn set_children(&mut self, children: Vec<NodeId>) -> Result<()> {
        self.set_children_of(self.node_id(), children)
    }

    /// Replace the children list for a specific parent node.
    fn set_children_of(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()>;

    /// Set the current node's visibility. Returns `true` if visibility changed.
    fn set_hidden(&mut self, hidden: bool) -> bool {
        self.set_hidden_of(self.node_id(), hidden)
    }

    /// Set a specific node's visibility. Returns `true` if visibility changed.
    fn set_hidden_of(&mut self, node: NodeId, hidden: bool) -> bool;

    /// Hide the current node. Returns `true` if visibility changed.
    fn hide(&mut self) -> bool {
        self.set_hidden(true)
    }

    /// Hide a specific node. Returns `true` if visibility changed.
    fn hide_node(&mut self, node: NodeId) -> bool {
        self.set_hidden_of(node, true)
    }

    /// Show the current node. Returns `true` if visibility changed.
    fn show(&mut self) -> bool {
        self.set_hidden(false)
    }

    /// Show a specific node. Returns `true` if visibility changed.
    fn show_node(&mut self, node: NodeId) -> bool {
        self.set_hidden_of(node, false)
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

    /// Add an effect to a node that will be applied during rendering.
    /// Effects stack and inherit through the tree.
    fn push_effect(&mut self, node: NodeId, effect: Box<dyn StyleEffect>) -> Result<()>;

    /// Clear all effects on a node.
    fn clear_effects(&mut self, node: NodeId) -> Result<()>;

    /// Set whether a node should clear inherited effects before applying local ones.
    fn set_clear_inherited_effects(&mut self, node: NodeId, clear: bool) -> Result<()>;

    /// Set the style map to be used for rendering.
    /// The style change will be applied before the next render.
    fn set_style(&mut self, style: StyleMap);
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

    /// Add a widget to the core but do not attach it to any parent (orphan).
    pub fn add_orphan<W: Widget + 'static>(&mut self, widget: W) -> NodeId {
        self.add(widget.into())
    }

    /// Add a widget to the core and return a typed node identifier (orphan).
    pub fn add_orphan_typed<W: Widget + 'static>(&mut self, widget: W) -> TypedId<W> {
        let node = self.add_orphan(widget);
        TypedId::new(node)
    }

    /// Add a widget as a child of the current node and return the new node ID.
    pub fn add_child<W: Widget + 'static>(&mut self, widget: W) -> Result<NodeId> {
        let child = self.add_orphan(widget);
        self.mount_child(child)?;
        Ok(child)
    }

    /// Add a widget as a child of a specific parent and return the new node ID.
    pub fn add_child_to<W: Widget + 'static>(
        &mut self,
        parent: NodeId,
        widget: W,
    ) -> Result<NodeId> {
        let child = self.add_orphan(widget);
        self.mount_child_to(parent, child)?;
        Ok(child)
    }

    /// Add multiple boxed widgets as children of the current node and return their node IDs.
    pub fn add_children<I>(&mut self, widgets: I) -> Result<Vec<NodeId>>
    where
        I: IntoIterator<Item = Box<dyn Widget>>,
    {
        let mut ids = Vec::new();
        for widget in widgets {
            let child = self.add(widget);
            self.mount_child(child)?;
            ids.push(child);
        }
        Ok(ids)
    }

    /// Add multiple boxed widgets as children of a specific parent and return their node IDs.
    pub fn add_children_to<I>(&mut self, parent: NodeId, widgets: I) -> Result<Vec<NodeId>>
    where
        I: IntoIterator<Item = Box<dyn Widget>>,
    {
        let mut ids = Vec::new();
        for widget in widgets {
            let child = self.add(widget);
            self.mount_child_to(parent, child)?;
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
        let children = self.children();
        match children.len() {
            0 => None,
            1 => children.into_iter().next(),
            _ => panic!("expected a single child for node {:?}", self.node_id()),
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
        .is_some_and(|widget| {
            let ctx = CoreViewContext::new(core, node_id);
            widget.accept_focus(&ctx)
        })
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

    fn view(&self) -> &View {
        self.core
            .nodes
            .get(self.node_id)
            .map(|n| &n.view)
            .unwrap_or(&DEFAULT_VIEW)
    }

    fn layout(&self) -> Layout {
        self.core
            .nodes
            .get(self.node_id)
            .map(|n| n.layout)
            .unwrap_or_default()
    }

    fn node_view(&self, node: NodeId) -> Option<View> {
        self.core.nodes.get(node).map(|n| n.view)
    }

    fn taint(&self) {
        if let Some(node) = self.core.nodes.get(self.node_id) {
            node.layout_dirty.set(true);
        }
    }

    fn children_of(&self, node: NodeId) -> Vec<NodeId> {
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

    fn parent_of(&self, node: NodeId) -> Option<NodeId> {
        self.core.nodes.get(node).and_then(|n| n.parent)
    }
}

impl<'a> Context for CoreContext<'a> {
    fn set_focus(&mut self, node: NodeId) -> bool {
        self.core.set_focus(node)
    }

    fn focus_dir_in(&mut self, root: NodeId, dir: Direction) {
        self.core.focus_dir(root, dir);
    }

    fn focus_first_in(&mut self, root: NodeId) {
        self.core.focus_first(root);
    }

    fn focus_next_in(&mut self, root: NodeId) {
        self.core.focus_next(root);
    }

    fn focus_prev_in(&mut self, root: NodeId) {
        self.core.focus_prev(root);
    }

    fn scroll_to(&mut self, x: u32, y: u32) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.scroll;
            node.scroll = Point { x, y };
            clamp_scroll_offset(&mut node.scroll, node.content_size, node.canvas);
            before != node.scroll
        } else {
            false
        }
    }

    fn scroll_by(&mut self, x: i32, y: i32) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.scroll;
            node.scroll = node.scroll.scroll(x, y);
            clamp_scroll_offset(&mut node.scroll, node.content_size, node.canvas);
            before != node.scroll
        } else {
            false
        }
    }

    fn page_up(&mut self) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.scroll;
            node.scroll = node.scroll.scroll(0, -(node.content_size.h as i32));
            clamp_scroll_offset(&mut node.scroll, node.content_size, node.canvas);
            before != node.scroll
        } else {
            false
        }
    }

    fn page_down(&mut self) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.scroll;
            node.scroll = node.scroll.scroll(0, node.content_size.h as i32);
            clamp_scroll_offset(&mut node.scroll, node.content_size, node.canvas);
            before != node.scroll
        } else {
            false
        }
    }

    fn scroll_up(&mut self) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.scroll;
            node.scroll = node.scroll.scroll(0, -1);
            clamp_scroll_offset(&mut node.scroll, node.content_size, node.canvas);
            before != node.scroll
        } else {
            false
        }
    }

    fn scroll_down(&mut self) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.scroll;
            node.scroll = node.scroll.scroll(0, 1);
            clamp_scroll_offset(&mut node.scroll, node.content_size, node.canvas);
            before != node.scroll
        } else {
            false
        }
    }

    fn scroll_left(&mut self) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.scroll;
            node.scroll = node.scroll.scroll(-1, 0);
            clamp_scroll_offset(&mut node.scroll, node.content_size, node.canvas);
            before != node.scroll
        } else {
            false
        }
    }

    fn scroll_right(&mut self) -> bool {
        let node = self.core.nodes.get_mut(self.node_id);
        if let Some(node) = node {
            let before = node.scroll;
            node.scroll = node.scroll.scroll(1, 0);
            clamp_scroll_offset(&mut node.scroll, node.content_size, node.canvas);
            before != node.scroll
        } else {
            false
        }
    }

    fn with_layout_of(&mut self, node: NodeId, f: &mut dyn FnMut(&mut Layout)) -> Result<()> {
        self.core.with_layout_of(node, |layout| f(layout))
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

    fn dispatch_command(
        &mut self,
        cmd: &commands::CommandInvocation,
    ) -> Result<Option<commands::ReturnValue>> {
        commands::dispatch(self.core, self.node_id, cmd)
    }

    fn mount_child_to(&mut self, parent: NodeId, child: NodeId) -> Result<()> {
        self.core.mount_child(parent, child)
    }

    fn detach_child_from(&mut self, parent: NodeId, child: NodeId) -> Result<()> {
        self.core.detach_child(parent, child)
    }

    fn set_children_of(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()> {
        self.core.set_children(parent, children)
    }

    fn set_hidden_of(&mut self, node: NodeId, hidden: bool) -> bool {
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

    fn push_effect(&mut self, node: NodeId, effect: Box<dyn StyleEffect>) -> Result<()> {
        let node = self
            .core
            .nodes
            .get_mut(node)
            .ok_or(Error::NodeNotFound(node))?;
        if let Some(ref mut effects) = node.effects {
            effects.push(effect);
        } else {
            node.effects = Some(vec![effect]);
        }
        Ok(())
    }

    fn clear_effects(&mut self, node: NodeId) -> Result<()> {
        let node = self
            .core
            .nodes
            .get_mut(node)
            .ok_or(Error::NodeNotFound(node))?;
        node.effects = None;
        Ok(())
    }

    fn set_clear_inherited_effects(&mut self, node: NodeId, clear: bool) -> Result<()> {
        let node = self
            .core
            .nodes
            .get_mut(node)
            .ok_or(Error::NodeNotFound(node))?;
        node.clear_inherited_effects = clear;
        Ok(())
    }

    fn set_style(&mut self, style: StyleMap) {
        self.core.pending_style = Some(style);
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

    fn view(&self) -> &View {
        self.core
            .nodes
            .get(self.node_id)
            .map(|n| &n.view)
            .unwrap_or(&DEFAULT_VIEW)
    }

    fn layout(&self) -> Layout {
        self.core
            .nodes
            .get(self.node_id)
            .map(|n| n.layout)
            .unwrap_or_default()
    }

    fn node_view(&self, node: NodeId) -> Option<View> {
        self.core.nodes.get(node).map(|n| n.view)
    }

    fn taint(&self) {
        if let Some(node) = self.core.nodes.get(self.node_id) {
            node.layout_dirty.set(true);
        }
    }

    fn children_of(&self, node: NodeId) -> Vec<NodeId> {
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

    fn parent_of(&self, node: NodeId) -> Option<NodeId> {
        self.core.nodes.get(node).and_then(|n| n.parent)
    }
}
