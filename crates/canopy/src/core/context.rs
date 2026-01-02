use std::{
    any::{Any, TypeId, type_name, type_name_of_val},
    process,
    result::Result as StdResult,
};

use super::{
    commands,
    id::{NodeId, TypedId},
    style::StyleEffect,
    view::View,
    world::Core,
};
use crate::{
    commands::{ArgValue, CommandError, CommandInvocation, CommandScopeFrame, ListRowContext},
    core::focus::FocusManager,
    error::{Error, Result},
    event::{Event, mouse::MouseEvent},
    geom::{Direction, Expanse, Point, PointI32, Rect, RectI32},
    layout::Layout,
    path::{Path, PathMatcher},
    style::StyleMap,
    widget::Widget,
};

/// Read-only context available to widgets during render and measure.
pub trait ReadContext {
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

    /// Widget type identifier for a specific node.
    fn node_type_id(&self, node: NodeId) -> Option<TypeId>;

    /// Mark this node dirty so the next frame re-runs layout.
    fn invalidate_layout(&self);

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

    /// Return the path for a node relative to a root.
    fn node_path(&self, root: NodeId, node: NodeId) -> Path;

    /// Return a keyed child relative to the current node.
    fn child_keyed(&self, key: &str) -> Option<NodeId>;

    /// Current focus generation counter.
    fn current_focus_gen(&self) -> u64 {
        0
    }

    /// Find the first node whose path matches the filter, relative to the current node.
    ///
    /// The filter is normalized to match full paths.
    fn find_node(&self, path_filter: &str) -> Option<NodeId> {
        let filter = normalize_path_filter(path_filter);
        let matcher = PathMatcher::new(&filter).ok()?;
        let root = self.node_id();
        let mut stack = vec![root];

        while let Some(id) = stack.pop() {
            let path = self.node_path(root, id);
            if matcher.check(&path).is_some() {
                return Some(id);
            }

            let children = self.children_of(id);
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }

        None
    }

    /// Find all nodes whose paths match the filter, relative to the current node.
    ///
    /// The filter is normalized to match full paths.
    fn find_nodes(&self, path_filter: &str) -> Vec<NodeId> {
        let filter = normalize_path_filter(path_filter);
        let Ok(matcher) = PathMatcher::new(&filter) else {
            return Vec::new();
        };
        let root = self.node_id();
        let mut out = Vec::new();
        let mut stack = vec![root];

        while let Some(id) = stack.pop() {
            let path = self.node_path(root, id);
            if matcher.check(&path).is_some() {
                out.push(id);
            }

            let children = self.children_of(id);
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }

        out
    }
}

