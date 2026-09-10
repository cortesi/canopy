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
    style::effects::Effect,
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
    geom::Point,
    layout::{Layout, LayoutOverride},
    path::{Path, PathFilter},
    style::StyleMap,
    widget::Widget,
};

/// A typed slot for keyed children.
///
/// This trait associates a string key with a specific widget type, providing
/// compile-time type safety for keyed child access.
///
/// Use the [`crate::slot!`] macro to define keys:
///
/// ```
/// use canopy::{ChildSlot, Widget, slot};
///
/// pub struct Modal;
/// impl Widget for Modal {}
///
/// slot!(ModalSlot: Modal);
/// assert_eq!(ModalSlot::KEY, "ModalSlot");
/// ```
pub trait ChildSlot {
    /// The widget type associated with this key.
    type Widget: Widget + 'static;
    /// The string key used for storage.
    const KEY: &'static str;
}

/// Define a typed slot for keyed children.
///
/// # Examples
///
/// ```
/// use canopy::{ChildSlot, Widget, slot};
///
/// slot!(Editor);
/// impl Widget for Editor {}
///
/// pub struct Modal;
/// impl Widget for Modal {}
/// slot!(pub ModalSlot: Modal);
///
/// assert_eq!(Editor::KEY, "Editor");
/// assert_eq!(ModalSlot::KEY, "ModalSlot");
/// ```
#[macro_export]
macro_rules! slot {
    ($vis:vis $name:ident) => {
        /// Typed key for a keyed child slot.
        #[derive(Debug, Clone, Copy)]
        $vis struct $name;

        impl $crate::ChildSlot for $name {
            type Widget = $name;
            const KEY: &'static str = ::std::stringify!($name);
        }
    };
    ($vis:vis $name:ident : $widget:ty) => {
        /// Typed key for a keyed child slot.
        #[derive(Debug, Clone, Copy)]
        $vis struct $name;

        impl $crate::ChildSlot for $name {
            type Widget = $widget;
            const KEY: &'static str = ::std::stringify!($name);
        }
    };
}

/// Implementation hooks reserved for Canopy's built-in contexts.
pub mod sealed {
    use super::{NodeId, Result};

    /// Marker implemented only by Canopy's built-in read-only contexts.
    pub trait ViewContext {}

    /// Mutation hooks implemented only by Canopy's built-in mutable contexts.
    pub trait Context {
        /// Borrow this implementation through the public context interface.
        fn as_context(&mut self) -> &mut dyn super::Context;

        /// Attach the topology produced by a completed composition pass.
        fn attach_composed(
            &mut self,
            parent: NodeId,
            roots: &[(NodeId, Option<&str>)],
            slots: &[(NodeId, NodeId, String)],
        ) -> Result<()>;
    }
}

/// Read-only context available to widgets during render and measure.
///
/// Applications consume contexts supplied by Canopy and cannot implement this
/// trait themselves.
pub trait ViewContext: sealed::ViewContext {
    /// The node currently being rendered.
    fn node_id(&self) -> NodeId;

    /// The root node of the tree.
    fn root_id(&self) -> NodeId;

    /// View information for the current node.
    fn view(&self) -> View {
        self.view_of(self.node_id()).unwrap_or_default()
    }

    /// Cached layout configuration for the current node.
    fn layout(&self) -> Layout {
        self.layout_of(self.node_id()).unwrap_or_default()
    }

    /// View information for a specific node.
    fn view_of(&self, node: NodeId) -> Option<View>;

    /// Layout configuration for a specific node.
    fn layout_of(&self, node: NodeId) -> Option<Layout>;

    /// Read a widget without extracting its slot or marking it changed.
    fn with_widget_dyn(
        &self,
        node: NodeId,
        callback: &mut dyn FnMut(&dyn Widget) -> Result<()>,
    ) -> Result<()>;

    /// Inspect a command for display. Registry and target resolution failures
    /// are returned as disabled reasons; eligibility hook failures remain
    /// errors.
    fn command_status(
        &self,
        target: CommandTarget,
        invocation: &CommandInvocation,
    ) -> Result<CommandStatus>;

    /// Widget type identifier for a specific node.
    fn type_id_of(&self, node: NodeId) -> Option<TypeId>;

