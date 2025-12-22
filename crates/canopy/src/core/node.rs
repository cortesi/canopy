use taffy::{node::Node as TaffyNode, style::Style};

use crate::{
    core::{id::NodeId, viewport::ViewPort},
    geom::Rect,
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

    /// Associated Taffy node.
    pub taffy_id: TaffyNode,
    /// Cached Taffy style for quick access.
    pub style: Style,

    /// Screen-space rectangle for the visible view.
    pub viewport: Rect,
    /// Viewport state for scrolling and clipping.
    pub vp: ViewPort,

    /// Node visibility.
    pub hidden: bool,
    /// Node name for commands and paths.
    pub name: NodeName,
    /// Whether polling has been initialized.
    pub initialized: bool,
}
