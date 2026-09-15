//! Viewport scroll mutation, reveal requests, and default input actions.
//!
//! Explicit scrolling moves a viewport at once. A reveal waits for layout to
//! settle the geometry it shows. Each scroll and reveal takes a call-order
//! stamp, and each viewport records the stamp of the last one it accepted, so a
//! later call wins a shared viewport whether or not layout ran between calls.

use super::{Core, WidgetOperation, layout_driver::clamp_scroll};
use crate::{
    ChangeOutcome, RevealAlign,
    core::{id::NodeId, node::Node},
    error::{Error, Result},
    geom::{Point, PointI32, Rect},
};

/// Runtime behavior applied to a route node that no widget or binding handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultAction {
    /// Scroll the node's viewport by a signed offset.
    Scroll(PointI32),
}

/// What a request shows in its own node's canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevealTarget {
    /// A rectangle in canvas coordinates.
    Area(Rect),
    /// The rectangle the widget's reveal anchor reports after layout.
    Anchor,
}

/// A reveal pending in a node's own viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalReveal {
    /// What to show.
    pub target: RevealTarget,
    /// How to align it.
    pub align: RevealAlign,
    /// Call-order stamp.
    pub order: u64,
}

/// A reveal of a node pending in its ancestor viewports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeReveal {
    /// How to align the node in each viewport.
    pub align: RevealAlign,
    /// Call-order stamp.
    pub order: u64,
}

/// The slot a queued request occupies.
#[derive(Clone, Copy)]
enum Slot {
    /// The node's own viewport.
    Local,
    /// The node's ancestor viewports.
    Ancestors,
}

impl Core {
    /// Return the next call-order stamp.
    fn next_scroll_stamp(&mut self) -> u64 {
        self.scroll_stamp = self.scroll_stamp.saturating_add(1);
        self.scroll_stamp
    }

    /// Scroll an attached node to an offset, clamped to its canvas.
    pub(crate) fn scroll_to_of(&mut self, node: NodeId, x: u32, y: u32) -> Result<ChangeOutcome> {
        self.validate_attached_node(node)?;
        Ok(self.update_scroll(node, |_| Point { x, y }))
    }

    /// Move a viewport by an explicit scroll, clamped to its canvas.
    ///
    /// The scroll supersedes older reveals of this viewport, even when the
    /// offset does not change. Reports a change when the offset moved or the
    /// node's pending reveal was cancelled.
    pub(crate) fn update_scroll(
        &mut self,
        node_id: NodeId,
        f: impl FnOnce(Point) -> Point,
    ) -> ChangeOutcome {
        let order = self.next_scroll_stamp();
        let Some(node) = self.nodes.get_mut(node_id) else {
            return ChangeOutcome::Unchanged;
        };
        let before = node.scroll;
        let cancelled_reveal = node.reveal.take().is_some();
        node.scroll_order = order;
        node.scroll = f(before);
        clamp_scroll(&mut node.scroll, node.content_size, node.canvas);
        node.view.scroll = node.scroll;
        if before == node.scroll && !cancelled_reveal {
            return ChangeOutcome::Unchanged;
        }
        self.invalidate(crate::Invalidation::Layout);
        ChangeOutcome::Changed
    }

    /// Queue a reveal in a node's own viewport, replacing a pending one.
    ///
    /// An empty area queues nothing. Reports a change when the pending target
    /// or alignment changed.
    pub(crate) fn reveal_in(
        &mut self,
        node_id: NodeId,
        target: RevealTarget,
        align: RevealAlign,
    ) -> ChangeOutcome {
        if matches!(target, RevealTarget::Area(area) if area.is_empty()) {
            return ChangeOutcome::Unchanged;
        }
        let order = self.next_scroll_stamp();
        let Some(node) = self.nodes.get_mut(node_id) else {
            return ChangeOutcome::Unchanged;
        };
        let changed = node
            .reveal
            .is_none_or(|pending| (pending.target, pending.align) != (target, align));
        node.reveal = Some(LocalReveal {
            target,
            align,
            order,
        });
        self.invalidate(crate::Invalidation::Layout);
        outcome(changed)
    }

    /// Queue a reveal of an existing node in its ancestor viewports.
    pub(crate) fn reveal_node(
        &mut self,
        node_id: NodeId,
        align: RevealAlign,
    ) -> Result<ChangeOutcome> {
        if !self.nodes.contains_key(node_id) {
            return Err(Error::NodeNotFound(node_id));
        }
        Ok(self.queue_node_reveal(node_id, align))
    }

    /// Queue a reveal of a node in its ancestor viewports, replacing a pending
    /// one.
    pub(crate) fn queue_node_reveal(
        &mut self,
        node_id: NodeId,
        align: RevealAlign,
    ) -> ChangeOutcome {
        let order = self.next_scroll_stamp();
        let Some(node) = self.nodes.get_mut(node_id) else {
            return ChangeOutcome::Unchanged;
        };
        let changed = node
            .reveal_in_ancestors
            .is_none_or(|pending| pending.align != align);
        node.reveal_in_ancestors = Some(NodeReveal { align, order });
        self.invalidate(crate::Invalidation::Layout);
        outcome(changed)
    }

    /// Apply a default action to a node and report whether the node moved.
    ///
    /// A step that cannot move leaves the node untouched, so pending reveals
    /// survive it.
    pub(crate) fn apply_default_action(&mut self, node_id: NodeId, action: DefaultAction) -> bool {
        match action {
            DefaultAction::Scroll(delta) => {
                let Some(node) = self.nodes.get(node_id) else {
                    return false;
                };
                let mut target = node.scroll.scroll(delta.x, delta.y);
                clamp_scroll(&mut target, node.content_size, node.canvas);
                if target == node.scroll {
                    return false;
                }
                self.update_scroll(node_id, |_| target);
                true
            }
        }
    }