    /// Resolve a semantic key in an explicit live subtree scope.
    fn find_identity(&self, scope: NodeId, key: &str) -> Result<Option<NodeId>>;

    /// Return a node's independently assigned semantic identity.
    fn semantic_identity(&self, node: NodeId) -> Option<SemanticIdentity>;

    /// Children of the current node in tree order.
    fn children(&self) -> Vec<NodeId> {
        self.children_of(self.node_id())
    }

    /// Children of a specific node in tree order.
    fn children_of(&self, node: NodeId) -> Vec<NodeId>;

    /// Does the current node have focus?
    fn is_focused(&self) -> bool {
        self.is_focused_of(self.node_id())
    }

    /// Does the specified node have focus?
    fn is_focused_of(&self, node: NodeId) -> bool;

    /// Return the currently focused node, including one not yet laid out.
    fn focused_node(&self) -> Option<NodeId>;

    /// Is the current node on the focus path?
    fn is_on_focus_path(&self) -> bool {
        self.is_on_focus_path_of(self.node_id())
    }

    /// Is the specified node on the focus path?
    fn is_on_focus_path_of(&self, node: NodeId) -> bool;

    /// Return the focused leaf under the subtree rooted at `root`.
    fn focused_leaf(&self, root: NodeId) -> Option<NodeId>;

    /// Return focusable leaves in pre-order under the subtree rooted at `root`.
    fn focusable_leaves(&self, root: NodeId) -> Vec<NodeId>;

    /// Return the parent of a node, or `None` if it is the root or not found.
    fn parent_of(&self, node: NodeId) -> Option<NodeId>;

    /// Return whether a node exists and is attached to the root tree.
    fn is_attached_of(&self, node: NodeId) -> bool;

    /// Whether a modal scope remains open, including pending deferred closes.
    fn modal_is_open(&self, token: InteractionToken) -> bool {
        let _ = token;
        false
    }

    /// Return the path for a node relative to a root.
    fn path_of(&self, root: NodeId, node: NodeId) -> Path;

    /// Locate the deepest visible node at a point within a subtree.
    fn locate(&self, root: NodeId, point: Point) -> Result<Option<NodeId>>;

    /// Return a keyed child relative to the current node.
    fn child_slot(&self, key: &str) -> Option<NodeId> {
        self.child_slot_of(self.node_id(), key)
    }

    /// Return a keyed child relative to a specific parent node.
    fn child_slot_of(&self, parent: NodeId, key: &str) -> Option<NodeId>;

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
        .filter(move |id| path_filter.check_match(&ctx.path_of(root, *id)).is_some())
}

/// Apply a scroll transform to a node, clamp it to the canvas, and report
/// whether it moved.
fn update_scroll(
    core: &mut Core,
    node_id: NodeId,
    f: impl FnOnce(Point) -> Point,
) -> ChangeOutcome {
    let Some(node) = core.nodes.get_mut(node_id) else {
        return ChangeOutcome::Unchanged;
    };
    let before = node.scroll;
    node.scroll = f(before);
    clamp_scroll(&mut node.scroll, node.content_size, node.canvas);
    node.view.scroll = node.scroll;
    let changed = before != node.scroll;
    if changed {
        core.invalidate(crate::Invalidation::Layout);
    }
    if changed {
        ChangeOutcome::Changed
    } else {
        ChangeOutcome::Unchanged
    }
}

/// Validate one raw node ID against a requested widget type.
fn checked_typed_id<W, C>(ctx: &C, node: NodeId) -> Result<TypedId<W>>
where
    W: Widget + 'static,
    C: ViewContext + ?Sized,
{
    let actual = ctx.type_id_of(node).ok_or(Error::NodeNotFound(node))?;
    if actual != TypeId::of::<W>() {
        return Err(Error::NodeTypeMismatch {
            node,
            expected: type_name::<W>(),
        });
    }
    Ok(TypedId::new(node))
}

