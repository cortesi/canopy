use std::{cell::RefCell, collections::HashSet, rc::Rc};

use super::{focus::FocusRecoveryHint, *};
use crate::{
    core::{
        context::CoreContext,
        node::Node,
        view::View,
        wake::PollOwner,
        widget_access::{WidgetCellPolicy, validate_slot},
    },
    layout::LayoutOverride,
    path::Path,
    widget::Widget,
};

impl TreeStateSnapshot {
    /// Capture structural state that tree hooks can mutate.
    ///
    /// The binding registry is runtime state and is intentionally outside
    /// tree-edit rollback.
    fn capture(core: &Core) -> Self {
        Self {
            removal_checkpoint: core.completion.requests.len(),
            nodes: core.nodes.clone(),
            semantic_keys: core.semantic_keys.clone(),
            modals: core.modals.clone(),
            root: core.root,
            focus: core.focus,
            exit_requested: core.exit_requested,
            pending_style: core.pending_style.clone(),
            mouse_capture: core.mouse_capture,
            deferred_focus_repair: core.deferred_focus_repair,
        }
    }

    /// Restore a previously captured core state.
    fn restore(self, core: &mut Core) {
        core.completion.requests.truncate(self.removal_checkpoint);
        core.nodes = self.nodes;
        core.semantic_keys = self.semantic_keys;
        core.modals = self.modals;
        core.sync_modal_bindings();
        core.root = self.root;
        core.focus = self.focus;
        core.exit_requested = self.exit_requested;
        core.pending_style = self.pending_style;
        core.mouse_capture = self.mouse_capture;
        core.deferred_focus_repair = self.deferred_focus_repair;
    }
}

impl TreeEditJournal {
    /// Start a journal from the current core state.
    fn new(core: &Core) -> Self {
        Self {
            before: TreeStateSnapshot::capture(core),
            mounted: Vec::new(),
            unmounted: HashSet::new(),
        }
    }
}

impl MountedWidget {
    /// Return the stable identity of this widget cell.
    fn identity(&self) -> usize {
        Rc::as_ptr(&self.widget) as usize
    }
}

impl Core {
    /// Allocate an identity that failed structural edits cannot reuse.
    fn next_generation(&mut self) -> u64 {
        let generation = self.next_generation;
        self.next_generation = generation
            .checked_add(1)
            .expect("node generation exhausted");
        generation
    }

    /// Update provisional attachment identities in the subtree at `root`
    /// after it moved.
    ///
    /// Every node in a subtree shares its root's attachment, so an edit
    /// refreshes only the subtree it moved. A node that joins the tree gets a
    /// new generation, and one that leaves loses its generation.
    fn refresh_attachment_generations(&mut self, root: NodeId) {
        let attached = self.is_attached_to_root(root);
        for id in self.subtree_pre_order(root) {
            if self.nodes[id].attachment_generation.is_some() == attached {
                continue;
            }
            let generation = attached.then(|| self.next_generation());
            let node = &mut self.nodes[id];
            node.attachment_generation = generation;
            if node.poll_lifetime == crate::PollLifetime::Attachment {
                node.initialized = false;
            }
        }
    }

    /// Commit live work identities and expire provisional or retired handles.
    fn sync_work_stamps(&self) -> Result<()> {
        self.wake_registry
            .sync(self.nodes.iter().map(|(node, entry)| PollOwner {
                node,
                incarnation: entry.incarnation,
                attachment: entry.attachment_generation,
            }))
    }

    /// Replace all persistent parent constraints for a node.
    pub fn set_layout_override(&mut self, node: NodeId, overrides: LayoutOverride) -> Result<()> {
        let current = self.nodes.get_mut(node).ok_or(Error::NodeNotFound(node))?;
        let layout = overrides.apply(current.base_layout)?;
        if current.layout_override != overrides || current.layout != layout {
            current.layout_override = overrides;
            current.layout = layout;
            self.invalidate(crate::Invalidation::Layout);
        }
        Ok(())
    }