    /// Apply pending reveals in call order against settled geometry.
    ///
    /// A request waits while its node is detached, outside the active modal
    /// region, or without geometry. Returns whether any request was pending, so
    /// the caller can publish the moved views.
    pub(crate) fn apply_reveals(&mut self) -> Result<bool> {
        let mut pending = Vec::new();
        for (node_id, node) in self.nodes.iter() {
            if let Some(request) = node.reveal {
                pending.push((request.order, node_id, Slot::Local));
            }
            if let Some(request) = node.reveal_in_ancestors {
                pending.push((request.order, node_id, Slot::Ancestors));
            }
        }
        if pending.is_empty() {
            return Ok(false);
        }
        pending.sort_unstable_by_key(|(order, ..)| *order);
        let boundary = self.modal_region().unwrap_or(self.root);
        for (_, node_id, slot) in pending {
            if !self.is_attached_to_root(node_id) || !self.is_ancestor_or_self(boundary, node_id) {
                continue;
            }
            match slot {
                Slot::Local => self.apply_local_reveal(node_id)?,
                Slot::Ancestors => self.apply_node_reveal(node_id, boundary),
            }
        }
        Ok(true)
    }

    /// Apply a node's own reveal, retaining it while the node has no viewport.
    fn apply_local_reveal(&mut self, node_id: NodeId) -> Result<()> {
        let Some(node) = self.nodes.get(node_id) else {
            return Ok(());
        };
        let (Some(request), view) = (node.reveal, node.content_size) else {
            return Ok(());
        };
        if view.w == 0 || view.h == 0 {
            return Ok(());
        }
        let area = match request.target {
            RevealTarget::Area(area) => Some(area),
            RevealTarget::Anchor => self.with_widget(
                node_id,
                WidgetOperation::layout("reveal anchor"),
                |widget, _core| widget.reveal_anchor(view),
            )?,
        };
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or(Error::NodeNotFound(node_id))?;
        node.reveal = None;
        if let Some(area) = area.filter(|area| !area.is_empty()) {
            align_viewport(node, area, request.align, request.order);
        }
        Ok(())
    }

    /// Reveal a node in each viewport from its parent out to `boundary`,
    /// retaining the request while the node has no layout.
    ///
    /// Each viewport shows the part of the node its inner viewport left
    /// visible, so a viewport a later call has moved still bounds the area its
    /// ancestors reveal.
    fn apply_node_reveal(&mut self, node_id: NodeId, boundary: NodeId) {
        let Some(node) = self.nodes.get(node_id) else {
            return;
        };
        let (Some(request), mut area) = (node.reveal_in_ancestors, node.rect) else {
            return;
        };
        if node_id != boundary && area.is_empty() {
            return;
        }
        self.nodes[node_id].reveal_in_ancestors = None;
        let mut current = node_id;
        while current != boundary {
            let Some(parent) = self.nodes.get(current).and_then(|node| node.parent) else {
                break;
            };
            let Some(viewport) = self.nodes.get_mut(parent) else {
                break;
            };
            align_viewport(viewport, area, request.align, request.order);
            let visible = Rect::new(
                viewport.scroll.x,
                viewport.scroll.y,
                viewport.content_size.w,
                viewport.content_size.h,
            );
            let Some(shown) = area.intersect(visible).filter(|shown| !shown.is_empty()) else {
                break;
            };
            let padding = viewport.layout.padding;
            area = Rect::new(
                viewport
                    .rect
                    .tl
                    .x
                    .saturating_add(padding.left)
                    .saturating_add(shown.tl.x - viewport.scroll.x),
                viewport
                    .rect
                    .tl
                    .y
                    .saturating_add(padding.top)
                    .saturating_add(shown.tl.y - viewport.scroll.y),
                shown.w,
                shown.h,
            );
            current = parent;
        }
    }
}

/// Convert a change flag into an outcome.
fn outcome(changed: bool) -> ChangeOutcome {
    if changed {
        ChangeOutcome::Changed
    } else {
        ChangeOutcome::Unchanged
    }
}

/// Move a viewport to show `area`, unless a later scroll or reveal already
/// claimed it.
fn align_viewport(viewport: &mut Node, area: Rect, align: RevealAlign, order: u64) {
    if viewport.scroll_order > order {
        return;
    }
    viewport.scroll_order = order;
    let size = viewport.content_size;
    viewport.scroll = Point {
        x: reveal_offset(viewport.scroll.x, size.w, area.tl.x, area.w, align),
        y: reveal_offset(viewport.scroll.y, size.h, area.tl.y, area.h, align),
    };
    clamp_scroll(&mut viewport.scroll, size, viewport.canvas);
}

/// Return the offset along one axis that shows `start..start + length` in a
/// viewport of `view` cells.
///
/// `Nearest` makes the smallest move, and keeps a viewport that already lies
/// inside an oversized area. `Center` centers an area shorter than the
/// viewport. Wide arithmetic keeps end coordinates intact.
fn reveal_offset(offset: u32, view: u32, start: u32, length: u32, align: RevealAlign) -> u32 {
    let (start, length, view) = (u64::from(start), u64::from(length), u64::from(view));
    let offset = if align == RevealAlign::Center && length < view {
        (2 * start + length).saturating_sub(view) / 2
    } else {
        let end_aligned = (start + length).saturating_sub(view);
        u64::from(offset).clamp(start.min(end_aligned), start.max(end_aligned))
    };
    u32::try_from(offset).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
