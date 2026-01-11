use std::{
    any::TypeId,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    ptr::NonNull,
};

use slotmap::SlotMap;

use super::{
    focus::{FocusManager, FocusRecoveryHint},
    help::OwnedHelpSnapshot,
};
use crate::{
    ReadContext,
    backend::BackendControl,
    commands::{CommandScopeFrame, CommandSet},
    core::{context::CoreContext, id::NodeId, node::Node, view::View},
    error::{Error, Result},
    event::Event,
    geom::{Expanse, Point, Rect, RectI32},
    layout::{
        Align, CanvasChild, CanvasContext, Constraint, Direction as LayoutDirection, Display,
        Layout, MeasureConstraints, Measurement, Size, Sizing, max_bound,
    },
    path::Path,
    render::Render,
    state::NodeName,
    style::StyleMap,
    widget::{EventOutcome, Widget},
};

/// Core state for the arena, layout engine, and focus.
pub struct Core {
    /// Node storage arena.
    pub(crate) nodes: SlotMap<NodeId, Node>,
    /// Root node ID.
    pub(crate) root: NodeId,
    /// Currently focused node.
    pub(crate) focus: Option<NodeId>,
    /// Focus generation counter.
    pub(crate) focus_gen: u64,
    /// Active backend controller.
    pub(crate) backend: Option<Box<dyn BackendControl>>,
    /// Exit code requested by a widget or command, if any.
    pub(crate) exit_requested: Option<i32>,
    /// Pending style map to be applied before next render.
    pub(crate) pending_style: Option<StyleMap>,
    /// Node that captures mouse events regardless of cursor position.
    pub(crate) mouse_capture: Option<NodeId>,
    /// Focus recovery hint for the most recent structural removal.
    pub(crate) focus_hint: Option<FocusRecoveryHint>,
    /// Active structural transaction for rollback on failure.
    transaction: Option<MountTransaction>,
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

#[derive(Default)]
/// Transaction state used to roll back structural mutations.
struct MountTransaction {
    /// Nodes created during the transaction.
    created: Vec<NodeId>,
    /// Node structure snapshots captured for rollback.
    snapshots: HashMap<NodeId, NodeStructureSnapshot>,
    /// Nodes that were mounted during the transaction.
    mounted: Vec<NodeId>,
}

#[derive(Clone)]
/// Snapshot of a node's structural fields for rollback.
struct NodeStructureSnapshot {
    /// Stored parent pointer.
    parent: Option<NodeId>,
    /// Stored children list.
    children: Vec<NodeId>,
    /// Stored keyed child map.
    child_keys: HashMap<String, NodeId>,
}

/// Guard that restores a widget slot on drop.
struct WidgetSlotGuard {
    /// Core pointer used for restoration.
    core: NonNull<Core>,
    /// Node that owns the widget slot.
    node_id: NodeId,
    /// Temporarily held widget.
    widget: Option<Box<dyn Widget>>,
}

impl WidgetSlotGuard {
    /// Take ownership of the widget from its slot.
    fn new(core: &Core, node_id: NodeId) -> Result<Self> {
        let node = core
            .nodes
            .get(node_id)
            .ok_or(Error::NodeNotFound(node_id))?;
        let mut slot = node
            .widget
            .try_borrow_mut()
            .map_err(|_| Error::ReentrantWidgetBorrow(node_id))?;
        let widget = slot.take().ok_or(Error::ReentrantWidgetBorrow(node_id))?;
        Ok(Self {
            core: NonNull::from(core),
            node_id,
            widget: Some(widget),
        })
    }