impl dyn ReadContext + '_ {
    /// Return the first node of type `W` within `root` and its descendants.
    pub fn first_from<W: Widget + 'static>(&self, root: NodeId) -> Option<TypedId<W>> {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if self.node_type_id(id) == Some(TypeId::of::<W>()) {
                return Some(TypedId::new(id));
            }
            for child in self.children_of(id).into_iter().rev() {
                stack.push(child);
            }
        }
        None
    }

    /// Return all nodes of type `W` within `root` and its descendants.
    pub fn all_from<W: Widget + 'static>(&self, root: NodeId) -> Vec<TypedId<W>> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if self.node_type_id(id) == Some(TypeId::of::<W>()) {
                out.push(TypedId::new(id));
            }
            for child in self.children_of(id).into_iter().rev() {
                stack.push(child);
            }
        }
        out
    }

    /// Return the first widget of type `W` anywhere in the tree, including the root.
    pub fn first_in_tree<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.first_from::<W>(self.root_id())
    }

    /// Return all widgets of type `W` anywhere in the tree, including the root.
    pub fn all_in_tree<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        self.all_from::<W>(self.root_id())
    }

    /// Find exactly one node matching a path filter.
    pub fn find_one(&self, path: &str) -> Result<NodeId> {
        let matches = self.find_nodes(path);
        match matches.len() {
            0 => Err(Error::NotFound(format!("path {path}"))),
            1 => Ok(matches[0]),
            _ => Err(Error::MultipleMatches),
        }
    }

    /// Try to find exactly one node matching a path filter.
    pub fn try_find_one(&self, path: &str) -> Result<Option<NodeId>> {
        let matches = self.find_nodes(path);
        match matches.len() {
            0 => Ok(None),
            1 => Ok(Some(matches[0])),
            _ => Err(Error::MultipleMatches),
        }
    }

    /// Return the first child of type `W`.
    pub fn first_child<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.children()
            .into_iter()
            .find(|id| self.node_matches_type::<W>(*id))
            .map(TypedId::new)
    }

    /// Return the unique child of type `W`, or error if more than one exists.
    pub fn unique_child<W: Widget + 'static>(&self) -> Result<Option<TypedId<W>>> {
        let mut found = None;
        for child in self.children() {
            if !self.node_matches_type::<W>(child) {
                continue;
            }
            if found.is_some() {
                return Err(Error::MultipleMatches);
            }
            found = Some(TypedId::new(child));
        }
        Ok(found)
    }

    /// Return all direct children of type `W`.
    pub fn children_of_type<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        self.children()
            .into_iter()
            .filter(|id| self.node_matches_type::<W>(*id))
            .map(TypedId::new)
            .collect()
    }

    /// Return the first descendant of type `W` (excluding self).
    pub fn first_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        let mut stack = self.children();
        while let Some(id) = stack.pop() {
            if self.node_matches_type::<W>(id) {
                return Some(TypedId::new(id));
            }
            for child in self.children_of(id).into_iter().rev() {
                stack.push(child);
            }
        }
        None
    }

    /// Return the unique descendant of type `W`, or error if more than one exists.
    pub fn unique_descendant<W: Widget + 'static>(&self) -> Result<Option<TypedId<W>>> {
        let mut found = None;
        let mut stack = self.children();
        while let Some(id) = stack.pop() {
            if self.node_matches_type::<W>(id) {
                if found.is_some() {
                    return Err(Error::MultipleMatches);
                }
                found = Some(TypedId::new(id));
            }
            for child in self.children_of(id).into_iter().rev() {
                stack.push(child);
            }
        }
        Ok(found)
    }

    /// Return all descendants of type `W` (excluding self).
    pub fn descendants_of_type<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        let mut out = Vec::new();
        let mut stack = self.children();
        while let Some(id) = stack.pop() {
            if self.node_matches_type::<W>(id) {
                out.push(TypedId::new(id));
            }
            for child in self.children_of(id).into_iter().rev() {
                stack.push(child);
            }
        }
        out
    }

    /// Return the descendant of type `W` that is on the focus path, if any.
    pub fn focused_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.descendants_of_type::<W>()
            .into_iter()
            .find(|id| self.node_is_on_focus_path((*id).into()))
    }

    /// Return the descendant of type `W` on the focus path, or the first if none focused.
    ///
    /// This searches only within the current node's subtree. Use the tree-wide helpers on
    /// `ReadContext` if you need to search from an arbitrary root.
    pub fn focused_or_first_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        let descendants = self.descendants_of_type::<W>();
        let focused = descendants
            .iter()
            .copied()
            .find(|id| self.node_is_on_focus_path((*id).into()));
        focused.or_else(|| descendants.into_iter().next())
    }

    /// Return true if the node's widget type matches `W`.
    fn node_matches_type<W: Widget + 'static>(&self, node: NodeId) -> bool {
        self.node_type_id(node) == Some(TypeId::of::<W>())
    }
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

/// Normalize a path filter to match a full path.
fn normalize_path_filter(path_filter: &str) -> String {
    let trimmed = path_filter.trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("/{trimmed}/")
    }
}

/// Mutable context available to widgets during event handling.
pub trait Context: ReadContext {
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

    /// Capture mouse events for the current node. Returns `true` if capture changed.
    fn capture_mouse(&mut self) -> bool;

    /// Release mouse capture if held by the current node. Returns `true` if capture changed.
    fn release_mouse(&mut self) -> bool;

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

    /// Create a new widget node detached from the tree.
    fn create_detached_boxed(&mut self, widget: Box<dyn Widget>) -> NodeId;

    /// Execute a closure with mutable access to a widget and its node-bound context.
    fn with_widget_mut(
        &mut self,
        node: NodeId,
        f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
    ) -> Result<()>;

    /// Dispatch a command relative to this node.
    fn dispatch_command(&mut self, cmd: &CommandInvocation) -> StdResult<ArgValue, CommandError>;

