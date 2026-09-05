use std::{
    any::{Any, TypeId, type_name},
    iter,
    ops::Deref,
    result::Result as StdResult,
};

use super::{
    commands,
    help::BindingSnapshot,
    id::{NodeId, TypedId},
    inputmap::{ExclusiveFrameToken, FrameworkBindingGroup},
    style::Effect,
    view::View,
    world::{Core, WidgetOperation, layout_driver::clamp_scroll},
};
use crate::{
    ChangeOutcome, InteractionToken, ModalOptions, SemanticIdentity,
    commands::{
        ArgValue, CommandError, CommandInvocation, CommandScopeFrame, CommandStatus, CommandTarget,
        ListRowContext,
    },
    error::{Error, Result},
    event::{Event, mouse::MouseEvent},
    geom::{Direction, Point, Rect},
    layout::{Layout, LayoutOverride},
    path::{Path, PathFilter},
    style::StyleMap,
    widget::Widget,
};

/// A typed key for keyed children.
///
/// This trait associates a string key with a specific widget type, providing
/// compile-time type safety for keyed child access.
///
/// Use the [`crate::key!`] macro to define keys:
///
/// ```
/// use canopy::{ChildKey, Widget, key};
///
/// pub struct Modal;
/// impl Widget for Modal {}
///
/// key!(ModalSlot: Modal);
/// assert_eq!(ModalSlot::KEY, "ModalSlot");
/// ```
pub trait ChildKey {
    /// The widget type associated with this key.
    type Widget: Widget + 'static;
    /// The string key used for storage.
    const KEY: &'static str;
}

/// Define a typed key for keyed children.
///
/// # Examples
///
/// ```
/// use canopy::{ChildKey, Widget, key};
///
/// key!(Editor);
/// impl Widget for Editor {}
///
/// pub struct Modal;
/// impl Widget for Modal {}
/// key!(pub ModalSlot: Modal);
///
/// assert_eq!(Editor::KEY, "Editor");
/// assert_eq!(ModalSlot::KEY, "ModalSlot");
/// ```
#[macro_export]
macro_rules! key {
    ($vis:vis $name:ident) => {
        /// Typed key for a keyed child slot.
        #[derive(Debug, Clone, Copy)]
        $vis struct $name;

        impl $crate::ChildKey for $name {
            type Widget = $name;
            const KEY: &'static str = ::std::stringify!($name);
        }
    };
    ($vis:vis $name:ident : $widget:ty) => {
        /// Typed key for a keyed child slot.
        #[derive(Debug, Clone, Copy)]
        $vis struct $name;

        impl $crate::ChildKey for $name {
            type Widget = $widget;
            const KEY: &'static str = ::std::stringify!($name);
        }
    };
}

/// Read-only context available to widgets during render and measure.
pub trait ViewContext {
    /// The node currently being rendered.
    fn node_id(&self) -> NodeId;

    /// The root node of the tree.
    fn root_id(&self) -> NodeId;

    /// View information for the current node.
    fn view(&self) -> View {
        self.node_view(self.node_id()).unwrap_or_default()
    }

    /// Cached layout configuration for the current node.
    fn layout(&self) -> Layout {
        self.node_layout(self.node_id()).unwrap_or_default()
    }

    /// View information for a specific node.
    fn node_view(&self, node: NodeId) -> Option<View>;

    /// Layout configuration for a specific node.
    fn node_layout(&self, node: NodeId) -> Option<Layout>;

    /// Read a widget without extracting its slot or marking it changed.
    fn read_widget(
        &self,
        node: NodeId,
        callback: &mut dyn FnMut(&dyn Widget) -> Result<()>,
    ) -> Result<()>;

    /// Inspect the current eligibility of an explicitly targeted command.
    fn command_status(
        &self,
        target: CommandTarget,
        invocation: &CommandInvocation,
    ) -> Result<CommandStatus>;

    /// Inspect an action for display. Built-in contexts report registry and
    /// target resolution failures as disabled reasons, while eligibility
    /// hook errors remain errors. Custom contexts default to their command
    /// status behavior.
    fn action_status(
        &self,
        target: CommandTarget,
        invocation: &CommandInvocation,
    ) -> Result<CommandStatus> {
        self.command_status(target, invocation)
    }

    /// Widget type identifier for a specific node.
    fn node_type_id(&self, node: NodeId) -> Option<TypeId>;

    /// Resolve a semantic key in an explicit live subtree scope.
    fn find_key(&self, scope: NodeId, key: &str) -> Result<Option<NodeId>>;

    /// Return a node's independently assigned semantic identity.
    fn semantic_identity(&self, node: NodeId) -> Option<SemanticIdentity>;

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
    fn is_focused(&self) -> bool {
        self.node_is_focused(self.node_id())
    }

    /// Does the specified node have focus?
    fn node_is_focused(&self, node: NodeId) -> bool;

    /// Return the currently focused node, including one not yet laid out.
    fn focused_node(&self) -> Option<NodeId>;

    /// Is the current node on the focus path?
    fn is_on_focus_path(&self) -> bool {
        self.node_is_on_focus_path(self.node_id())
    }

    /// Is the specified node on the focus path?
    fn node_is_on_focus_path(&self, node: NodeId) -> bool;

    /// Return the focused leaf under the subtree rooted at `root`.
    fn focused_leaf(&self, root: NodeId) -> Option<NodeId>;