    /// Restore the widget's base layout.
    #[cfg(test)]
    pub fn clear_layout_override_of(&mut self, node: NodeId) -> Result<()> {
        self.set_layout_override(node, LayoutOverride::default())
    }

    /// Override every field of a node's layout.
    #[cfg(test)]
    pub fn set_layout_of(&mut self, node: impl Into<NodeId>, layout: Layout) -> Result<()> {
        self.set_layout_override(node.into(), LayoutOverride::from(layout))
    }

    /// Replace a widget and remove all descendant nodes.
    pub fn replace_subtree<W>(&mut self, node_id: impl Into<NodeId>, widget: W) -> Result<()>
    where
        W: Widget + 'static,
    {
        let node_id = node_id.into();
        self.with_tree_edit("replace subtree", move |core| {
            core.replace_subtree_inner(node_id, Box::new(widget))
        })
    }

    /// Replace one subtree's widget inside an active tree edit.
    fn replace_subtree_inner(&mut self, node_id: NodeId, widget: Box<dyn Widget>) -> Result<()> {
        if !self.nodes.contains_key(node_id) {
            return Err(Error::NodeNotFound(node_id));
        }
        let name = widget.name();
        let layout = widget.layout();
        layout.validate()?;
        let widget_type = widget.as_ref().type_id();
        let poll_lifetime = widget.poll_lifetime();

        let lost = self.delete_subtree(node_id, "replace subtree", true)?;
        self.clear_identity(node_id)?;
        let incarnation = self.next_generation();
        let node = &mut self.nodes[node_id];
        node.incarnation = incarnation;
        node.reveal = None;
        node.reveal_in_ancestors = None;
        node.scroll_order = 0;
        node.attachment_generation = None;
        node.poll_lifetime = poll_lifetime;
        node.widget = Rc::new(RefCell::new(Some(widget)));
        node.name = name;
        node.layout = layout;
        node.base_layout = layout;
        node.layout_override = LayoutOverride::default();
        node.widget_type = widget_type;
        node.mounted = false;
        node.initialized = false;

        self.refresh_attachment_generations(node_id);
        if self.is_attached_to_root(node_id) {
            self.mount_node(node_id)?;
        }
        self.repair_focus_and_capture(lost)
    }

    /// Run the mount hook for a node if it has not been mounted yet.
    pub(crate) fn mount_node(&mut self, node_id: NodeId) -> Result<()> {
        let should_mount = self
            .nodes
            .get(node_id)
            .map(|node| !node.mounted)
            .ok_or(Error::NodeNotFound(node_id))?;
        if !should_mount {
            return Ok(());
        }

        let widget_slot = Rc::clone(&self.nodes[node_id].widget);

        self.with_widget_ctx(node_id, |widget, ctx| widget.on_mount(ctx))??;

        if let Some(node) = self.nodes.get_mut(node_id) {
            node.mounted = true;
        }
        if let Some(journal) = self.tree_edit.as_mut() {
            journal.mounted.push(MountedWidget {
                node_id,
                widget: widget_slot,
            });
        }

        Ok(())
    }

    /// Run a tree edit, joining an active journal or rolling back the outermost
    /// edit on failure.
    pub(crate) fn with_tree_edit<R>(
        &mut self,
        operation: &'static str,
        f: impl FnOnce(&mut Self) -> Result<R>,
    ) -> Result<R> {
        if self.rolling_back_tree_edit {
            return Err(Error::TreeEditDuringRollback { operation });
        }
        self.invalidate(crate::Invalidation::Layout);
        if self.tree_edit.is_some() {
            let before = TreeStateSnapshot::capture(self);
            let (mounted_len, unmounted_before) = {
                let journal = self.tree_edit.as_ref().expect("tree edit journal missing");
                (journal.mounted.len(), journal.unmounted.clone())
            };
            let result = f(self);
            return match result {
                Ok(value) => Ok(value),
                Err(error) => {
                    let (mounted, unmounted) = {
                        let journal = self.tree_edit.as_mut().expect("tree edit journal missing");
                        let mounted = journal.mounted.split_off(mounted_len);
                        let unmounted = journal.unmounted.clone();
                        journal.unmounted = unmounted_before;
                        (mounted, unmounted)
                    };
                    self.unwind_mounted_widgets(&mounted, &unmounted);
                    before.restore(self);
                    // Keep committed registrations until the outer edit
                    // settles.
                    Err(error)
                }
            };
        }

        self.tree_edit = Some(TreeEditJournal::new(self));
        let result = f(self);
        let journal = self.tree_edit.take().expect("tree edit journal missing");

        match result {
            Ok(value) => {
                self.prune_semantic_keys();
                self.sync_work_stamps()?;
                self.retire_invalid_modals()?;
                Ok(value)
            }
            Err(err) => {
                self.rollback_tree_edit(journal)?;
                Err(err)
            }
        }
    }