    /// Borrow the widget mutably for the call.
    fn widget_mut(&mut self) -> &mut dyn Widget {
        self.widget
            .as_deref_mut()
            .expect("widget missing from guard")
    }
}

impl Drop for WidgetSlotGuard {
    fn drop(&mut self) {
        // SAFETY: core pointer remains valid for the lifetime of the guard.
        unsafe {
            let core = self.core.as_ref();
            if let Some(node) = core.nodes.get(self.node_id)
                && let Ok(mut slot) = node.widget.try_borrow_mut()
                && slot.is_none()
            {
                *slot = self.widget.take();
            }
        }
    }
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
            widget: RefCell::new(Some(Box::new(root_widget))),
            widget_type: root_type,
            parent: None,
            children: Vec::new(),
            child_keys: HashMap::new(),
            layout,
            rect: Rect::zero(),
            content_size: Expanse::default(),
            canvas: Expanse::default(),
            scroll: Point::zero(),
            view: View::default(),
            hidden: false,
            name: root_name,
            initialized: false,
            mounted: false,
            layout_dirty: false,
            effects: None,
            clear_inherited_effects: false,
        });

        Self {
            nodes,
            root,
            focus: None,
            focus_gen: 1,
            backend: None,
            exit_requested: None,
            pending_style: None,
            mouse_capture: None,
            focus_hint: None,
            transaction: None,
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

    /// Return the focus generation counter.
    pub fn focus_generation(&self) -> u64 {
        self.focus_gen
    }

    /// Return a reference to a node by id.
    pub fn node(&self, node_id: impl Into<NodeId>) -> Option<&Node> {
        self.nodes.get(node_id.into())
    }

    /// Add a boxed widget to the arena and return its node ID.
    fn add_boxed(&mut self, widget: Box<dyn Widget>) -> NodeId {
        let layout = widget.layout();
        let name = widget.name();
        let widget_type = widget.as_ref().type_id();

        let node_id = self.nodes.insert(Node {
            widget: RefCell::new(Some(widget)),
            widget_type,
            parent: None,
            children: Vec::new(),
            child_keys: HashMap::new(),
            layout,
            rect: Rect::zero(),
            content_size: Expanse::default(),
            canvas: Expanse::default(),
            scroll: Point::zero(),
            view: View::default(),
            hidden: false,
            name,
            initialized: false,
            mounted: false,
            layout_dirty: false,
            effects: None,
            clear_inherited_effects: false,
        });
        self.record_created(node_id);
        node_id
    }

    /// Update the layout for a node.
    pub fn with_layout_of(
        &mut self,
        node: impl Into<NodeId>,
        f: impl FnOnce(&mut Layout),
    ) -> Result<()> {
        let node = node.into();
        let node_ref = self
            .nodes
            .get(node)
            .ok_or_else(|| Error::Internal("missing node".into()))?;
        let mut layout = node_ref.layout;
        f(&mut layout);
        if let Some(node) = self.nodes.get_mut(node) {
            node.layout = layout;
        }
        Ok(())
    }

    /// Set the layout for a node.
    pub fn set_layout_of(&mut self, node: impl Into<NodeId>, layout: Layout) -> Result<()> {
        self.with_layout_of(node, |l| *l = layout)
    }

    /// Replace the widget stored at a node.
    pub fn replace_widget_keep_children<W>(
        &mut self,
        node_id: impl Into<NodeId>,
        widget: W,
    ) -> Result<()>
    where
        W: Widget + 'static,
    {
        let node_id = node_id.into();
        let name = widget.name();
        let layout = widget.layout();
        let widget_type = TypeId::of::<W>();
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or(Error::NodeNotFound(node_id))?;
        node.widget = RefCell::new(Some(Box::new(widget)));
        node.name = name;
        node.layout = layout;
        node.widget_type = widget_type;
        node.mounted = false;
        node.initialized = false;
        Ok(())
    }

    /// Replace a widget and remove all descendant nodes.
    pub fn replace_subtree<W>(&mut self, node_id: impl Into<NodeId>, widget: W) -> Result<()>
    where
        W: Widget + 'static,
    {
        let node_id = node_id.into();
        let children = self
            .nodes
            .get(node_id)
            .ok_or(Error::NodeNotFound(node_id))?
            .children
            .clone();
        for child in children {
            self.remove_subtree(child)?;
        }
        self.replace_widget_keep_children(node_id, widget)
    }

    /// Run the mount hook for a node if it has not been mounted yet.
    pub(crate) fn mount_node(&mut self, node_id: NodeId) -> Result<()> {
        let should_mount = self
            .nodes
            .get(node_id)
            .map(|node| !node.mounted)
            .unwrap_or(false);
        if !should_mount {
            return Ok(());
        }

        self.with_widget_mut(node_id, |widget, core| {
            let mut ctx = CoreContext::new(core, node_id);
            widget.on_mount(&mut ctx)
        })??;

        if let Some(node) = self.nodes.get_mut(node_id) {
            node.mounted = true;
        }
        if let Some(tx) = self.transaction.as_mut() {
            tx.mounted.push(node_id);
        }

        Ok(())
    }

    /// Run a structural mutation transaction, rolling back on error.
    fn with_transaction<R>(&mut self, f: impl FnOnce(&mut Self) -> Result<R>) -> Result<R> {
        if self.transaction.is_some() {
            return f(self);
        }

        self.transaction = Some(MountTransaction::default());
        let result = f(self);
        let transaction = self.transaction.take().expect("transaction missing");

        match result {
            Ok(value) => Ok(value),
            Err(err) => {
                self.rollback_transaction(&transaction);
                Err(err)
            }
        }
    }

    /// Record a node created during the active transaction.
    fn record_created(&mut self, node_id: NodeId) {
        if let Some(tx) = self.transaction.as_mut() {
            tx.created.push(node_id);
        }
    }

    /// Snapshot the structure of a node if it hasn't been recorded yet.
    fn record_snapshot(&mut self, node_id: NodeId) {
        let Some(tx) = self.transaction.as_mut() else {
            return;
        };
        if tx.snapshots.contains_key(&node_id) {
            return;
        }
        let Some(node) = self.nodes.get(node_id) else {
            return;
        };
        tx.snapshots.insert(
            node_id,
            NodeStructureSnapshot {
                parent: node.parent,
                children: node.children.clone(),
                child_keys: node.child_keys.clone(),
            },
        );
    }

    /// Restore node structure and cleanup after a failed transaction.
    fn rollback_transaction(&mut self, tx: &MountTransaction) {
        self.run_unmount_for_created(&tx.created);
        self.restore_snapshots(&tx.snapshots);
        self.restore_mount_flags(&tx.mounted, &tx.created);
        self.remove_created_nodes(&tx.created);
    }

    /// Restore parent/child relationships from snapshots.
    fn restore_snapshots(&mut self, snapshots: &HashMap<NodeId, NodeStructureSnapshot>) {
        for (node_id, snapshot) in snapshots {
            let Some(node) = self.nodes.get_mut(*node_id) else {
                continue;
            };
            node.parent = snapshot.parent;
            node.children = snapshot.children.clone();
            node.child_keys = snapshot.child_keys.clone();
        }
    }

    /// Reset mounted flags for nodes mounted during a failed transaction.
    fn restore_mount_flags(&mut self, mounted: &[NodeId], created: &[NodeId]) {
        for node_id in mounted {
            if created.contains(node_id) {
                continue;
            }
            if let Some(node) = self.nodes.get_mut(*node_id) {
                node.mounted = false;
            }
        }
    }

    /// Remove nodes created during a failed transaction.
    fn remove_created_nodes(&mut self, created: &[NodeId]) {
        for node_id in created {
            self.nodes.remove(*node_id);
        }
    }

    /// Run unmount hooks for nodes created during a failed transaction.
    fn run_unmount_for_created(&mut self, created: &[NodeId]) {
        if created.is_empty() {
            return;
        }

        let created_set: HashSet<NodeId> = created.iter().copied().collect();
        let mut roots = Vec::new();
        for node_id in created {
            let parent = self.nodes.get(*node_id).and_then(|node| node.parent);
            let parent_created = parent.is_some_and(|id| created_set.contains(&id));
            if !parent_created {
                roots.push(*node_id);
            }
        }

        for root in roots {
            let order = self.post_order_filtered(root, &created_set);
            for node_id in order {
                if !self.nodes.contains_key(node_id) {
                    continue;
                }
                let _ignored = self.with_widget_mut(node_id, |widget, core| {
                    let mut ctx = CoreContext::new(core, node_id);
                    widget.on_unmount(&mut ctx);
                });
            }
        }
    }

    /// Return a post-order traversal restricted to nodes in `allowed`.
    fn post_order_filtered(&self, root: NodeId, filter: &HashSet<NodeId>) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![(root, false)];
        while let Some((node_id, visited)) = stack.pop() {
            if !filter.contains(&node_id) {
                continue;
            }
            if visited {
                out.push(node_id);
                continue;
            }
            stack.push((node_id, true));
            if let Some(node) = self.nodes.get(node_id) {
                for child in node.children.iter().rev() {
                    if filter.contains(child) {
                        stack.push((*child, false));
                    }
                }
            }
        }
        out
    }

    /// Return true if `ancestor` appears in the parent chain of `node`.
    fn is_ancestor(&self, ancestor: NodeId, node: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.nodes.get(id).and_then(|n| n.parent);
        }
        false
    }

    /// Return true if `node_id` is attached to the root.
    pub fn is_attached_to_root(&self, node_id: impl Into<NodeId>) -> bool {
        let mut current = Some(node_id.into());
        while let Some(id) = current {
            if id == self.root {
                return true;
            }
            current = self.nodes.get(id).and_then(|n| n.parent);
        }
        false
    }

    /// Assert structural invariants on the node tree in debug builds.
    #[cfg(debug_assertions)]
    pub(crate) fn debug_assert_tree_invariants(&self) {
        self.debug_assert_root();
        for (id, node) in self.nodes.iter() {
            self.debug_assert_node_links(id, node);
            self.debug_assert_no_cycle(id);
        }
        self.debug_assert_focus();
    }

    #[cfg(not(debug_assertions))]
    pub(crate) fn debug_assert_tree_invariants(&self) {}

    /// Assert root node invariants in debug builds.
    #[cfg(debug_assertions)]
    fn debug_assert_root(&self) {
        debug_assert!(self.nodes.contains_key(self.root), "root node missing");
        if let Some(root) = self.nodes.get(self.root) {
            debug_assert!(root.parent.is_none(), "root has parent");
        }
    }

    /// Assert parent/child link invariants for a specific node in debug builds.
    #[cfg(debug_assertions)]
    fn debug_assert_node_links(&self, id: NodeId, node: &Node) {
        let mut seen = HashSet::with_capacity(node.children.len());
        for child in &node.children {
            debug_assert!(
                seen.insert(*child),
                "duplicate child {child:?} under {id:?}"
            );
            let child_node = self.nodes.get(*child);
            debug_assert!(child_node.is_some(), "child {child:?} missing");
            if let Some(child_node) = child_node {
                debug_assert!(
                    child_node.parent == Some(id),
                    "child {child:?} parent mismatch under {id:?}"
                );
            }
        }

        for (key, child) in &node.child_keys {
            debug_assert!(
                node.children.contains(child),
                "child key {key} points to non-child {child:?} under {id:?}"
            );
            let child_node = self.nodes.get(*child);
            debug_assert!(child_node.is_some(), "child {child:?} missing");
            if let Some(child_node) = child_node {
                debug_assert!(
                    child_node.parent == Some(id),
                    "child {child:?} parent mismatch for key {key}"
                );
            }
        }

        if let Some(parent) = node.parent {
            let parent_node = self.nodes.get(parent);
            debug_assert!(parent_node.is_some(), "parent {parent:?} missing");
            if let Some(parent_node) = parent_node {
                debug_assert!(
                    parent_node.children.contains(&id),
                    "parent {parent:?} missing child {id:?}"
                );
            }
        }
    }

    /// Assert focus invariants in debug builds.
    #[cfg(debug_assertions)]
    fn debug_assert_focus(&self) {
        if let Some(focus) = self.focus {
            debug_assert!(
                self.nodes.contains_key(focus),
                "focus points at missing node {focus:?}"
            );
            debug_assert!(
                self.attached_to_root_debug(focus),
                "focus points at detached node {focus:?}"
            );
        }
    }

    /// Assert that the parent chain for a node contains no cycles.
    #[cfg(debug_assertions)]
    fn debug_assert_no_cycle(&self, start: NodeId) {
        debug_assert!(
            !self.parent_chain_has_cycle(start),
            "cycle detected from {start:?}"
        );
    }

    /// Return true if a node's parent chain contains a cycle.
    #[cfg(debug_assertions)]
    fn parent_chain_has_cycle(&self, start: NodeId) -> bool {
        let mut seen = HashSet::new();
        let mut current = Some(start);
        while let Some(id) = current {
            if !seen.insert(id) {
                return true;
            }
            current = self.nodes.get(id).and_then(|n| n.parent);
        }
        false
    }

    /// Return true if a node reaches the root without cycles.
    #[cfg(debug_assertions)]
    fn attached_to_root_debug(&self, start: NodeId) -> bool {
        let mut seen = HashSet::new();
        let mut current = Some(start);
        while let Some(id) = current {
            if id == self.root {
                return true;
            }
            if !seen.insert(id) {
                return false;
            }
            current = self.nodes.get(id).and_then(|n| n.parent);
        }
        false
    }

    /// Create a node in the arena detached from the tree.
    pub fn create_detached<W>(&mut self, widget: W) -> NodeId
    where
        W: Widget + 'static,
    {
        self.add_boxed(Box::new(widget))
    }

    /// Create a node in the arena detached from the tree using a boxed widget.
    pub fn create_detached_boxed(&mut self, widget: Box<dyn Widget>) -> NodeId {
        self.add_boxed(widget)
    }

    /// Add a boxed widget as a child of a specific parent and return the new node ID.
    pub fn add_child_to_boxed(
        &mut self,
        parent: impl Into<NodeId>,
        widget: Box<dyn Widget>,
    ) -> Result<NodeId> {
        let parent = parent.into();
        self.with_transaction(|core| {
            let child = core.create_detached_boxed(widget);
            core.attach(parent, child)?;
            Ok(child)
        })
    }

    /// Add a boxed widget as a keyed child of a specific parent and return the new node ID.
    pub fn add_child_to_keyed_boxed(
        &mut self,
        parent: impl Into<NodeId>,
        key: &str,
        widget: Box<dyn Widget>,
    ) -> Result<NodeId> {
        let parent = parent.into();
        self.with_transaction(|core| {
            if core.child_keyed(parent, key).is_some() {
                return Err(Error::DuplicateChildKey(key.to_string()));
            }
            let child = core.create_detached_boxed(widget);
            core.attach_keyed(parent, key, child)?;
            Ok(child)
        })
    }

    /// Return the keyed child under a parent.
    pub fn child_keyed(&self, parent: impl Into<NodeId>, key: &str) -> Option<NodeId> {
        self.nodes
            .get(parent.into())
            .and_then(|node| node.child_keys.get(key).copied())
    }

    /// Attach a detached child under a parent.
    pub fn attach(&mut self, parent: impl Into<NodeId>, child: impl Into<NodeId>) -> Result<()> {
        let parent = parent.into();
        let child = child.into();
        self.with_transaction(|core| core.attach_inner(parent, child, None))
    }

    /// Attach a detached child under a parent with a unique key.
    pub fn attach_keyed(
        &mut self,
        parent: impl Into<NodeId>,
        key: &str,
        child: impl Into<NodeId>,
    ) -> Result<()> {
        let parent = parent.into();
        let child = child.into();
        self.with_transaction(|core| core.attach_inner(parent, child, Some(key)))
    }

    /// Attach a child under a parent, optionally tracking a keyed association.
    fn attach_inner(&mut self, parent: NodeId, child: NodeId, key: Option<&str>) -> Result<()> {
        if !self.nodes.contains_key(parent) {
            return Err(Error::NodeNotFound(parent));
        }
        if !self.nodes.contains_key(child) {
            return Err(Error::NodeNotFound(child));
        }
        let child_parent = self.nodes.get(child).and_then(|node| node.parent);
        if child_parent.is_some() {
            return Err(Error::AlreadyAttached(child));
        }
        if parent == child || self.is_ancestor(child, parent) {
            return Err(Error::WouldCreateCycle { parent, child });
        }
        if let Some(key) = key
            && self
                .nodes
                .get(parent)
                .is_some_and(|node| node.child_keys.contains_key(key))
        {
            return Err(Error::DuplicateChildKey(key.to_string()));
        }

        self.record_snapshot(parent);
        self.record_snapshot(child);

        if let Some(key) = key
            && let Some(node) = self.nodes.get_mut(parent)
        {
            node.child_keys.insert(key.to_string(), child);
        }

        if let Some(node) = self.nodes.get_mut(child) {
            node.parent = Some(parent);
        }
        if let Some(node) = self.nodes.get_mut(parent) {
            node.children.push(child);
        }

        if self.is_attached_to_root(parent) {
            self.mount_subtree_pre_order(child)?;
        }

        self.ensure_invariants(None);
        Ok(())
    }

    /// Detach a child from its parent if attached.
    pub fn detach(&mut self, child: impl Into<NodeId>) -> Result<()> {
        let child = child.into();
        if !self.nodes.contains_key(child) {
            return Err(Error::NodeNotFound(child));
        }

        let parent = self.nodes.get(child).and_then(|node| node.parent);
        let hint = parent
            .filter(|_| self.is_attached_to_root(child))
            .map(|_| self.focus_recovery_hint(child));

        self.with_transaction(|core| {
            let Some(parent) = parent else {
                return Ok(());
            };
            core.record_snapshot(parent);
            core.record_snapshot(child);
            if let Some(node) = core.nodes.get_mut(parent) {
                node.children.retain(|id| *id != child);
                node.child_keys.retain(|_, id| *id != child);
            }
            if let Some(node) = core.nodes.get_mut(child) {
                node.parent = None;
            }
            Ok(())
        })?;

        self.focus_hint = hint;
        self.ensure_invariants(Some(child));
        Ok(())
    }

    /// Mount unmounted nodes in a subtree using pre-order traversal.
    fn mount_subtree_pre_order(&mut self, root: NodeId) -> Result<()> {
        let mut stack = vec![root];
        while let Some(node_id) = stack.pop() {
            if !self.nodes.contains_key(node_id) {
                continue;
            }
            let should_mount = self.nodes.get(node_id).is_some_and(|node| !node.mounted);
            if should_mount {
                self.mount_node(node_id)?;
            }
            let children = self
                .nodes
                .get(node_id)
                .map(|node| node.children.clone())
                .unwrap_or_default();
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }
        Ok(())
    }

    /// Retain only keyed mappings that still point to direct children.
    fn retain_child_keys(&mut self, parent: NodeId) {
        let Some(node) = self.nodes.get_mut(parent) else {
            return;
        };
        let keep: HashSet<NodeId> = node.children.iter().copied().collect();
        node.child_keys.retain(|_, id| keep.contains(id));
    }

    /// Replace the children list for a parent in the arena tree.
    pub fn set_children(&mut self, parent: impl Into<NodeId>, children: Vec<NodeId>) -> Result<()> {
        let parent = parent.into();
        if !self.nodes.contains_key(parent) {
            return Err(Error::NodeNotFound(parent));
        }

        let mut seen = HashSet::with_capacity(children.len());
        for child in &children {
            if !seen.insert(*child) {
                return Err(Error::DuplicateChild {
                    parent,
                    child: *child,
                });
            }
        }

        for child in &children {
            if *child == parent || self.is_ancestor(*child, parent) {
                return Err(Error::WouldCreateCycle {
                    parent,
                    child: *child,
                });
            }
            if !self.nodes.contains_key(*child) {
                return Err(Error::NodeNotFound(*child));
            }
        }

        let parent_attached = self.is_attached_to_root(parent);
        self.record_snapshot(parent);

        for child in &children {
            let old_parent = self.nodes.get(*child).and_then(|n| n.parent);
            if let Some(old_parent) = old_parent
                && old_parent != parent
            {
                self.record_snapshot(old_parent);
                self.record_snapshot(*child);
                if let Some(node) = self.nodes.get_mut(old_parent) {
                    node.children.retain(|id| *id != *child);
                    node.child_keys.retain(|_, id| *id != *child);
                }
                if let Some(node) = self.nodes.get_mut(*child) {
                    node.parent = None;
                }
            }
        }

        let old_children = self.nodes[parent].children.clone();
        for child in old_children {
            self.record_snapshot(child);
            if let Some(node) = self.nodes.get_mut(child) {
                node.parent = None;
            }
        }

        for child in &children {
            self.record_snapshot(*child);
            if let Some(node) = self.nodes.get_mut(*child) {
                node.parent = Some(parent);
            }
        }

        self.nodes[parent].children = children;
        self.retain_child_keys(parent);

        let new_children = self.nodes[parent].children.clone();
        if parent_attached {
            for child in new_children {
                self.mount_subtree_pre_order(child)?;
            }
        }

        self.ensure_invariants(None);
        Ok(())
    }

    /// Remove a node and all descendants from the arena.
    pub fn remove_subtree(&mut self, root_id: impl Into<NodeId>) -> Result<()> {
        let root_id = root_id.into();
        if root_id == self.root {
            return Err(Error::InvalidOperation("cannot remove root".into()));
        }
        if !self.nodes.contains_key(root_id) {
            return Err(Error::NodeNotFound(root_id));
        }

        let hint = if self.is_attached_to_root(root_id) {
            Some(self.focus_recovery_hint(root_id))
        } else {
            None
        };

        let pre_order = self.subtree_pre_order(root_id);
        for node_id in &pre_order {
            self.with_widget_mut(*node_id, |widget, core| {
                let mut ctx = CoreContext::new(core, *node_id);
                widget.pre_remove(&mut ctx)
            })??;
        }

        let post_order = self.subtree_post_order(root_id);
        for node_id in &post_order {
            self.with_widget_mut(*node_id, |widget, core| {
                let mut ctx = CoreContext::new(core, *node_id);
                widget.on_unmount(&mut ctx);
            })?;
        }

        let parent = self.nodes.get(root_id).and_then(|node| node.parent);
        if let Some(parent) = parent
            && let Some(node) = self.nodes.get_mut(parent)
        {
            node.children.retain(|id| *id != root_id);
            node.child_keys.retain(|_, id| *id != root_id);
        }

        for node_id in &post_order {
            self.nodes.remove(*node_id);
        }

        self.focus_hint = hint;
        self.ensure_invariants(Some(root_id));
        Ok(())
    }

    /// Collect a subtree in pre-order, including the root.
    fn subtree_pre_order(&self, root: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(node_id) = stack.pop() {
            let Some(node) = self.nodes.get(node_id) else {
                continue;
            };
            out.push(node_id);
            for child in node.children.iter().rev() {
                stack.push(*child);
            }
        }
        out
    }

    /// Collect a subtree in post-order, including the root.
    fn subtree_post_order(&self, root: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![(root, false)];
        while let Some((node_id, visited)) = stack.pop() {
            let Some(node) = self.nodes.get(node_id) else {
                continue;
            };
            if visited {
                out.push(node_id);
                continue;
            }
            stack.push((node_id, true));
            for child in node.children.iter().rev() {
                stack.push((*child, false));
            }
        }
        out
    }

    /// Set a node's hidden flag. Returns `true` if visibility changed.
    pub fn set_hidden(&mut self, node_id: impl Into<NodeId>, hidden: bool) -> bool {
        let node_id = node_id.into();
        let Some(node) = self.nodes.get_mut(node_id) else {
            return false;
        };
        let changed = node.hidden != hidden;
        node.hidden = hidden;
        if changed {
            self.ensure_invariants(None);
        }
        changed
    }

    /// Hide a node. Returns `true` if visibility changed.
    pub fn hide(&mut self, node_id: impl Into<NodeId>) -> bool {
        self.set_hidden(node_id, true)
    }

    /// Show a node. Returns `true` if visibility changed.
    pub fn show(&mut self, node_id: impl Into<NodeId>) -> bool {
        self.set_hidden(node_id, false)
    }

    /// Run layout computation and synchronize views.
    pub fn update_layout(&mut self, screen_size: Expanse) -> Result<()> {
        refresh_layouts(self);
        let root = self.root;
        let mut pass = LayoutPass::new(self);
        pass.layout_node(root, screen_size, Point::zero(), Overflow::none());
        let screen_view = View::new(
            RectI32::new(0, 0, screen_size.w, screen_size.h),
            RectI32::new(0, 0, screen_size.w, screen_size.h),
            Point::zero(),
            screen_size,
        );
        pass.update_views(root, screen_view);

        self.ensure_focus_valid(None);

        Ok(())
    }

    /// Take a mutable reference to a widget for a single call.
    pub(crate) fn with_widget_mut<R>(
        &mut self,
        node_id: NodeId,
        f: impl FnOnce(&mut dyn Widget, &mut Self) -> R,
    ) -> Result<R> {
        let mut guard = WidgetSlotGuard::new(self, node_id)?;
        Ok(f(guard.widget_mut(), self))
    }

    /// Take a mutable reference to a widget for rendering with a shared Core context.
    pub(crate) fn with_widget_view<R>(
        &self,
        node_id: NodeId,
        f: impl FnOnce(&mut dyn Widget, &Self) -> R,
    ) -> Result<R> {
        let mut guard = WidgetSlotGuard::new(self, node_id)?;
        Ok(f(guard.widget_mut(), self))
    }

    /// Build a command-scope frame for a specific event.
    pub(crate) fn command_scope_for_event(&self, event: &Event) -> CommandScopeFrame {
        let mut frame = self.current_command_scope().cloned().unwrap_or_default();
        frame.event = Some(event.clone());
        frame.mouse = match event {
            Event::Mouse(mouse) => Some(*mouse),
            _ => None,
        };
        frame
    }

    /// Dispatch an event to a node, bubbling to parents if unhandled.
    pub fn dispatch_event(
        &mut self,
        start: impl Into<NodeId>,
        event: &Event,
    ) -> Result<EventOutcome> {
        let start = start.into();
        let depth = self.push_command_scope(self.command_scope_for_event(event));
        let outcome = self.dispatch_event_inner(start, event);
        self.pop_command_scope(depth);
        outcome
    }

    /// Dispatch an event to a node and bubble until handled.
    fn dispatch_event_inner(&mut self, start: NodeId, event: &Event) -> Result<EventOutcome> {
        let mut target = Some(start);
        while let Some(id) = target {
            let outcome = self.with_widget_mut(id, |w, core| {
                let mut ctx = CoreContext::new(core, id);
                w.on_event(event, &mut ctx)
            })??;
            match outcome {
                EventOutcome::Handle | EventOutcome::Consume => return Ok(outcome),
                EventOutcome::Ignore => {
                    target = self.nodes[id].parent;
                }
            }
        }
        Ok(EventOutcome::Ignore)
    }

    /// Dispatch an event to a single node without bubbling.
    pub fn dispatch_event_on_node(
        &mut self,
        node_id: impl Into<NodeId>,
        event: &Event,
    ) -> Result<EventOutcome> {
        let node_id = node_id.into();
        let depth = self.push_command_scope(self.command_scope_for_event(event));
        let outcome = self.with_widget_mut(node_id, |w, core| {
            let mut ctx = CoreContext::new(core, node_id);
            w.on_event(event, &mut ctx)
        });
        self.pop_command_scope(depth);
        let outcome = outcome??;
        Ok(outcome)
    }

    /// Return the path for a node relative to a root.
    pub fn node_path(&self, root: impl Into<NodeId>, node_id: impl Into<NodeId>) -> Path {
        let root = root.into();
        let node_id = node_id.into();
        let mut parts = Vec::new();
        let mut current = Some(node_id);
        while let Some(id) = current {
            if let Some(node) = self.nodes.get(id) {
                parts.push(node.name.to_string());
                if id == root {
                    break;
                }
                current = node.parent;
            } else {
                break;
            }
        }
        if current != Some(root) {
            return Path::empty();
        }
        parts.reverse();
        Path::new(parts)
    }

    /// Locate the deepest node under a screen-space point.
    pub fn locate_node(&self, root: impl Into<NodeId>, point: Point) -> Result<Option<NodeId>> {
        let root = root.into();
        let root_view = self
            .nodes
            .get(root)
            .ok_or_else(|| Error::Internal("missing root node".into()))?
            .view;
        let clip = root_view
            .outer
            .intersect_rect(Rect::new(0, 0, root_view.outer.w, root_view.outer.h))
            .unwrap_or_else(Rect::zero);
        let mut result = None;
        locate_recursive(self, root, point, clip, &mut result)?;
        Ok(result)
    }
}

