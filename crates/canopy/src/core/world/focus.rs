use super::Core;
use crate::{
    ChangeOutcome, FocusDirection, RevealAlign,
    core::{context::CoreViewContext, id::NodeId, widget_access::WidgetReadGuard},
    error::{Error, Result},
    geom::RectI32,
    layout::Display,
    path::Path,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Focus recovery candidates around a subtree that an edit takes out of the
/// tree while it holds focus.
pub(super) struct FocusRecoveryHint {
    /// Next focusable node after the subtree.
    next: Option<NodeId>,
    /// Previous focusable node before the subtree.
    prev: Option<NodeId>,
    /// Focusable ancestor of the subtree.
    ancestor: Option<NodeId>,
}

/// A focus repair that waits until mutable callbacks return their widget
/// cells.
///
/// A callback holds its widget's cell, and a widget without its cell cannot
/// answer `accept_focus`. A repair inside a callback would skip that widget, so
/// an edit made inside a callback records the repair instead. The repair runs
/// once every cell has returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeferredFocusRepair {
    /// Focus stays on an attached node that may no longer hold it.
    Check,
    /// An edit took the focused node out of the tree, and focus was cleared.
    /// The hint, when the edit recorded one, names the recovery candidates.
    Recover(Option<FocusRecoveryHint>),
}

impl Core {
    /// Check whether a node is on the focus path.
    pub fn is_on_focus_path(&self, node: NodeId) -> bool {
        self.focus
            .is_some_and(|focus| self.is_ancestor_or_self(node, focus))
    }

    /// Does the node have terminal focus?
    pub fn is_focused(&self, node: NodeId) -> bool {
        self.focus == Some(node)
    }

    /// Focus an attached node.
    pub fn set_focus(&mut self, node: NodeId) -> Result<ChangeOutcome> {
        self.transition_focus(Some(node))
    }

    /// Apply one validated focus transition. `None` clears focus.
    pub(crate) fn transition_focus(&mut self, target: Option<NodeId>) -> Result<ChangeOutcome> {
        if let Some(node) = target {
            self.validate_attached_node(node)?;
            if !self.interaction_admits(node) {
                return Err(Error::InvalidOperation(
                    "node is outside the active modal".into(),
                ));
            }
        }
        if self.focus == target {
            Ok(ChangeOutcome::Unchanged)
        } else {
            self.focus = target;
            if let Some(node) = target {
                self.queue_node_reveal(node, RevealAlign::Nearest);
            }
            self.invalidate(crate::Invalidation::Paint);
            Ok(ChangeOutcome::Changed)
        }
    }

    /// Validate that a node exists and belongs to the active tree.
    pub(crate) fn validate_attached_node(&self, node: NodeId) -> Result<()> {
        if !self.nodes.contains_key(node) {
            return Err(Error::NodeNotFound(node));
        }
        if !self.is_attached_to_root(node) {
            return Err(Error::NodeDetached(node));
        }
        Ok(())
    }

    /// Return whether a node's widget accepts focus.
    ///
    /// A widget whose cell a callback holds cannot answer, and does not.
    pub(crate) fn accepts_focus(&self, node: NodeId) -> bool {
        focus_acceptance(self, node).unwrap_or(false)
    }

    /// Return the focus path for the subtree under `root`.
    pub fn focus_path(&self, root: NodeId) -> Path {
        self.focus
            .map_or_else(Path::empty, |focus| self.path_of(root, focus))
    }

    /// Collect the focusable leaves under `root` in pre-order.
    pub fn focusable_leaves(&self, root: NodeId) -> Vec<NodeId> {
        self.subtree_pre_order(root)
            .into_iter()
            .filter(|id| is_focus_candidate(self, *id, true))
            .collect()
    }

    /// Return the focused node when it is a focusable leaf under `root`.
    pub fn focused_leaf(&self, root: NodeId) -> Option<NodeId> {
        let focused = self.focus?;
        (is_focus_candidate(self, focused, true) && self.is_ancestor_or_self(root, focused))
            .then_some(focused)
    }

