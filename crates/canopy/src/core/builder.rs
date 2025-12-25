use super::{id::NodeId, world::Core};
use crate::{
    Context,
    error::Result,
    layout::{Dimension, Display, FlexDirection, Style},
};

/// Shared builder hooks for applying layout and mounting nodes.
pub trait BuildContext {
    /// Update the style for a node.
    fn with_style(&mut self, node: NodeId, f: &mut dyn FnMut(&mut Style)) -> Result<()>;

    /// Attach a child to a parent node.
    fn mount_child(&mut self, parent: NodeId, child: NodeId) -> Result<()>;
}

impl BuildContext for dyn Context + '_ {
    fn with_style(&mut self, node: NodeId, f: &mut dyn FnMut(&mut Style)) -> Result<()> {
        self.with_style(node, f)
    }

    fn mount_child(&mut self, parent: NodeId, child: NodeId) -> Result<()> {
        self.mount_child(parent, child)
    }
}

impl BuildContext for Core {
    fn with_style(&mut self, node: NodeId, f: &mut dyn FnMut(&mut Style)) -> Result<()> {
        self.with_style(node, f)
    }

    fn mount_child(&mut self, parent: NodeId, child: NodeId) -> Result<()> {
        self.mount_child(parent, child)
    }
}

/// Fluent builder for node layout and hierarchy.
pub struct NodeBuilder<'a, C: BuildContext + ?Sized> {
    /// Mutable context reference used to apply changes.
    pub(crate) ctx: &'a mut C,
    /// Node being configured.
    pub(crate) id: NodeId,
}

impl<'a, C: BuildContext + ?Sized> NodeBuilder<'a, C> {
    /// Modify the layout style for this node.
    pub fn style(self, f: impl FnOnce(&mut Style)) -> Self {
        let mut f = Some(f);
        let mut apply = |style: &mut Style| {
            if let Some(f) = f.take() {
                f(style);
            }
        };
        self.ctx
            .with_style(self.id, &mut apply)
            .expect("Failed to set node style");
        self
    }

    /// Set flex layout in a row direction.
    pub fn flex_row(self) -> Self {
        self.style(|s| {
            s.display = Display::Flex;
            s.flex_direction = FlexDirection::Row;
        })
    }

    /// Set flex layout in a column direction.
    pub fn flex_col(self) -> Self {
        self.style(|s| {
            s.display = Display::Flex;
            s.flex_direction = FlexDirection::Column;
        })
    }

    /// Configure this node as a flex item with the provided factors.
    pub fn flex_item(self, grow: f32, shrink: f32, basis: Dimension) -> Self {
        self.style(|s| {
            s.flex_grow = grow;
            s.flex_shrink = shrink;
            s.flex_basis = basis;
        })
    }

    /// Set width to 100%.
    pub fn w_full(self) -> Self {
        self.style(|s| s.size.width = Dimension::Percent(1.0))
    }

    /// Set height to 100%.
    pub fn h_full(self) -> Self {
        self.style(|s| s.size.height = Dimension::Percent(1.0))
    }

    /// Set width and height to 100%.
    pub fn fill(self) -> Self {
        self.w_full().h_full()
    }

    /// Add a child and return the parent builder.
    pub fn add_child(self, child_id: NodeId) -> Self {
        self.ctx
            .mount_child(self.id, child_id)
            .expect("Failed to mount child node");
        self
    }
}