/// Refresh cached layout configurations for nodes marked dirty.
fn refresh_layouts(core: &mut Core) {
    for (_id, node) in core.nodes.iter_mut() {
        if !node.layout_dirty {
            continue;
        }
        if let Ok(widget) = node.widget.try_borrow()
            && let Some(widget) = widget.as_ref()
        {
            node.layout = widget.layout();
            node.layout_dirty = false;
        }
    }
}

#[derive(Hash, PartialEq, Eq)]
/// Cache key for per-pass measurements.
struct MeasureKey {
    /// Node being measured.
    node: NodeId,
    /// Constraints used for the measurement.
    constraints: MeasureConstraints,
}

/// Layout traversal with per-pass measurement caching.
struct LayoutPass<'a> {
    /// Core state being updated.
    core: &'a mut Core,
    /// Cached measurements for this pass.
    measure_cache: HashMap<MeasureKey, Measurement>,
}

#[derive(Clone, Copy)]
/// Overflow flags propagated from parent layouts.
struct Overflow {
    /// Allow horizontal overflow during measurement.
    x: bool,
    /// Allow vertical overflow during measurement.
    y: bool,
}

impl Overflow {
    /// Return a zero-overflow configuration.
    fn none() -> Self {
        Self { x: false, y: false }
    }