    /// Focus the first node that accepts focus in the pre-order traversal of
    /// the subtree at root.
    pub fn focus_first(&mut self, root: NodeId) -> Result<ChangeOutcome> {
        if let Some(target) = first_focusable(self, root) {
            self.set_focus(target)
        } else {
            Ok(ChangeOutcome::Unchanged)
        }
    }

    /// Focus the next node in the pre-order traversal of root.
    pub fn focus_next(&mut self, root: NodeId) -> Result<ChangeOutcome> {
        if let Some(current) = self.focus
            && let Some(target) = find_next_focus(self, root, current, false)
        {
            return self.set_focus(target);
        }
        self.transition_focus(first_focusable(self, root))
    }

    /// Focus the previous node in the pre-order traversal of `root`.
    pub fn focus_prev(&mut self, root: NodeId) -> Result<ChangeOutcome> {
        if let Some(current) = self.focus
            && let Some(target) = find_prev_focus(self, root, current)
        {
            return self.set_focus(target);
        }

        self.transition_focus(find_last_focusable(self, root))
    }

    /// Move focus within the subtree at `root`.
    pub fn focus_move(&mut self, root: NodeId, direction: FocusDirection) -> Result<ChangeOutcome> {
        match direction {
            FocusDirection::Next => self.focus_next(root),
            FocusDirection::Prev => self.focus_prev(root),
            direction => self.focus_dir(root, direction),
        }
    }

    /// Move focus in a specified direction within the subtree at root.
    fn focus_dir(&mut self, root: NodeId, dir: FocusDirection) -> Result<ChangeOutcome> {
        let focusables = self.focusable_leaves(root);

        let current = match self.focus {
            Some(id) => id,
            None => {
                if let Some(first) = focusables.first().copied() {
                    return self.set_focus(first);
                }
                return Ok(ChangeOutcome::Unchanged);
            }
        };

        let current_rect = match self.nodes.get(current).map(|n| n.view.outer) {
            Some(r) => r,
            None => return Ok(ChangeOutcome::Unchanged),
        };

        let current_center = current_rect.center();
        let target = focusables
            .into_iter()
            .filter(|id| *id != current)
            .filter_map(|id| self.nodes.get(id).map(|n| (id, n.view.outer)))
            .filter_map(|(id, rect)| {
                focus_dir_key(dir, current_rect, current_center, rect).map(|key| (id, key))
            })
            .min_by_key(|(_, key)| *key)
            .map(|(id, _)| id);

        if let Some(target) = target {
            self.set_focus(target)
        } else {
            Ok(ChangeOutcome::Unchanged)
        }
    }

    /// Ensure focus rests on an attached, visible node that accepts focus and
    /// has a view.
    ///
    /// Layout runs this once it publishes views, so focus leaves a node that
    /// layout gave no area.
    pub fn ensure_focus_valid(&mut self) -> Result<ChangeOutcome> {
        self.repair_focus(None, true)
    }

    /// Move focus off a node that is detached, hidden, or refuses focus. With
    /// `require_view`, also move it off a node without a view.
    ///
    /// `lost` carries the recovery candidates when an edit took the focused
    /// node out of the tree.
    fn repair_focus(
        &mut self,
        lost: Option<FocusRecoveryHint>,
        require_view: bool,
    ) -> Result<ChangeOutcome> {
        let Some(focus) = self.focus else {
            return Ok(ChangeOutcome::Unchanged);
        };
        let retain = Candidates {
            require_view,
            admit_taken: true,
        };
        if retain.admits(self, focus) {
            return Ok(ChangeOutcome::Unchanged);
        }
        let candidate = match lost {
            Some(hint) => self.recovery_candidate(Some(hint), require_view),
            None => find_next_focus(self, self.root, focus, false)
                .or_else(|| first_focusable(self, self.root)),
        };
        self.transition_focus(candidate)
    }

    /// Choose where focus goes after it left the tree: the first hint
    /// candidate that can hold focus now, else the first focusable node.
    fn recovery_candidate(
        &self,
        hint: Option<FocusRecoveryHint>,
        require_view: bool,
    ) -> Option<NodeId> {
        let accept = Candidates {
            require_view,
            admit_taken: false,
        };
        hint.into_iter()
            .flat_map(|hint| [hint.next, hint.prev, hint.ancestor])
            .flatten()
            .find(|node| accept.admits(self, *node))
            .or_else(|| first_focusable(self, self.root))
    }