    /// Dispatch a command with an explicit command-scope frame.
    fn dispatch_command_scoped(
        &mut self,
        frame: CommandScopeFrame,
        cmd: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError>;

    /// Return the current event snapshot for injection.
    fn current_event(&self) -> Option<&Event>;

    /// Return the current mouse event for injection.
    fn current_mouse_event(&self) -> Option<MouseEvent>;

    /// Return the current list-row context for injection.
    fn current_list_row(&self) -> Option<ListRowContext>;

    /// Add a boxed widget as a child of a specific parent and return the new node ID.
    fn add_child_to_boxed(&mut self, parent: NodeId, widget: Box<dyn Widget>) -> Result<NodeId>;

    /// Add a boxed widget as a keyed child of a specific parent and return the new node ID.
    fn add_child_to_keyed_boxed(
        &mut self,
        parent: NodeId,
        key: &str,
        widget: Box<dyn Widget>,
    ) -> Result<NodeId>;

    /// Attach a detached child to a parent.
    fn attach(&mut self, parent: NodeId, child: NodeId) -> Result<()>;

    /// Attach a detached child to a parent using a unique key.
    fn attach_keyed(&mut self, parent: NodeId, key: &str, child: NodeId) -> Result<()>;

    /// Detach a child from its parent.
    fn detach(&mut self, child: NodeId) -> Result<()>;

    /// Remove a node and all descendants from the arena.
    fn remove_subtree(&mut self, node: NodeId) -> Result<()>;

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
    /// Get a read-only view of this context.
    ///
    /// This is useful for calling methods defined on `ReadContext` that are not
    /// directly available on `Context`.
    pub fn read(&self) -> &dyn ReadContext {
        self
    }

    /// Find exactly one node matching a path filter.
    pub fn find_one(&self, path: &str) -> Result<NodeId> {
        self.read().find_one(path)
    }

    /// Try to find exactly one node matching a path filter.
    pub fn try_find_one(&self, path: &str) -> Result<Option<NodeId>> {
        self.read().try_find_one(path)
    }

    /// Return the first child of type `W`.
    pub fn first_child<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.read().first_child()
    }