    /// Build overflow flags from a layout.
    fn from_layout(layout: Layout) -> Self {
        Self {
            x: layout.overflow_x,
            y: layout.overflow_y,
        }
    }
}

impl<'a> LayoutPass<'a> {
    /// Create a new layout pass with a fresh measurement cache.
    fn new(core: &'a mut Core) -> Self {
        Self {
            core,
            measure_cache: HashMap::new(),
        }
    }

    /// Lay out a node subtree and return its outer size.
    fn layout_node(
        &mut self,
        node_id: NodeId,
        available_outer: Expanse,
        position: Point,
        parent_overflow: Overflow,
    ) -> Size<u32> {
        let (layout, hidden) = self.node_layout_snapshot(node_id);
        if hidden || layout.display == Display::None {
            self.clear_layout(node_id, position);
            return Size::ZERO;
        }

        let mut effective_layout = layout;
        if parent_overflow.x {
            effective_layout.overflow_x = true;
        }
        if parent_overflow.y {
            effective_layout.overflow_y = true;
        }

        let outer = self.resolve_outer_size(node_id, effective_layout, available_outer);
        let pad_x = layout.padding.horizontal();
        let pad_y = layout.padding.vertical();
        let content_size = Size::new(
            outer.width.saturating_sub(pad_x),
            outer.height.saturating_sub(pad_y),
        );

        {
            let node = self.core.nodes.get_mut(node_id).expect("missing node");
            node.rect = Rect::new(position.x, position.y, outer.width, outer.height);
            node.content_size = content_size.into();
        }

        self.layout_children(node_id, effective_layout, content_size);

        let canvas = self.compute_canvas(node_id, content_size);
        self.update_canvas(node_id, content_size, canvas);

        outer
    }

    /// Update view rectangles for a subtree based on parent view data.
    fn update_views(&mut self, node_id: NodeId, parent_view: View) {
        let (layout, hidden, rect, content_size, canvas, scroll, children) = {
            let node = match self.core.nodes.get(node_id) {
                Some(node) => node,
                None => return,
            };
            (
                node.layout,
                node.hidden,
                node.rect,
                node.content_size,
                node.canvas,
                node.scroll,
                node.children.clone(),
            )
        };

        if hidden || layout.display == Display::None {
            if let Some(node) = self.core.nodes.get_mut(node_id) {
                node.view = View::default();
            }
            return;
        }

        let outer_x = parent_view.content.tl.x as i64 + rect.tl.x as i64 - parent_view.tl.x as i64;
        let outer_y = parent_view.content.tl.y as i64 + rect.tl.y as i64 - parent_view.tl.y as i64;

        let outer = RectI32::new(
            outer_x.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            outer_y.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            rect.w,
            rect.h,
        );

        let content_x = outer.tl.x as i64 + layout.padding.left as i64;
        let content_y = outer.tl.y as i64 + layout.padding.top as i64;
        let content = RectI32::new(
            content_x.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            content_y.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            content_size.w,
            content_size.h,
        );

        let view = View::new(outer, content, scroll, canvas);
        if let Some(node) = self.core.nodes.get_mut(node_id) {
            node.view = view;
        }

        for child in children {
            self.update_views(child, view);
        }
    }

    /// Resolve a node's outer size using its layout configuration.
    fn resolve_outer_size(
        &mut self,
        node_id: NodeId,
        layout: Layout,
        available_outer: Expanse,
    ) -> Size<u32> {
        self.resolve_outer_size_with_layout(node_id, layout, available_outer)
    }

    /// Resolve a node's outer size using an explicit layout snapshot.
    fn resolve_outer_size_with_layout(
        &mut self,
        node_id: NodeId,
        layout: Layout,
        available_outer: Expanse,
    ) -> Size<u32> {
        let available: Size<u32> = available_outer.into();
        let pad_x = layout.padding.horizontal();
        let pad_y = layout.padding.vertical();
        let available_content_w = available.width.saturating_sub(pad_x);
        let available_content_h = available.height.saturating_sub(pad_y);

        let c0 = MeasureConstraints {
            width: constraint_for_axis(
                layout.width,
                available_content_w,
                layout.min_width,
                layout.max_width,
                pad_x,
                layout.overflow_x,
            ),
            height: constraint_for_axis(
                layout.height,
                available_content_h,
                layout.min_height,
                layout.max_height,
                pad_y,
                layout.overflow_y,
            ),
        };

        let did_measure =
            matches!(layout.width, Sizing::Measure) || matches!(layout.height, Sizing::Measure);

        let mut measured_content = Size::ZERO;
        if did_measure {
            let m0 = self.measure_cached(node_id, c0);
            let raw0 = match m0 {
                Measurement::Fixed(content) => content,
                Measurement::Wrap => self.measure_wrap_content(node_id, layout, c0),
            };
            measured_content = c0.clamp_size(raw0);
        }

        let outer_w0 = match layout.width {
            Sizing::Flex(_) => available.width,
            Sizing::Measure => measured_content.width.saturating_add(pad_x),
        };
        let outer_h0 = match layout.height {
            Sizing::Flex(_) => available.height,
            Sizing::Measure => measured_content.height.saturating_add(pad_y),
        };

        let mut outer = Size::new(outer_w0, outer_h0);
        outer = clamp_outer(outer, layout);

        let mut content = Size::new(
            outer.width.saturating_sub(pad_x),
            outer.height.saturating_sub(pad_y),
        );

        if did_measure {
            let width_seen = match c0.width {
                Constraint::Exact(n) => n,
                Constraint::AtMost(_) | Constraint::Unbounded => measured_content.width,
            };

            if content.width != width_seen {
                let c1 = MeasureConstraints {
                    width: Constraint::Exact(content.width),
                    height: c0.height,
                };
                let m1 = self.measure_cached(node_id, c1);
                let raw1 = match m1 {
                    Measurement::Fixed(content) => content,
                    Measurement::Wrap => self.measure_wrap_content(node_id, layout, c1),
                };
                let content1 = c1.clamp_size(raw1);

                if matches!(layout.height, Sizing::Measure) {
                    let outer_h1 = content1.height.saturating_add(pad_y);
                    outer.height = outer_h1;
                    outer = clamp_outer(outer, layout);
                    content = Size::new(
                        outer.width.saturating_sub(pad_x),
                        outer.height.saturating_sub(pad_y),
                    );
                }
            }

            let c_final = MeasureConstraints {
                width: Constraint::Exact(content.width),
                height: Constraint::Exact(content.height),
            };
            let _ = self.measure_cached(node_id, c_final);
        }

        outer
    }

    /// Measure content size by wrapping children when requested.
    fn measure_wrap_content(
        &mut self,
        node_id: NodeId,
        layout: Layout,
        constraints: MeasureConstraints,
    ) -> Size<u32> {
        let children = self.visible_children(node_id);
        if children.is_empty() {
            return Size::ZERO;
        }

        // For Stack direction, content size is the max of all children
        if layout.direction == LayoutDirection::Stack {
            return self.measure_wrap_content_stack(layout, constraints, &children);
        }

        let main_fixed = constraints.main_is_exact(layout.direction);
        let cross_fixed = constraints.cross_is_exact(layout.direction);
        let avail_main = max_bound(constraints.main(layout.direction));
        let avail_cross = max_bound(constraints.cross(layout.direction));
        let avail = Size::from_main_cross(layout.direction, avail_main, avail_cross);

        let mut fixed_main_total = 0u32;
        let mut flex_children: Vec<(usize, u32)> = Vec::new();
        let mut child_sizes = vec![Size::ZERO; children.len()];

        for (i, child) in children.iter().enumerate() {
            let child_layout = self.node_layout_snapshot(*child).0;
            let mut effective = child_layout;

            let child_main = main_sizing(child_layout, layout.direction);
            if !main_fixed && matches!(child_main, Sizing::Flex(_)) {
                set_main_sizing(&mut effective, layout.direction, Sizing::Measure);
            }

            let child_cross = cross_sizing(child_layout, layout.direction);
            if !cross_fixed && matches!(child_cross, Sizing::Flex(_)) {
                set_cross_sizing(&mut effective, layout.direction, Sizing::Measure);
            }

            if layout.overflow_x {
                effective.overflow_x = true;
            }
            if layout.overflow_y {
                effective.overflow_y = true;
            }

            let eff_main = main_sizing(effective, layout.direction);
            if let Sizing::Flex(w) = eff_main {
                flex_children.push((i, w.max(1)));
                continue;
            }

            let size = self.resolve_outer_size_with_layout(*child, effective, avail.into());
            child_sizes[i] = size;
            fixed_main_total = fixed_main_total.saturating_add(size.main(layout.direction));
        }

        let gap_total = layout
            .gap
            .saturating_mul(children.len().saturating_sub(1) as u32);
        let remaining = avail_main.saturating_sub(fixed_main_total.saturating_add(gap_total));

        if main_fixed && !flex_children.is_empty() {
            let weights: Vec<u32> = flex_children.iter().map(|(_, w)| (*w).max(1)).collect();
            let shares = allocate_flex_shares(remaining, &weights);
            for (idx, (child_index, _)) in flex_children.iter().enumerate() {
                let child_layout = self.node_layout_snapshot(children[*child_index]).0;
                let mut effective = child_layout;
                let child_cross = cross_sizing(child_layout, layout.direction);
                if !cross_fixed && matches!(child_cross, Sizing::Flex(_)) {
                    set_cross_sizing(&mut effective, layout.direction, Sizing::Measure);
                }
                if layout.overflow_x {
                    effective.overflow_x = true;
                }
                if layout.overflow_y {
                    effective.overflow_y = true;
                }
                let child_available =
                    Size::from_main_cross(layout.direction, shares[idx], avail_cross);
                let size = self.resolve_outer_size_with_layout(
                    children[*child_index],
                    effective,
                    child_available.into(),
                );
                child_sizes[*child_index] = size;
            }
        }

        let mut main_total = 0u32;
        let mut cross_max = 0u32;
        for size in &child_sizes {
            main_total = main_total.saturating_add(size.main(layout.direction));
            cross_max = cross_max.max(size.cross(layout.direction));
        }
        main_total = main_total.saturating_add(gap_total);

        let content = Size::from_main_cross(layout.direction, main_total, cross_max);
        constraints.clamp_size(content)
    }

