use std::{any::TypeId, cell::RefCell, collections::HashMap, rc::Rc};

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
    pub(crate) attachment_generation: Option<u64>,
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
    /// Whether polling has been initialized.
    pub(crate) initialized: bool,
    /// Whether the widget mount hook has run.
    pub(crate) mounted: bool,
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
            attachment_generation: None,
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
            initialized: false,
            mounted: false,
            layout_dirty: false,
            effects: Vec::new(),
        }
    }
}