    /// Return focusable leaves in pre-order under the subtree rooted at `root`.
    fn focusable_leaves(&self, root: NodeId) -> Vec<NodeId>;

    /// Return the parent of a node, or `None` if it is the root or not found.
    fn parent_of(&self, node: NodeId) -> Option<NodeId>;

    /// Return whether a node exists and is attached to the root tree.
    fn node_is_attached(&self, node: NodeId) -> bool;

    /// Whether a modal scope remains open, including pending deferred closes.
    fn modal_is_open(&self, token: InteractionToken) -> bool {
        let _ = token;
        false
    }

    /// Return the path for a node relative to a root.
    fn node_path(&self, root: NodeId, node: NodeId) -> Path;

    /// Locate the deepest visible node at a point within a subtree.
    fn locate(&self, root: NodeId, point: Point) -> Result<Option<NodeId>>;

    /// Return a keyed child relative to the current node.
    fn child_keyed(&self, key: &str) -> Option<NodeId> {
        self.child_keyed_in(self.node_id(), key)
    }

    /// Return a keyed child relative to a specific parent node.
    fn child_keyed_in(&self, parent: NodeId, key: &str) -> Option<NodeId>;

    /// Find the first node whose path matches the validated filter.
    fn find_node_matching(&self, path_filter: &PathFilter) -> Option<NodeId> {
        matching_nodes(self, path_filter).next()
    }

    /// Find all nodes whose paths match the filter, relative to the current
    /// node.
    ///
    /// The filter is normalized to match full paths.
    fn find_nodes(&self, path_filter: &str) -> Vec<NodeId> {
        let Ok(filter) = PathFilter::normalized(path_filter) else {
            return Vec::new();
        };
        self.find_nodes_matching(&filter)
    }

    /// Find all nodes whose paths match the validated filter.
    fn find_nodes_matching(&self, path_filter: &PathFilter) -> Vec<NodeId> {
        matching_nodes(self, path_filter).collect()
    }
}

/// Walk a subtree in pre-order, root first, children in declaration order.
fn preorder_from<C: ViewContext + ?Sized>(
    ctx: &C,
    root: NodeId,
) -> impl Iterator<Item = NodeId> + '_ {
    let mut stack = vec![root];
    iter::from_fn(move || {
        let id = stack.pop()?;
        for child in ViewContext::children_of(ctx, id).into_iter().rev() {
            stack.push(child);
        }
        Some(id)
    })
}

/// Walk the current node's subtree, yielding nodes whose path matches the
/// filter.
fn matching_nodes<'a, C: ViewContext + ?Sized>(
    ctx: &'a C,
    path_filter: &'a PathFilter,
) -> impl Iterator<Item = NodeId> + 'a {
    let root = ctx.node_id();
    preorder_from(ctx, root)
        .filter(move |id| path_filter.check_match(&ctx.node_path(root, *id)).is_some())
}

/// Apply a scroll transform to a node, clamp it to the canvas, and report
/// whether it moved.
fn update_scroll(core: &mut Core, node_id: NodeId, f: impl FnOnce(Point) -> Point) -> bool {
    let Some(node) = core.nodes.get_mut(node_id) else {
        return false;
    };
    let before = node.scroll;
    node.scroll = f(before);
    clamp_scroll(&mut node.scroll, node.content_size, node.canvas);
    node.view.tl = node.scroll;
    let changed = before != node.scroll;
    if changed {
        core.invalidate(crate::Invalidation::Layout);
    }
    changed
}

/// Validate one raw node ID against a requested widget type.
fn checked_typed_id<W, C>(ctx: &C, node: NodeId) -> Result<TypedId<W>>
where
    W: Widget + 'static,
    C: ViewContext + ?Sized,
{
    let actual = ctx.node_type_id(node).ok_or(Error::NodeNotFound(node))?;
    if actual != TypeId::of::<W>() {
        return Err(Error::NodeTypeMismatch {
            node,
            expected: type_name::<W>(),
        });
    }
    Ok(TypedId::new(node))
}

