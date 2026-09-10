#![expect(
    clippy::multiple_inherent_impl,
    reason = "Core methods are split by arena, layout, and dispatch concerns."
)]

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use self::focus::FocusRecoveryHint;
use super::{
    inputmap::InputMap,
    wake::WakeRegistry,
    widget_access::{WidgetMutGuard, WidgetReadGuard, WidgetSlotGuard},
};
use crate::{
    ChangeOutcome,
    commands::{CommandScopeFrame, CommandSet},
    core::{
        context::CoreContext,
        id::{NodeArena, NodeId},
        node::Node,
        path::Path,
    },
    error::{Error, NodeOperationKind, Result},
    layout::Layout,
    state::NodeName,
    style::StyleMap,
    widget::Widget,
};

#[cfg(test)]
mod change_tests;
/// Event dispatch and bubbling helpers.
mod dispatch;
/// Focus and mouse-capture management.
mod focus;
/// Modal interaction scopes and retirement.
pub mod interaction;
/// Layout traversal, measurement, and hit-testing.
pub mod layout_driver;
/// Scoped application identity and index invariants.
mod semantic;
/// Removal requests completed after callback restoration.
mod teardown;
/// Widgets and node builders shared by the world test modules.
#[cfg(test)]
pub mod test_support;
#[cfg(test)]
mod tests;
/// Arena mutation, structural invariants, and path helpers.
mod tree;

/// Core state for the arena, layout engine, and focus.
pub struct Core {
    /// Changes awaiting frame publication.
    pub(crate) changes: crate::ChangeSet,
    /// Node storage arena.
    pub(crate) nodes: NodeArena<Node>,
    /// Application keys unique within explicit arena scopes.
    semantic_keys: HashMap<(NodeId, String), NodeId>,
    /// Runtime-owned modal interaction scopes.
    interaction: interaction::InteractionState,
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
    /// Monotonic widget and attachment generation source, outside rollback.
    next_generation: u64,
    /// Widget slots currently extracted by mutation callbacks.
    pub(crate) callback_depth: usize,
    /// Completion-boundary removal queue and dispatch nesting.
    completion: teardown::CompletionBatch,
    /// Cross-thread work handles synchronized with committed structural state.
    pub(crate) wake_registry: WakeRegistry,
    /// Whether lifecycle cleanup is unwinding a failed tree edit.
    rolling_back_tree_edit: bool,
    /// Registered command specs.
    pub(crate) commands: CommandSet,
    /// Application and framework input bindings.
    pub(crate) input_map: InputMap,
    /// Command scope stack for injection.
    command_scope: Vec<CommandScopeFrame>,
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
    widget: Rc<RefCell<Option<Box<dyn Widget>>>>,
}

/// Stable node and widget identities for one removal lifecycle.
struct RemovalPlan {
    /// Root whose lifecycle initiated the plan.
    root: NodeId,
    /// Nodes in deterministic pre-order with their original widget slots.
    pre_order: Vec<RemovalEntry>,
    /// Nodes in deterministic post-order.
    post_order: Vec<NodeId>,
}

/// One node expected to survive unchanged until a removal plan commits.
struct RemovalEntry {
    /// Planned node.
    node_id: NodeId,
    /// Widget slot validated before lifecycle hooks run.
    widget: Rc<RefCell<Option<Box<dyn Widget>>>>,
}

/// Core-owned state restored when a tree edit fails.
struct TreeStateSnapshot {
    /// Requests preceding this structural checkpoint.
    removal_checkpoint: usize,
    /// Arena contents and all node metadata.
    nodes: NodeArena<Node>,
    /// Scoped identities captured with arena metadata.
    semantic_keys: HashMap<(NodeId, String), NodeId>,
    /// Modal scopes captured with structural state.
    interaction: interaction::InteractionState,
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
    /// Pending diagnostic target.
    pending_diagnostic_dump: Option<NodeId>,
}

/// Widget operation whose failures should carry node context.
#[derive(Clone, Copy)]
pub struct WidgetOperation {
    /// Error category used for reporting.
    kind: NodeOperationKind,
    /// Short operation name.
    name: &'static str,
}

impl WidgetOperation {
    /// Construct a generic widget access operation.
    pub(crate) const fn access(name: &'static str) -> Self {
        Self {
            kind: NodeOperationKind::Access,
            name,
        }
    }

    /// Construct a layout-phase widget operation.
    pub(crate) const fn layout(name: &'static str) -> Self {
        Self {
            kind: NodeOperationKind::Layout,
            name,
        }
    }

    /// Construct a render-phase widget operation.
    pub(crate) const fn render(name: &'static str) -> Self {
        Self {
            kind: NodeOperationKind::Render,
            name,
        }
    }
}

impl Core {
    /// Create a new Core with a default root node.
    pub fn new() -> Self {
        let mut nodes = NodeArena::new();
        let mut root_node = Node::new(Box::new(RootContainer), 1);
        root_node.attachment_generation = Some(1);
        let root = nodes.insert(root_node);

        Self {
            changes: crate::ChangeSet::default(),
            nodes,
            semantic_keys: HashMap::new(),
            interaction: interaction::InteractionState::default(),
            root,
            focus: None,
            exit_requested: None,
            pending_style: None,
            mouse_capture: None,
            focus_hint: None,
            tree_edit: None,
            next_generation: 2,
            callback_depth: 0,
            completion: teardown::CompletionBatch::default(),
            wake_registry: WakeRegistry::default(),
            rolling_back_tree_edit: false,
            commands: CommandSet::default(),
            input_map: InputMap::new(),
            command_scope: Vec::new(),
            pending_diagnostic_dump: None,
        }
    }

    /// Request a cooperative exit with the provided status code.
    pub(crate) fn request_exit(&mut self, code: i32) {
        if self.exit_requested.is_none() {
            self.exit_requested = Some(code);
        }
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

    /// Take a mutable reference to a widget for a single call.
    pub(crate) fn with_widget_dyn_mut<R>(
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
        self.invalidate(crate::Invalidation::Layout);
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.layout_dirty = true;
        }
        self.callback_depth += 1;
        let result = f(guard.widget_mut(), self);
        drop(guard);
        self.callback_depth -= 1;
        Ok(result)
    }

    /// Borrow a widget mutably alongside a context bound to the same node.
    pub(crate) fn with_widget_ctx<R>(
        &mut self,
        node_id: NodeId,
        f: impl FnOnce(&mut dyn Widget, &mut CoreContext<'_>) -> R,
    ) -> Result<R> {
        self.with_widget_dyn_mut(node_id, |widget, core| {
            let mut ctx = CoreContext::new(core, node_id);
            f(widget, &mut ctx)
        })
    }

    /// Borrow a widget immutably for a read-only core query.
    pub(crate) fn with_widget<R>(
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
        Error::NodeOperation {
            kind: operation.kind,
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
        let path = self.path_of(self.root, node_id);
        if path == Path::empty() {
            "<detached>".into()
        } else {
            path.to_string()
        }
    }
}

/// Root widget container used for the implicit root node.
struct RootContainer;

impl Widget for RootContainer {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn name(&self) -> NodeName {
        NodeName::convert("root")
    }
}

impl Core {
    /// Record pending work. Flags survive callback and preparation errors.
    pub(crate) fn invalidate(&mut self, invalidation: crate::Invalidation) {
        self.changes.invalidate(invalidation);
    }
}
