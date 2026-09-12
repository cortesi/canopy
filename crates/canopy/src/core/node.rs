use std::{any::TypeId, cell::RefCell, collections::HashMap, rc::Rc};

use crate::{
    core::{id::NodeId, style::effects::Effect, view::View},
    geom::{Point, Rect, Size},
    layout::{Layout, LayoutOverride},
    state::NodeName,
    widget::Widget,
};

/// Application identity unique within an explicit arena subtree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticIdentity {
    /// Scope root, which may itself be detached.
    pub scope: NodeId,
    /// Application-defined key independent of structural child keys.
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
    pub(crate) poll_lifetime: crate::WorkLifetime,

    /// Parent in the arena tree.
    pub(crate) parent: Option<NodeId>,
    /// Children in the arena tree.
    pub(crate) children: Vec<NodeId>,
    /// Mapping of child role keys to node IDs.
    pub(crate) child_keys: HashMap<String, NodeId>,
    /// Optional application identity registered in the Core scope index.
    pub(crate) semantic_identity: Option<SemanticIdentity>,

    /// Last validated widget layout.
    pub(crate) base_layout: Layout,
    /// Persistent parent constraints.
    pub(crate) layout_override: LayoutOverride,

    /// Cached layout configuration for quick access.
    pub(crate) layout: Layout,

    /// Outer rect relative to the parent content origin.
    pub(crate) rect: Rect,
    /// Content size (outer minus padding).
    pub(crate) content_size: Size,
    /// Canvas size in content coordinates.
    pub(crate) canvas: Size,
    /// Scroll offset in content coordinates.
    pub(crate) scroll: Point,
    /// Content rectangle to reveal after the next nonempty layout.
    pub(crate) pending_reveal: Option<Rect>,
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
            child_keys: HashMap::new(),
            semantic_identity: None,
            layout,
            base_layout: layout,
            layout_override: LayoutOverride::default(),
            rect: Rect::ZERO,
            content_size: Size::default(),
            canvas: Size::default(),
            scroll: Point::ZERO,
            pending_reveal: None,
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