    /// Repair focus and mouse capture after a structural change.
    ///
    /// `lost` carries the recovery candidates when the change took the focused
    /// node out of the tree; [`Core::focus_loss`] records them. Inside a
    /// mutable callback the focus repair waits until every widget cell
    /// returns, as [`DeferredFocusRepair`] describes. Mouse capture involves no
    /// widget, so its repair never waits.
    pub(super) fn repair_focus_and_capture(
        &mut self,
        lost: Option<FocusRecoveryHint>,
    ) -> Result<()> {
        // Views are stale between layouts, and a node added or shown since the
        // last layout has none, so a structural change does not judge focus by
        // its view. The next layout does.
        if self.callback_depth == 0 {
            self.repair_focus(lost, false)?;
        } else {
            self.defer_focus_repair(lost)?;
        }
        self.ensure_mouse_capture_valid()?;
        self.debug_assert_tree_invariants();
        Ok(())
    }

    /// Record a focus repair until mutable callbacks return their widget
    /// cells.
    ///
    /// Focus that left the tree clears now, so it never names a node outside
    /// the tree. Focus on a node still in the tree stays until the repair
    /// runs.
    fn defer_focus_repair(&mut self, lost: Option<FocusRecoveryHint>) -> Result<()> {
        let Some(focus) = self.focus else {
            return Ok(());
        };
        if self.is_attached_to_root(focus) {
            let retain = Candidates {
                require_view: false,
                admit_taken: true,
            };
            if !retain.admits(self, focus) {
                self.deferred_focus_repair = Some(DeferredFocusRepair::Check);
            }
            return Ok(());
        }
        self.deferred_focus_repair = Some(DeferredFocusRepair::Recover(lost));
        self.transition_focus(None)?;
        Ok(())
    }

    /// Run the deferred focus repair, if any, once every widget cell has
    /// returned.
    ///
    /// A callback that set focus after its edit keeps that focus, subject to
    /// the usual check.
    pub(super) fn run_deferred_focus_repair(&mut self) -> Result<()> {
        match self.deferred_focus_repair.take() {
            None => Ok(()),
            Some(DeferredFocusRepair::Recover(hint)) if self.focus.is_none() => {
                let candidate = self.recovery_candidate(hint, false);
                self.transition_focus(candidate).map(drop)
            }
            Some(_) => self.repair_focus(None, false).map(drop),
        }
    }

    /// Record recovery candidates when an edit is about to take the subtree
    /// at `root` out of the tree, if the subtree holds focus.
    ///
    /// Removal, replacement, and detachment all record this hint. The
    /// candidates count a widget whose cell a callback holds as focusable,
    /// since it cannot answer until its cell returns; recovery checks each
    /// candidate again.
    pub(super) fn focus_loss(&self, root: NodeId) -> Option<FocusRecoveryHint> {
        let focus = self.focus?;
        if !self.is_ancestor_or_self(root, focus) {
            return None;
        }
        Some(FocusRecoveryHint {
            next: prefer_view(true, |accept| {
                find_next_with(self, self.root, root, true, accept)
            }),
            prev: prefer_view(true, |accept| {
                find_prev_with(self, self.root, Some(root), accept)
            }),
            ancestor: prefer_view(true, |accept| nearest_ancestor_with(self, root, accept)),
        })
    }

    /// Capture mouse events for an attached node.
    pub fn capture_mouse(&mut self, node: NodeId) -> Result<ChangeOutcome> {
        self.transition_mouse_capture(Some(node))
    }

    /// Release mouse capture when held by the requesting node.
    pub fn release_mouse(&mut self, requester: NodeId) -> Result<ChangeOutcome> {
        self.validate_attached_node(requester)?;
        if self.mouse_capture == Some(requester) {
            self.transition_mouse_capture(None)
        } else {
            Ok(ChangeOutcome::Unchanged)
        }
    }

    /// Clear mouse capture without a requester.
    pub fn clear_mouse_capture(&mut self) -> Result<ChangeOutcome> {
        self.transition_mouse_capture(None)
    }

