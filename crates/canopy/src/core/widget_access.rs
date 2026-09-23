use std::{
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};

use super::{id::NodeId, node::Node, world::Core};
use crate::{
    error::{Error, Result},
    widget::Widget,
};

#[derive(Clone, Copy, PartialEq, Eq)]
/// Policy used when validating a node's widget cell.
pub enum WidgetCellPolicy {
    /// Widget cells must be present and available for inspection.
    RequirePresent,
    /// Widget cells may be borrowed or temporarily empty during a callback.
    AllowBorrowed,
}

/// Immutable borrow of a widget cell.
pub struct WidgetReadGuard<'a> {
    /// Borrowed widget cell.
    slot: Ref<'a, Option<Box<dyn Widget>>>,
}

impl<'a> WidgetReadGuard<'a> {
    /// Borrow a node's widget cell immutably.
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

/// Mutable borrow of a widget cell that does not need mutable core access.
pub struct WidgetMutGuard<'a> {
    /// Borrowed widget cell.
    slot: RefMut<'a, Option<Box<dyn Widget>>>,
}

impl<'a> WidgetMutGuard<'a> {
    /// Borrow a node's widget cell mutably.
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

/// Temporary widget extraction guard for callbacks that need mutable core
/// access.
pub struct WidgetCellGuard {
    /// Owned reference to the extracted widget cell.
    slot: Rc<RefCell<Option<Box<dyn Widget>>>>,
    /// Widget owned while the node slot is empty.
    widget: Option<Box<dyn Widget>>,
}

impl WidgetCellGuard {
    /// Take a widget out of its node slot.
    pub(crate) fn take(core: &Core, node_id: NodeId) -> Result<Self> {
        let node = core
            .nodes
            .get(node_id)
            .ok_or(Error::NodeNotFound(node_id))?;
        let slot = Rc::clone(&node.widget);
        let widget = {
            let mut widget = slot
                .try_borrow_mut()
                .map_err(|_| Error::ReentrantWidgetBorrow(node_id))?;
            widget.take().ok_or(Error::ReentrantWidgetBorrow(node_id))?
        };
        Ok(Self {
            slot,
            widget: Some(widget),
        })
    }

    /// Return the extracted widget mutably.
    pub(crate) fn widget_mut(&mut self) -> &mut dyn Widget {
        self.widget
            .as_deref_mut()
            .expect("widget missing from cell guard")
    }
}

impl Drop for WidgetCellGuard {
    fn drop(&mut self) {
        match self.slot.try_borrow_mut() {
            Ok(mut slot) if slot.is_none() => {
                *slot = self.widget.take();
            }
            Ok(_) => {
                debug_assert!(false, "widget cell was not empty during cell guard drop");
                tracing::error!("widget cell was not empty during cell guard drop");
            }
            Err(_) => {
                debug_assert!(false, "widget cell was borrowed during cell guard drop");
                tracing::error!("widget cell was borrowed during cell guard drop");
            }
        }
    }
}

/// Validate a widget cell according to the supplied policy.
pub fn validate_slot(node_id: NodeId, node: &Node, policy: WidgetCellPolicy) -> Result<()> {
    let Ok(widget) = node.widget.try_borrow() else {
        return match policy {
            WidgetCellPolicy::RequirePresent => Err(Error::ReentrantWidgetBorrow(node_id)),
            WidgetCellPolicy::AllowBorrowed => Ok(()),
        };
    };
    if widget.is_some() || policy == WidgetCellPolicy::AllowBorrowed {
        return Ok(());
    }
    Err(Error::Internal(format!(
        "node {node_id:?} has an empty widget cell"
    )))
}
