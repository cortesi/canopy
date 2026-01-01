use crate::{
    core::{
        context::CoreViewContext,
        id::NodeId,
        world::Core,
    },
    geom::{Direction, RectI32},
    path::Path,
};

#[derive(Clone, Copy)]
/// Preferred focus recovery candidates around a removed subtree.
pub struct FocusRecoveryHint {
    /// Next focusable node after the removed subtree.
    pub next: Option<NodeId>,
    /// Previous focusable node before the removed subtree.
    pub prev: Option<NodeId>,
    /// Focusable ancestor of the removed subtree.
    pub ancestor: Option<NodeId>,
}

/// Trait for managing focus and mouse capture.
pub trait FocusManager {
    /// Check whether a node is on the focus path.
    fn is_on_focus_path(&self, node: NodeId) -> bool;

    /// Does the node have terminal focus?
    fn is_focused(&self, node: NodeId) -> bool;

    /// Focus a node. Returns `true` if focus changed.
    fn set_focus(&mut self, node: NodeId) -> bool;

    /// Return the focus path for the subtree under `root`.
    fn focus_path(&self, root: NodeId) -> Path;

    /// Focus the first node that accepts focus in the pre-order traversal of the subtree at root.
    fn focus_first(&mut self, root: NodeId);

    /// Focus the next node in the pre-order traversal of root.
    fn focus_next(&mut self, root: NodeId);

    /// Focus the previous node in the pre-order traversal of `root`.
    fn focus_prev(&mut self, root: NodeId);

    /// Move focus in a specified direction within the subtree at root.
    fn focus_dir(&mut self, root: NodeId, dir: Direction);

    /// Ensure the focus invariant is satisfied.
    fn ensure_focus_valid(&mut self, removed_root: Option<NodeId>);

    /// Ensure mouse capture only points at attached nodes.
    fn ensure_mouse_capture_valid(&mut self);

    /// Ensure focus and mouse capture invariants after structural changes.
    fn ensure_invariants(&mut self, removed_root: Option<NodeId>);

    /// Return the focus path as node IDs from root to focus.
    fn focus_path_ids(&self) -> Vec<NodeId>;

    /// Precompute focus recovery candidates for a removed subtree.
    fn focus_recovery_hint(&self, removed_root: NodeId) -> FocusRecoveryHint;

    /// Return the next focusable node after the subtree rooted at `removed_root`.
    fn next_focusable_after_subtree(&self, removed_root: NodeId) -> Option<NodeId>;

    /// Return the previous focusable node before the subtree rooted at `removed_root`.
    fn prev_focusable_before_subtree(&self, removed_root: NodeId) -> Option<NodeId>;

    /// Return the nearest focusable ancestor of `start`.
    fn nearest_focusable_ancestor(&self, start: NodeId) -> Option<NodeId>;
}

impl FocusManager for Core {
    fn is_on_focus_path(&self, node: NodeId) -> bool {
        let mut current = self.focus;
        while let Some(id) = current {
            if id == node {
                return true;
            }
            current = self.nodes[id].parent;
        }
        false
    }

    fn is_focused(&self, node: NodeId) -> bool {
        self.focus == Some(node)
    }

    fn set_focus(&mut self, node: NodeId) -> bool {
        if self.is_focused(node) {
            false
        } else {
            self.focus_gen = self.focus_gen.saturating_add(1);
            self.focus = Some(node);
            true
        }
    }

    fn focus_path(&self, root: NodeId) -> Path {
        let mut parts = Vec::new();
        let mut current = self.focus;
        while let Some(id) = current {
            parts.push(self.nodes[id].name.to_string());
            if id == root {
                break;
            }
            current = self.nodes[id].parent;
        }
        if current != Some(root) {
            return Path::empty();
        }
        parts.reverse();
        Path::new(parts)
    }

    fn focus_first(&mut self, root: NodeId) {
        if let Some(target) = first_focusable(self, root) {
            self.set_focus(target);
        }
    }

    fn focus_next(&mut self, root: NodeId) {
        if let Some(current) = self.focus {
            if let Some(target) = find_next_focus(self, root, current, false) {
                self.set_focus(target);
                return;
            }
        }
        if let Some(target) = first_focusable(self, root) {
            self.set_focus(target);
        } else {
            self.focus = None;
        }
    }

    fn focus_prev(&mut self, root: NodeId) {
        if let Some(current) = self.focus {
            if let Some(target) = find_prev_focus(self, root, current) {
                self.set_focus(target);
                return;
            }
        }
        
        if let Some(last) = find_last_focusable(self, root) {
            self.set_focus(last);
        } else {
            self.focus = None;
        }
    }