    /// Measure content size for Stack direction - max of all children sizes.
    fn measure_wrap_content_stack(
        &mut self,
        layout: Layout,
        constraints: MeasureConstraints,
        children: &[NodeId],
    ) -> Size<u32> {
        let avail_w = max_bound(constraints.width);
        let avail_h = max_bound(constraints.height);
        let avail = Size::new(avail_w, avail_h);

        let mut max_w = 0u32;
        let mut max_h = 0u32;

        for child in children {
            let child_layout = self.node_layout_snapshot(*child).0;
            let mut effective = child_layout;

            // Treat flex as measure when parent is not exact
            if !matches!(constraints.width, Constraint::Exact(_))
                && matches!(child_layout.width, Sizing::Flex(_))
            {
                effective.width = Sizing::Measure;
            }
            if !matches!(constraints.height, Constraint::Exact(_))
                && matches!(child_layout.height, Sizing::Flex(_))
            {
                effective.height = Sizing::Measure;
            }

            if layout.overflow_x {
                effective.overflow_x = true;
            }
            if layout.overflow_y {
                effective.overflow_y = true;
            }

            let size = self.resolve_outer_size_with_layout(*child, effective, avail.into());
            max_w = max_w.max(size.width);
            max_h = max_h.max(size.height);
        }

        let content = Size::new(max_w, max_h);
        constraints.clamp_size(content)
    }

    /// Lay out visible children inside the provided content box.
    fn layout_children(&mut self, node_id: NodeId, layout: Layout, content: Size<u32>) {
        let children = self.visible_children(node_id);
        if children.is_empty() {
            return;
        }

        let parent_overflow = Overflow::from_layout(layout);
        match layout.direction {
            LayoutDirection::Stack => {
                // Stack: all children get full content area, positioned according to alignment
                for child in &children {
                    // First, layout the child to determine its size
                    self.layout_node(*child, content.into(), Point::zero(), parent_overflow);

                    // Then apply alignment to position the child within content area
                    let child_size = self.node_size(*child);
                    let offset_x =
                        align_offset(child_size.width, content.width, layout.align_horizontal);
                    let offset_y =
                        align_offset(child_size.height, content.height, layout.align_vertical);
                    self.set_node_position(
                        *child,
                        Point {
                            x: offset_x,
                            y: offset_y,
                        },
                    );
                }
            }
            LayoutDirection::Row | LayoutDirection::Column => {
                self.layout_children_sequential(layout, content, &children, parent_overflow);
            }
        }
    }

    /// Layout children sequentially (Row or Column direction).
    fn layout_children_sequential(
        &mut self,
        layout: Layout,
        content: Size<u32>,
        children: &[NodeId],
        parent_overflow: Overflow,
    ) {
        let mut fixed_main_total = 0u32;
        let mut flex_children: Vec<(usize, u32)> = Vec::new();
        let mut pre_sizes = vec![Size::ZERO; children.len()];

        for (i, child) in children.iter().enumerate() {
            let child_layout = self.node_layout_snapshot(*child).0;
            let main = main_sizing(child_layout, layout.direction);
            if let Sizing::Flex(w) = main {
                flex_children.push((i, w.max(1)));
                continue;
            }

            let mut effective = child_layout;
            if parent_overflow.x {
                effective.overflow_x = true;
            }
            if parent_overflow.y {
                effective.overflow_y = true;
            }

            let child_available = content;
            let size =
                self.resolve_outer_size_with_layout(*child, effective, child_available.into());
            pre_sizes[i] = size;
            fixed_main_total = fixed_main_total.saturating_add(size.main(layout.direction));
        }

        let gap_total = layout
            .gap
            .saturating_mul(children.len().saturating_sub(1) as u32);
        let remaining = content
            .main(layout.direction)
            .saturating_sub(fixed_main_total.saturating_add(gap_total));

        let weights: Vec<u32> = flex_children.iter().map(|(_, w)| (*w).max(1)).collect();
        let shares = allocate_flex_shares(remaining, &weights);

        let mut pos_main = 0u32;
        let mut flex_idx = 0usize;
        for (i, child) in children.iter().enumerate() {
            let child_layout = self.node_layout_snapshot(*child).0;
            let mut effective = child_layout;
            if parent_overflow.x {
                effective.overflow_x = true;
            }
            if parent_overflow.y {
                effective.overflow_y = true;
            }

            let main = match main_sizing(effective, layout.direction) {
                Sizing::Flex(_) => {
                    let share = shares[flex_idx];
                    flex_idx += 1;
                    share
                }
                Sizing::Measure => pre_sizes[i].main(layout.direction),
            };

            let child_available =
                Size::from_main_cross(layout.direction, main, content.cross(layout.direction));
            let child_pos = match layout.direction {
                LayoutDirection::Row => Point { x: pos_main, y: 0 },
                LayoutDirection::Column => Point { x: 0, y: pos_main },
                LayoutDirection::Stack => unreachable!(),
            };

            let actual =
                self.layout_node(*child, child_available.into(), child_pos, parent_overflow);
            pos_main = pos_main
                .saturating_add(actual.main(layout.direction))
                .saturating_add(layout.gap);
        }
    }

    /// Get a node's outer size.
    fn node_size(&self, node_id: NodeId) -> Size<u32> {
        self.core
            .nodes
            .get(node_id)
            .map(|n| Size::new(n.rect.w, n.rect.h))
            .unwrap_or(Size::ZERO)
    }

    /// Set a node's position within its parent's content area.
    fn set_node_position(&mut self, node_id: NodeId, position: Point) {
        if let Some(node) = self.core.nodes.get_mut(node_id) {
            node.rect.tl = position;
        }
    }

    /// Compute the scrollable canvas size for a node.
    fn compute_canvas(&self, node_id: NodeId, view_size: Size<u32>) -> Size<u32> {
        let children = self.visible_children(node_id);
        let mut canvas_children = Vec::with_capacity(children.len());
        for child in children {
            if let Some(node) = self.core.nodes.get(child) {
                let child_canvas: Size<u32> = node.canvas.into();
                canvas_children.push(CanvasChild::new(node.rect, child_canvas));
            }
        }
        let ctx = CanvasContext::new(&canvas_children);
        let canvas = self
            .core
            .with_widget_view(node_id, |widget, _core| widget.canvas(view_size, &ctx))
            .unwrap_or(view_size);
        Size::new(
            canvas.width.max(view_size.width),
            canvas.height.max(view_size.height),
        )
    }

    /// Store canvas size and clamp scroll offset for a node.
    fn update_canvas(&mut self, node_id: NodeId, view_size: Size<u32>, canvas: Size<u32>) {
        if let Some(node) = self.core.nodes.get_mut(node_id) {
            let mut canvas = canvas;
            canvas.width = canvas.width.max(view_size.width);
            canvas.height = canvas.height.max(view_size.height);

            let mut scroll = node.scroll;
            clamp_scroll(&mut scroll, view_size, canvas);
            node.scroll = scroll;
            node.canvas = canvas.into();
        }
    }

    /// Snapshot a node's layout and hidden state.
    fn node_layout_snapshot(&self, node_id: NodeId) -> (Layout, bool) {
        self.core
            .nodes
            .get(node_id)
            .map(|node| (node.layout, node.hidden))
            .unwrap_or((Layout::default(), true))
    }

    /// Collect visible child nodes in tree order.
    fn visible_children(&self, node_id: NodeId) -> Vec<NodeId> {
        let Some(node) = self.core.nodes.get(node_id) else {
            return Vec::new();
        };
        node.children
            .iter()
            .copied()
            .filter(|child| {
                self.core
                    .nodes
                    .get(*child)
                    .is_some_and(|n| !n.hidden && n.layout.display == Display::Block)
            })
            .collect()
    }

    /// Get a cached measurement or compute and store it for this pass.
    fn measure_cached(&mut self, node_id: NodeId, constraints: MeasureConstraints) -> Measurement {
        let key = MeasureKey {
            node: node_id,
            constraints,
        };
        if let Some(m) = self.measure_cache.get(&key) {
            return *m;
        }
        let measured = self
            .core
            .with_widget_view(node_id, |widget, _core| widget.measure(constraints))
            .unwrap_or_else(|_| constraints.clamp(Size::ZERO));
        self.measure_cache.insert(key, measured);
        measured
    }

    /// Reset layout data for a hidden subtree.
    fn clear_layout(&mut self, node_id: NodeId, position: Point) {
        if let Some(node) = self.core.nodes.get_mut(node_id) {
            node.rect = Rect::new(position.x, position.y, 0, 0);
            node.content_size = Expanse::default();
            node.canvas = Expanse::default();
            node.scroll = Point::zero();
            node.view = View::default();
        }
        let children = self
            .core
            .nodes
            .get(node_id)
            .map(|node| node.children.clone())
            .unwrap_or_default();
        for child in children {
            self.clear_layout(child, Point::zero());
        }
    }
}

/// Clamp an outer size against min/max constraints.
fn clamp_outer(size: Size<u32>, layout: Layout) -> Size<u32> {
    Size::new(
        clamp_axis(size.width, layout.min_width, layout.max_width),
        clamp_axis(size.height, layout.min_height, layout.max_height),
    )
}

/// Clamp a single axis against optional min/max bounds.
fn clamp_axis(value: u32, min: Option<u32>, max: Option<u32>) -> u32 {
    let (min, max) = match (min, max) {
        (Some(min), Some(max)) if min > max => (Some(max), Some(max)),
        other => other,
    };
    let mut value = value;
    if let Some(max) = max {
        value = value.min(max);
    }
    if let Some(min) = min {
        value = value.max(min);
    }
    value
}

/// Build a content-box constraint for a single axis.
fn constraint_for_axis(
    sizing: Sizing,
    available_content: u32,
    min_outer: Option<u32>,
    max_outer: Option<u32>,
    pad_axis: u32,
    overflow: bool,
) -> Constraint {
    match sizing {
        Sizing::Flex(_) => Constraint::Exact(available_content),
        Sizing::Measure => {
            if overflow && max_outer.is_none() {
                return Constraint::Unbounded;
            }
            let effective_max_outer = match max_outer {
                Some(m) => m.min(available_content.saturating_add(pad_axis)),
                None => available_content.saturating_add(pad_axis),
            };
            let effective_max_content = effective_max_outer.saturating_sub(pad_axis);

            if let (Some(min_o), Some(max_o)) = (min_outer, max_outer)
                && min_o == max_o
            {
                return Constraint::Exact(max_o.saturating_sub(pad_axis));
            }

            Constraint::AtMost(effective_max_content)
        }
    }
}