    /// Apply one validated mouse-capture transition.
    fn transition_mouse_capture(&mut self, target: Option<NodeId>) -> Result<ChangeOutcome> {
        if let Some(node) = target {
            self.validate_attached_node(node)?;
            if !self.interaction_admits(node) {
                return Err(Error::InvalidOperation(
                    "node is outside the active modal".into(),
                ));
            }
        }
        if self.mouse_capture == target {
            Ok(ChangeOutcome::Unchanged)
        } else {
            self.mouse_capture = target;
            self.invalidate(crate::Invalidation::Semantics);
            Ok(ChangeOutcome::Changed)
        }
    }

    /// Ensure mouse capture only points at attached nodes.
    pub fn ensure_mouse_capture_valid(&mut self) -> Result<ChangeOutcome> {
        // `is_attached_to_root` is already false for a missing id.
        if let Some(capture) = self.mouse_capture
            && !self.is_attached_to_root(capture)
        {
            self.clear_mouse_capture()
        } else {
            Ok(ChangeOutcome::Unchanged)
        }
    }
}

/// Which nodes a focus search accepts.
#[derive(Clone, Copy)]
struct Candidates {
    /// Require a laid-out view.
    require_view: bool,
    /// Count a widget whose cell a callback holds as accepting focus.
    ///
    /// Such a widget cannot answer `accept_focus` until the callback returns.
    /// Established focus stays on it, and recovery hints may name it, because
    /// both are checked again once the cell returns. Candidate discovery
    /// otherwise rejects it. Structural and interaction constraints never
    /// wait.
    admit_taken: bool,
}

impl Candidates {
    /// Return whether `node` qualifies.
    fn admits(self, core: &Core, node: NodeId) -> bool {
        is_focus_position_valid(core, node, self.require_view)
            && focus_acceptance(core, node).unwrap_or(self.admit_taken)
    }
}

/// Run a focus search that prefers nodes with a view, then accepts any.
fn prefer_view(admit_taken: bool, search: impl Fn(Candidates) -> Option<NodeId>) -> Option<NodeId> {
    search(Candidates {
        require_view: true,
        admit_taken,
    })
    .or_else(|| {
        search(Candidates {
            require_view: false,
            admit_taken,
        })
    })
}

/// Return the first focusable node under `root`, preferring nodes with views.
fn first_focusable(core: &Core, root: NodeId) -> Option<NodeId> {
    prefer_view(false, |accept| {
        core.subtree_pre_order(root)
            .into_iter()
            .find(|id| accept.admits(core, *id))
    })
}

/// Find the next focusable node after `target`, preferring nodes with views.
/// If `skip_subtree` is true, traversal skips `target`'s children.
fn find_next_focus(
    core: &Core,
    root: NodeId,
    target: NodeId,
    skip_subtree: bool,
) -> Option<NodeId> {
    prefer_view(false, |accept| {
        find_next_with(core, root, target, skip_subtree, accept)
    })
}

/// Find the next node after `target` in pre-order that `accept` admits.
fn find_next_with(
    core: &Core,
    root: NodeId,
    target: NodeId,
    skip_subtree: bool,
    accept: Candidates,
) -> Option<NodeId> {
    let mut past_target = false;
    for id in core.subtree_pre_order(root) {
        if id == target {
            past_target = true;
            continue;
        }
        if !past_target {
            continue;
        }
        if skip_subtree && core.is_ancestor_or_self(target, id) {
            continue;
        }
        if accept.admits(core, id) {
            return Some(id);
        }
    }
    None
}

/// Find the last focusable node before `target` in pre-order, preferring
/// nodes with views.
fn find_prev_focus(core: &Core, root: NodeId, target: NodeId) -> Option<NodeId> {
    prefer_view(false, |accept| {
        find_prev_with(core, root, Some(target), accept)
    })
}

/// Find the last focusable node under `root`, preferring nodes with views.
fn find_last_focusable(core: &Core, root: NodeId) -> Option<NodeId> {
    prefer_view(false, |accept| find_prev_with(core, root, None, accept))
}

