use std::{
    cell::{Ref, RefMut},
    ptr::NonNull,
};

use super::{context::CoreViewContext, id::NodeId, node::Node, world::Core};
use crate::{
    error::{Error, Result},
    widget::Widget,
};

#[derive(Clone, Copy, PartialEq, Eq)]
/// Policy used when validating a node's widget slot.
pub enum WidgetSlotPolicy {
    /// Widget slots must be present and available for inspection.
    RequirePresent,
    /// Widget slots may be borrowed or temporarily empty during a callback.
    AllowBorrowed,
}

/// Immutable borrow of a widget slot.
pub struct WidgetReadGuard<'a> {
    /// Borrowed widget slot.
    slot: Ref<'a, Option<Box<dyn Widget>>>,
}

impl<'a> WidgetReadGuard<'a> {
    /// Borrow a node's widget slot immutably.
    pub(crate) fn borrow(node_id: NodeId, node: &'a Node) -> Result<Self> {
        let slot = node
            .widget
            .try_borrow()
            .map_err(|_| Error::ReentrantWidgetBorrow(node_id))?;
        if slot.is_none() {
            return Err(Error::ReentrantWidgetBorrow(node_id));
        }
        Ok(Self { slot })
    }

    /// Return the borrowed widget.
    pub(crate) fn widget(&self) -> &dyn Widget {
        self.slot
            .as_deref()
            .expect("widget missing from read guard")
    }
}

/// Mutable borrow of a widget slot that does not need mutable core access.
pub struct WidgetMutGuard<'a> {
    /// Borrowed widget slot.
    slot: RefMut<'a, Option<Box<dyn Widget>>>,
}

impl<'a> WidgetMutGuard<'a> {
    /// Borrow a node's widget slot mutably.
    pub(crate) fn borrow(node_id: NodeId, node: &'a Node) -> Result<Self> {
        let slot = node
            .widget
            .try_borrow_mut()
            .map_err(|_| Error::ReentrantWidgetBorrow(node_id))?;
        if slot.is_none() {
            return Err(Error::ReentrantWidgetBorrow(node_id));
        }
        Ok(Self { slot })
    }

    /// Return the borrowed widget mutably.
    pub(crate) fn widget_mut(&mut self) -> &mut dyn Widget {
        self.slot
            .as_deref_mut()
            .expect("widget missing from mutable guard")
    }
}

/// Temporary widget extraction guard for callbacks that need mutable core access.
pub struct WidgetSlotGuard {
    /// Core pointer used to restore the slot on drop.
    core: NonNull<Core>,
    /// Node that owns the widget slot.
    node_id: NodeId,
    /// Widget owned while the node slot is empty.
    widget: Option<Box<dyn Widget>>,
}

impl WidgetSlotGuard {
    /// Take a widget out of its node slot.
    pub(crate) fn take(core: &Core, node_id: NodeId) -> Result<Self> {
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

    /// Return the extracted widget mutably.
    pub(crate) fn widget_mut(&mut self) -> &mut dyn Widget {
        self.widget
            .as_deref_mut()
            .expect("widget missing from slot guard")
    }
}

impl Drop for WidgetSlotGuard {
    fn drop(&mut self) {
        // SAFETY: `WidgetSlotGuard` is created only from a live `Core` reference and never leaves
        // the callback that created it. During the guard lifetime, the removed widget is owned by
        // this guard. On drop, restoration is best-effort: if the node was removed, or if a
        // replacement widget already occupies the slot, this guard drops its widget instead of
        // overwriting current tree state.
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

/// Validate a widget slot according to the supplied policy.
pub fn validate_slot(node_id: NodeId, node: &Node, policy: WidgetSlotPolicy) -> Result<()> {
    let Ok(widget) = node.widget.try_borrow() else {
        return match policy {
            WidgetSlotPolicy::RequirePresent => Err(Error::ReentrantWidgetBorrow(node_id)),
            WidgetSlotPolicy::AllowBorrowed => Ok(()),
        };
    };
    if widget.is_some() || policy == WidgetSlotPolicy::AllowBorrowed {
        return Ok(());
    }
    Err(Error::Invariant(format!(
        "node {node_id:?} has an empty widget slot"
    )))
}

/// Return whether a widget accepts focus, treating unavailable slots as not focusable.
pub fn accepts_focus(core: &Core, node_id: NodeId) -> bool {
    let Some(node) = core.nodes.get(node_id) else {
        return false;
    };
    let Ok(widget) = WidgetReadGuard::borrow(node_id, node) else {
        return false;
    };
    let ctx = CoreViewContext::new(core, node_id);
    widget.widget().accept_focus(&ctx)
}
