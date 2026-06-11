use std::{
    any::TypeId,
    cell::RefCell,
    collections::{HashMap, HashSet},
};

use super::*;
use crate::{
    core::{
        context::CoreContext,
        node::Node,
        view::View,
        widget_access::{WidgetSlotPolicy, validate_slot},
    },
    geom::{Point, Rect, Size},
    layout::Layout,
    path::Path,
    widget::Widget,
};

impl Core {
    /// Add a boxed widget to the arena and return its node ID.
    pub(super) fn add_boxed(&mut self, widget: Box<dyn Widget>) -> NodeId {
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
            content_size: Size::default(),
            canvas: Size::default(),
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
        self.ensure_invariants(None);
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
        self.ensure_subtree_widget_slots_available(node_id, "replace subtree")?;
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

    /// Validate structural and cached state invariants for the core arena.
    pub fn validate_invariants(&self) -> Result<()> {
        self.validate_invariants_with(WidgetSlotPolicy::RequirePresent)
    }

    /// Validate core invariants using a caller-specific widget slot policy.
    fn validate_invariants_with(&self, widget_slot_policy: WidgetSlotPolicy) -> Result<()> {
        self.validate_root()?;
        for (node_id, node) in &self.nodes {
            self.validate_widget_slot(node_id, node, widget_slot_policy)?;
            self.validate_node_links(node_id, node)?;
            self.validate_parent_chain(node_id)?;
            self.validate_lifecycle_state(node_id, node)?;
            self.validate_cached_state(node_id, node)?;
        }
        self.validate_focus_and_capture()?;
        self.validate_pending_targets()?;
        Ok(())
    }

    /// Assert structural invariants on the node tree in debug builds.
    #[cfg(debug_assertions)]
    pub(crate) fn debug_assert_tree_invariants(&self) {
        if let Err(error) = self.validate_invariants_with(WidgetSlotPolicy::AllowBorrowed) {
            debug_assert!(false, "{error}");
        }
    }

    #[cfg(not(debug_assertions))]
    pub(crate) fn debug_assert_tree_invariants(&self) {}

    /// Validate root node invariants.
    fn validate_root(&self) -> Result<()> {
        let root = self
            .nodes
            .get(self.root)
            .ok_or_else(|| invariant_violation("root node is missing"))?;
        if root.parent.is_some() {
            return Err(invariant_violation("root node has a parent"));
        }
        Ok(())
    }

    /// Validate that the widget slot is present and currently inspectable.
    fn validate_widget_slot(
        &self,
        node_id: NodeId,
        node: &Node,
        policy: WidgetSlotPolicy,
    ) -> Result<()> {
        validate_slot(node_id, node, policy)
    }

    /// Validate parent, child, and keyed child links for a node.
    fn validate_node_links(&self, node_id: NodeId, node: &Node) -> Result<()> {
        let mut seen = HashSet::with_capacity(node.children.len());
        for child in &node.children {
            if !seen.insert(*child) {
                return Err(invariant_violation(format!(
                    "duplicate child {child:?} under {node_id:?}"
                )));
            }
            if *child == node_id {
                return Err(invariant_violation(format!(
                    "node {node_id:?} lists itself as a child"
                )));
            }
            let child_node = self.nodes.get(*child).ok_or_else(|| {
                invariant_violation(format!("child {child:?} under {node_id:?} is missing"))
            })?;
            if child_node.parent != Some(node_id) {
                return Err(invariant_violation(format!(
                    "child {child:?} parent is {:?}, expected {node_id:?}",
                    child_node.parent
                )));
            }
        }

        for (key, child) in &node.child_keys {
            if !node.children.contains(child) {
                return Err(invariant_violation(format!(
                    "child key {key:?} points to non-child {child:?} under {node_id:?}"
                )));
            }
            let child_node = self.nodes.get(*child).ok_or_else(|| {
                invariant_violation(format!(
                    "child key {key:?} under {node_id:?} points to missing {child:?}"
                ))
            })?;
            if child_node.parent != Some(node_id) {
                return Err(invariant_violation(format!(
                    "keyed child {child:?} parent is {:?}, expected {node_id:?}",
                    child_node.parent
                )));
            }
        }

        if let Some(parent) = node.parent {
            let parent_node = self.nodes.get(parent).ok_or_else(|| {
                invariant_violation(format!("parent {parent:?} of {node_id:?} is missing"))
            })?;
            if !parent_node.children.contains(&node_id) {
                return Err(invariant_violation(format!(
                    "parent {parent:?} does not list child {node_id:?}"
                )));
            }
        }

        Ok(())
    }

    /// Validate that the parent chain from a node contains no cycles.
    fn validate_parent_chain(&self, start: NodeId) -> Result<()> {
        let mut seen = HashSet::new();
        let mut current = Some(start);
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(invariant_violation(format!(
                    "parent cycle detected from {start:?} through {id:?}"
                )));
            }
            let node = self.nodes.get(id).ok_or_else(|| {
                invariant_violation(format!(
                    "parent chain from {start:?} references missing node {id:?}"
                ))
            })?;
            current = node.parent;
        }
        Ok(())
    }

    /// Validate focus and mouse capture targets.
    fn validate_focus_and_capture(&self) -> Result<()> {
        if let Some(focus) = self.focus {
            self.validate_attached_target("focus", focus)?;
        }
        if let Some(capture) = self.mouse_capture {
            self.validate_attached_target("mouse capture", capture)?;
        }
        Ok(())
    }

    /// Validate a stored node target that must be attached.
    fn validate_attached_target(&self, label: &str, node_id: NodeId) -> Result<()> {
        if !self.nodes.contains_key(node_id) {
            return Err(invariant_violation(format!(
                "{label} points at missing node {node_id:?}"
            )));
        }
        if !self.is_attached_to_root(node_id) {
            return Err(invariant_violation(format!(
                "{label} points at detached node {node_id:?}"
            )));
        }
        Ok(())
    }

    /// Validate pending target references stored by auxiliary runtime features.
    fn validate_pending_targets(&self) -> Result<()> {
        if let Some((target, pre_focus)) = self.pending_help_request {
            if !self.nodes.contains_key(target) {
                return Err(invariant_violation(format!(
                    "pending help request points at missing target {target:?}"
                )));
            }
            if let Some(pre_focus) = pre_focus
                && !self.nodes.contains_key(pre_focus)
            {
                return Err(invariant_violation(format!(
                    "pending help request points at missing focus {pre_focus:?}"
                )));
            }
        }
        if let Some(target) = self.pending_diagnostic_dump
            && !self.nodes.contains_key(target)
        {
            return Err(invariant_violation(format!(
                "pending diagnostic dump points at missing node {target:?}"
            )));
        }
        Ok(())
    }

    /// Validate lifecycle flags that are independent of widget behavior.
    fn validate_lifecycle_state(&self, node_id: NodeId, node: &Node) -> Result<()> {
        if node.initialized && !node.mounted && self.is_attached_to_root(node_id) {
            return Err(invariant_violation(format!(
                "attached node {node_id:?} is initialized before it is mounted"
            )));
        }
        Ok(())
    }

    /// Validate cached layout and view state for a node.
    fn validate_cached_state(&self, node_id: NodeId, node: &Node) -> Result<()> {
        if node.content_size.w > node.rect.w || node.content_size.h > node.rect.h {
            return Err(invariant_violation(format!(
                "node {node_id:?} content size {:?} exceeds rect {:?}",
                node.content_size, node.rect
            )));
        }
        if node.canvas.w < node.content_size.w || node.canvas.h < node.content_size.h {
            return Err(invariant_violation(format!(
                "node {node_id:?} canvas {:?} is smaller than content {:?}",
                node.canvas, node.content_size
            )));
        }

        let max_scroll_x = node.canvas.w.saturating_sub(node.content_size.w);
        let max_scroll_y = node.canvas.h.saturating_sub(node.content_size.h);
        if node.scroll.x > max_scroll_x || node.scroll.y > max_scroll_y {
            return Err(invariant_violation(format!(
                "node {node_id:?} scroll {:?} exceeds canvas {:?} and content {:?}",
                node.scroll, node.canvas, node.content_size
            )));
        }

        if view_has_cached_state(node.view) {
            self.validate_view_cache(node_id, node)?;
        }

        Ok(())
    }

    /// Validate a computed view cache against node layout caches.
    fn validate_view_cache(&self, node_id: NodeId, node: &Node) -> Result<()> {
        if node.view.canvas != node.canvas {
            return Err(invariant_violation(format!(
                "node {node_id:?} view canvas {:?} does not match node canvas {:?}",
                node.view.canvas, node.canvas
            )));
        }
        if node.view.tl != node.scroll {
            return Err(invariant_violation(format!(
                "node {node_id:?} view scroll {:?} does not match node scroll {:?}",
                node.view.tl, node.scroll
            )));
        }
        if node.view.outer.w != node.rect.w || node.view.outer.h != node.rect.h {
            return Err(invariant_violation(format!(
                "node {node_id:?} view outer size {:?} does not match rect {:?}",
                node.view.outer_size(),
                node.rect
            )));
        }
        if node.view.content.w != node.content_size.w || node.view.content.h != node.content_size.h
        {
            return Err(invariant_violation(format!(
                "node {node_id:?} view content size {:?} does not match {:?}",
                node.view.content_size(),
                node.content_size
            )));
        }
        Ok(())
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
        self.ensure_subtree_widget_slots_available(root_id, "remove subtree")?;

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

    /// Ensure a structural edit will not delete a widget currently owned by a callback guard.
    fn ensure_subtree_widget_slots_available(
        &self,
        root: NodeId,
        operation: &'static str,
    ) -> Result<()> {
        for node_id in self.subtree_pre_order(root) {
            let Some(node) = self.nodes.get(node_id) else {
                continue;
            };
            let Ok(widget) = node.widget.try_borrow() else {
                return Err(self.widget_operation_error(
                    WidgetOperation::access(operation),
                    node_id,
                    &Error::ReentrantWidgetBorrow(node_id),
                ));
            };
            if widget.is_none() {
                return Err(self.widget_operation_error(
                    WidgetOperation::access(operation),
                    node_id,
                    &Error::ReentrantWidgetBorrow(node_id),
                ));
            }
        }
        Ok(())
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
}

/// Build an invariant violation error.
fn invariant_violation(message: impl Into<String>) -> Error {
    Error::Invariant(message.into())
}

/// Return true when a view contains computed cache data.
fn view_has_cached_state(view: View) -> bool {
    view != View::default()
}