    /// Unwind completed mounts in reverse order and restore the captured state.
    fn rollback_tree_edit(&mut self, journal: TreeEditJournal) -> Result<()> {
        self.unwind_mounted_widgets(&journal.mounted, &journal.unmounted);
        journal.before.restore(self);
        self.sync_work_stamps()?;
        self.debug_assert_tree_invariants();
        Ok(())
    }

    /// Run rollback cleanup for mounts completed after a journal checkpoint.
    fn unwind_mounted_widgets(&mut self, mounted: &[MountedWidget], unmounted: &HashSet<usize>) {
        self.rolling_back_tree_edit = true;
        for mounted in mounted.iter().rev() {
            if unmounted.contains(&mounted.identity()) {
                continue;
            }
            let Ok(mut widget) = mounted.widget.try_borrow_mut() else {
                debug_assert!(false, "mounted widget borrowed during tree rollback");
                continue;
            };
            let Some(widget) = widget.as_deref_mut() else {
                debug_assert!(false, "mounted widget missing during tree rollback");
                continue;
            };
            let mut ctx = CoreContext::new(self, mounted.node_id);
            widget.on_unmount(&mut ctx);
        }
        self.rolling_back_tree_edit = false;
    }

    /// Run an unmount hook once for a successfully mounted node.
    fn unmount_node(&mut self, node_id: NodeId) -> Result<()> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or(Error::NodeNotFound(node_id))?;
        if !node.mounted {
            return Ok(());
        }
        let widget = Rc::clone(&node.widget);
        self.with_widget_ctx(node_id, |widget, ctx| widget.on_unmount(ctx))?;
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.mounted = false;
        }
        if let Some(journal) = self.tree_edit.as_mut() {
            journal.unmounted.insert(Rc::as_ptr(&widget) as usize);
        }
        Ok(())
    }

    /// Return true if `ancestor` is `node` or appears in the parent chain of
    /// `node`.
    pub(crate) fn is_ancestor_or_self(&self, ancestor: NodeId, node: NodeId) -> bool {
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
        self.is_ancestor_or_self(self.root, node_id.into())
    }

    /// Validate structural and cached state invariants for the core arena.
    pub fn validate_invariants(&self) -> Result<()> {
        self.validate_invariants_with(WidgetCellPolicy::RequirePresent)
    }

    /// Validate core invariants using a caller-specific widget cell policy.
    fn validate_invariants_with(&self, widget_slot_policy: WidgetCellPolicy) -> Result<()> {
        self.validate_root()?;
        for (node_id, node) in self.nodes.iter() {
            validate_slot(node_id, node, widget_slot_policy)?;
            self.validate_node_links(node_id, node)?;
            self.validate_parent_chain(node_id)?;
            self.validate_lifecycle_state(node_id, node)?;
            self.validate_cached_state(node_id, node)?;
        }
        self.validate_focus_and_capture()?;
        self.validate_semantic_keys()?;
        Ok(())
    }

    /// Assert structural invariants on the node tree in debug builds.
    #[cfg(debug_assertions)]
    pub(crate) fn debug_assert_tree_invariants(&self) {
        if let Err(error) = self.validate_invariants_with(WidgetCellPolicy::AllowBorrowed) {
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

        for (key, child) in &node.slots {
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

    /// Validate lifecycle flags and attachment generations, which are
    /// independent of widget behavior.
    fn validate_lifecycle_state(&self, node_id: NodeId, node: &Node) -> Result<()> {
        let attached = self.is_attached_to_root(node_id);
        if node.initialized && !node.mounted && attached {
            return Err(invariant_violation(format!(
                "attached node {node_id:?} is initialized before it is mounted"
            )));
        }
        if node.attachment_generation.is_some() != attached {
            return Err(invariant_violation(format!(
                "node {node_id:?} attachment generation disagrees with its attachment"
            )));
        }
        Ok(())
    }

    /// Validate cached layout and view state for a node.
    fn validate_cached_state(&self, node_id: NodeId, node: &Node) -> Result<()> {
        if node.layout_override.apply(node.base_layout)? != node.layout {
            return Err(invariant_violation(format!(
                "node {node_id:?} effective layout differs from its base and overrides"
            )));
        }
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
        if node.view.scroll != node.scroll {
            return Err(invariant_violation(format!(
                "node {node_id:?} view scroll {:?} does not match node scroll {:?}",
                node.view.scroll, node.scroll
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
    #[cfg(test)]
    pub fn create_detached<W>(&mut self, widget: W) -> Result<NodeId>
    where
        W: Widget + 'static,
    {
        self.create_detached_boxed(Box::new(widget))
    }

    /// Create a node in the arena detached from the tree using a boxed widget.
    pub fn create_detached_boxed(&mut self, widget: Box<dyn Widget>) -> Result<NodeId> {
        if self.rolling_back_tree_edit {
            return Err(Error::TreeEditDuringRollback {
                operation: "create detached",
            });
        }
        let generation = self.next_generation();
        let node = Node::new(widget, generation);
        node.layout.validate()?;
        let id = self.nodes.insert(node);
        self.invalidate(crate::Invalidation::Semantics);
        if self.tree_edit.is_none() {
            self.sync_work_stamps()?;
        }
        Ok(id)
    }

    /// Add a boxed widget as a child of a specific parent and return the new
    /// node ID.
    #[cfg(test)]
    pub fn add_child_to_boxed(
        &mut self,
        parent: impl Into<NodeId>,
        widget: Box<dyn Widget>,
    ) -> Result<NodeId> {
        let parent = parent.into();
        self.with_tree_edit("add child", |core| {
            let child = core.create_detached_boxed(widget)?;
            core.attach_inner(parent, child, None)?;
            Ok(child)
        })
    }

    /// Add a boxed widget as a keyed child of a specific parent and return the
    /// new node ID.
    #[cfg(test)]
    pub fn add_child_to_slot_boxed(
        &mut self,
        parent: impl Into<NodeId>,
        key: &str,
        widget: Box<dyn Widget>,
    ) -> Result<NodeId> {
        let parent = parent.into();
        self.with_tree_edit("add keyed child", |core| {
            if core.child_slot(parent, key).is_some() {
                return Err(Error::DuplicateSlot(key.to_string()));
            }
            let child = core.create_detached_boxed(widget)?;
            core.attach_inner(parent, child, Some(key))?;
            Ok(child)
        })
    }

    /// Return the keyed child under a parent.
    pub fn child_slot(&self, parent: impl Into<NodeId>, key: &str) -> Option<NodeId> {
        self.nodes
            .get(parent.into())
            .and_then(|node| node.slots.get(key).copied())
    }

    /// Attach a detached child under a parent.
    pub fn attach(&mut self, parent: impl Into<NodeId>, child: impl Into<NodeId>) -> Result<()> {
        let parent = parent.into();
        let child = child.into();
        self.with_tree_edit("attach", |core| core.attach_inner(parent, child, None))
    }

    /// Attach a detached child under a parent with a unique key.
    pub fn attach_slot(
        &mut self,
        parent: impl Into<NodeId>,
        key: &str,
        child: impl Into<NodeId>,
    ) -> Result<()> {
        let parent = parent.into();
        let child = child.into();
        self.with_tree_edit("attach keyed", |core| {
            core.attach_inner(parent, child, Some(key))
        })
    }

    /// Attach a child under a parent, optionally tracking a keyed association.
    fn attach_inner(&mut self, parent: NodeId, child: NodeId, key: Option<&str>) -> Result<()> {
        self.attach_topology(parent, child, key)?;
        if self.is_attached_to_root(parent) {
            self.mount_subtree_pre_order(child)?;
        }
        self.repair_focus_and_capture(None)
    }

    /// Update validated topology without running mount callbacks yet.
    fn attach_topology(&mut self, parent: NodeId, child: NodeId, key: Option<&str>) -> Result<()> {
        if !self.nodes.contains_key(parent) {
            return Err(Error::NodeNotFound(parent));
        }
        if self
            .nodes
            .get(child)
            .is_some_and(|node| node.parent.is_some())
        {
            return Err(Error::AlreadyAttached(child));
        }
        if let Some(key) = key
            && self.nodes[parent].slots.contains_key(key)
        {
            return Err(Error::DuplicateSlot(key.to_string()));
        }
        self.validate_attach(parent, child, "attach")?;

        let node = &mut self.nodes[parent];
        if let Some(key) = key {
            node.slots.insert(key.to_string(), child);
        }
        node.children.push(child);
        self.nodes[child].parent = Some(parent);
        self.refresh_attachment_generations(child);
        Ok(())
    }

    /// Check that `child` can move under `parent`: it exists, it is not the
    /// root, and the move makes no cycle. Under an attached parent, the
    /// child's unmounted widgets must be available to mount.
    fn validate_attach(
        &self,
        parent: NodeId,
        child: NodeId,
        operation: &'static str,
    ) -> Result<()> {
        if !self.nodes.contains_key(child) {
            return Err(Error::NodeNotFound(child));
        }
        if child == self.root {
            return Err(Error::Invalid("cannot attach root as a child".into()));
        }
        if self.is_ancestor_or_self(child, parent) {
            return Err(Error::WouldCreateCycle { parent, child });
        }
        if self.is_attached_to_root(parent) {
            self.ensure_unmounted_widget_slots_available(child, operation)?;
        }
        Ok(())
    }

    /// Take `child` out of its parent's children and slots, and clear its
    /// parent link.
    fn unlink(&mut self, child: NodeId) {
        let Some(parent) = self
            .nodes
            .get_mut(child)
            .and_then(|node| node.parent.take())
        else {
            return;
        };
        if let Some(node) = self.nodes.get_mut(parent) {
            node.children.retain(|id| *id != child);
            node.slots.retain(|_, id| *id != child);
        }
    }

    /// Detach a child from its parent if attached.
    pub fn detach(&mut self, child: impl Into<NodeId>) -> Result<()> {
        let child = child.into();
        self.with_tree_edit("detach", |core| {
            let node = core.nodes.get(child).ok_or(Error::NodeNotFound(child))?;
            if node.parent.is_none() {
                return Ok(());
            }
            let lost = core.focus_loss(child);
            core.unlink(child);
            core.refresh_attachment_generations(child);
            core.repair_focus_and_capture(lost)
        })
    }

    /// Mount unmounted nodes in a subtree using pre-order traversal.
    fn mount_subtree_pre_order(&mut self, root: NodeId) -> Result<()> {
        let mut stack = vec![root];
        while let Some(node_id) = stack.pop() {
            if !self.nodes.contains_key(node_id) {
                continue;
            }
            // `mount_node` returns early for nodes that are already mounted.
            self.mount_node(node_id)?;
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

    /// Replace the children list for a parent in the arena tree.
    ///
    /// The list reorders the current children and may add new ones, including
    /// nodes taken from another parent. It must keep every current child: a
    /// caller that wants a child out detaches or removes it first, so no
    /// child is left detached in the arena by omission.
    pub fn set_children(&mut self, parent: impl Into<NodeId>, children: Vec<NodeId>) -> Result<()> {
        let parent = parent.into();
        self.with_tree_edit("set children", move |core| {
            core.set_children_inner(parent, children)
        })
    }

    /// Replace a parent's children inside an active tree edit.
    fn set_children_inner(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()> {
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
        if let Some(omitted) = self.nodes[parent]
            .children
            .iter()
            .find(|child| !seen.contains(*child))
        {
            return Err(Error::Invalid(format!(
                "set children omits child {omitted:?} of {parent:?}; detach or remove it first"
            )));
        }

        for child in &children {
            self.validate_attach(parent, *child, "set children")?;
        }

        let parent_attached = self.is_attached_to_root(parent);
        let moved: Vec<NodeId> = children
            .iter()
            .copied()
            .filter(|child| self.nodes[*child].parent != Some(parent))
            .collect();
        // A focused child that moves under a detached parent leaves the tree.
        let lost = if parent_attached {
            None
        } else {
            moved.iter().find_map(|child| self.focus_loss(*child))
        };
        for child in &moved {
            self.unlink(*child);
            self.nodes[*child].parent = Some(parent);
        }
        self.nodes[parent].children = children.clone();
        for child in &moved {
            self.refresh_attachment_generations(*child);
        }
        if parent_attached {
            for child in children {
                self.mount_subtree_pre_order(child)?;
            }
        }
        self.repair_focus_and_capture(lost)
    }

    /// Remove a node and all descendants from the arena.
    pub fn remove_subtree(&mut self, root_id: impl Into<NodeId>) -> Result<()> {
        let root_id = root_id.into();
        self.with_tree_edit("remove subtree", |core| core.remove_subtree_inner(root_id))
    }

    /// Remove a subtree inside an active tree edit.
    fn remove_subtree_inner(&mut self, root_id: NodeId) -> Result<()> {
        if root_id == self.root {
            return Err(Error::Invalid("cannot remove root".into()));
        }
        if !self.nodes.contains_key(root_id) {
            return Err(Error::NodeNotFound(root_id));
        }
        let lost = self.delete_subtree(root_id, "remove subtree", false)?;
        self.repair_focus_and_capture(lost)
    }

    /// Run removal hooks for the subtree at `root`, then delete its nodes.
    ///
    /// Removal and replacement share this. With `keep_root`, the root node
    /// stays in place without children, ready for a replacement widget.
    /// Returns the focus recovery candidates when the deleted nodes held
    /// focus.
    fn delete_subtree(
        &mut self,
        root: NodeId,
        operation: &'static str,
        keep_root: bool,
    ) -> Result<Option<FocusRecoveryHint>> {
        let plan = self.plan_subtree_removal(root, operation)?;
        // Focus is lost from the topmost deleted node on its path. A replaced
        // root survives, so that is its child on the path.
        let lost_root = if keep_root {
            self.focus.and_then(|focus| {
                self.nodes[root]
                    .children
                    .iter()
                    .copied()
                    .find(|child| self.is_ancestor_or_self(*child, focus))
            })
        } else {
            Some(root)
        };
        let lost = lost_root.and_then(|node| self.focus_loss(node));
        self.run_removal_hooks(&plan)?;

        if keep_root {
            let node = &mut self.nodes[root];
            node.children.clear();
            node.slots.clear();
        } else {
            self.unlink(root);
        }
        for node_id in plan.post_order {
            if !keep_root || node_id != root {
                self.clear_identity(node_id)?;
                self.nodes.remove(node_id);
            }
        }
        Ok(lost)
    }

    /// Build a stable plan for removing a complete subtree.
    fn plan_subtree_removal(&self, root: NodeId, operation: &'static str) -> Result<RemovalPlan> {
        let pre_order = self
            .subtree_pre_order(root)
            .into_iter()
            .map(|node_id| {
                self.ensure_widget_slot_available(node_id, operation)?;
                let node = self
                    .nodes
                    .get(node_id)
                    .ok_or(Error::NodeNotFound(node_id))?;
                Ok(RemovalEntry {
                    node_id,
                    widget: Rc::clone(&node.widget),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(RemovalPlan {
            root,
            pre_order,
            post_order: self.subtree_post_order(root),
        })
    }

    /// Run the full fallible removal hook sequence for a plan.
    fn run_removal_hooks(&mut self, plan: &RemovalPlan) -> Result<()> {
        self.run_pre_remove_plan(plan)?;
        self.validate_removal_plan(plan)?;
        self.run_unmount_plan(plan)?;
        self.validate_removal_plan(plan)?;
        Ok(())
    }

    /// Run fallible removal hooks in deterministic pre-order.
    fn run_pre_remove_plan(&mut self, plan: &RemovalPlan) -> Result<()> {
        for entry in &plan.pre_order {
            self.with_widget_ctx(entry.node_id, |widget, ctx| widget.pre_remove(ctx))??;
        }
        Ok(())
    }

    /// Run unmount hooks in deterministic post-order.
    fn run_unmount_plan(&mut self, plan: &RemovalPlan) -> Result<()> {
        for node_id in &plan.post_order {
            self.unmount_node(*node_id)?;
        }
        Ok(())
    }

    /// Confirm lifecycle hooks did not replace or reshape the planned removal
    /// target.
    fn validate_removal_plan(&self, plan: &RemovalPlan) -> Result<()> {
        if !self
            .subtree_pre_order(plan.root)
            .iter()
            .eq(plan.pre_order.iter().map(|entry| &entry.node_id))
        {
            return Err(Error::Invalid(
                "removal target changed during lifecycle hooks".into(),
            ));
        }
        for entry in &plan.pre_order {
            let node = self
                .nodes
                .get(entry.node_id)
                .ok_or(Error::NodeNotFound(entry.node_id))?;
            if !Rc::ptr_eq(&node.widget, &entry.widget) {
                return Err(Error::Invalid(
                    "removal widget changed during lifecycle hooks".into(),
                ));
            }
        }
        Ok(())
    }

    /// Collect a subtree in pre-order, including the root.
    pub(crate) fn subtree_pre_order(&self, root: NodeId) -> Vec<NodeId> {
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

    /// Ensure every unmounted widget in a subtree is available before topology
    /// publication.
    fn ensure_unmounted_widget_slots_available(
        &self,
        root: NodeId,
        operation: &'static str,
    ) -> Result<()> {
        for node_id in self.subtree_pre_order(root) {
            if self.nodes.get(node_id).is_some_and(|node| !node.mounted) {
                self.ensure_widget_slot_available(node_id, operation)?;
            }
        }
        Ok(())
    }

    /// Ensure one widget cell is present and not held by a callback.
    fn ensure_widget_slot_available(&self, node_id: NodeId, operation: &'static str) -> Result<()> {
        self.with_widget(
            node_id,
            WidgetOperation::access(operation),
            |_widget, _core| (),
        )
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

    /// Set a node's hidden flag.
    pub fn set_hidden(
        &mut self,
        node_id: impl Into<NodeId>,
        hidden: bool,
    ) -> Result<ChangeOutcome> {
        let node_id = node_id.into();
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or(Error::NodeNotFound(node_id))?;
        let changed = node.hidden != hidden;
        node.hidden = hidden;
        if changed {
            self.invalidate(crate::Invalidation::Layout);
            self.repair_focus_and_capture(None)?;
            Ok(ChangeOutcome::Changed)
        } else {
            Ok(ChangeOutcome::Unchanged)
        }
    }

    /// Return the path for a node relative to a root.
    pub fn path_of(&self, root: impl Into<NodeId>, node_id: impl Into<NodeId>) -> Path {
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
    Error::Internal(message.into())
}

/// Return true when a view contains computed cache data.
fn view_has_cached_state(view: View) -> bool {
    view != View::default()
}