/// Find the last node that `accept` admits before `target` in pre-order, or
/// under `root` when `target` is `None`.
fn find_prev_with(
    core: &Core,
    root: NodeId,
    target: Option<NodeId>,
    accept: Candidates,
) -> Option<NodeId> {
    let mut prev = None;
    for id in core.subtree_pre_order(root) {
        if target == Some(id) {
            break;
        }
        if accept.admits(core, id) {
            prev = Some(id);
        }
    }
    prev
}

/// Return the nearest ancestor of `start` that `accept` admits.
fn nearest_ancestor_with(core: &Core, start: NodeId, accept: Candidates) -> Option<NodeId> {
    let mut current = core.nodes.get(start).and_then(|node| node.parent);
    while let Some(id) = current {
        if accept.admits(core, id) {
            return Some(id);
        }
        current = core.nodes.get(id).and_then(|node| node.parent);
    }
    None
}

/// Return whether the node can take focus, respecting hidden and view
/// requirements. A widget whose cell a callback holds cannot.
pub(super) fn is_focus_candidate(core: &Core, node_id: NodeId, require_view: bool) -> bool {
    Candidates {
        require_view,
        admit_taken: false,
    }
    .admits(core, node_id)
}

/// Ask a widget whether it accepts focus, or return `None` while a callback
/// holds its cell.
fn focus_acceptance(core: &Core, node_id: NodeId) -> Option<bool> {
    let node = core.nodes.get(node_id)?;
    let widget = WidgetReadGuard::borrow(node_id, node).ok()?;
    let ctx = CoreViewContext::new(core, node_id);
    Some(widget.widget().accept_focus(&ctx))
}

/// Return whether focus may occupy this node apart from widget acceptance: it
/// is attached, admitted by the active modal, and not hidden.
fn is_focus_position_valid(core: &Core, node_id: NodeId, require_view: bool) -> bool {
    if !core.interaction_admits(node_id) {
        return false;
    }
    let Some(node) = core.nodes.get(node_id) else {
        return false;
    };
    if require_view && node.view.is_empty() {
        return false;
    }
    let mut current = node_id;
    loop {
        let Some(entry) = core.nodes.get(current) else {
            return false;
        };
        if entry.hidden || entry.layout.display == Display::None {
            return false;
        }
        match entry.parent {
            Some(parent) => current = parent,
            None => return current == core.root,
        }
    }
}

/// Return the focus sort key for a candidate, or `None` if it is not in `dir`.
fn focus_dir_key(
    dir: FocusDirection,
    current_rect: RectI32,
    current_center: (i64, i64),
    rect: RectI32,
) -> Option<u64> {
    let center = rect.center();
    match dir {
        FocusDirection::Right => {
            (center.0 > current_center.0 && rect.overlaps_vertical(current_rect)).then(|| {
                let edge_dist = (rect.left() - current_rect.right()).max(0) as u64;
                let vert_center_dist = current_center.1.abs_diff(center.1);
                edge_dist * 10000 + vert_center_dist
            })
        }
        FocusDirection::Left => {
            (center.0 < current_center.0 && rect.overlaps_vertical(current_rect)).then(|| {
                let edge_dist = (current_rect.left() - rect.right()).max(0) as u64;
                let vert_center_dist = current_center.1.abs_diff(center.1);
                edge_dist * 10000 + vert_center_dist
            })
        }
        FocusDirection::Down => {
            (center.1 > current_center.1 && rect.overlaps_horizontal(current_rect)).then(|| {
                let edge_dist = (rect.top() - current_rect.bottom()).max(0) as u64;
                let horiz_center_dist = current_center.0.abs_diff(center.0);
                edge_dist * 10000 + horiz_center_dist
            })
        }
        FocusDirection::Up => {
            (center.1 < current_center.1 && rect.overlaps_horizontal(current_rect)).then(|| {
                let edge_dist = (current_rect.top() - rect.bottom()).max(0) as u64;
                let horiz_center_dist = current_center.0.abs_diff(center.0);
                edge_dist * 10000 + horiz_center_dist
            })
        }
        FocusDirection::Next | FocusDirection::Prev => None,
    }
}