impl dyn ViewContext + '_ {
    /// Read a typed widget while preserving immutable access and borrow errors.
    pub fn with_widget_read<W: Widget + 'static, R>(
        &self,
        node: TypedId<W>,
        callback: impl FnOnce(&W) -> Result<R>,
    ) -> Result<R> {
        let mut callback = Some(callback);
        let mut result = None;
        self.read_widget(node.into(), &mut |widget| {
            let any = widget as &dyn Any;
            let widget = any
                .downcast_ref::<W>()
                .ok_or_else(|| Error::Internal("widget type mismatch".into()))?;
            let callback = callback
                .take()
                .ok_or_else(|| Error::Internal("widget callback repeated".into()))?;
            result = Some(callback(widget)?);
            Ok(())
        })?;
        result.ok_or_else(|| Error::Internal("widget callback omitted".into()))
    }

    /// Validate an untyped node ID and return its typed form.
    pub fn typed_id<W: Widget + 'static>(&self, node: impl Into<NodeId>) -> Result<TypedId<W>> {
        checked_typed_id(self, node.into())
    }

    /// Pre-order traversal of the subtree rooted at `root`.
    pub fn preorder(&self, root: impl Into<NodeId>) -> impl Iterator<Item = NodeId> + '_ {
        preorder_from(self, root.into())
    }

    /// Return the first widget of type `W` anywhere in the tree, including the
    /// root.
    pub fn first_in_tree<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.preorder(self.root_id())
            .find(|id| ViewContext::node_type_id(self, *id) == Some(TypeId::of::<W>()))
            .map(TypedId::new)
    }

    /// Return all widgets of type `W` anywhere in the tree, including the root.
    pub fn all_in_tree<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        self.preorder(self.root_id())
            .filter(|id| ViewContext::node_type_id(self, *id) == Some(TypeId::of::<W>()))
            .map(TypedId::new)
            .collect()
    }

    /// Find exactly one node matching a path filter.
    pub fn find_one(&self, path: &str) -> Result<NodeId> {
        let filter = PathFilter::normalized(path)?;
        let matches = self.find_nodes_matching(&filter);
        match matches.len() {
            0 => Err(Error::NotFound(format!("path {}", filter.as_str()))),
            1 => Ok(matches[0]),
            _ => Err(Error::MultipleMatches),
        }
    }

    /// Return the unique child of type `W`, or error if more than one exists.
    pub fn unique_child<W: Widget + 'static>(&self) -> Result<Option<TypedId<W>>> {
        self.unique_typed(self.children().into_iter())
    }

    /// Return all direct children of type `W`.
    pub fn children_of_type<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        self.children()
            .into_iter()
            .filter(|id| self.node_matches_type::<W>(*id))
            .map(TypedId::new)
            .collect()
    }

    /// Return the unique descendant of type `W`, or error if more than one
    /// exists.
    pub fn unique_descendant<W: Widget + 'static>(&self) -> Result<Option<TypedId<W>>> {
        self.unique_typed(self.preorder(self.node_id()).skip(1))
    }

    /// Return all descendants of type `W` (excluding self).
    pub fn descendants_of_type<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        self.preorder(self.node_id())
            .skip(1)
            .filter(|id| self.node_matches_type::<W>(*id))
            .map(TypedId::new)
            .collect()
    }

    /// Return the descendant of type `W` that is on the focus path, if any.
    pub fn focused_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.descendants_of_type::<W>()
            .into_iter()
            .find(|id| ViewContext::node_is_on_focus_path(self, (*id).into()))
    }

    /// Return the descendant of type `W` on the focus path, or the first if
    /// none focused.
    ///
    /// This searches only within the current node's subtree. Use the tree-wide
    /// helpers on `ViewContext` if you need to search from an arbitrary
    /// root.
    pub fn focused_or_first_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        let descendants = self.descendants_of_type::<W>();
        let focused = descendants
            .iter()
            .copied()
            .find(|id| ViewContext::node_is_on_focus_path(self, (*id).into()));
        focused.or_else(|| descendants.into_iter().next())
    }

    /// Return true if the node's widget type matches `W`.
    fn node_matches_type<W: Widget + 'static>(&self, node: NodeId) -> bool {
        ViewContext::node_type_id(self, node) == Some(TypeId::of::<W>())
    }

    /// Return the single node of type `W` among `ids`, or error when more than
    /// one matches.
    fn unique_typed<W: Widget + 'static>(
        &self,
        ids: impl Iterator<Item = NodeId>,
    ) -> Result<Option<TypedId<W>>> {
        let mut found = None;
        for id in ids.filter(|id| self.node_matches_type::<W>(*id)) {
            if found.is_some() {
                return Err(Error::MultipleMatches);
            }
            found = Some(TypedId::new(id));
        }
        Ok(found)
    }

    /// Return the first leaf node under `root` using pre-order traversal.
    ///
    /// A leaf is a node with no children.
    pub fn first_leaf(&self, root: impl Into<NodeId>) -> Option<NodeId> {
        let root = root.into();
        self.preorder(root)
            .find(|id| ViewContext::children_of(self, *id).is_empty())
    }
}

/// Subtree used by a focus traversal operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusScope {
    /// The current widget's subtree.
    Current,
    /// The complete widget tree.
    Root,
    /// A subtree rooted at an explicit node.
    Node(NodeId),
}

impl FocusScope {
    /// Resolve this scope against a node-bound context.
    fn resolve(self, context: &dyn ViewContext) -> NodeId {
        match self {
            Self::Current => context.node_id(),
            Self::Root => context.root_id(),
            Self::Node(node) => node,
        }
    }
}

/// Mutable context available to widgets during event handling.
pub trait Context: ViewContext {
    /// Commit configured composition topology and semantic keys before mount.
    #[doc(hidden)]
    fn attach_composed(
        &mut self,
        parent: NodeId,
        roots: &[(NodeId, Option<&str>)],
        keys: &[(NodeId, NodeId, String)],
    ) -> Result<()>;

    /// Assign a unique semantic key within a containing subtree scope.
    fn set_semantic_key(&mut self, node: NodeId, scope: NodeId, key: &str) -> Result<()>;

    /// Remove the semantic identity of a live node.
    fn clear_semantic_key(&mut self, node: NodeId) -> Result<()>;

    /// Focus an attached node.
    fn set_focus(&mut self, node: NodeId) -> Result<ChangeOutcome>;

    /// Move focus in a direction within an explicit scope.
    fn focus_dir(&mut self, scope: FocusScope, dir: Direction) -> Result<ChangeOutcome>;

    /// Focus the first focusable node within an explicit scope.
    fn focus_first(&mut self, scope: FocusScope) -> Result<ChangeOutcome>;

    /// Focus the next focusable node within an explicit scope.
    fn focus_next(&mut self, scope: FocusScope) -> Result<ChangeOutcome>;