/// Typed helpers shared by read-only and mutable contexts.
pub trait ViewContextExt: ViewContext {
    /// Read a typed widget while preserving immutable access and borrow errors.
    fn with_widget<W: Widget + 'static, R>(
        &self,
        node: TypedId<W>,
        callback: impl FnOnce(&W) -> Result<R>,
    ) -> Result<R> {
        let mut callback = Some(callback);
        let mut result = None;
        self.with_widget_dyn(node.into(), &mut |widget| {
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
    fn typed_id<W: Widget + 'static>(&self, node: impl Into<NodeId>) -> Result<TypedId<W>> {
        checked_typed_id(self, node.into())
    }

    /// Pre-order traversal of the subtree rooted at `root`.
    fn preorder(&self, root: impl Into<NodeId>) -> impl Iterator<Item = NodeId> + '_ {
        preorder_from(self, root.into())
    }

    /// Return the first widget of type `W` anywhere in the tree, including the
    /// root.
    fn first_in_tree<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.preorder(self.root_id())
            .find(|id| ViewContext::type_id_of(self, *id) == Some(TypeId::of::<W>()))
            .map(TypedId::new)
    }

    /// Return all widgets of type `W` anywhere in the tree, including the root.
    fn all_in_tree<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        self.preorder(self.root_id())
            .filter(|id| ViewContext::type_id_of(self, *id) == Some(TypeId::of::<W>()))
            .map(TypedId::new)
            .collect()
    }

    /// Find exactly one node matching a path filter.
    fn find_one(&self, path: &str) -> Result<NodeId> {
        let filter = PathFilter::normalized(path)?;
        let matches = self.find_nodes_matching(&filter);
        match matches.len() {
            0 => Err(Error::NotFound(format!("path {}", filter.as_str()))),
            1 => Ok(matches[0]),
            _ => Err(Error::MultipleMatches),
        }
    }

    /// Return the unique child of type `W`, or error if more than one exists.
    fn unique_child<W: Widget + 'static>(&self) -> Result<Option<TypedId<W>>> {
        unique_typed(self, self.children().into_iter())
    }

    /// Return all direct children of type `W`.
    fn children_of_type<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        self.children()
            .into_iter()
            .filter(|id| node_matches_type::<W, _>(self, *id))
            .map(TypedId::new)
            .collect()
    }

    /// Return the unique descendant of type `W`, or error if more than one
    /// exists.
    fn unique_descendant<W: Widget + 'static>(&self) -> Result<Option<TypedId<W>>> {
        unique_typed(self, self.preorder(self.node_id()).skip(1))
    }

    /// Return all descendants of type `W` (excluding self).
    fn descendants_of_type<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        self.preorder(self.node_id())
            .skip(1)
            .filter(|id| node_matches_type::<W, _>(self, *id))
            .map(TypedId::new)
            .collect()
    }

    /// Return the descendant of type `W` that is on the focus path, if any.
    fn focused_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        self.descendants_of_type::<W>()
            .into_iter()
            .find(|id| ViewContext::is_on_focus_path_of(self, (*id).into()))
    }

    /// Return the descendant of type `W` on the focus path, or the first if
    /// none focused.
    ///
    /// This searches only within the current node's subtree. Use the tree-wide
    /// helpers on `ViewContext` if you need to search from an arbitrary
    /// root.
    fn focused_or_first_descendant<W: Widget + 'static>(&self) -> Option<TypedId<W>> {
        let descendants = self.descendants_of_type::<W>();
        let focused = descendants
            .iter()
            .copied()
            .find(|id| ViewContext::is_on_focus_path_of(self, (*id).into()));
        focused.or_else(|| descendants.into_iter().next())
    }

    /// Return the first leaf node under `root` using pre-order traversal.
    ///
    /// A leaf is a node with no children.
    fn first_leaf(&self, root: impl Into<NodeId>) -> Option<NodeId> {
        let root = root.into();
        self.preorder(root)
            .find(|id| ViewContext::children_of(self, *id).is_empty())
    }
}

impl<T: ViewContext + ?Sized> ViewContextExt for T {}

/// Return whether a node stores the requested concrete widget type.
fn node_matches_type<W: Widget + 'static, C: ViewContext + ?Sized>(
    context: &C,
    node: NodeId,
) -> bool {
    context.type_id_of(node) == Some(TypeId::of::<W>())
}

