use std::cell::Cell;

use crate::{
    core::{id::NodeId, view::View},
    geom::{Expanse, Point, Rect},
    layout::Layout,
    state::NodeName,
    widget::Widget,
};

/// Core node data stored in the arena.
pub struct Node {
    /// Widget behavior and state.
    pub widget: Option<Box<dyn Widget>>,

    /// Parent in the arena tree.
    pub parent: Option<NodeId>,
    /// Children in the arena tree.
    pub children: Vec<NodeId>,

    /// Cached layout configuration for quick access.
    pub layout: Layout,

    /// Outer rect relative to the parent content origin.
    pub rect: Rect,
    /// Content size (outer minus padding).
    pub content_size: Expanse,
    /// Canvas size in content coordinates.
    pub canvas: Expanse,
    /// Scroll offset in content coordinates.
    pub scroll: Point,
    /// View information in screen coordinates.
    pub view: View,

    /// Node visibility.
    pub hidden: bool,
    /// Node name for commands and paths.
    pub name: NodeName,
    /// Whether polling has been initialized.
    pub initialized: bool,
    /// Whether the widget mount hook has run.
    pub mounted: bool,
    /// Whether layout configuration should be refreshed from the widget.
    pub layout_dirty: Cell<bool>,
}