    /// Focus the previous focusable node within an explicit scope.
    fn focus_prev(&mut self, scope: FocusScope) -> Result<ChangeOutcome>;

    /// Capture mouse events for the current node.
    fn capture_mouse(&mut self) -> Result<ChangeOutcome>;

    /// Release mouse capture if held by the current node.
    fn release_mouse(&mut self) -> Result<ChangeOutcome>;

    /// Clear and return the current mouse-capture target.
    fn take_mouse_capture(&mut self) -> Result<Option<NodeId>>;

    /// Restore mouse capture to an attached node.
    fn restore_mouse_capture(&mut self, node: NodeId) -> Result<ChangeOutcome>;

    /// Return effective key bindings for a node or the current focus.
    fn available_bindings(&self, node: Option<NodeId>) -> Result<BindingSnapshot>;

    /// Open a modal scope that owns focus, input admission, and visual effects.
    fn open_modal(&mut self, options: ModalOptions) -> Result<InteractionToken> {
        let _ = options;
        Err(Error::InvalidOperation(
            "modal interaction is unavailable".into(),
        ))
    }

    /// Close this scope and its nested scopes after active callbacks return.
    fn close_modal(&mut self, token: InteractionToken) -> Result<()> {
        let _ = token;
        Err(Error::InvalidOperation(
            "modal interaction is unavailable".into(),
        ))
    }

    /// Push an exclusive framework binding frame owned by the current node.
    fn push_exclusive_bindings(
        &mut self,
        group: FrameworkBindingGroup,
    ) -> Result<ExclusiveFrameToken>;

    /// Remove one exclusive binding frame.
    fn pop_exclusive_bindings(&mut self, token: ExclusiveFrameToken) -> Result<()>;

    /// Scroll the view to the specified position. Returns `true` if movement
    /// occurred.
    fn scroll_to(&mut self, x: u32, y: u32) -> bool;

    /// Scroll the view by the given offsets. Returns `true` if movement
    /// occurred.
    fn scroll_by(&mut self, x: i32, y: i32) -> bool;

    /// Scroll the view up by one page. Returns `true` if movement occurred.
    fn page_up(&mut self) -> bool {
        let view = self.view();
        self.scroll_to(view.tl.x, view.tl.y.saturating_sub(view.content.h))
    }

    /// Scroll the view down by one page. Returns `true` if movement occurred.
    fn page_down(&mut self) -> bool {
        let view = self.view();
        self.scroll_to(view.tl.x, view.tl.y.saturating_add(view.content.h))
    }

    /// Scroll the view up by one line. Returns `true` if movement occurred.
    fn scroll_up(&mut self) -> bool {
        self.scroll_by(0, -1)
    }

    /// Scroll the view down by one line. Returns `true` if movement occurred.
    fn scroll_down(&mut self) -> bool {
        self.scroll_by(0, 1)
    }

    /// Scroll the view left by one line. Returns `true` if movement occurred.
    fn scroll_left(&mut self) -> bool {
        self.scroll_by(-1, 0)
    }

    /// Scroll the view right by one line. Returns `true` if movement occurred.
    fn scroll_right(&mut self) -> bool {
        self.scroll_by(1, 0)
    }

    /// Mark this node dirty so the next frame re-runs layout.
    fn invalidate_layout(&mut self);

    /// Record changes for the next runtime preparation.
    fn invalidate(&mut self, _invalidation: crate::Invalidation) {
        self.invalidate_layout();
    }

    /// Update the layout for the current node.
    fn with_layout(&mut self, f: &mut dyn FnMut(&mut Layout)) -> Result<()> {
        let node = self.node_id();
        self.with_layout_of(node, f)
    }

    /// Update the layout for a specific node.
    fn with_layout_of(&mut self, node: NodeId, f: &mut dyn FnMut(&mut Layout)) -> Result<()>;

    /// Replace persistent parent constraints without replacing widget layout
    /// fields.
    fn set_layout_override_of(&mut self, node: NodeId, overrides: LayoutOverride) -> Result<()>;

    /// Clear parent constraints and restore the widget's base layout.
    fn clear_layout_override_of(&mut self, node: NodeId) -> Result<()>;

    /// Create a new widget node detached from the tree.
    fn create_detached_boxed(&mut self, widget: Box<dyn Widget>) -> Result<NodeId>;

    /// Run immediate mutations with structural rollback on error.
    ///
    /// Rollback restores arena metadata, topology, child keys, layouts, views,
    /// lifecycle flags, root, focus, mouse capture, focus recovery hints, exit
    /// requests, pending styles, command registry and scope, and diagnostic
    /// requests. Each failed nested edit restores its own structural
    /// checkpoint.
    ///
    /// Widget slots are shared with the checkpoint: widget-owned mutations
    /// survive. Binding registration and external effects also survive and
    /// require explicit compensation. Cleanup hooks must be safe to repeat.
    /// This is not a widget state or database transaction.
    fn edit_structure(
        &mut self,
        edit: &mut dyn FnMut(&mut dyn Context) -> Result<()>,
    ) -> Result<()>;

    /// Execute a closure with mutable access to a widget and its node-bound
    /// context.
    fn with_widget_mut(
        &mut self,
        node: NodeId,
        f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
    ) -> Result<()>;