/// Clamp a scroll offset so it stays within view/canvas bounds.
fn clamp_scroll(scroll: &mut Point, view: Size<u32>, canvas: Size<u32>) {
    let max_x = if view.width == 0 {
        0
    } else {
        canvas.width.saturating_sub(view.width)
    };
    let max_y = if view.height == 0 {
        0
    } else {
        canvas.height.saturating_sub(view.height)
    };
    scroll.x = scroll.x.min(max_x);
    scroll.y = scroll.y.min(max_y);
}

/// Allocate remaining space proportionally across flex weights.
fn allocate_flex_shares(remaining: u32, weights: &[u32]) -> Vec<u32> {
    if remaining == 0 || weights.is_empty() {
        return vec![0; weights.len()];
    }
    let total: u64 = weights.iter().map(|w| (*w).max(1) as u64).sum();
    if total == 0 {
        return vec![0; weights.len()];
    }

    let mut base = Vec::with_capacity(weights.len());
    let mut rem = Vec::with_capacity(weights.len());
    for w in weights {
        let weight = (*w).max(1) as u64;
        let prod = remaining as u64 * weight;
        base.push((prod / total) as u32);
        rem.push((prod % total) as u32);
    }

    let used: u32 = base.iter().sum();
    let extra = remaining.saturating_sub(used);
    if extra == 0 {
        return base;
    }

    let mut idx: Vec<usize> = (0..weights.len()).collect();
    idx.sort_by(|a, b| rem[*b].cmp(&rem[*a]).then_with(|| a.cmp(b)));
    for i in 0..extra as usize {
        if let Some(target) = idx.get(i) {
            base[*target] = base[*target].saturating_add(1);
        }
    }

    base
}

/// Extract the main-axis sizing from a layout.
fn main_sizing(layout: Layout, direction: LayoutDirection) -> Sizing {
    match direction {
        LayoutDirection::Row => layout.width,
        LayoutDirection::Column | LayoutDirection::Stack => layout.height,
    }
}

/// Extract the cross-axis sizing from a layout.
fn cross_sizing(layout: Layout, direction: LayoutDirection) -> Sizing {
    match direction {
        LayoutDirection::Row => layout.height,
        LayoutDirection::Column | LayoutDirection::Stack => layout.width,
    }
}

/// Set the main-axis sizing on a layout.
fn set_main_sizing(layout: &mut Layout, direction: LayoutDirection, sizing: Sizing) {
    match direction {
        LayoutDirection::Row => layout.width = sizing,
        LayoutDirection::Column | LayoutDirection::Stack => layout.height = sizing,
    }
}

/// Set the cross-axis sizing on a layout.
fn set_cross_sizing(layout: &mut Layout, direction: LayoutDirection, sizing: Sizing) {
    match direction {
        LayoutDirection::Row => layout.height = sizing,
        LayoutDirection::Column | LayoutDirection::Stack => layout.width = sizing,
    }
}

/// Calculate the offset for aligning a child within available space.
fn align_offset(child_size: u32, available: u32, align: Align) -> u32 {
    match align {
        Align::Start => 0,
        Align::Center => available.saturating_sub(child_size) / 2,
        Align::End => available.saturating_sub(child_size),
    }
}

/// Depth-first search for a node at a screen-space point.
fn locate_recursive(
    core: &Core,
    node_id: NodeId,
    point: Point,
    parent_clip: Rect,
    result: &mut Option<NodeId>,
) -> Result<()> {
    let node = core
        .nodes
        .get(node_id)
        .ok_or_else(|| Error::Internal("missing node".into()))?;

    if node.hidden || node.layout.display == Display::None {
        return Ok(());
    }

    let Some(outer_clip) = node.view.outer.intersect_rect(parent_clip) else {
        return Ok(());
    };
    if !outer_clip.contains_point(point) {
        return Ok(());
    }

    *result = Some(node_id);

    let Some(child_clip) = node.view.content.intersect_rect(parent_clip) else {
        return Ok(());
    };
    let children = node.children.clone();
    for child in children {
        locate_recursive(core, child, point, child_clip, result)?;
    }

    Ok(())
}

#[derive(Default)]
/// Root widget container used for the implicit root node.
struct RootContainer;

impl Widget for RootContainer {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, _frame: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("root")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rand::{Rng, SeedableRng, rngs::StdRng};

    use super::*;
    use crate::{
        Context,
        error::{Error, Result},
        geom::{Expanse, Point},
        layout::{
            Align, CanvasContext, Constraint, Direction, Edges, Layout, MeasureConstraints,
            Measurement, Size, Sizing,
        },
        widget::Widget,
    };

    type MeasureFn = dyn Fn(MeasureConstraints) -> Measurement + Send + Sync;
    type CanvasFn = dyn Fn(Size<u32>, &CanvasContext) -> Size<u32> + Send + Sync;

    struct TestWidget {
        measure_fn: Arc<MeasureFn>,
        canvas_fn: Arc<CanvasFn>,
    }

    impl TestWidget {
        fn new<F>(measure_fn: F) -> (Self, Arc<Mutex<Vec<MeasureConstraints>>>)
        where
            F: Fn(MeasureConstraints) -> Measurement + Send + Sync + 'static,
        {
            Self::with_canvas(measure_fn, |view, _ctx| view)
        }

        fn with_canvas<F, C>(
            measure_fn: F,
            canvas_fn: C,
        ) -> (Self, Arc<Mutex<Vec<MeasureConstraints>>>)
        where
            F: Fn(MeasureConstraints) -> Measurement + Send + Sync + 'static,
            C: Fn(Size<u32>, &CanvasContext) -> Size<u32> + Send + Sync + 'static,
        {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let calls_clone = Arc::clone(&calls);
            let measure_fn = Arc::new(move |c: MeasureConstraints| {
                calls_clone.lock().unwrap().push(c);
                measure_fn(c)
            });
            let canvas_fn = Arc::new(canvas_fn);
            (
                Self {
                    measure_fn,
                    canvas_fn,
                },
                calls,
            )
        }
    }

    impl Widget for TestWidget {
        fn measure(&self, c: MeasureConstraints) -> Measurement {
            (self.measure_fn)(c)
        }

        fn canvas(&self, view: Size<u32>, ctx: &CanvasContext) -> Size<u32> {
            (self.canvas_fn)(view, ctx)
        }
    }

    struct FocusableWidget;