    /// Return the unique child of type `W`, or error if more than one exists.
    pub fn unique_child<W: Widget + 'static>(&self) -> Result<Option<TypedId<W>>> {
        self.read().unique_child()
    }

    /// Return all direct children of type `W`.
    pub fn children_of_type<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        self.read().children_of_type()
    }

    /// Return the first descendant of type `W` (excluding self).
    pub fn first_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.read().first_descendant()
    }

    /// Return the unique descendant of type `W`, or error if more than one exists.
    pub fn unique_descendant<W: Widget + 'static>(&self) -> Result<Option<TypedId<W>>> {
        self.read().unique_descendant()
    }

    /// Return all descendants of type `W` (excluding self).
    pub fn descendants_of_type<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        self.read().descendants_of_type()
    }

    /// Return the descendant of type `W` that is on the focus path, if any.
    pub fn focused_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.read().focused_descendant()
    }

    /// Return the descendant of type `W` on the focus path, or the first if none focused.
    pub fn focused_or_first_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.read().focused_or_first_descendant()
    }

    /// Execute a closure with mutable access to a widget of type `W`.
    pub fn with_widget<W, R>(
        &mut self,
        node: NodeId,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R>
    where
        W: Widget + 'static,
    {
        let mut output = None;
        let mut f = Some(f);
        let expected = TypeId::of::<W>();
        self.with_widget_mut(node, &mut |widget, ctx| {
            let actual = ctx.node_type_id(node).ok_or(Error::NodeNotFound(node))?;
            if actual != expected {
                return Err(Error::TypeMismatch {
                    expected: type_name::<W>().to_string(),
                    actual: type_name_of_val(widget).to_string(),
                });
            }
            let any = widget as &mut dyn Any;
            let widget = any
                .downcast_mut::<W>()
                .ok_or_else(|| Error::Internal("widget type mismatch".into()))?;
            let f = f
                .take()
                .ok_or_else(|| Error::Internal("missing widget closure".into()))?;
            output = Some(f(widget, ctx)?);
            Ok(())
        })?;
        output.ok_or_else(|| Error::Internal("missing widget result".into()))
    }

    /// Execute a closure with mutable access to a widget using a typed node ID.
    pub fn with_typed<W, R>(
        &mut self,
        node: TypedId<W>,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
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
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<Option<R>>
    where
        W: Widget + 'static,
    {
        let mut output = None;
        let mut matched = false;
        let mut f = Some(f);
        let expected = TypeId::of::<W>();
        self.with_widget_mut(node, &mut |widget, ctx| {
            let actual = ctx.node_type_id(node).ok_or(Error::NodeNotFound(node))?;
            if actual != expected {
                return Ok(());
            }
            let any = widget as &mut dyn Any;
            let widget = any
                .downcast_mut::<W>()
                .ok_or_else(|| Error::Internal("widget type mismatch".into()))?;
            matched = true;
            let f = f
                .take()
                .ok_or_else(|| Error::Internal("missing widget closure".into()))?;
            output = Some(f(widget, ctx)?);
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
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<Option<R>>
    where
        W: Widget + 'static,
    {
        self.try_with_widget(node.into(), f)
    }

    /// Create a widget node detached from the tree.
    pub fn create_detached<W: Widget + 'static>(&mut self, widget: W) -> NodeId {
        self.create_detached_boxed(widget.into())
    }

    /// Add a widget as a child of the current node and return the new node ID.
    pub fn add_child<W: Widget + 'static>(&mut self, widget: W) -> Result<NodeId> {
        self.add_child_to(self.node_id(), widget)
    }

    /// Add a widget as a child of a specific parent and return the new node ID.
    pub fn add_child_to<W: Widget + 'static>(
        &mut self,
        parent: NodeId,
        widget: W,
    ) -> Result<NodeId> {
        self.add_child_to_boxed(parent, widget.into())
    }

    /// Add a widget as a keyed child of the current node and return the new node ID.
    pub fn add_child_keyed<W: Widget + 'static>(&mut self, key: &str, widget: W) -> Result<NodeId> {
        self.add_child_to_keyed(self.node_id(), key, widget)
    }

    /// Add a widget as a keyed child of a specific parent and return the new node ID.
    pub fn add_child_to_keyed<W: Widget + 'static>(
        &mut self,
        parent: NodeId,
        key: &str,
        widget: W,
    ) -> Result<NodeId> {
        self.add_child_to_keyed_boxed(parent, key, widget.into())
    }

    /// Add multiple boxed widgets as children of the current node and return their node IDs.
    pub fn add_children<I>(&mut self, widgets: I) -> Result<Vec<NodeId>>
    where
        I: IntoIterator<Item = Box<dyn Widget>>,
    {
        let mut ids = Vec::new();
        for widget in widgets {
            let child = self.add_child_to_boxed(self.node_id(), widget)?;
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
            let child = self.add_child_to_boxed(parent, widget)?;
            ids.push(child);
        }
        Ok(ids)
    }

    /// Execute a closure with a widget at a unique path match.
    pub fn with_node_at<W: Widget + 'static, R>(
        &mut self,
        path: &str,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        let node = (self as &dyn ReadContext).find_one(path)?;
        self.with_widget(node, f)
    }

    /// Execute a closure with a widget at a unique path match if it exists.
    pub fn try_with_node_at<W: Widget + 'static, R>(
        &mut self,
        path: &str,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<Option<R>> {
        let node = (self as &dyn ReadContext).try_find_one(path)?;
        let Some(node) = node else {
            return Ok(None);
        };
        self.with_widget(node, f).map(Some)
    }

    /// Execute a closure with a keyed child of type `W`.
    pub fn with_keyed<W: Widget + 'static, R>(
        &mut self,
        key: &str,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        let node = self
            .child_keyed(key)
            .ok_or_else(|| Error::NotFound(format!("key {key}")))?;
        self.with_widget(node, f)
    }

    /// Execute a closure with a keyed child of type `W` if it exists.
    pub fn try_with_keyed<W: Widget + 'static, R>(
        &mut self,
        key: &str,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<Option<R>> {
        let Some(node) = self.child_keyed(key) else {
            return Ok(None);
        };
        self.with_widget(node, f).map(Some)
    }

    /// Execute a closure with the focused descendant of type `W`, or the first if none focused.
    pub fn with_focused_or_first_descendant<W: Widget + 'static, R>(
        &mut self,
        f: impl FnMut(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        let node = (self as &dyn ReadContext)
            .focused_or_first_descendant::<W>()
            .ok_or_else(|| Error::NotFound(type_name::<W>().to_string()))?;
        self.with_typed(node, f)
    }

    /// Execute a closure with the first descendant of type `W`.
    pub fn with_first_descendant<W: Widget + 'static, R>(
        &mut self,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        let node = (self as &dyn ReadContext)
            .first_descendant::<W>()
            .ok_or_else(|| Error::NotFound(type_name::<W>().to_string()))?;
        self.with_typed(node, f)
    }

    /// Execute a closure with the first descendant of type `W` if it exists.
    pub fn try_with_first_descendant<W: Widget + 'static, R>(
        &mut self,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<Option<R>> {
        let Some(node) = (self as &dyn ReadContext).first_descendant::<W>() else {
            return Ok(None);
        };
        self.with_typed(node, f).map(Some)
    }

    /// Execute a closure with the unique descendant of type `W`.
    pub fn with_unique_descendant<W: Widget + 'static, R>(
        &mut self,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        let node = (self as &dyn ReadContext)
            .unique_descendant::<W>()?
            .ok_or_else(|| Error::NotFound(type_name::<W>().to_string()))?;
        self.with_typed(node, f)
    }

    /// Execute a closure with the unique descendant of type `W` if it exists.
    pub fn try_with_unique_descendant<W: Widget + 'static, R>(
        &mut self,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<Option<R>> {
        let node = (self as &dyn ReadContext).unique_descendant::<W>()?;
        let Some(node) = node else {
            return Ok(None);
        };
        self.with_typed(node, f).map(Some)
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
        if node.hidden || node.view.is_zero() {
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
    if node.hidden || node.view.is_zero() || !is_descendant(core, root, focused) {
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

impl<'a> ReadContext for CoreContext<'a> {
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

    fn node_type_id(&self, node: NodeId) -> Option<TypeId> {
        self.core.nodes.get(node).map(|n| n.widget_type)
    }

    fn invalidate_layout(&self) {
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

    fn node_path(&self, root: NodeId, node: NodeId) -> Path {
        self.core.node_path(root, node)
    }

    fn child_keyed(&self, key: &str) -> Option<NodeId> {
        self.core.child_keyed(self.node_id, key)
    }

    fn current_focus_gen(&self) -> u64 {
        self.core.focus_gen
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

    fn capture_mouse(&mut self) -> bool {
        if self.core.mouse_capture == Some(self.node_id) {
            false
        } else {
            self.core.mouse_capture = Some(self.node_id);
            true
        }
    }

    fn release_mouse(&mut self) -> bool {
        if self.core.mouse_capture == Some(self.node_id) {
            self.core.mouse_capture = None;
            true
        } else {
            false
        }
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

    fn create_detached_boxed(&mut self, widget: Box<dyn Widget>) -> NodeId {
        self.core.create_detached_boxed(widget)
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

    fn dispatch_command(&mut self, cmd: &CommandInvocation) -> StdResult<ArgValue, CommandError> {
        let frame = self
            .core
            .current_command_scope()
            .cloned()
            .unwrap_or_default();
        self.dispatch_command_scoped(frame, cmd)
    }

    fn dispatch_command_scoped(
        &mut self,
        frame: CommandScopeFrame,
        cmd: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError> {
        let guard = self.core.push_command_scope(frame);
        let result = commands::dispatch(self.core, self.node_id, cmd);
        self.core.pop_command_scope(guard);
        result
    }

    fn current_event(&self) -> Option<&Event> {
        self.core
            .current_command_scope()
            .and_then(|frame| frame.event.as_ref())
    }

    fn current_mouse_event(&self) -> Option<MouseEvent> {
        self.core
            .current_command_scope()
            .and_then(|frame| frame.mouse)
    }

    fn current_list_row(&self) -> Option<ListRowContext> {
        self.core
            .current_command_scope()
            .and_then(|frame| frame.list_row)
    }

    fn add_child_to_boxed(&mut self, parent: NodeId, widget: Box<dyn Widget>) -> Result<NodeId> {
        self.core.add_child_to_boxed(parent, widget)
    }

    fn add_child_to_keyed_boxed(
        &mut self,
        parent: NodeId,
        key: &str,
        widget: Box<dyn Widget>,
    ) -> Result<NodeId> {
        self.core.add_child_to_keyed_boxed(parent, key, widget)
    }

    fn attach(&mut self, parent: NodeId, child: NodeId) -> Result<()> {
        self.core.attach(parent, child)
    }

    fn attach_keyed(&mut self, parent: NodeId, key: &str, child: NodeId) -> Result<()> {
        self.core.attach_keyed(parent, key, child)
    }

    fn detach(&mut self, child: NodeId) -> Result<()> {
        self.core.detach(child)
    }

    fn remove_subtree(&mut self, node: NodeId) -> Result<()> {
        self.core.remove_subtree(node)
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

impl<'a> ReadContext for CoreViewContext<'a> {
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

    fn node_type_id(&self, node: NodeId) -> Option<TypeId> {
        self.core.nodes.get(node).map(|n| n.widget_type)
    }

    fn invalidate_layout(&self) {
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

    fn node_path(&self, root: NodeId, node: NodeId) -> Path {
        self.core.node_path(root, node)
    }

    fn child_keyed(&self, key: &str) -> Option<NodeId> {
        self.core.child_keyed(self.node_id, key)
    }

    fn current_focus_gen(&self) -> u64 {
        self.core.focus_gen
    }
}