    /// Dispatch according to an explicit target policy.
    fn dispatch_target(
        &mut self,
        target: CommandTarget,
        cmd: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError>;

    /// Invoke only the specified command owner.
    fn dispatch_exact(
        &mut self,
        node: NodeId,
        cmd: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError> {
        self.dispatch_target(CommandTarget::Exact(node), cmd)
    }

    /// Search the supplied origin subtree, then its ancestors.
    fn dispatch_from(
        &mut self,
        node: NodeId,
        cmd: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError> {
        self.dispatch_target(CommandTarget::From(node), cmd)
    }

    /// Invoke with explicit target and input scope.
    fn dispatch_target_scoped(
        &mut self,
        target: CommandTarget,
        frame: CommandScopeFrame,
        cmd: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError>;

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

    /// Add a boxed widget as a child of a specific parent and return the new
    /// node ID.
    fn add_child_to_boxed(&mut self, parent: NodeId, widget: Box<dyn Widget>) -> Result<NodeId>;

    /// Add a boxed widget as a keyed child of a specific parent and return the
    /// new node ID.
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

    /// Remove the current widget incarnation after the outer dispatch succeeds.
    /// Failed dispatches discard their requests. Missing or replaced targets
    /// are harmless. Lifecycle hooks run after active widget slots are
    /// restored. The shared batch accepts at most 1024 requests. Overflow
    /// returns an error without running removals or discarding previously
    /// accepted requests.
    fn remove_after_dispatch(&mut self, node: NodeId) -> Result<()>;

    /// Capture a thread-safe wake handle for this widget's work lifetime.
    /// Attachment handles require this node to be attached when acquired.
    fn wake_handle(&self, lifetime: crate::WorkLifetime) -> Result<crate::NodeWakeHandle>;

    /// Replace the children list for the current node.
    fn set_children(&mut self, children: Vec<NodeId>) -> Result<()> {
        self.set_children_of(self.node_id(), children)
    }

    /// Replace the children list for a specific parent node.
    fn set_children_of(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()>;

    /// Set the current node's visibility.
    fn set_hidden(&mut self, hidden: bool) -> Result<ChangeOutcome> {
        self.set_hidden_of(self.node_id(), hidden)
    }

    /// Set a specific node's visibility.
    fn set_hidden_of(&mut self, node: NodeId, hidden: bool) -> Result<ChangeOutcome>;

    /// Request a cooperative shutdown with the provided status code.
    fn exit(&mut self, code: i32);

    /// Add an effect to a node that will be applied during rendering.
    /// Effects stack and inherit through the tree.
    fn push_effect(&mut self, node: NodeId, effect: Effect) -> Result<()>;

    /// Clear all effects on a node.
    fn clear_effects(&mut self, node: NodeId) -> Result<()>;

    /// Set the style map to be used for rendering.
    /// The style change will be applied before the next render.
    fn set_style(&mut self, style: StyleMap);

    /// Request a diagnostic dump for a target node.
    fn request_diagnostic_dump(&mut self, target: NodeId);
}

impl dyn Context + '_ {
    /// Build detached children and attach them after configuration succeeds.
    /// Structural rollback follows [`Context::edit_structure`].
    pub fn compose<R>(
        &mut self,
        parent: NodeId,
        build: impl FnOnce(&mut crate::ChildBuilder<'_>) -> Result<R>,
    ) -> Result<R> {
        super::children::compose(self, parent, build)
    }

    /// Set the layout for the current node.
    pub fn set_layout(&mut self, layout: Layout) -> Result<()> {
        self.set_layout_of(self.node_id(), layout)
    }

    /// Set the layout for a specific node.
    pub fn set_layout_of(&mut self, node: impl Into<NodeId>, layout: Layout) -> Result<()> {
        Context::set_layout_override_of(self, node.into(), LayoutOverride::full(layout))
    }

    /// Execute a closure with mutable access through a typed widget ID.
    pub fn with_widget<W, R>(
        &mut self,
        node: TypedId<W>,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R>
    where
        W: Widget + 'static,
    {
        self.with_node(node, f)
    }

    /// Execute a closure with mutable access to a runtime-checked widget node.
    pub fn with_node<W, R>(
        &mut self,
        node: impl Into<NodeId>,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R>
    where
        W: Widget + 'static,
    {
        let node = node.into();
        checked_typed_id::<W, _>(&*self, node)?;
        let mut output = None;
        let mut f = Some(f);
        self.with_widget_mut(node, &mut |widget, ctx| {
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

    /// Create a widget node detached from the tree.
    pub fn create_detached<W: Widget + 'static>(&mut self, widget: W) -> Result<TypedId<W>> {
        let id = self.create_detached_boxed(widget.into())?;
        Ok(TypedId::new(id))
    }

    /// Add a widget as a child of the current node and return the new typed
    /// node ID.
    pub fn add_child<W: Widget + 'static>(&mut self, widget: W) -> Result<TypedId<W>> {
        self.add_child_to(self.node_id(), widget)
    }

    /// Add a widget as a child of a specific parent and return the new typed
    /// node ID.
    pub fn add_child_to<W: Widget + 'static>(
        &mut self,
        parent: impl Into<NodeId>,
        widget: W,
    ) -> Result<TypedId<W>> {
        let id = self.add_child_to_boxed(parent.into(), widget.into())?;
        Ok(TypedId::new(id))
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
        self.with_node(node, f)
    }

    /// Check if a typed keyed child exists.
    pub fn has_child<K: ChildKey>(&self) -> Result<bool> {
        self.get_child::<K>().map(|child| child.is_some())
    }

    /// Get a typed keyed child's node ID.
    pub fn get_child<K: ChildKey>(&self) -> Result<Option<TypedId<K::Widget>>> {
        self.child_keyed(K::KEY)
            .map(|node| checked_typed_id(self, node))
            .transpose()
    }

    /// Get a typed keyed child's node ID from a specific parent.
    pub fn get_child_in<K: ChildKey>(
        &self,
        parent: impl Into<NodeId>,
    ) -> Result<Option<TypedId<K::Widget>>> {
        ViewContext::child_keyed_in(self, parent.into(), K::KEY)
            .map(|node| checked_typed_id(self, node))
            .transpose()
    }

    /// Get or create the keyed child under the current node.
    pub fn get_or_create<K: ChildKey>(
        &mut self,
        make: impl FnOnce() -> K::Widget,
    ) -> Result<TypedId<K::Widget>> {
        let parent = self.node_id();
        self.get_or_create_in::<K>(parent, make)
    }

    /// Get or create the keyed child under a specific parent node.
    pub fn get_or_create_in<K: ChildKey>(
        &mut self,
        parent: impl Into<NodeId>,
        make: impl FnOnce() -> K::Widget,
    ) -> Result<TypedId<K::Widget>> {
        let parent = parent.into();
        if let Some(id) = self.get_child_in::<K>(parent)? {
            return Ok(id);
        }
        self.add_keyed_to(parent, K::KEY, make())
    }

    /// Add a typed keyed child to the current node and return its typed node
    /// ID.
    pub fn add_keyed<K: ChildKey>(&mut self, widget: K::Widget) -> Result<TypedId<K::Widget>> {
        self.add_keyed_to(self.node_id(), K::KEY, widget)
    }

    /// Add a typed keyed child to a specific parent and return its typed node
    /// ID.
    pub fn add_keyed_to<W: Widget + 'static>(
        &mut self,
        parent: impl Into<NodeId>,
        key: &str,
        widget: W,
    ) -> Result<TypedId<W>> {
        let id = self.add_child_to_keyed_boxed(parent.into(), key, widget.into())?;
        Ok(TypedId::new(id))
    }

    /// Execute a closure with a typed keyed child.
    pub fn with_child<K: ChildKey, R>(
        &mut self,
        f: impl FnOnce(&mut K::Widget, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        self.with_keyed(K::KEY, f)
    }

    /// Execute a closure with a typed keyed child if it exists.
    pub fn try_with_child<K: ChildKey, R>(
        &mut self,
        f: impl FnOnce(&mut K::Widget, &mut dyn Context) -> Result<R>,
    ) -> Result<Option<R>> {
        let Some(node) = self.child_keyed(K::KEY) else {
            return Ok(None);
        };
        self.with_node(node, f).map(Some)
    }

    /// Execute a closure with the unique descendant of type `W`.
    pub fn with_unique_descendant<W: Widget + 'static, R>(
        &mut self,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        let node = (self as &dyn ViewContext)
            .unique_descendant::<W>()?
            .ok_or_else(|| Error::NotFound(type_name::<W>().to_string()))?;
        self.with_widget(node, f)
    }

    /// Execute a closure with the unique descendant of type `W` if it exists.
    pub fn try_with_unique_descendant<W: Widget + 'static, R>(
        &mut self,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<Option<R>> {
        let node = (self as &dyn ViewContext).unique_descendant::<W>()?;
        let Some(node) = node else {
            return Ok(None);
        };
        self.with_widget(node, f).map(Some)
    }
}

/// Context bound to a specific node, over a shared or exclusive borrow of the
/// core.
pub struct NodeCtx<C> {
    /// Core state reference.
    core: C,
    /// Node bound to this context.
    node_id: NodeId,
}

/// Mutating context bound to a specific node.
pub type CoreContext<'a> = NodeCtx<&'a mut Core>;

/// Read-only context bound to a specific node.
pub type CoreViewContext<'a> = NodeCtx<&'a Core>;

impl<C> NodeCtx<C> {
    /// Create a new context for a node.
    pub fn new(core: C, node_id: NodeId) -> Self {
        Self { core, node_id }
    }
}

impl<C: Deref<Target = Core>> ViewContext for NodeCtx<C> {
    fn node_id(&self) -> NodeId {
        self.node_id
    }

    fn root_id(&self) -> NodeId {
        self.core.root
    }

    fn node_view(&self, node: NodeId) -> Option<View> {
        self.core.nodes.get(node).map(|n| n.view)
    }

    fn node_layout(&self, node: NodeId) -> Option<Layout> {
        self.core.nodes.get(node).map(|n| n.layout)
    }

    fn read_widget(
        &self,
        node: NodeId,
        callback: &mut dyn FnMut(&dyn Widget) -> Result<()>,
    ) -> Result<()> {
        self.core
            .with_widget_read(node, WidgetOperation::access("read widget"), |widget, _| {
                callback(widget)
            })?
    }

    fn command_status(
        &self,
        target: CommandTarget,
        invocation: &CommandInvocation,
    ) -> Result<CommandStatus> {
        commands::command_status(&self.core, target, invocation)
    }

    fn action_status(
        &self,
        target: CommandTarget,
        invocation: &CommandInvocation,
    ) -> Result<CommandStatus> {
        commands::action_status(&self.core, target, invocation)
    }

    fn find_key(&self, scope: NodeId, key: &str) -> Result<Option<NodeId>> {
        self.core.find_key(scope, key)
    }

    fn semantic_identity(&self, node: NodeId) -> Option<SemanticIdentity> {
        self.core
            .nodes
            .get(node)
            .and_then(|entry| entry.semantic_identity.clone())
    }

    fn node_type_id(&self, node: NodeId) -> Option<TypeId> {
        self.core.nodes.get(node).map(|n| n.widget_type)
    }

    fn children_of(&self, node: NodeId) -> Vec<NodeId> {
        self.core
            .nodes
            .get(node)
            .map(|n| n.children.clone())
            .unwrap_or_default()
    }

    fn node_is_focused(&self, node: NodeId) -> bool {
        self.core.is_focused(node)
    }

    fn focused_node(&self) -> Option<NodeId> {
        self.core.focus
    }

    fn node_is_on_focus_path(&self, node: NodeId) -> bool {
        self.core.is_on_focus_path(node)
    }

    fn focused_leaf(&self, root: NodeId) -> Option<NodeId> {
        self.core.focused_leaf(root)
    }

    fn focusable_leaves(&self, root: NodeId) -> Vec<NodeId> {
        self.core.focusable_leaves(root)
    }

    fn parent_of(&self, node: NodeId) -> Option<NodeId> {
        self.core.nodes.get(node).and_then(|n| n.parent)
    }

    fn modal_is_open(&self, token: InteractionToken) -> bool {
        self.core.modal_is_open(token)
    }

    fn node_is_attached(&self, node: NodeId) -> bool {
        self.core.is_attached_to_root(node)
    }

    fn node_path(&self, root: NodeId, node: NodeId) -> Path {
        self.core.node_path(root, node)
    }

    fn locate(&self, root: NodeId, point: Point) -> Result<Option<NodeId>> {
        self.core.locate_node(root, point)
    }

    fn child_keyed_in(&self, parent: NodeId, key: &str) -> Option<NodeId> {
        self.core.child_keyed(parent, key)
    }
}

impl Context for NodeCtx<&mut Core> {
    fn attach_composed(
        &mut self,
        parent: NodeId,
        roots: &[(NodeId, Option<&str>)],
        keys: &[(NodeId, NodeId, String)],
    ) -> Result<()> {
        self.core.attach_composed(parent, roots, keys)
    }

    fn set_semantic_key(&mut self, node: NodeId, scope: NodeId, key: &str) -> Result<()> {
        self.core.set_semantic_key(node, scope, key)
    }

    fn clear_semantic_key(&mut self, node: NodeId) -> Result<()> {
        self.core.clear_semantic_key(node)
    }

    fn set_focus(&mut self, node: NodeId) -> Result<ChangeOutcome> {
        self.core.set_focus(node)
    }

    fn focus_dir(&mut self, scope: FocusScope, dir: Direction) -> Result<ChangeOutcome> {
        self.core.focus_dir(scope.resolve(self), dir)
    }

    fn focus_first(&mut self, scope: FocusScope) -> Result<ChangeOutcome> {
        self.core.focus_first(scope.resolve(self))
    }

    fn focus_next(&mut self, scope: FocusScope) -> Result<ChangeOutcome> {
        self.core.focus_next(scope.resolve(self))
    }

    fn focus_prev(&mut self, scope: FocusScope) -> Result<ChangeOutcome> {
        self.core.focus_prev(scope.resolve(self))
    }

    fn capture_mouse(&mut self) -> Result<ChangeOutcome> {
        self.core.capture_mouse(self.node_id)
    }

    fn release_mouse(&mut self) -> Result<ChangeOutcome> {
        self.core.release_mouse(self.node_id)
    }

    fn take_mouse_capture(&mut self) -> Result<Option<NodeId>> {
        self.core.take_mouse_capture()
    }

    fn restore_mouse_capture(&mut self, node: NodeId) -> Result<ChangeOutcome> {
        self.core.capture_mouse(node)
    }

    fn available_bindings(&self, node: Option<NodeId>) -> Result<BindingSnapshot> {
        self.core.available_bindings(node)
    }

    fn open_modal(&mut self, options: ModalOptions) -> Result<InteractionToken> {
        self.core.open_modal(options)
    }

    fn close_modal(&mut self, token: InteractionToken) -> Result<()> {
        self.core.close_modal(token)
    }

    fn push_exclusive_bindings(
        &mut self,
        group: FrameworkBindingGroup,
    ) -> Result<ExclusiveFrameToken> {
        self.core
            .input_map
            .push_exclusive_bindings(group, self.node_id)
    }

    fn pop_exclusive_bindings(&mut self, token: ExclusiveFrameToken) -> Result<()> {
        self.core.input_map.pop_exclusive_bindings(token)
    }

    fn scroll_to(&mut self, x: u32, y: u32) -> bool {
        update_scroll(self.core, self.node_id, |_| Point { x, y })
    }

    fn scroll_by(&mut self, x: i32, y: i32) -> bool {
        update_scroll(self.core, self.node_id, |scroll| scroll.scroll(x, y))
    }

    fn invalidate(&mut self, invalidation: crate::Invalidation) {
        self.core.invalidate(invalidation);
        if invalidation == crate::Invalidation::Layout {
            self.invalidate_layout();
        }
    }

    fn invalidate_layout(&mut self) {
        self.core.invalidate(crate::Invalidation::Layout);
        if let Some(node) = self.core.nodes.get_mut(self.node_id) {
            node.layout_dirty = true;
        }
    }

    fn set_layout_override_of(&mut self, node: NodeId, overrides: LayoutOverride) -> Result<()> {
        self.core.set_layout_override_of(node, overrides)
    }

    fn clear_layout_override_of(&mut self, node: NodeId) -> Result<()> {
        self.core.clear_layout_override_of(node)
    }

    fn with_layout_of(&mut self, node: NodeId, f: &mut dyn FnMut(&mut Layout)) -> Result<()> {
        self.core.with_layout_of(node, |layout| f(layout))
    }

    fn create_detached_boxed(&mut self, widget: Box<dyn Widget>) -> Result<NodeId> {
        self.core.create_detached_boxed(widget)
    }

    fn edit_structure(
        &mut self,
        edit: &mut dyn FnMut(&mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        let node_id = self.node_id;
        self.core.with_tree_edit("context tree edit", |core| {
            let mut ctx = CoreContext::new(core, node_id);
            edit(&mut ctx)
        })
    }

    fn with_widget_mut(
        &mut self,
        node: NodeId,
        f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        self.core
            .with_widget_ctx(node, |widget, ctx| f(widget, ctx))?
    }

    fn dispatch_target(
        &mut self,
        target: CommandTarget,
        cmd: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError> {
        commands::dispatch_target(self.core, target, cmd)
    }

    fn dispatch_target_scoped(
        &mut self,
        target: CommandTarget,
        frame: CommandScopeFrame,
        cmd: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError> {
        let guard = self.core.push_command_scope(frame);
        let result = commands::dispatch_target(self.core, target, cmd);
        self.core.pop_command_scope(guard);
        result
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
            .and_then(|frame| frame.list_row.clone())
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

    fn wake_handle(&self, lifetime: crate::WorkLifetime) -> Result<crate::NodeWakeHandle> {
        self.core.wake_handle(self.node_id, lifetime)
    }

    fn remove_after_dispatch(&mut self, node: NodeId) -> Result<()> {
        self.core.remove_after_dispatch(node)
    }

    fn remove_subtree(&mut self, node: NodeId) -> Result<()> {
        self.core.remove_subtree(node)
    }

    fn set_children_of(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()> {
        self.core.set_children(parent, children)
    }

    fn set_hidden_of(&mut self, node: NodeId, hidden: bool) -> Result<ChangeOutcome> {
        self.core.set_hidden(node, hidden)
    }

    fn exit(&mut self, code: i32) {
        self.core.request_exit(code);
    }

    fn push_effect(&mut self, node: NodeId, effect: Effect) -> Result<()> {
        let node = self
            .core
            .nodes
            .get_mut(node)
            .ok_or(Error::NodeNotFound(node))?;
        node.effects.push(effect);
        self.core.invalidate(crate::Invalidation::Paint);
        Ok(())
    }

    fn clear_effects(&mut self, node: NodeId) -> Result<()> {
        let node = self
            .core
            .nodes
            .get_mut(node)
            .ok_or(Error::NodeNotFound(node))?;
        if !node.effects.is_empty() {
            node.effects.clear();
            self.core.invalidate(crate::Invalidation::Paint);
        }
        Ok(())
    }

    fn set_style(&mut self, style: StyleMap) {
        self.core.pending_style = Some(style);
        self.core.invalidate(crate::Invalidation::Paint);
    }

    fn request_diagnostic_dump(&mut self, target: NodeId) {
        self.core.request_diagnostic_dump(target);
    }
}

#[cfg(test)]
mod tests {
    use crate::{ChildKey, Widget};

    key!(Editor);
    impl Widget for Editor {}

    pub struct Modal;
    impl Widget for Modal {}
    key!(pub ModalSlot: Modal);

    #[test]
    fn key_macro_names_the_slot_after_the_key_type() {
        assert_eq!(Editor::KEY, "Editor");
        assert_eq!(ModalSlot::KEY, "ModalSlot");
    }
    #[test]
    fn page_scrolling_preserves_unsigned_offsets_and_clamps() {
        use super::{Context, CoreContext, ViewContext};
        use crate::{
            core::Core,
            geom::{Point, Size},
        };

        let mut core = Core::new();
        let root = core.root_id();
        for height in [0, 3, i32::MAX as u32 + 1, u32::MAX] {
            let max_y = if height == 0 { 0 } else { u32::MAX - height };
            let node = &mut core.nodes[root];
            node.content_size = Size::new(1, height);
            node.canvas = Size::new(10, u32::MAX);
            node.view.content.h = height;
            node.scroll = Point { x: 2, y: 0 };
            node.view.tl = node.scroll;
            let mut context = CoreContext::new(&mut core, root);
            for step in 1u32..=2 {
                let before = context.view().tl.y;
                let expected = height.saturating_mul(step).min(max_y);
                assert_eq!(context.page_down(), before != expected);
                assert_eq!(context.view().tl, Point { x: 2, y: expected });
            }
            context.scroll_to(2, max_y);
            let expected = max_y.saturating_sub(height);
            assert_eq!(context.page_up(), max_y != expected);
            assert_eq!(context.view().tl, Point { x: 2, y: expected });
            context.scroll_to(2, 0);
            assert!(!context.page_up());
        }
    }
}