    impl Widget for FocusableWidget {
        fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
            true
        }
    }

    struct MountFailWidget;

    impl Widget for MountFailWidget {
        fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
            Err(Error::Invalid("mount failed".into()))
        }
    }

    fn attach_root_child(core: &mut Core, child: NodeId) -> Result<()> {
        core.set_children(core.root, vec![child])
    }

    #[test]
    fn clamp_outer_no_bounds() {
        let layout = Layout::column();
        let size = Size::new(5, 7);
        assert_eq!(clamp_outer(size, layout), size);
    }

    #[test]
    fn clamp_outer_min_only() {
        let mut layout = Layout::column();
        layout.min_width = Some(10);
        layout.min_height = Some(2);
        assert_eq!(clamp_outer(Size::new(5, 1), layout), Size::new(10, 2));
    }

    #[test]
    fn clamp_outer_max_only() {
        let mut layout = Layout::column();
        layout.max_width = Some(3);
        layout.max_height = Some(4);
        assert_eq!(clamp_outer(Size::new(5, 7), layout), Size::new(3, 4));
    }

    #[test]
    fn clamp_outer_min_greater_than_max() {
        let mut layout = Layout::column();
        layout.min_width = Some(10);
        layout.max_width = Some(5);
        assert_eq!(clamp_outer(Size::new(8, 1), layout), Size::new(5, 1));
    }

    #[test]
    fn constraint_for_axis_flex_is_exact() {
        let c = constraint_for_axis(Sizing::Flex(1), 10, None, None, 0, false);
        assert_eq!(c, Constraint::Exact(10));
    }

    #[test]
    fn constraint_for_axis_min_equals_max_is_exact() {
        let c = constraint_for_axis(Sizing::Measure, 10, Some(6), Some(6), 2, false);
        assert_eq!(c, Constraint::Exact(4));
    }

    #[test]
    fn constraint_for_axis_max_caps_available() {
        let c = constraint_for_axis(Sizing::Measure, 10, None, Some(6), 2, false);
        assert_eq!(c, Constraint::AtMost(4));
    }

    #[test]
    fn leaf_measure_adds_padding() -> Result<()> {
        let mut core = Core::new();
        let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 5)));
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column().padding(Edges::all(1));
        })?;
        core.update_layout(Expanse::new(50, 50))?;
        let node = &core.nodes[child];
        assert_eq!(node.rect.w, 7);
        assert_eq!(node.rect.h, 7);
        assert_eq!(node.content_size, Expanse::new(5, 5));
        Ok(())
    }

    #[test]
    fn leaf_padding_consumes_all() -> Result<()> {
        let mut core = Core::new();
        let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::fill().padding(Edges::all(1));
        })?;
        core.update_layout(Expanse::new(1, 1))?;
        let node = &core.nodes[child];
        assert_eq!(node.content_size, Expanse::new(0, 0));
        Ok(())
    }

    #[test]
    fn flex_axis_constraints_are_exact() -> Result<()> {
        let mut core = Core::new();
        let (widget, calls) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column().flex_horizontal(1);
        })?;
        core.update_layout(Expanse::new(10, 5))?;
        let calls = calls.lock().unwrap();
        assert!(!calls.is_empty());
        assert_eq!(calls[0].width, Constraint::Exact(10));
        Ok(())
    }

    #[test]
    fn remeasure_when_min_width_expands_measured() -> Result<()> {
        let mut core = Core::new();
        let (widget, calls) = TestWidget::new(|c| {
            let width = match c.width {
                Constraint::Exact(n) => n,
                _ => 4,
            };
            let height = if width >= 10 { 2 } else { 4 };
            Measurement::Fixed(Size::new(width, height))
        });
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column().min_width(10);
        })?;
        core.update_layout(Expanse::new(20, 20))?;
        let calls = calls.lock().unwrap();
        assert!(calls.iter().any(|c| c.width == Constraint::Exact(10)));
        let node = &core.nodes[child];
        assert_eq!(node.content_size.h, 2);
        Ok(())
    }

    #[test]
    fn remeasure_when_min_width_expands_flex() -> Result<()> {
        let mut core = Core::new();
        let (widget, calls) = TestWidget::new(|c| {
            let width = match c.width {
                Constraint::Exact(n) => n,
                _ => 0,
            };
            Measurement::Fixed(Size::new(width, width.max(1)))
        });
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column()
                .flex_horizontal(1)
                .padding(Edges::all(1))
                .min_width(30);
        })?;
        core.update_layout(Expanse::new(10, 10))?;
        let calls = calls.lock().unwrap();
        assert!(calls.iter().any(|c| c.width == Constraint::Exact(8)));
        assert!(calls.iter().any(|c| c.width == Constraint::Exact(28)));
        Ok(())
    }

    #[test]
    fn wrap_no_children() -> Result<()> {
        let mut core = Core::new();
        let (widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::column().padding(Edges::all(1));
        })?;
        core.update_layout(Expanse::new(20, 20))?;
        let node = &core.nodes[parent];
        assert_eq!(node.content_size, Expanse::new(0, 0));
        assert_eq!(node.rect.w, 2);
        assert_eq!(node.rect.h, 2);
        Ok(())
    }

    #[test]
    fn wrap_sum_main_max_cross() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 3)));
        let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 2)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        let child3 = core.add_boxed(Box::new(c3));
        core.set_children(parent, vec![child1, child2, child3])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::column().gap(1);
        })?;
        core.update_layout(Expanse::new(50, 50))?;
        let node = &core.nodes[parent];
        assert_eq!(node.content_size, Expanse::new(4, 8));
        Ok(())
    }

    #[test]
    fn wrap_includes_child_padding() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 1)));
        let child = core.add_boxed(Box::new(child_widget));
        core.set_children(parent, vec![child])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::column();
        })?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column().padding(Edges::all(1));
        })?;
        core.update_layout(Expanse::new(50, 50))?;
        let node = &core.nodes[parent];
        assert_eq!(node.content_size, Expanse::new(5, 3));
        Ok(())
    }

    #[test]
    fn wrap_flex_child_treated_as_measure_when_parent_not_exact() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 4)));
        let child = core.add_boxed(Box::new(child_widget));
        core.set_children(parent, vec![child])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::column();
        })?;
        core.with_layout_of(child, |layout| {
            layout.height = Sizing::Flex(1);
        })?;
        core.update_layout(Expanse::new(20, 20))?;
        let node = &core.nodes[parent];
        assert_eq!(node.content_size.h, 4);
        Ok(())
    }

    #[test]
    fn wrap_flex_child_behaves_as_flex_when_parent_exact() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child1_widget, calls1) = TestWidget::new(|c| {
            let width = match c.width {
                Constraint::Exact(n) => n,
                Constraint::AtMost(n) => n,
                Constraint::Unbounded => 0,
            };
            Measurement::Fixed(Size::new(width, width))
        });
        let (child2_widget, calls2) = TestWidget::new(|c| {
            let width = match c.width {
                Constraint::Exact(n) => n,
                Constraint::AtMost(n) => n,
                Constraint::Unbounded => 0,
            };
            Measurement::Fixed(Size::new(width, width))
        });
        let child1 = core.add_boxed(Box::new(child1_widget));
        let child2 = core.add_boxed(Box::new(child2_widget));
        core.set_children(parent, vec![child1, child2])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::row().flex_horizontal(1);
        })?;
        core.with_layout_of(child1, |layout| {
            layout.width = Sizing::Flex(1);
        })?;
        core.with_layout_of(child2, |layout| {
            layout.width = Sizing::Flex(1);
        })?;
        core.update_layout(Expanse::new(10, 10))?;
        let calls1 = calls1.lock().unwrap();
        let calls2 = calls2.lock().unwrap();
        assert!(calls1.iter().any(|c| c.width == Constraint::Exact(5)));
        assert!(calls2.iter().any(|c| c.width == Constraint::Exact(5)));
        let parent_node = &core.nodes[parent];
        assert_eq!(parent_node.content_size.h, 5);
        Ok(())
    }

    #[test]
    fn wrap_gap_counts_only_visible_children() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        let child3 = core.add_boxed(Box::new(c3));
        core.set_children(parent, vec![child1, child2, child3])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::column().gap(2);
        })?;
        core.with_layout_of(child2, |layout| {
            *layout = Layout::column().none();
        })?;
        core.update_layout(Expanse::new(20, 20))?;
        let node = &core.nodes[parent];
        assert_eq!(node.content_size.h, 4);
        Ok(())
    }

    #[test]
    fn flex_shares_sum_equals_remaining() {
        let shares = allocate_flex_shares(17, &[1, 2, 3, 4]);
        let sum: u32 = shares.iter().sum();
        assert_eq!(sum, 17);
    }

    #[test]
    fn flex_shares_proportional_sanity() {
        let shares = allocate_flex_shares(5, &[3, 7]);
        assert_eq!(shares, vec![2, 3]);
    }

    #[test]
    fn flex_shares_stable_tie_break() {
        let shares = allocate_flex_shares(2, &[1, 1, 1]);
        assert_eq!(shares, vec![1, 1, 0]);
    }

    #[test]
    fn flex_weight_zero_clamped() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        core.set_children(parent, vec![child1, child2])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::row().flex_horizontal(1);
        })?;
        core.with_layout_of(child1, |layout| {
            layout.width = Sizing::Flex(0);
        })?;
        core.with_layout_of(child2, |layout| {
            layout.width = Sizing::Flex(0);
        })?;
        core.update_layout(Expanse::new(10, 5))?;
        assert_eq!(core.nodes[child1].rect.w, 5);
        assert_eq!(core.nodes[child2].rect.w, 5);
        Ok(())
    }

    #[test]
    fn positions_monotonic_main() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
        let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        let child3 = core.add_boxed(Box::new(c3));
        core.set_children(parent, vec![child1, child2, child3])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::row().flex_horizontal(1).gap(1);
        })?;
        core.update_layout(Expanse::new(20, 5))?;
        let p1 = core.nodes[child1].rect.tl.x;
        let p2 = core.nodes[child2].rect.tl.x;
        let p3 = core.nodes[child3].rect.tl.x;
        assert!(p1 <= p2 && p2 <= p3);
        Ok(())
    }

    #[test]
    fn no_overlaps_with_min_expansion() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        core.set_children(parent, vec![child1, child2])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::row().flex_horizontal(1);
        })?;
        core.with_layout_of(child1, |layout| {
            layout.width = Sizing::Flex(1);
            layout.min_width = Some(10);
        })?;
        core.with_layout_of(child2, |layout| {
            layout.width = Sizing::Flex(1);
            layout.min_width = Some(10);
        })?;
        core.update_layout(Expanse::new(5, 5))?;
        let first = &core.nodes[child1];
        let second = &core.nodes[child2];
        assert_eq!(second.rect.tl.x, first.rect.tl.x + first.rect.w);
        Ok(())
    }

    #[test]
    fn overflow_positions_consistent() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 1)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 1)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        core.set_children(parent, vec![child1, child2])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::row().flex_horizontal(1).gap(1);
        })?;
        core.update_layout(Expanse::new(5, 5))?;
        assert_eq!(core.nodes[child2].rect.tl.x, 5);
        Ok(())
    }

    #[test]
    fn canvas_clamped_at_least_view() -> Result<()> {
        let mut core = Core::new();
        let (widget, _) =
            TestWidget::with_canvas(|_c| Measurement::Wrap, |_view, _ctx| Size::new(1, 1));
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::fill();
        })?;
        core.update_layout(Expanse::new(5, 5))?;
        let node = &core.nodes[child];
        assert_eq!(node.canvas, Expanse::new(5, 5));
        Ok(())
    }

    #[test]
    fn offset_clamped_when_canvas_shrinks() -> Result<()> {
        let mut core = Core::new();
        let canvas = Arc::new(Mutex::new(Size::new(20, 20)));
        let canvas_clone = Arc::clone(&canvas);
        let (widget, _) = TestWidget::with_canvas(
            |_c| Measurement::Wrap,
            move |_view, _ctx| *canvas_clone.lock().unwrap(),
        );
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::fill();
        })?;
        if let Some(node) = core.nodes.get_mut(child) {
            node.scroll = Point { x: 15, y: 15 };
        }
        core.update_layout(Expanse::new(10, 10))?;
        assert_eq!(core.nodes[child].scroll, Point { x: 10, y: 10 });
        *canvas.lock().unwrap() = Size::new(12, 12);
        core.update_layout(Expanse::new(10, 10))?;
        assert_eq!(core.nodes[child].scroll, Point { x: 2, y: 2 });
        Ok(())
    }

    #[test]
    fn offset_clamped_when_view_grows() -> Result<()> {
        let mut core = Core::new();
        let canvas = Arc::new(Mutex::new(Size::new(20, 20)));
        let canvas_clone = Arc::clone(&canvas);
        let (widget, _) = TestWidget::with_canvas(
            |_c| Measurement::Wrap,
            move |_view, _ctx| *canvas_clone.lock().unwrap(),
        );
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::fill();
        })?;
        if let Some(node) = core.nodes.get_mut(child) {
            node.scroll = Point { x: 15, y: 15 };
        }
        core.update_layout(Expanse::new(5, 5))?;
        assert_eq!(core.nodes[child].scroll, Point { x: 15, y: 15 });
        core.update_layout(Expanse::new(10, 10))?;
        assert_eq!(core.nodes[child].scroll, Point { x: 10, y: 10 });
        Ok(())
    }

    #[test]
    fn zero_view_clamps_scroll() -> Result<()> {
        let mut core = Core::new();
        let canvas = Arc::new(Mutex::new(Size::new(10, 10)));
        let canvas_clone = Arc::clone(&canvas);
        let (widget, _) = TestWidget::with_canvas(
            |_c| Measurement::Wrap,
            move |_view, _ctx| *canvas_clone.lock().unwrap(),
        );
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::fill();
        })?;
        if let Some(node) = core.nodes.get_mut(child) {
            node.scroll = Point { x: 5, y: 5 };
        }
        core.update_layout(Expanse::new(0, 0))?;
        assert_eq!(core.nodes[child].scroll, Point { x: 0, y: 0 });
        Ok(())
    }

    #[test]
    fn child_screen_origin_signed() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) =
            TestWidget::with_canvas(|_c| Measurement::Wrap, |_view, _ctx| Size::new(20, 10));
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 2)));
        let child = core.add_boxed(Box::new(child_widget));
        core.set_children(parent, vec![child])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::fill();
        })?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column().fixed_width(2).fixed_height(2);
        })?;
        if let Some(node) = core.nodes.get_mut(parent) {
            node.scroll = Point { x: 5, y: 0 };
        }
        core.update_layout(Expanse::new(10, 10))?;
        let child_view = core.nodes[child].view;
        assert_eq!(child_view.outer.tl.x, -5);
        Ok(())
    }

    #[test]
    fn content_rect_respects_padding() -> Result<()> {
        let mut core = Core::new();
        let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 5)));
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column().padding(Edges::all(1));
        })?;
        core.update_layout(Expanse::new(20, 20))?;
        let view = core.nodes[child].view;
        assert_eq!(view.content.tl.x, view.outer.tl.x + 1);
        assert_eq!(view.content.tl.y, view.outer.tl.y + 1);
        assert_eq!(view.content.w, view.outer.w.saturating_sub(2));
        assert_eq!(view.content.h, view.outer.h.saturating_sub(2));
        Ok(())
    }

    #[test]
    fn random_tree_no_panics() -> Result<()> {
        let mut core = Core::new();
        let mut rng = StdRng::seed_from_u64(0x5eed);
        let root_child = build_random_tree(&mut core, &mut rng, 3)?;
        attach_root_child(&mut core, root_child)?;
        core.update_layout(Expanse::new(40, 20))?;

        for node in core.nodes.values() {
            let expected_w = node.rect.w.saturating_sub(node.layout.padding.horizontal());
            let expected_h = node.rect.h.saturating_sub(node.layout.padding.vertical());
            assert_eq!(node.content_size.w, expected_w);
            assert_eq!(node.content_size.h, expected_h);
            assert!(node.canvas.w >= node.content_size.w);
            assert!(node.canvas.h >= node.content_size.h);
            let max_x = node.canvas.w.saturating_sub(node.content_size.w);
            let max_y = node.canvas.h.saturating_sub(node.content_size.h);
            assert!(node.scroll.x <= max_x);
            assert!(node.scroll.y <= max_y);
        }

        for node in core.nodes.values() {
            // For Stack direction, children can overlap, so skip position ordering check
            if node.layout.direction == LayoutDirection::Stack {
                continue;
            }
            let mut last = 0u32;
            for child in &node.children {
                let child = &core.nodes[*child];
                if child.layout.display == Display::None || child.hidden {
                    continue;
                }
                let pos = match node.layout.direction {
                    LayoutDirection::Row => child.rect.tl.x,
                    LayoutDirection::Column => child.rect.tl.y,
                    LayoutDirection::Stack => continue,
                };
                assert!(pos >= last);
                last = pos;
            }
        }

        Ok(())
    }

    fn build_random_tree(core: &mut Core, rng: &mut StdRng, depth: usize) -> Result<NodeId> {
        let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let node = core.add_boxed(Box::new(widget));
        let mut layout = if rng.random_bool(0.5) {
            Layout::row()
        } else {
            Layout::column()
        };
        if rng.random_bool(0.6) {
            layout.width = Sizing::Flex(rng.random_range(0..3));
        }
        if rng.random_bool(0.6) {
            layout.height = Sizing::Flex(rng.random_range(0..3));
        }
        layout.padding = Edges::new(
            rng.random_range(0..3),
            rng.random_range(0..3),
            rng.random_range(0..3),
            rng.random_range(0..3),
        );
        layout.gap = rng.random_range(0..3);
        if rng.random_bool(0.3) {
            layout.min_width = Some(rng.random_range(0..6));
        }
        if rng.random_bool(0.3) {
            layout.max_width = Some(rng.random_range(0..6));
        }
        if rng.random_bool(0.3) {
            layout.min_height = Some(rng.random_range(0..6));
        }
        if rng.random_bool(0.3) {
            layout.max_height = Some(rng.random_range(0..6));
        }
        core.with_layout_of(node, |l| {
            *l = layout;
        })?;

        if depth > 0 {
            let child_count = rng.random_range(0..=3);
            if child_count > 0 {
                let mut children = Vec::new();
                for _ in 0..child_count {
                    children.push(build_random_tree(core, rng, depth - 1)?);
                }
                core.set_children(node, children)?;
            }
        }

        Ok(node)
    }

    #[test]
    fn stack_children_overlap() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 5)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        core.set_children(parent, vec![child1, child2])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::stack();
        })?;
        core.update_layout(Expanse::new(50, 50))?;

        // Both children should be at the same position (0, 0) by default
        assert_eq!(core.nodes[child1].rect.tl.x, 0);
        assert_eq!(core.nodes[child1].rect.tl.y, 0);
        assert_eq!(core.nodes[child2].rect.tl.x, 0);
        assert_eq!(core.nodes[child2].rect.tl.y, 0);

        // Parent content size should be the max of children
        let parent_node = &core.nodes[parent];
        assert_eq!(parent_node.content_size, Expanse::new(10, 10));
        Ok(())
    }

    #[test]
    fn stack_with_center_alignment() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
        let child = core.add_boxed(Box::new(child_widget));
        core.set_children(parent, vec![child])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::fill().direction(Direction::Stack).align_center();
        })?;
        core.update_layout(Expanse::new(50, 50))?;

        // Child should be centered in the 50x50 parent
        let child_node = &core.nodes[child];
        assert_eq!(child_node.rect.tl.x, 20); // (50 - 10) / 2
        assert_eq!(child_node.rect.tl.y, 20); // (50 - 10) / 2
        Ok(())
    }

    #[test]
    fn stack_with_end_alignment() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
        let child = core.add_boxed(Box::new(child_widget));
        core.set_children(parent, vec![child])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::fill()
                .direction(Direction::Stack)
                .align_horizontal(Align::End)
                .align_vertical(Align::End);
        })?;
        core.update_layout(Expanse::new(50, 50))?;

        // Child should be at the end (bottom-right)
        let child_node = &core.nodes[child];
        assert_eq!(child_node.rect.tl.x, 40); // 50 - 10
        assert_eq!(child_node.rect.tl.y, 40); // 50 - 10
        Ok(())
    }

    #[test]
    fn stack_multiple_children_centered() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(20, 20)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        core.set_children(parent, vec![child1, child2])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::fill().direction(Direction::Stack).align_center();
        })?;
        core.update_layout(Expanse::new(50, 50))?;

        // Both children should be centered independently
        let c1_node = &core.nodes[child1];
        let c2_node = &core.nodes[child2];
        assert_eq!(c1_node.rect.tl.x, 15); // (50 - 20) / 2
        assert_eq!(c1_node.rect.tl.y, 15);
        assert_eq!(c2_node.rect.tl.x, 20); // (50 - 10) / 2
        assert_eq!(c2_node.rect.tl.y, 20);
        Ok(())
    }

    #[test]
    fn set_children_detaches_from_previous_parent() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent_a = core.add_boxed(Box::new(parent_widget));
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent_b = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let child = core.add_boxed(Box::new(child_widget));

        core.set_children(parent_a, vec![child])?;
        core.set_children(parent_b, vec![child])?;

        assert!(core.nodes[parent_a].children.is_empty());
        assert_eq!(core.nodes[parent_b].children, vec![child]);
        assert_eq!(core.nodes[child].parent, Some(parent_b));
        Ok(())
    }

    #[test]
    fn set_children_rejects_cycles() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let child = core.add_boxed(Box::new(child_widget));
        core.set_children(parent, vec![child])?;

        let err = core.set_children(child, vec![parent]).unwrap_err();
        assert!(matches!(err, Error::WouldCreateCycle { .. }));
        Ok(())
    }

    #[test]
    fn set_children_rejects_duplicates() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let child = core.add_boxed(Box::new(child_widget));

        let err = core
            .set_children(parent, vec![child, child])
            .expect_err("expected duplicate child error");
        assert!(matches!(
            err,
            Error::DuplicateChild {
                parent: err_parent,
                child: err_child,
            } if err_parent == parent && err_child == child
        ));
        Ok(())
    }

    #[test]
    fn attach_rejects_cycles() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.create_detached(parent_widget);
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let child = core.create_detached(child_widget);

        core.attach(parent, child)?;
        let err = core.attach(child, parent).unwrap_err();
        assert!(matches!(err, Error::WouldCreateCycle { .. }));
        Ok(())
    }

    #[test]
    fn remove_subtree_recovers_focus_to_next() -> Result<()> {
        let mut core = Core::new();
        let first = core.create_detached(FocusableWidget);
        let second = core.create_detached(FocusableWidget);
        core.set_children(core.root, vec![first, second])?;
        core.with_layout_of(core.root, |layout| {
            *layout = Layout::fill();
        })?;
        core.with_layout_of(first, |layout| {
            *layout = Layout::fill();
        })?;
        core.with_layout_of(second, |layout| {
            *layout = Layout::fill();
        })?;
        core.update_layout(Expanse::new(10, 10))?;

        core.set_focus(first);
        core.remove_subtree(first)?;

        assert_eq!(core.focus, Some(second));
        Ok(())
    }

    #[test]
    fn remove_subtree_recovers_focus_to_prev() -> Result<()> {
        let mut core = Core::new();
        let first = core.create_detached(FocusableWidget);
        let second = core.create_detached(FocusableWidget);
        core.set_children(core.root, vec![first, second])?;
        core.with_layout_of(core.root, |layout| {
            *layout = Layout::fill();
        })?;
        core.with_layout_of(first, |layout| {
            *layout = Layout::fill();
        })?;
        core.with_layout_of(second, |layout| {
            *layout = Layout::fill();
        })?;
        core.update_layout(Expanse::new(10, 10))?;

        core.set_focus(second);
        core.remove_subtree(second)?;

        assert_eq!(core.focus, Some(first));
        Ok(())
    }

    #[test]
    fn detach_clears_mouse_capture() -> Result<()> {
        let mut core = Core::new();
        let child = core.create_detached(FocusableWidget);
        core.attach(core.root, child)?;
        core.mouse_capture = Some(child);

        core.detach(child)?;

        assert!(core.mouse_capture.is_none());
        assert!(core.nodes.get(child).is_some());
        assert!(core.nodes[child].parent.is_none());
        Ok(())
    }

    #[test]
    fn remove_subtree_clears_mouse_capture() -> Result<()> {
        let mut core = Core::new();
        let child = core.create_detached(FocusableWidget);
        core.attach(core.root, child)?;
        core.mouse_capture = Some(child);

        core.remove_subtree(child)?;

        assert!(core.mouse_capture.is_none());
        assert!(core.nodes.get(child).is_none());
        Ok(())
    }

    #[test]
    fn keyed_children_require_unique_keys() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.create_detached(parent_widget);
        core.attach(core.root, parent)?;
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let child = core.add_child_to_keyed_boxed(parent, "slot", Box::new(child_widget))?;
        let node_count = core.nodes.len();

        let (other_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let err = core
            .add_child_to_keyed_boxed(parent, "slot", Box::new(other_widget))
            .unwrap_err();

        assert!(matches!(err, Error::DuplicateChildKey(_)));
        assert_eq!(core.nodes.len(), node_count);
        assert_eq!(core.child_keyed(parent, "slot"), Some(child));
        Ok(())
    }

    #[test]
    fn detach_clears_keyed_mapping() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.create_detached(parent_widget);
        core.attach(core.root, parent)?;
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let child = core.add_child_to_keyed_boxed(parent, "slot", Box::new(child_widget))?;

        core.detach(child)?;

        assert!(core.child_keyed(parent, "slot").is_none());
        assert!(core.nodes[child].parent.is_none());
        Ok(())
    }

    #[test]
    fn add_child_rolls_back_on_mount_failure() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.create_detached(parent_widget);
        core.attach(core.root, parent)?;
        let node_count = core.nodes.len();

        let err = core
            .add_child_to_boxed(parent, Box::new(MountFailWidget))
            .unwrap_err();

        assert!(matches!(err, Error::Invalid(_)));
        assert_eq!(core.nodes.len(), node_count);
        assert!(core.nodes[parent].children.is_empty());
        Ok(())
    }
}
