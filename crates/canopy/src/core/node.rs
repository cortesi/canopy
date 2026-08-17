use std::{any::TypeId, collections::HashMap, rc::Rc};

use parking_lot::RwLock;

use crate::{
    core::{id::NodeId, style::Effect, view::View},
    geom::{Point, Rect, Size},
    layout::Layout,
    state::NodeName,
    widget::Widget,
};

/// Core node data stored in the arena.
#[derive(Clone)]
pub struct Node {
    /// Widget behavior and state.
    pub(crate) widget: Rc<RwLock<Option<Box<dyn Widget>>>>,

    /// Widget type identifier for fast type checks.
    pub(crate) widget_type: TypeId,

    /// Parent in the arena tree.
    pub(crate) parent: Option<NodeId>,
    /// Children in the arena tree.
    pub(crate) children: Vec<NodeId>,
    /// Mapping of child role keys to node IDs.
    pub(crate) child_keys: HashMap<String, NodeId>,

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
    /// None for the common case of no effects (avoids per-node Vec allocation).
    pub(crate) effects: Option<Vec<Effect>>,
}

impl Node {
    /// Construct a detached node for a boxed widget.
    ///
    /// The caller validates the widget's layout before insertion.
    pub(crate) fn new(widget: Box<dyn Widget>) -> Self {
        let layout = widget.layout();
        let name = widget.name();
        let widget_type = widget.as_ref().type_id();
        Self {
            widget: Rc::new(RwLock::new(Some(widget))),
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
        }
    }
}
