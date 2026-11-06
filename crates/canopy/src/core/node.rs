use std::{any::TypeId, cell::RefCell, collections::HashMap, mem, num::NonZeroU64, rc::Rc};

use crate::{
    NodeId, NodeName,
    core::{
        style::effects::Effect,
        view::View,
        world::scroll::{LocalReveal, NodeReveal},
    },
    geom::{Point, Rect, Size},
    layout::{Layout, LayoutOverride, LayoutValidationError},
    runtime::{NodeWakeHandle, PollLifetime, WakeOutcome},
    widget::Widget,
};

/// Application identity unique within an explicit arena subtree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeIdentity {
    /// Scope root, which may itself be detached.
    pub scope: NodeId,
    /// Application-defined key independent of structural slots.
    pub key: String,
}

/// Where a node's widget is in its lifecycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lifecycle {
    /// The mount hook has not run, or the unmount hook has.
    #[default]
    Unmounted,
    /// The mount hook has run. `polled` says whether the first poll that
    /// starts the widget's polling has run, for its current poll lifetime.
    Mounted {
        /// Whether polling has started.
        polled: bool,
    },
}

/// Core node data stored in the arena.
#[derive(Clone)]
pub struct Node {
    /// Widget behavior and state.
    pub(crate) widget: Rc<RefCell<Option<Box<dyn Widget>>>>,

    /// Widget type identifier for fast type checks.
    pub(crate) widget_type: TypeId,
    /// Widget identity independent of the arena slot generation.
    pub(crate) incarnation: u64,
    /// Current committed attachment generation, absent while detached.
    pub(crate) attachment: Option<NonZeroU64>,
    /// Poll ownership chosen by the widget at construction.
    pub(crate) poll_lifetime: PollLifetime,

    /// Parent in the arena tree.
    pub(crate) parent: Option<NodeId>,
    /// Children in the arena tree.
    pub(crate) children: Vec<NodeId>,
    /// Mapping of child role keys to node IDs.
    pub(crate) slots: HashMap<String, NodeId>,
    /// Optional application identity registered in the Core scope index.
    pub(crate) identity: Option<NodeIdentity>,

    /// Last validated widget layout.
    pub(crate) base_layout: Layout,
    /// Persistent parent constraints.
    pub(crate) layout_override: LayoutOverride,

    /// Node whose top edge bounds this node's content from below while it
    /// shows. See
    /// [`Context::set_layout_floor`](crate::Context::set_layout_floor).
    pub(crate) floor: Option<NodeId>,
    /// Bottom padding that the floor asks for, from the last layout.
    pub(crate) floor_inset: u32,

    /// Cached layout configuration for quick access.
    pub(crate) layout: Layout,

    /// Outer rect relative to the parent content origin.
    pub(crate) rect: Rect,
    /// Content size (outer minus padding).
    pub(crate) content_size: Size,
    /// Canvas size in content coordinates.
    pub(crate) canvas: Size,
    /// Scroll offset in content coordinates. Change it through
    /// [`Node::set_scroll`], so the node's scroll observers hear of it.
    pub(crate) scroll: Point,
    /// Wake handles of the widgets that observe this node's scroll.
    pub(crate) scroll_observers: Vec<NodeWakeHandle>,
    /// Reveal pending in this node's own viewport.
    pub(crate) reveal: Option<LocalReveal>,
    /// Reveal of this node pending in its ancestor viewports.
    pub(crate) reveal_in_ancestors: Option<NodeReveal>,
    /// Call-order stamp of the last scroll or reveal this viewport accepted.
    pub(crate) scroll_order: u64,
    /// View information in screen coordinates.
    pub(crate) view: View,

    /// Node visibility.
    pub(crate) hidden: bool,
    /// Node name for commands and paths.
    pub(crate) name: NodeName,
    /// Where the widget is in its lifecycle.
    pub(crate) lifecycle: Lifecycle,
    /// Whether layout configuration should be refreshed from the widget.
    pub(crate) layout_dirty: bool,

    /// Effects to apply to this node and descendants during rendering.
    /// Empty until an effect is pushed; `Vec::new()` does not allocate.
    pub(crate) effects: Vec<Effect>,
}

impl Node {
    /// Return the layout that `base` takes with this node's overrides and
    /// floor: the floor raises the bottom padding to its inset.
    pub(crate) fn effective_layout(&self, base: Layout) -> Result<Layout, LayoutValidationError> {
        let mut layout = self.layout_override.apply(base)?;
        if self.floor_inset > layout.padding.bottom {
            layout.padding.bottom = self.floor_inset;
        }
        Ok(layout)
    }

    /// Move the scroll offset, and wake each scroll observer when it changed.
    ///
    /// An observer whose widget is gone has an expired handle, and leaves
    /// the list.
    pub(crate) fn set_scroll(&mut self, scroll: Point) {
        if self.scroll == scroll {
            return;
        }
        self.scroll = scroll;
        // Only a poisoned registry fails a wake, and the application that
        // owns it has already panicked. Keep the handle in that case.
        self.scroll_observers
            .retain(|wake| !matches!(wake.wake(), Ok(WakeOutcome::Expired)));
    }