    fn focus_dir(&mut self, root: NodeId, dir: Direction) {
        let mut focusables = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let Some(node) = self.nodes.get(id) else {
                continue;
            };
            if node.hidden {
                continue;
            }
            if is_focus_candidate(self, id, true) {
                focusables.push(id);
            }
            for child in node.children.iter().rev() {
                stack.push(*child);
            }
        }

        let current = match self.focus {
            Some(id) => id,
            None => {
                if let Some(first) = focusables.first().copied() {
                    self.set_focus(first);
                }
                return;
            }
        };

        let current_rect = match self.nodes.get(current).map(|n| n.view.outer) {
            Some(r) => r,
            None => return,
        };

        let mut candidates: Vec<(NodeId, RectI32)> = focusables
            .into_iter()
            .filter(|id| *id != current)
            .filter_map(|id| self.nodes.get(id).map(|n| (id, n.view.outer)))
            .collect();

        let current_center = current_rect.center();

        candidates.retain(|(_, rect)| {
            let center = rect.center();
            match dir {
                Direction::Right | Direction::Left => {
                    let center_ok = match dir {
                        Direction::Right => center.0 > current_center.0,
                        Direction::Left => center.0 < current_center.0,
                        _ => false,
                    };
                    let vertical_overlap = rect.overlaps_vertical(current_rect);
                    center_ok && vertical_overlap
                }
                Direction::Down | Direction::Up => {
                    let center_ok = match dir {
                        Direction::Down => center.1 > current_center.1,
                        Direction::Up => center.1 < current_center.1,
                        _ => false,
                    };
                    let horizontal_overlap = rect.overlaps_horizontal(current_rect);
                    center_ok && horizontal_overlap
                }
            }
        });

        if candidates.is_empty() {
            return;
        }

        candidates.sort_by_key(|(_, rect)| match dir {
            Direction::Right => {
                let edge_dist = (rect.left() - current_rect.right()).max(0) as u64;
                let vert_center_dist = current_center.1.abs_diff(rect.center().1) as u64;
                edge_dist * 10000 + vert_center_dist
            }
            Direction::Left => {
                let edge_dist = (current_rect.left() - rect.right()).max(0) as u64;
                let vert_center_dist = current_center.1.abs_diff(rect.center().1) as u64;
                edge_dist * 10000 + vert_center_dist
            }
            Direction::Down => {
                let edge_dist = (rect.top() - current_rect.bottom()).max(0) as u64;
                let horiz_center_dist = current_center.0.abs_diff(rect.center().0) as u64;
                edge_dist * 10000 + horiz_center_dist
            }
            Direction::Up => {
                let edge_dist = (current_rect.top() - rect.bottom()).max(0) as u64;
                let horiz_center_dist = current_center.0.abs_diff(rect.center().0) as u64;
                edge_dist * 10000 + horiz_center_dist
            }
        });

        if let Some((target, _)) = candidates.first().copied() {
            self.set_focus(target);
        }
    }

    fn ensure_focus_valid(&mut self, removed_root: Option<NodeId>) {
        let Some(focus) = self.focus else {
            self.focus_hint = None;
            return;
        };

        if self.is_attached_to_root(focus) && is_focus_candidate(self, focus, true) {
            self.focus_hint = None;
            return;
        }

        let hint = self.focus_hint.take();
        if let Some(removed_root) = removed_root {
            let candidate = if let Some(hint) = hint {
                hint.next.or(hint.prev).or(hint.ancestor)
            } else {
                self.next_focusable_after_subtree(removed_root)
                    .or_else(|| self.prev_focusable_before_subtree(removed_root))
                    .or_else(|| self.nearest_focusable_ancestor(removed_root))
            };
            if let Some(target) = candidate {
                self.set_focus(target);
            } else {
                self.focus = None;
            }
            return;
        }

        let candidate = find_next_focus(self, self.root, focus, false)
            .or_else(|| first_focusable(self, self.root));
        if let Some(target) = candidate {
            self.set_focus(target);
        } else {
            self.focus = None;
        }
    }

    fn ensure_mouse_capture_valid(&mut self) {
       if let Some(capture) = self.mouse_capture
            && (!self.nodes.contains_key(capture) || !self.is_attached_to_root(capture))
        {
            self.mouse_capture = None;
        }
    }

    fn ensure_invariants(&mut self, removed_root: Option<NodeId>) {
        self.ensure_focus_valid(removed_root);
        self.ensure_mouse_capture_valid();
    }

    fn focus_path_ids(&self) -> Vec<NodeId> {
        let mut ids = Vec::new();
        let mut current = self.focus;
        while let Some(id) = current {
            ids.push(id);
            current = self.nodes.get(id).and_then(|n| n.parent);
        }
        ids.reverse();
        ids
    }

    fn focus_recovery_hint(&self, removed_root: NodeId) -> FocusRecoveryHint {
        FocusRecoveryHint {
            next: self.next_focusable_after_subtree(removed_root),
            prev: self.prev_focusable_before_subtree(removed_root),
            ancestor: self.nearest_focusable_ancestor(removed_root),
        }
    }

    fn next_focusable_after_subtree(&self, removed_root: NodeId) -> Option<NodeId> {
        if !self.is_attached_to_root(removed_root) {
            return None;
        }
        find_next_focus(self, self.root, removed_root, true)
    }

    fn prev_focusable_before_subtree(&self, removed_root: NodeId) -> Option<NodeId> {
        if !self.is_attached_to_root(removed_root) {
            return None;
        }
        find_prev_focus(self, self.root, removed_root)
    }

    fn nearest_focusable_ancestor(&self, start: NodeId) -> Option<NodeId> {
        nearest_focusable_ancestor_with(self, start, true)
            .or_else(|| nearest_focusable_ancestor_with(self, start, false))
    }
}

