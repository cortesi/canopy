use taffy::style::{Dimension, Display, FlexDirection, Style};

use super::{id::NodeId, world::Core};

/// Fluent builder for node layout and hierarchy.
pub struct NodeBuilder<'a> {
    /// Mutable core reference used to apply changes.
    pub(crate) core: &'a mut Core,
    /// Node being configured.
    pub(crate) id: NodeId,
}

impl<'a> NodeBuilder<'a> {
    /// Modify the Taffy style for this node.
    pub fn style(self, f: impl FnOnce(&mut Style)) -> Self {
        let t_id = self.core.nodes[self.id].taffy_id;
        let mut style = self.core.taffy.style(t_id).cloned().unwrap_or_default();
        f(&mut style);
        self.core
            .taffy
            .set_style(t_id, style.clone())
            .expect("Failed to set Taffy style");
        self.core.nodes[self.id].style = style;
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

    /// Set width to 100%.
    pub fn w_full(self) -> Self {
        self.style(|s| s.size.width = Dimension::Percent(1.0))
    }

    /// Add a child and return the parent builder.
    pub fn add_child(self, child_id: NodeId) -> Self {
        self.core
            .mount_child(self.id, child_id)
            .expect("Failed to mount child node");
        self
    }
}