    /// Construct a detached node for a boxed widget.
    ///
    /// The caller validates the widget's layout before insertion.
    pub(crate) fn new(widget: Box<dyn Widget>, incarnation: u64) -> Self {
        let layout = widget.layout();
        let name = widget.name();
        let widget_type = widget.as_ref().type_id();
        let poll_lifetime = widget.poll_lifetime();
        Self {
            widget: Rc::new(RefCell::new(Some(widget))),
            widget_type,
            incarnation,
            attachment: None,
            poll_lifetime,
            parent: None,
            children: Vec::new(),
            slots: HashMap::new(),
            identity: None,
            layout,
            base_layout: layout,
            layout_override: LayoutOverride::default(),
            floor: None,
            floor_inset: 0,
            rect: Rect::ZERO,
            content_size: Size::default(),
            canvas: Size::default(),
            scroll: Point::ZERO,
            scroll_observers: Vec::new(),
            reveal: None,
            reveal_in_ancestors: None,
            scroll_order: 0,
            view: View::default(),
            hidden: false,
            name,
            lifecycle: Lifecycle::Unmounted,
            layout_dirty: false,
            effects: Vec::new(),
        }
    }

    /// Return whether the mount hook has run.
    pub(crate) fn is_mounted(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Mounted { .. })
    }

    /// Return whether the widget's polling has started.
    pub(crate) fn is_polled(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Mounted { polled: true })
    }

    /// Record that the mount hook ran. A mounted node stays as it is.
    pub(crate) fn mark_mounted(&mut self) {
        if self.lifecycle == Lifecycle::Unmounted {
            self.lifecycle = Lifecycle::Mounted { polled: false };
        }
    }

    /// Record that the unmount hook ran.
    pub(crate) fn mark_unmounted(&mut self) {
        self.lifecycle = Lifecycle::Unmounted;
    }

    /// Record that the first poll ran. An unmounted node has no polling to
    /// start, and stays unmounted.
    pub(crate) fn mark_polled(&mut self) {
        if let Lifecycle::Mounted { polled } = &mut self.lifecycle {
            *polled = true;
        }
    }

    /// Start polling again at the next sweep, as a new attachment does for
    /// work bound to the attachment.
    pub(crate) fn restart_polling(&mut self) {
        if let Lifecycle::Mounted { polled } = &mut self.lifecycle {
            *polled = false;
        }
    }

    /// Return the committed attachment generation as a work stamp reads it.
    pub(crate) fn attachment_id(&self) -> Option<u64> {
        self.attachment.map(NonZeroU64::get)
    }

    /// Give this node a new widget incarnation in one assignment.
    ///
    /// The widget's identity, cached configuration, lifecycle, attachment,
    /// and reveals start fresh, as for a new node. The node keeps its place
    /// in the tree, its geometry and view, its visibility, its layout floor,
    /// its scroll and its observers, and its effects. The caller clears the
    /// node's identity and children first.
    pub(crate) fn replace(&mut self, widget: Box<dyn Widget>, incarnation: u64) {
        let fresh = Self::new(widget, incarnation);
        *self = Self {
            parent: self.parent,
            children: mem::take(&mut self.children),
            slots: mem::take(&mut self.slots),
            identity: self.identity.take(),
            floor: self.floor,
            rect: self.rect,
            content_size: self.content_size,
            canvas: self.canvas,
            scroll: self.scroll,
            scroll_observers: mem::take(&mut self.scroll_observers),
            view: self.view,
            hidden: self.hidden,
            layout_dirty: self.layout_dirty,
            effects: mem::take(&mut self.effects),
            ..fresh
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Widget with default behavior.
    struct Blank;

    impl Widget for Blank {}

    #[test]
    fn lifecycle_transitions_follow_their_table() {
        type Event = fn(&mut Node);
        let unmounted = Lifecycle::Unmounted;
        let fresh = Lifecycle::Mounted { polled: false };
        let polled = Lifecycle::Mounted { polled: true };
        let events: [(&str, Event); 4] = [
            ("mount", Node::mark_mounted),
            ("unmount", Node::mark_unmounted),
            ("poll", Node::mark_polled),
            ("restart", Node::restart_polling),
        ];
        let table = [
            (unmounted, [fresh, unmounted, unmounted, unmounted]),
            (fresh, [fresh, unmounted, polled, fresh]),
            (polled, [polled, unmounted, polled, fresh]),
        ];
        for (start, nexts) in table {
            for ((name, event), next) in events.iter().zip(nexts) {
                let mut node = Node::new(Box::new(Blank), 1);
                node.lifecycle = start;
                event(&mut node);
                assert_eq!(node.lifecycle, next, "{name} from {start:?}");
                assert_eq!(node.is_mounted(), next != unmounted);
                assert_eq!(node.is_polled(), next == polled);
            }
        }
    }

    #[test]
    fn a_replacement_starts_a_fresh_incarnation_in_place() {
        let mut node = Node::new(Box::new(Blank), 1);
        node.lifecycle = Lifecycle::Mounted { polled: true };
        node.attachment = Some(NonZeroU64::MIN);
        node.hidden = true;
        node.scroll_order = 7;
        node.replace(Box::new(Blank), 2);
        assert_eq!(node.incarnation, 2);
        assert_eq!(node.lifecycle, Lifecycle::Unmounted);
        assert_eq!(node.attachment, None);
        assert_eq!(node.scroll_order, 0);
        assert!(node.hidden, "the node keeps its visibility");
    }
}
