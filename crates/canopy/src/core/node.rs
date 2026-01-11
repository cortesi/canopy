use std::{any::TypeId, cell::RefCell, collections::HashMap};

use crate::{
    core::{id::NodeId, style::Effect, view::View},
    geom::{Expanse, Point, Rect},
    layout::Layout,
    state::NodeName,
    widget::Widget,
};

/// Core node data stored in the arena.
pub struct Node {
    /// Widget behavior and state.
    pub(crate) widget: RefCell<Option<Box<dyn Widget>>>,

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
    pub(crate) content_size: Expanse,
    /// Canvas size in content coordinates.
    pub(crate) canvas: Expanse,
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
    /// If true, clear inherited effects before applying local effects.
    pub(crate) clear_inherited_effects: bool,
}

impl Node {
    /// Return the node's widget name.
    pub fn name(&self) -> &NodeName {
        &self.name
    }

    /// Return the node's parent, if any.
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// Return the node's children.
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// Return the cached layout configuration.
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Return the outer rectangle relative to the parent content origin.
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// Return the content size.
    pub fn content_size(&self) -> Expanse {
        self.content_size
    }

    /// Return the canvas size.
    pub fn canvas(&self) -> Expanse {
        self.canvas
    }

    /// Return the scroll offset.
    pub fn scroll(&self) -> Point {
        self.scroll
    }

    /// Return the view data.
    pub fn view(&self) -> View {
        self.view
    }

    /// Return true if the node is hidden.
    pub fn hidden(&self) -> bool {
        self.hidden
    }

    /// Return true if polling has been initialized.
    pub fn initialized(&self) -> bool {
        self.initialized
    }

    /// Return true if the widget mount hook has run.
    pub fn mounted(&self) -> bool {
        self.mounted
    }
}
