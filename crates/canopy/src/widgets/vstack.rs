use crate::{
    Context, NodeId, derive_commands,
    error::Result,
    layout::{Layout, Sizing},
    state::NodeName,
    widget::Widget,
};

/// Child sizing for a vertical stack.
enum StackItem {
    /// Flex-weighted row.
    Flex {
        /// Node ID for the row.
        node: NodeId,
        /// Flex weight for the row.
        weight: u32,
    },
    /// Fixed-height row.
    Fixed {
        /// Node ID for the row.
        node: NodeId,
        /// Fixed height for the row.
        height: u32,
    },
}

impl StackItem {
    /// Return the node ID for this stack item.
    fn node(&self) -> NodeId {
        match self {
            Self::Flex { node, .. } => *node,
            Self::Fixed { node, .. } => *node,
        }
    }
}

/// A vertical stack that arranges children with fixed or flex heights.
pub struct VStack {
    /// Stack entries in visual order.
    items: Vec<StackItem>,
}

#[derive_commands]
impl VStack {
    /// Construct an empty vertical stack.
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Add a flex row with a weight.
    pub fn push_flex(mut self, node: NodeId, weight: u32) -> Self {
        self.items.push(StackItem::Flex {
            node,
            weight: weight.max(1),
        });
        self
    }

    /// Add a fixed-height row.
    pub fn push_fixed(mut self, node: NodeId, height: u32) -> Self {
        self.items.push(StackItem::Fixed { node, height });
        self
    }

    /// Apply the stack ordering and layout sizing to child nodes.
    fn sync_layout(&self, ctx: &mut dyn Context) -> Result<()> {
        let mut children = Vec::with_capacity(self.items.len());
        for item in &self.items {
            let node = item.node();
            ctx.detach(node)?;
            children.push(node);
        }
        ctx.set_children(children)?;

        for item in &self.items {
            let node = item.node();
            ctx.with_layout_of(node, &mut |layout| {
                layout.width = Sizing::Flex(1);
                match item {
                    StackItem::Flex { weight, .. } => {
                        layout.height = Sizing::Flex(*weight);
                        layout.min_height = None;
                        layout.max_height = None;
                    }
                    StackItem::Fixed { height, .. } => {
                        layout.height = Sizing::Measure;
                        layout.min_height = Some(*height);
                        layout.max_height = Some(*height);
                    }
                }
            })?;
        }

        Ok(())
    }
}

impl Default for VStack {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for VStack {
    fn layout(&self) -> Layout {
        Layout::column().flex_horizontal(1).flex_vertical(1)
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.sync_layout(ctx)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("vstack")
    }
}