/// Convert a node stream into zero or one typed identifier.
fn unique_typed<W: Widget + 'static, C: ViewContext + ?Sized>(
    context: &C,
    ids: impl Iterator<Item = NodeId>,
) -> Result<Option<TypedId<W>>> {
    let mut found = None;
    for id in ids.filter(|id| node_matches_type::<W, _>(context, *id)) {
        if found.is_some() {
            return Err(Error::MultipleMatches);
        }
        found = Some(TypedId::new(id));
    }
    Ok(found)
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

/// Direction for focus movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, crate::CommandEnum)]
pub enum FocusDirection {
    /// Move to the next focusable node.
    Next,
    /// Move to the previous focusable node.
    Prev,
    /// Move focus up.
    Up,
    /// Move focus down.
    Down,
    /// Move focus left.
    Left,
    /// Move focus right.
    Right,
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
///
/// Applications consume contexts supplied by Canopy and cannot implement this
/// trait themselves.
pub trait Context: ViewContext + sealed::Context {
    /// Assign a unique semantic key within a containing subtree scope.
    fn set_semantic_key(&mut self, node: NodeId, scope: NodeId, key: &str) -> Result<()>;

    /// Remove the semantic identity of a live node.
    fn clear_semantic_key(&mut self, node: NodeId) -> Result<()>;

    /// Focus an attached node.
    fn set_focus(&mut self, node: NodeId) -> Result<ChangeOutcome>;

    /// Move focus in a direction within an explicit scope.
    fn focus_move(&mut self, scope: FocusScope, direction: FocusDirection)
    -> Result<ChangeOutcome>;

    /// Focus the first focusable node within an explicit scope.
    fn focus_first(&mut self, scope: FocusScope) -> Result<ChangeOutcome>;

    /// Capture mouse events for the current node.
    fn capture_mouse(&mut self) -> Result<ChangeOutcome>;

    /// Release mouse capture if held by the current node.
    fn release_mouse(&mut self) -> Result<ChangeOutcome>;

    /// Clear and return the current mouse-capture target.
    fn take_mouse_capture(&mut self) -> Result<Option<NodeId>>;

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

    /// Scroll the view to the specified position.
    fn scroll_to(&mut self, x: u32, y: u32) -> ChangeOutcome;

    /// Scroll the view by the given offsets.
    fn scroll_by(&mut self, x: i32, y: i32) -> ChangeOutcome;

    /// Scroll the view up by one page.
    fn page_up(&mut self) -> ChangeOutcome {
        let view = self.view();
        self.scroll_to(view.scroll.x, view.scroll.y.saturating_sub(view.content.h))
    }

    /// Scroll the view down by one page.
    fn page_down(&mut self) -> ChangeOutcome {
        let view = self.view();
        self.scroll_to(view.scroll.x, view.scroll.y.saturating_add(view.content.h))
    }

    /// Scroll the view up by one line.
    fn scroll_up(&mut self) -> ChangeOutcome {
        self.scroll_by(0, -1)
    }

    /// Scroll the view down by one line.
    fn scroll_down(&mut self) -> ChangeOutcome {
        self.scroll_by(0, 1)
    }

    /// Scroll the view left by one line.
    fn scroll_left(&mut self) -> ChangeOutcome {
        self.scroll_by(-1, 0)
    }

    /// Scroll the view right by one line.
    fn scroll_right(&mut self) -> ChangeOutcome {
        self.scroll_by(1, 0)
    }

    /// Mark this node dirty so the next frame re-runs layout.
    fn invalidate_layout(&mut self);

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

    /// Create a new widget node detached from the tree.
    fn create_detached_boxed(&mut self, widget: Box<dyn Widget>) -> Result<NodeId>;

    /// Run immediate mutations with structural rollback on error.
    ///
    /// Rollback restores arena metadata, topology, child keys, layouts, views,
    /// lifecycle flags, root, focus, mouse capture, focus recovery hints, exit
    /// requests, pending styles, and diagnostic requests. Each failed nested
    /// edit restores its own structural checkpoint.
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
    fn with_widget_dyn_mut(
        &mut self,
        node: NodeId,
        f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
    ) -> Result<()>;

    /// Dispatch according to an explicit target policy.
    fn dispatch(
        &mut self,
        target: CommandTarget,
        cmd: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError>;

    /// Invoke with explicit target and input scope.
    fn dispatch_scoped(
        &mut self,
        target: CommandTarget,
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
    fn add_child_to_slot_boxed(
        &mut self,
        parent: NodeId,
        key: &str,
        widget: Box<dyn Widget>,
    ) -> Result<NodeId>;

    /// Attach a detached child to a parent.
    fn attach(&mut self, parent: NodeId, child: NodeId) -> Result<()>;

    /// Attach a detached child to a parent using a unique key.
    fn attach_slot(&mut self, parent: NodeId, key: &str, child: NodeId) -> Result<()>;

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

/// Typed mutation and composition helpers for contexts.
pub trait ContextExt: Context + ViewContextExt {
    /// Invoke only the specified command owner.
    fn dispatch_exact(
        &mut self,
        node: NodeId,
        command: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError> {
        self.dispatch(CommandTarget::Exact(node), command)
    }

    /// Build detached children and attach them after configuration succeeds.
    /// Structural rollback follows [`Context::edit_structure`].
    fn compose<R>(
        &mut self,
        parent: NodeId,
        build: impl FnOnce(&mut crate::ChildBuilder<'_>) -> Result<R>,
    ) -> Result<R> {
        super::children::compose(sealed::Context::as_context(self), parent, build)
    }

    /// Set the layout for the current node.
    fn set_layout(&mut self, layout: Layout) -> Result<()> {
        self.set_layout_of(self.node_id(), layout)
    }

    /// Set the layout for a specific node.
    fn set_layout_of(&mut self, node: impl Into<NodeId>, layout: Layout) -> Result<()> {
        Context::set_layout_override_of(self, node.into(), LayoutOverride::full(layout))
    }

    /// Execute a closure with mutable access to a runtime-checked widget node.
    fn with_widget_mut<W, R>(
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
        self.with_widget_dyn_mut(node, &mut |widget, ctx| {
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
    fn create_detached<W: Widget + 'static>(&mut self, widget: W) -> Result<TypedId<W>> {
        let id = self.create_detached_boxed(widget.into())?;
        Ok(TypedId::new(id))
    }

    /// Add a widget as a child of the current node and return the new typed
    /// node ID.
    fn add_child<W: Widget + 'static>(&mut self, widget: W) -> Result<TypedId<W>> {
        self.add_child_to(self.node_id(), widget)
    }

    /// Add a widget as a child of a specific parent and return the new typed
    /// node ID.
    fn add_child_to<W: Widget + 'static>(
        &mut self,
        parent: impl Into<NodeId>,
        widget: W,
    ) -> Result<TypedId<W>> {
        let id = self.add_child_to_boxed(parent.into(), widget.into())?;
        Ok(TypedId::new(id))
    }

    /// Check if a typed keyed child exists.
    fn has_slot<K: ChildSlot>(&self) -> Result<bool> {
        self.get_slot::<K>().map(|child| child.is_some())
    }

    /// Get a typed keyed child's node ID.
    fn get_slot<K: ChildSlot>(&self) -> Result<Option<TypedId<K::Widget>>> {
        self.child_slot(K::KEY)
            .map(|node| checked_typed_id(self, node))
            .transpose()
    }

    /// Get a typed keyed child's node ID from a specific parent.
    fn get_slot_of<K: ChildSlot>(
        &self,
        parent: impl Into<NodeId>,
    ) -> Result<Option<TypedId<K::Widget>>> {
        ViewContext::child_slot_of(self, parent.into(), K::KEY)
            .map(|node| checked_typed_id(self, node))
            .transpose()
    }

    /// Get or create the keyed child under the current node.
    fn get_or_create_slot<K: ChildSlot>(
        &mut self,
        make: impl FnOnce() -> K::Widget,
    ) -> Result<TypedId<K::Widget>> {
        let parent = self.node_id();
        self.get_or_create_slot_of::<K>(parent, make)
    }

    /// Get or create the keyed child under a specific parent node.
    fn get_or_create_slot_of<K: ChildSlot>(
        &mut self,
        parent: impl Into<NodeId>,
        make: impl FnOnce() -> K::Widget,
    ) -> Result<TypedId<K::Widget>> {
        let parent = parent.into();
        if let Some(id) = self.get_slot_of::<K>(parent)? {
            return Ok(id);
        }
        self.add_slot_to(parent, K::KEY, make())
    }

    /// Add a typed keyed child to the current node and return its typed node
    /// ID.
    fn add_slot<K: ChildSlot>(&mut self, widget: K::Widget) -> Result<TypedId<K::Widget>> {
        self.add_slot_to(self.node_id(), K::KEY, widget)
    }

    /// Add a typed keyed child to a specific parent and return its typed node
    /// ID.
    fn add_slot_to<W: Widget + 'static>(
        &mut self,
        parent: impl Into<NodeId>,
        key: &str,
        widget: W,
    ) -> Result<TypedId<W>> {
        let id = self.add_child_to_slot_boxed(parent.into(), key, widget.into())?;
        Ok(TypedId::new(id))
    }

    /// Execute a closure with a typed keyed child.
    fn with_typed_slot<K: ChildSlot, R>(
        &mut self,
        f: impl FnOnce(&mut K::Widget, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        let node = self
            .child_slot(K::KEY)
            .ok_or_else(|| Error::NotFound(format!("key {}", K::KEY)))?;
        self.with_widget_mut(node, f)
    }

    /// Execute a closure with the unique descendant of type `W`.
    fn with_unique_descendant<W: Widget + 'static, R>(
        &mut self,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        let node = self
            .unique_descendant::<W>()?
            .ok_or_else(|| Error::NotFound(type_name::<W>().to_string()))?;
        self.with_widget_mut(node, f)
    }

    /// Execute a closure with the unique descendant of type `W` if it exists.
    fn try_with_unique_descendant<W: Widget + 'static, R>(
        &mut self,
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<Option<R>> {
        let node = self.unique_descendant::<W>()?;
        let Some(node) = node else {
            return Ok(None);
        };
        self.with_widget_mut(node, f).map(Some)
    }
}

impl<T: Context + ?Sized> ContextExt for T {}

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

impl<C: Deref<Target = Core>> sealed::ViewContext for NodeCtx<C> {}

impl<C: Deref<Target = Core>> ViewContext for NodeCtx<C> {
    fn node_id(&self) -> NodeId {
        self.node_id
    }

    fn root_id(&self) -> NodeId {
        self.core.root
    }

    fn view_of(&self, node: NodeId) -> Option<View> {
        self.core.nodes.get(node).map(|n| n.view)
    }

    fn layout_of(&self, node: NodeId) -> Option<Layout> {
        self.core.nodes.get(node).map(|n| n.layout)
    }

    fn with_widget_dyn(
        &self,
        node: NodeId,
        callback: &mut dyn FnMut(&dyn Widget) -> Result<()>,
    ) -> Result<()> {
        self.core
            .with_widget(node, WidgetOperation::access("read widget"), |widget, _| {
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

    fn find_identity(&self, scope: NodeId, key: &str) -> Result<Option<NodeId>> {
        self.core.find_identity(scope, key)
    }

    fn semantic_identity(&self, node: NodeId) -> Option<SemanticIdentity> {
        self.core
            .nodes
            .get(node)
            .and_then(|entry| entry.semantic_identity.clone())
    }

    fn type_id_of(&self, node: NodeId) -> Option<TypeId> {
        self.core.nodes.get(node).map(|n| n.widget_type)
    }

    fn children_of(&self, node: NodeId) -> Vec<NodeId> {
        self.core
            .nodes
            .get(node)
            .map(|n| n.children.clone())
            .unwrap_or_default()
    }

    fn is_focused_of(&self, node: NodeId) -> bool {
        self.core.is_focused(node)
    }

    fn focused_node(&self) -> Option<NodeId> {
        self.core.focus
    }

    fn is_on_focus_path_of(&self, node: NodeId) -> bool {
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

    fn is_attached_of(&self, node: NodeId) -> bool {
        self.core.is_attached_to_root(node)
    }

    fn path_of(&self, root: NodeId, node: NodeId) -> Path {
        self.core.path_of(root, node)
    }

    fn locate(&self, root: NodeId, point: Point) -> Result<Option<NodeId>> {
        self.core.locate_node(root, point)
    }

    fn child_slot_of(&self, parent: NodeId, key: &str) -> Option<NodeId> {
        self.core.child_slot(parent, key)
    }
}

impl sealed::Context for NodeCtx<&mut Core> {
    fn as_context(&mut self) -> &mut dyn Context {
        self
    }

    fn attach_composed(
        &mut self,
        parent: NodeId,
        roots: &[(NodeId, Option<&str>)],
        keys: &[(NodeId, NodeId, String)],
    ) -> Result<()> {
        self.core.attach_composed(parent, roots, keys)
    }
}

impl Context for NodeCtx<&mut Core> {
    fn set_semantic_key(&mut self, node: NodeId, scope: NodeId, key: &str) -> Result<()> {
        self.core.set_semantic_key(node, scope, key)
    }

    fn clear_semantic_key(&mut self, node: NodeId) -> Result<()> {
        self.core.clear_semantic_key(node)
    }

    fn set_focus(&mut self, node: NodeId) -> Result<ChangeOutcome> {
        self.core.set_focus(node)
    }

    fn focus_first(&mut self, scope: FocusScope) -> Result<ChangeOutcome> {
        self.core.focus_first(scope.resolve(self))
    }

    fn focus_move(
        &mut self,
        scope: FocusScope,
        direction: FocusDirection,
    ) -> Result<ChangeOutcome> {
        self.core.focus_move(scope.resolve(self), direction)
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

    fn available_bindings(&self, node: Option<NodeId>) -> Result<BindingSnapshot> {
        self.core.available_bindings(node)
    }

    fn open_modal(&mut self, options: ModalOptions) -> Result<InteractionToken> {
        self.core.open_modal(options)
    }

    fn close_modal(&mut self, token: InteractionToken) -> Result<()> {
        self.core.close_modal(token)
    }

    fn scroll_to(&mut self, x: u32, y: u32) -> ChangeOutcome {
        update_scroll(self.core, self.node_id, |_| Point { x, y })
    }

    fn scroll_by(&mut self, x: i32, y: i32) -> ChangeOutcome {
        update_scroll(self.core, self.node_id, |scroll| scroll.scroll(x, y))
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

    fn with_widget_dyn_mut(
        &mut self,
        node: NodeId,
        f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        self.core
            .with_widget_ctx(node, |widget, ctx| f(widget, ctx))?
    }

    fn dispatch(
        &mut self,
        target: CommandTarget,
        cmd: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError> {
        commands::dispatch_target(self.core, target, cmd)
    }

    fn dispatch_scoped(
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

    fn add_child_to_slot_boxed(
        &mut self,
        parent: NodeId,
        key: &str,
        widget: Box<dyn Widget>,
    ) -> Result<NodeId> {
        self.core.add_child_to_slot_boxed(parent, key, widget)
    }

    fn attach(&mut self, parent: NodeId, child: NodeId) -> Result<()> {
        self.core.attach(parent, child)
    }

    fn attach_slot(&mut self, parent: NodeId, key: &str, child: NodeId) -> Result<()> {
        self.core.attach_slot(parent, key, child)
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
        self.core.pending_diagnostic_dump = Some(target);
    }
}

#[cfg(test)]
mod tests {
    use crate::{ChildSlot, Widget};

    slot!(Editor);
    impl Widget for Editor {}

    pub struct Modal;
    impl Widget for Modal {}
    slot!(pub ModalSlot: Modal);

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
            node.view.scroll = node.scroll;
            let mut context = CoreContext::new(&mut core, root);
            for step in 1u32..=2 {
                let before = context.view().scroll.y;
                let expected = height.saturating_mul(step).min(max_y);
                assert_eq!(context.page_down().changed(), before != expected);
                assert_eq!(context.view().scroll, Point { x: 2, y: expected });
            }
            context.scroll_to(2, max_y);
            let expected = max_y.saturating_sub(height);
            assert_eq!(context.page_up().changed(), max_y != expected);
            assert_eq!(context.view().scroll, Point { x: 2, y: expected });
            context.scroll_to(2, 0);
            assert!(!context.page_up().changed());
        }
    }
}
