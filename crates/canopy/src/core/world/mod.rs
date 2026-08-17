#![expect(
    clippy::multiple_inherent_impl,
    reason = "Core methods are split by arena, layout, and dispatch concerns."
)]

use std::{
    any::TypeId,
    cell::Cell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use parking_lot::RwLock;
use slotmap::SlotMap;

use self::focus::FocusRecoveryHint;
use super::{
    help::OwnedHelpSnapshot,
    widget_access::{WidgetMutGuard, WidgetReadGuard, WidgetSlotGuard},
};
use crate::{
    ChangeOutcome, ViewContext,
    commands::{CommandScopeFrame, CommandSet},
    core::{id::NodeId, node::Node, view::View},
    error::{Error, NodeOperationKind, Result},
    geom::{Point, Rect, Size},
    layout::Layout,
    render::Render,
    state::NodeName,
    style::StyleMap,
    widget::Widget,
};

/// Event dispatch and bubbling helpers.
mod dispatch;
/// Focus and mouse-capture management.
mod focus;
/// Layout traversal, measurement, and hit-testing.
pub(crate) mod layout_driver;
#[cfg(test)]
mod tests;
/// Arena mutation, structural invariants, and path helpers.
mod tree;

/// Core state for the arena, layout engine, and focus.
pub struct Core {
    /// Node storage arena.
    pub(crate) nodes: SlotMap<NodeId, Node>,
    /// Root node ID.
    pub(crate) root: NodeId,
    /// Currently focused node.
    pub(crate) focus: Option<NodeId>,
    /// Exit code requested by a widget or command, if any.
    pub(crate) exit_requested: Option<i32>,
    /// Pending style map to be applied before next render.
    pub(crate) pending_style: Option<StyleMap>,
    /// Node that captures mouse events regardless of cursor position.
    pub(crate) mouse_capture: Option<NodeId>,
    /// Focus recovery hint for the most recent structural removal.
    pub(crate) focus_hint: Option<FocusRecoveryHint>,
    /// Active tree edit and its rollback state.
    tree_edit: Option<TreeEditJournal>,
    /// Whether lifecycle cleanup is unwinding a failed tree edit.
    rolling_back_tree_edit: bool,
    /// Registered command specs.
    pub(crate) commands: CommandSet,
    /// Command scope stack for injection.
    command_scope: Vec<CommandScopeFrame>,
    /// Pending help snapshot request - (target node, pre-request focus node).
    pub(crate) pending_help_request: Option<(NodeId, Option<NodeId>)>,
    /// Ready help snapshot for widgets to retrieve.
    pub(crate) pending_help_snapshot: Option<OwnedHelpSnapshot>,
    /// Tracks whether a pending help snapshot was observed during render.
    pending_help_snapshot_observed: Cell<bool>,
    /// Pending diagnostic dump request.
    pub(crate) pending_diagnostic_dump: Option<NodeId>,
}

/// Journal for one outermost tree edit and all nested edits it performs.
struct TreeEditJournal {
    /// Core-owned state from before the edit began.
    before: TreeStateSnapshot,
    /// Widgets whose mount hooks completed during the edit.
    mounted: Vec<MountedWidget>,
    /// Mounted widgets already unmounted by a later nested edit.
    unmounted: HashSet<usize>,
}

/// A successfully mounted widget retained for reverse-order cleanup.
struct MountedWidget {
    /// Node that owned the widget when its mount completed.
    node_id: NodeId,
    /// Stable widget slot identity across node snapshots and replacement.
    widget: Rc<RwLock<Option<Box<dyn Widget>>>>,
}

/// Stable node and widget identities for one removal lifecycle.
struct RemovalPlan {
    /// Root whose lifecycle initiated the plan.
    root: NodeId,
    /// Nodes in deterministic pre-order with their original widget slots.
    pre_order: Vec<RemovalEntry>,
    /// Nodes in deterministic post-order.
    post_order: Vec<NodeId>,
    /// Whether the plan covers the root's complete subtree.
    covers_subtree: bool,
}

/// One node expected to survive unchanged until a removal plan commits.
struct RemovalEntry {
    /// Planned node.
    node_id: NodeId,
    /// Widget slot validated before lifecycle hooks run.
    widget: Rc<RwLock<Option<Box<dyn Widget>>>>,
}

/// Core-owned state restored when a tree edit fails.
struct TreeStateSnapshot {
    /// Arena contents and all node metadata.
    nodes: SlotMap<NodeId, Node>,
    /// Root node ID.
    root: NodeId,
    /// Focus target.
    focus: Option<NodeId>,
    /// Requested process exit.
    exit_requested: Option<i32>,
    /// Pending style replacement.
    pending_style: Option<StyleMap>,
    /// Mouse capture target.
    mouse_capture: Option<NodeId>,
    /// Focus recovery candidates.
    focus_hint: Option<FocusRecoveryHint>,
    /// Registered commands.
    commands: CommandSet,
    /// Active command-dispatch scopes.
    command_scope: Vec<CommandScopeFrame>,
    /// Pending help request.
    pending_help_request: Option<(NodeId, Option<NodeId>)>,
    /// Pending owned help data.
    pending_help_snapshot: Option<OwnedHelpSnapshot>,
    /// Whether pending help was observed.
    pending_help_snapshot_observed: bool,
    /// Pending diagnostic target.
    pending_diagnostic_dump: Option<NodeId>,
}

/// Widget operation whose failures should carry node context.
#[derive(Clone, Copy)]
pub struct WidgetOperation {
    /// Error category used for reporting.
    kind: WidgetOperationKind,
    /// Short operation name.
    name: &'static str,
}

impl WidgetOperation {
    /// Construct a generic widget access operation.
    pub(crate) const fn access(name: &'static str) -> Self {
        Self {
            kind: WidgetOperationKind::Access,
            name,
        }
    }

    /// Construct a layout-phase widget operation.
    pub(crate) const fn layout(name: &'static str) -> Self {
        Self {
            kind: WidgetOperationKind::Layout,
            name,
        }
    }

    /// Construct a render-phase widget operation.
    pub(crate) const fn render(name: &'static str) -> Self {
        Self {
            kind: WidgetOperationKind::Render,
            name,
        }
    }
}

/// Error category for contextual widget operation failures.
#[derive(Clone, Copy)]
enum WidgetOperationKind {
    /// Generic widget access failure.
    Access,
    /// Layout-phase failure.
    Layout,
    /// Render-phase failure.
    Render,
}

impl Core {
    /// Create a new Core with a default root node.
    pub fn new() -> Self {
        let mut nodes = SlotMap::with_key();
        let root_widget = RootContainer;
        let root_type = TypeId::of::<RootContainer>();
        let layout = root_widget.layout();
        let root_name = root_widget.name();
        let root = nodes.insert(Node {
            widget: Rc::new(RwLock::new(Some(Box::new(root_widget)))),
            widget_type: root_type,
            parent: None,
            children: Vec::new(),
            child_keys: HashMap::new(),
            layout,
            rect: Rect::zero(),
            content_size: Size::default(),
            canvas: Size::default(),
            scroll: Point::zero(),
            view: View::default(),
            hidden: false,
            name: root_name,
            initialized: false,
            mounted: false,
            layout_dirty: false,
            effects: None,
        });

        Self {
            nodes,
            root,
            focus: None,
            exit_requested: None,
            pending_style: None,
            mouse_capture: None,
            focus_hint: None,
            tree_edit: None,
            rolling_back_tree_edit: false,
            commands: CommandSet::new(),
            command_scope: Vec::new(),
            pending_help_request: None,
            pending_help_snapshot: None,
            pending_help_snapshot_observed: Cell::new(false),
            pending_diagnostic_dump: None,
        }
    }

    /// Mark the pending help snapshot as observed during render.
    pub(crate) fn mark_help_snapshot_observed(&self) {
        self.pending_help_snapshot_observed.set(true);
    }

    /// Take and clear the observed flag for a pending help snapshot.
    pub(crate) fn take_help_snapshot_observed(&self) -> bool {
        self.pending_help_snapshot_observed.replace(false)
    }

    /// Request a cooperative exit with the provided status code.
    pub(crate) fn request_exit(&mut self, code: i32) {
        if self.exit_requested.is_none() {
            self.exit_requested = Some(code);
        }
    }

    /// Take the pending exit request, if any.
    pub(crate) fn take_exit_request(&mut self) -> Option<i32> {
        self.exit_requested.take()
    }

    /// Request a diagnostic dump for a target node.
    pub(crate) fn request_diagnostic_dump(&mut self, target: NodeId) {
        self.pending_diagnostic_dump = Some(target);
    }

    /// Take and clear any pending diagnostic dump request.
    pub(crate) fn take_diagnostic_dump_request(&mut self) -> Option<NodeId> {
        self.pending_diagnostic_dump.take()
    }

    /// Return the current command-scope frame, if any.
    pub(crate) fn current_command_scope(&self) -> Option<&CommandScopeFrame> {
        self.command_scope.last()
    }

    /// Push a command-scope frame and return the previous depth.
    pub(crate) fn push_command_scope(&mut self, frame: CommandScopeFrame) -> usize {
        let depth = self.command_scope.len();
        self.command_scope.push(frame);
        depth
    }

    /// Restore the command-scope stack to a previous depth.
    pub(crate) fn pop_command_scope(&mut self, depth: usize) {
        self.command_scope.truncate(depth);
    }

    /// Return the root node id.
    pub fn root_id(&self) -> NodeId {
        self.root
    }

    /// Return the currently focused node id, if any.
    pub fn focus_id(&self) -> Option<NodeId> {
        self.focus
    }

    /// Return a reference to a node by id.
    pub fn node(&self, node_id: impl Into<NodeId>) -> Option<&Node> {
        self.nodes.get(node_id.into())
    }

    /// Take a mutable reference to a widget for a single call.
    pub(crate) fn with_widget_mut<R>(
        &mut self,
        node_id: NodeId,
        f: impl FnOnce(&mut dyn Widget, &mut Self) -> R,
    ) -> Result<R> {
        let mut guard = WidgetSlotGuard::take(self, node_id).map_err(|error| {
            self.widget_operation_error(
                WidgetOperation::access("mutation callback"),
                node_id,
                error,
            )
        })?;
        Ok(f(guard.widget_mut(), self))
    }

    /// Borrow a widget immutably for a read-only core query.
    pub(crate) fn with_widget_read<R>(
        &self,
        node_id: NodeId,
        operation: WidgetOperation,
        f: impl FnOnce(&dyn Widget, &Self) -> R,
    ) -> Result<R> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or(Error::NodeNotFound(node_id))
            .map_err(|error| self.widget_operation_error(operation, node_id, error))?;
        let guard = WidgetReadGuard::borrow(node_id, node)
            .map_err(|error| self.widget_operation_error(operation, node_id, error))?;
        Ok(f(guard.widget(), self))
    }

    /// Borrow a widget mutably for rendering with a shared Core context.
    pub(crate) fn with_widget_render<R>(
        &self,
        node_id: NodeId,
        f: impl FnOnce(&mut dyn Widget, &Self) -> R,
    ) -> Result<R> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or(Error::NodeNotFound(node_id))
            .map_err(|error| {
                self.widget_operation_error(
                    WidgetOperation::render("render access"),
                    node_id,
                    error,
                )
            })?;
        let mut guard = WidgetMutGuard::borrow(node_id, node).map_err(|error| {
            self.widget_operation_error(WidgetOperation::render("render access"), node_id, error)
        })?;
        Ok(f(guard.widget_mut(), self))
    }

    /// Attach node and operation context to a widget operation failure.
    pub(crate) fn widget_operation_error(
        &self,
        operation: WidgetOperation,
        node_id: NodeId,
        source: Error,
    ) -> Error {
        let kind = match operation.kind {
            WidgetOperationKind::Access => NodeOperationKind::Access,
            WidgetOperationKind::Layout => NodeOperationKind::Layout,
            WidgetOperationKind::Render => NodeOperationKind::Render,
        };
        Error::NodeOperation {
            kind,
            operation: operation.name,
            node: node_id,
            path: self.node_path_label(node_id),
            source: Box::new(source),
        }
    }

    /// Return a path label suitable for diagnostics.
    fn node_path_label(&self, node_id: NodeId) -> String {
        if !self.nodes.contains_key(node_id) {
            return "<missing>".into();
        }
        let path = self.node_path(self.root, node_id).to_string();
        if path == "/" && node_id != self.root {
            "<detached>".into()
        } else {
            path
        }
    }
}

#[derive(Default)]
/// Root widget container used for the implicit root node.
struct RootContainer;

impl Widget for RootContainer {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, _frame: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("root")
    }
}
