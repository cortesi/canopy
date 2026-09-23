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
    world::{Core, WidgetOperation, scroll::RevealTarget},
};
use crate::{
    ChangeOutcome, InteractionToken, ModalOptions, Notice, SemanticIdentity,
    commands::{ArgValue, CommandCall, CommandError, CommandStatus, CommandTarget},
    error::{Error, Result},
    event::{Event, mouse::MouseEvent},
    geom::{Point, Rect},
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
    /// Marker implemented only by Canopy's built-in read-only contexts.
    pub trait ViewContext {}

    /// Marker implemented only by Canopy's built-in mutable contexts.
    pub trait Context {}
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

    /// View information for a specific node.
    fn view_of(&self, node: NodeId) -> Option<View>;

    /// Layout configuration for a specific node.
    fn layout_of(&self, node: NodeId) -> Option<Layout>;

    /// Return what scrolling `node` by `(x, y)` would change, without
    /// mutating the tree.
    ///
    /// This mirrors [`Context::scroll_by`], including the change a cancelled
    /// pending reveal produces. It returns `None` when the node is missing.
    /// Contextual key prediction uses this to answer for a scrollable target
    /// without running widget effects.
    fn scroll_outcome(&self, node: NodeId, x: i32, y: i32) -> Option<ChangeOutcome>;

    /// Read a widget without extracting its slot or marking it changed.
    fn with_widget_dyn(
        &self,
        node: NodeId,
        callback: &mut dyn FnMut(&dyn Widget) -> Result<()>,
    ) -> Result<()>;

    /// Inspect a command call for display. A call without a target resolves
    /// from the current node. Registry and target resolution failures are
    /// returned as disabled reasons; eligibility hook failures remain errors.
    fn command_status(&self, call: &CommandCall) -> Result<CommandStatus>;

    /// Widget type identifier for a specific node.
    fn type_id_of(&self, node: NodeId) -> Option<TypeId>;

    /// Resolve a semantic key in an explicit live subtree scope.
    fn find_identity(&self, scope: NodeId, key: &str) -> Result<Option<NodeId>>;

    /// Return a node's independently assigned semantic identity.
    fn semantic_identity(&self, node: NodeId) -> Option<SemanticIdentity>;

    /// Children of a specific node in tree order.
    fn children_of(&self, node: NodeId) -> Vec<NodeId>;

    /// Does the current node have focus?
    fn is_focused(&self) -> bool;

    /// Return the currently focused node, including one not yet laid out.
    fn focused_node(&self) -> Option<NodeId>;

    /// Does the current node hold mouse capture?
    fn has_mouse_capture(&self) -> bool;

    /// Is the specified node on the focus path?
    fn is_on_focus_path(&self, node: NodeId) -> bool;

    /// Return the focused leaf under the subtree rooted at `root`.
    fn focused_leaf(&self, root: NodeId) -> Option<NodeId>;

    /// Return focusable leaves in pre-order under the subtree rooted at `root`.
    fn focusable_leaves(&self, root: NodeId) -> Vec<NodeId>;

    /// Return the parent of a node, or `None` if it is the root or not found.
    fn parent_of(&self, node: NodeId) -> Option<NodeId>;

    /// Return whether a node exists and is attached to the root tree.
    fn is_attached(&self, node: NodeId) -> bool;

    /// Whether a modal scope remains open, including pending deferred closes.
    fn modal_is_open(&self, token: InteractionToken) -> bool;

    /// Return the notice the application shows: the newest one, from its
    /// record until the next input event.
    fn notice(&self) -> Option<&Notice>;

    /// Return the path for a node relative to a root.
    fn path_of(&self, root: NodeId, node: NodeId) -> Path;

    /// Locate the deepest visible node at a point within a subtree.
    fn locate(&self, root: NodeId, point: Point) -> Result<Option<NodeId>>;

    /// Return a keyed child relative to a specific parent node.
    fn child_slot_of(&self, parent: NodeId, key: &str) -> Option<NodeId>;

    /// Find the first node whose path matches the validated filter.
    fn find_node_matching(&self, path_filter: &PathFilter) -> Option<NodeId> {
        matching_nodes(self, path_filter).next()
    }

    /// Find all nodes whose paths match the filter, relative to the current
    /// node.
    ///
    /// The filter is normalized to match full paths. An invalid filter returns
    /// the parse error.
    fn find_nodes(&self, path_filter: &str) -> Result<Vec<NodeId>> {
        let filter = PathFilter::normalized(path_filter)?;
        Ok(self.find_nodes_matching(&filter))
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

/// A one-shot closure passed through an API that takes `&mut dyn FnMut`.
///
/// Object-safe context methods take `&mut dyn FnMut` callbacks, while typed
/// helpers accept `FnOnce` closures and return values. The `FnMut` shim runs
/// the closure through [`OneShot::call`] and stores its output. Calling the
/// shim twice, or never, is an internal error.
pub(super) struct OneShot<F, R> {
    /// Closure to run, until it runs.
    f: Option<F>,
    /// Output of the closure, once it ran.
    output: Option<R>,
}

impl<F, R> OneShot<F, R> {
    /// Hold a closure until the shim calls it.
    pub(super) fn new(f: F) -> Self {
        Self {
            f: Some(f),
            output: None,
        }
    }

    /// Run the closure through `run` and store its output.
    pub(super) fn call(&mut self, run: impl FnOnce(F) -> Result<R>) -> Result<()> {
        let f = self
            .f
            .take()
            .ok_or_else(|| Error::Internal("one-shot callback called twice".into()))?;
        self.output = Some(run(f)?);
        Ok(())
    }

    /// Return the stored output.
    pub(super) fn finish(self) -> Result<R> {
        self.output
            .ok_or_else(|| Error::Internal("one-shot callback never called".into()))
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
        return Err(type_mismatch::<W>(node));
    }
    Ok(TypedId::new(node))
}

/// Report a widget of another type as a type mismatch.
fn type_mismatch<W: Widget + 'static>(node: NodeId) -> Error {
    Error::NodeTypeMismatch {
        node,
        expected: type_name::<W>(),
    }
}

/// Typed helpers shared by read-only and mutable contexts.
pub trait ViewContextExt: ViewContext {
    /// Read a runtime-checked widget node.
    ///
    /// This is the read path: it borrows the widget in place and does not
    /// invalidate layout or paint. Use [`ContextExt::with_widget_mut`] to
    /// change a widget. A node of another widget type fails with
    /// [`Error::NodeTypeMismatch`].
    fn with_widget<W: Widget + 'static, R>(
        &self,
        node: impl Into<NodeId>,
        f: impl FnOnce(&W) -> Result<R>,
    ) -> Result<R> {
        let node = node.into();
        checked_typed_id::<W, _>(self, node)?;
        let mut once = OneShot::new(f);
        self.with_widget_dyn(node, &mut |widget| {
            let widget = (widget as &dyn Any)
                .downcast_ref::<W>()
                .ok_or_else(|| type_mismatch::<W>(node))?;
            once.call(|f| f(widget))
        })?;
        once.finish()
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
        unique_typed(self, self.children_of(self.node_id()).into_iter())
    }

    /// Return all direct children of type `W`.
    fn children_of_type<W: Widget + 'static>(&self) -> Vec<TypedId<W>> {
        self.children_of(self.node_id())
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
            .find(|id| ViewContext::is_on_focus_path(self, (*id).into()))
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
            .find(|id| ViewContext::is_on_focus_path(self, (*id).into()));
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

/// How a reveal places a rectangle inside a viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RevealAlign {
    /// Make the smallest move that shows the rectangle. A viewport that
    /// already lies inside a larger rectangle stays where it is.
    #[default]
    Nearest,
    /// Center each axis on which the rectangle is shorter than the viewport,
    /// and use `Nearest` on the others.
    Center,
    /// Place the rectangle's start `context` cells past the viewport origin,
    /// clamping to the scrollable range. Text reads top to bottom, so only
    /// the vertical axis offsets; the horizontal axis uses `Nearest`.
    Top(u32),
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

    /// Return effective key bindings for a node or the current focus.
    fn available_bindings(&self, node: Option<NodeId>) -> Result<BindingSnapshot>;

    /// Open a modal scope that owns focus, input admission, and visual effects.
    fn open_modal(&mut self, options: ModalOptions) -> Result<InteractionToken>;

    /// Close this scope and its nested scopes after active callbacks return.
    fn close_modal(&mut self, token: InteractionToken) -> Result<()>;

    /// Scroll the view to the specified position.
    ///
    /// Scrolling moves the view at once. It supersedes older reveal requests
    /// for this view, even when the offset does not change.
    fn scroll_to(&mut self, x: u32, y: u32) -> ChangeOutcome;

    /// Scroll the view by the given offsets.
    fn scroll_by(&mut self, x: i32, y: i32) -> ChangeOutcome;

    /// Scroll an attached node's view to the specified position.
    ///
    /// Owners of scrollbars use this to move the node they display. The
    /// offset is clamped as [`Context::scroll_to`] clamps it. Returns an error
    /// when the node is missing or detached.
    fn scroll_to_of(&mut self, node: NodeId, x: u32, y: u32) -> Result<ChangeOutcome>;

    /// Reveal a rectangle of this node's canvas once layout settles.
    ///
    /// The request replaces this node's pending area or anchor request. It
    /// waits while the node is hidden, detached, zero-sized, or outside the
    /// active modal region, and a later scroll or reveal of this view
    /// supersedes it. Empty areas do nothing. Returns whether the pending
    /// request changed.
    fn reveal_area(&mut self, area: Rect, align: RevealAlign) -> ChangeOutcome;

    /// Reveal this node's logical anchor once layout settles.
    ///
    /// Layout asks [`Widget::reveal_anchor`] for the anchor using the final
    /// content size, so changes made in the same turn count. Otherwise this
    /// behaves as [`Context::reveal_area`].
    fn reveal_anchor(&mut self, align: RevealAlign) -> ChangeOutcome;

    /// Reveal a node's outer rectangle in its ancestor views once layout
    /// settles.
    ///
    /// Each view from the node's parent out to the active modal region or the
    /// root shows what the view inside it left visible. A later scroll or
    /// reveal of a shared view wins there without affecting other views. The
    /// request waits while the node is hidden, detached, or outside the modal
    /// region. Returns an error when the node does not exist.
    fn reveal_node(&mut self, node: NodeId, align: RevealAlign) -> Result<ChangeOutcome>;

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

    /// Update the layout for a specific node.
    fn with_layout_of(&mut self, node: NodeId, f: &mut dyn FnMut(&mut Layout)) -> Result<()>;

    /// Replace persistent parent constraints without replacing widget layout
    /// fields.
    fn set_layout_override(&mut self, node: NodeId, overrides: LayoutOverride) -> Result<()>;

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

    /// Dispatch a command call. A call without a target resolves from the
    /// current node.
    fn dispatch(&mut self, call: &CommandCall) -> StdResult<ArgValue, CommandError>;

    /// Return the event being handled, for injection.
    fn current_event(&self) -> Option<&Event>;

    /// Return the event being handled when it is a mouse event, for
    /// injection.
    fn current_mouse_event(&self) -> Option<MouseEvent> {
        match self.current_event()? {
            Event::Mouse(mouse) => Some(*mouse),
            _ => None,
        }
    }

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

    /// Replace the children list for a parent node.
    ///
    /// The list reorders the current children and may add new ones. Omitting
    /// a current child is an error: detach or remove it first.
    fn set_children(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()>;

    /// Set a node's visibility.
    fn set_hidden(&mut self, node: NodeId, hidden: bool) -> Result<ChangeOutcome>;

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
}

/// Typed mutation and composition helpers for contexts.
pub trait ContextExt: Context + ViewContextExt {
    /// Dispatch a command call to exactly `node`, replacing the call's target.
    fn dispatch_exact(
        &mut self,
        node: NodeId,
        call: &CommandCall,
    ) -> StdResult<ArgValue, CommandError> {
        self.dispatch(&call.clone().with_target(CommandTarget::Exact(node)))
    }

    /// Set the layout for a specific node.
    fn set_layout_of(&mut self, node: impl Into<NodeId>, layout: Layout) -> Result<()> {
        Context::set_layout_override(self, node.into(), LayoutOverride::full(layout))
    }

    /// Execute a closure with mutable access to a runtime-checked widget node.
    ///
    /// The call invalidates layout, because the closure may change what the
    /// widget measures or draws. Use [`ViewContextExt::with_widget`] to read
    /// a widget. A node of another widget type fails with
    /// [`Error::NodeTypeMismatch`].
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
        let mut once = OneShot::new(f);
        self.with_widget_dyn_mut(node, &mut |widget, ctx| {
            let widget = (widget as &mut dyn Any)
                .downcast_mut::<W>()
                .ok_or_else(|| type_mismatch::<W>(node))?;
            once.call(|f| f(widget, ctx))
        })?;
        once.finish()
    }

    /// Create a widget node detached from the tree.
    fn create_detached<W: Widget + 'static>(&mut self, widget: W) -> Result<TypedId<W>> {
        let id = self.create_detached_boxed(widget.into())?;
        Ok(TypedId::new(id))
    }

    /// Add a widget as a child of `parent` and return the new typed node ID.
    fn add_child<W: Widget + 'static>(
        &mut self,
        parent: impl Into<NodeId>,
        widget: W,
    ) -> Result<TypedId<W>> {
        add_attached(self, parent.into(), None, widget.into()).map(TypedId::new)
    }

    /// Return the typed keyed child `K` of `parent`. A missing slot is a
    /// [`Error::NotFound`] that names `K::KEY`.
    fn get_slot<K: ChildSlot>(&self, parent: impl Into<NodeId>) -> Result<TypedId<K::Widget>> {
        let node = self
            .child_slot_of(parent.into(), K::KEY)
            .ok_or_else(|| Error::NotFound(format!("slot {}", K::KEY)))?;
        checked_typed_id(self, node)
    }

    /// Return the keyed child `K` of `parent`, creating it with `make` when
    /// it is missing.
    fn get_or_create_slot<K: ChildSlot>(
        &mut self,
        parent: impl Into<NodeId>,
        make: impl FnOnce() -> K::Widget,
    ) -> Result<TypedId<K::Widget>> {
        let parent = parent.into();
        if let Some(node) = self.child_slot_of(parent, K::KEY) {
            return checked_typed_id(self, node);
        }
        self.add_slot::<K>(parent, make())
    }

    /// Add `widget` as the keyed child `K` of `parent` and return its typed
    /// node ID.
    fn add_slot<K: ChildSlot>(
        &mut self,
        parent: impl Into<NodeId>,
        widget: K::Widget,
    ) -> Result<TypedId<K::Widget>> {
        add_attached(self, parent.into(), Some(K::KEY), widget.into()).map(TypedId::new)
    }

    /// Execute a closure with the typed keyed child `K` of `parent`.
    fn with_slot<K: ChildSlot, R>(
        &mut self,
        parent: impl Into<NodeId>,
        f: impl FnOnce(&mut K::Widget, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        let node = self.get_slot::<K>(parent)?;
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

/// Create `widget` and attach it under `parent`, keyed when `key` is given.
/// A failed attach removes the new node again.
fn add_attached<C: Context + ?Sized>(
    ctx: &mut C,
    parent: NodeId,
    key: Option<&str>,
    widget: Box<dyn Widget>,
) -> Result<NodeId> {
    let mut once = OneShot::new(widget);
    ctx.edit_structure(&mut |ctx| {
        once.call(|widget| {
            let child = ctx.create_detached_boxed(widget)?;
            match key {
                Some(key) => ctx.attach_slot(parent, key, child)?,
                None => ctx.attach(parent, child)?,
            }
            Ok(child)
        })
    })?;
    once.finish()
}

/// Context bound to a specific node, over a shared or exclusive borrow of the
/// core.
pub struct NodeCtx<C> {
    /// Core state reference.
    core: C,
    /// Node bound to this context.
    node_id: NodeId,
    /// Focus this context reports instead of the live focus.
    ///
    /// Prediction uses this so a hypothetical route focus answers focus
    /// queries as if that node held the keyboard.
    focus_override: Option<NodeId>,
}

/// Mutating context bound to a specific node.
pub type CoreContext<'a> = NodeCtx<&'a mut Core>;

/// Read-only context bound to a specific node.
pub type CoreViewContext<'a> = NodeCtx<&'a Core>;

impl<C> NodeCtx<C> {
    /// Create a new context for a node.
    pub fn new(core: C, node_id: NodeId) -> Self {
        Self {
            core,
            node_id,
            focus_override: None,
        }
    }

    /// Create a read-only context that reports `focus` as the focused node.
    ///
    /// Contextual key prediction uses this so focus-dependent widgets answer
    /// for the route focus rather than the live focus.
    pub(crate) fn with_focus(core: C, node_id: NodeId, focus: NodeId) -> Self {
        Self {
            core,
            node_id,
            focus_override: Some(focus),
        }
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

    fn scroll_outcome(&self, node: NodeId, x: i32, y: i32) -> Option<ChangeOutcome> {
        self.core.scroll_outcome(node, x, y)
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

    fn command_status(&self, call: &CommandCall) -> Result<CommandStatus> {
        commands::command_status(&self.core, self.node_id, call)
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

    fn is_focused(&self) -> bool {
        let node = self.node_id;
        match self.focus_override {
            Some(focus) => focus == node,
            None => self.core.is_focused(node),
        }
    }

    fn focused_node(&self) -> Option<NodeId> {
        self.focus_override.or(self.core.focus)
    }

    fn has_mouse_capture(&self) -> bool {
        self.core.mouse_capture == Some(self.node_id)
    }

    fn is_on_focus_path(&self, node: NodeId) -> bool {
        match self.focus_override {
            Some(focus) => self.core.is_ancestor_or_self(node, focus),
            None => self.core.is_on_focus_path(node),
        }
    }

    fn focused_leaf(&self, root: NodeId) -> Option<NodeId> {
        match self.focus_override {
            Some(focus) => self.core.is_ancestor_or_self(root, focus).then_some(focus),
            None => self.core.focused_leaf(root),
        }
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

    fn notice(&self) -> Option<&Notice> {
        self.core.notices.shown()
    }

    fn is_attached(&self, node: NodeId) -> bool {
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

impl sealed::Context for NodeCtx<&mut Core> {}

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

    fn available_bindings(&self, node: Option<NodeId>) -> Result<BindingSnapshot> {
        self.core.available_bindings(node)
    }

    fn open_modal(&mut self, options: ModalOptions) -> Result<InteractionToken> {
        self.core.open_modal(options)
    }

    fn close_modal(&mut self, token: InteractionToken) -> Result<()> {
        self.core.close_modal_after_dispatch(token)
    }

    fn scroll_to(&mut self, x: u32, y: u32) -> ChangeOutcome {
        self.core.update_scroll(self.node_id, |_| Point { x, y })
    }

    fn scroll_by(&mut self, x: i32, y: i32) -> ChangeOutcome {
        self.core
            .update_scroll(self.node_id, |scroll| scroll.scroll(x, y))
    }

    fn scroll_to_of(&mut self, node: NodeId, x: u32, y: u32) -> Result<ChangeOutcome> {
        self.core.scroll_to_of(node, x, y)
    }

    fn reveal_area(&mut self, area: Rect, align: RevealAlign) -> ChangeOutcome {
        self.core
            .reveal_in(self.node_id, RevealTarget::Area(area), align)
    }

    fn reveal_anchor(&mut self, align: RevealAlign) -> ChangeOutcome {
        self.core
            .reveal_in(self.node_id, RevealTarget::Anchor, align)
    }

    fn reveal_node(&mut self, node: NodeId, align: RevealAlign) -> Result<ChangeOutcome> {
        self.core.reveal_node(node, align)
    }

    fn invalidate_layout(&mut self) {
        self.core.invalidate(crate::Invalidation::Layout);
        if let Some(node) = self.core.nodes.get_mut(self.node_id) {
            node.layout_dirty = true;
        }
    }

    fn set_layout_override(&mut self, node: NodeId, overrides: LayoutOverride) -> Result<()> {
        self.core.set_layout_override(node, overrides)
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

    fn dispatch(&mut self, call: &CommandCall) -> StdResult<ArgValue, CommandError> {
        commands::dispatch(self.core, self.node_id, call)
    }

    fn current_event(&self) -> Option<&Event> {
        self.core.current_event()
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

    fn set_children(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()> {
        self.core.set_children(parent, children)
    }

    fn set_hidden(&mut self, node: NodeId, hidden: bool) -> Result<ChangeOutcome> {
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