// Private helper functions

fn first_focusable(core: &Core, root: NodeId) -> Option<NodeId> {
    first_focusable_with(core, root, true)
        .or_else(|| first_focusable_with(core, root, false))
}

fn first_focusable_with(core: &Core, root: NodeId, require_view: bool) -> Option<NodeId> {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let Some(node) = core.nodes.get(id) else {
            continue;
        };
        if is_focus_candidate(core, id, require_view) {
            return Some(id);
        }
        for child in node.children.iter().rev() {
            stack.push(*child);
        }
    }
    None
}

/// Find next focusable node after `target`. 
/// If `skip_subtree` is true, traversal skips `target`'s children.
fn find_next_focus(core: &Core, root: NodeId, target: NodeId, skip_subtree: bool) -> Option<NodeId> {
    find_next_focus_with(core, root, target, skip_subtree, true)
        .or_else(|| find_next_focus_with(core, root, target, skip_subtree, false))
}

fn find_next_focus_with(
    core: &Core, 
    root: NodeId, 
    target: NodeId, 
    skip_subtree: bool, 
    require_view: bool
) -> Option<NodeId> {
    let mut stack = vec![root];
    let mut past_target = false;
    while let Some(id) = stack.pop() {
        if id == target {
            past_target = true;
            if skip_subtree {
                continue;
            }
        } else if past_target && is_focus_candidate(core, id, require_view) {
            return Some(id);
        }
        
        if let Some(node) = core.nodes.get(id) {
            for child in node.children.iter().rev() {
                stack.push(*child);
            }
        }
    }
    None
}

/// Find the last focusable node before `target` in pre-order.
fn find_prev_focus(core: &Core, root: NodeId, target: NodeId) -> Option<NodeId> {
    find_prev_focus_with(core, root, Some(target), true)
        .or_else(|| find_prev_focus_with(core, root, Some(target), false))
}

fn find_last_focusable(core: &Core, root: NodeId) -> Option<NodeId> {
    find_prev_focus_with(core, root, None, true)
        .or_else(|| find_prev_focus_with(core, root, None, false))
}

fn find_prev_focus_with(
    core: &Core,
    root: NodeId,
    target: Option<NodeId>,
    require_view: bool
) -> Option<NodeId> {
    let mut prev = None;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(t) = target {
            if id == t {
                break;
            }
        }
        if is_focus_candidate(core, id, require_view) {
            prev = Some(id);
        }
        if let Some(node) = core.nodes.get(id) {
            for child in node.children.iter().rev() {
                stack.push(*child);
            }
        }
    }
    prev
}

fn nearest_focusable_ancestor_with(core: &Core, start: NodeId, require_view: bool) -> Option<NodeId> {
    let mut current = core.nodes.get(start).and_then(|node| node.parent);
    while let Some(id) = current {
        if is_focus_candidate(core, id, require_view) {
            return Some(id);
        }
        current = core.nodes.get(id).and_then(|node| node.parent);
    }
    None
}

fn is_focus_candidate(core: &Core, node_id: NodeId, require_view: bool) -> bool {
    let Some(node) = core.nodes.get(node_id) else {
        return false;
    };
    if node.hidden {
        return false;
    }
    if require_view && node.view.is_zero() {
        return false;
    }
    // Inline node_accepts_focus
    node.widget.as_ref().is_some_and(|widget| {
        let ctx = CoreViewContext::new(core, node_id);
        widget.accept_focus(&ctx)
    })
}
