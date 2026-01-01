//! Grid test utility for creating configurable grid layouts.

use crate::{
    NodeId, ViewContext,
    core::Core,
    derive_commands,
    error::Result,
    geom::{Expanse, Point},
    layout::Layout,
    state::NodeName,
    widget::Widget,
};

/// Grid node kind used for layout selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GridKind {
    /// Leaf cell with fixed size.
    Cell,
    /// Row container.
    Row,
    /// Column container.
    Column,
}

/// A grid node widget used for testing.
struct GridNode {
    /// Node name for identification.
    name: String,
    /// Layout role for this node.
    kind: GridKind,
}

#[derive_commands]
impl GridNode {
    /// Construct a new grid node.
    fn new(name: String, kind: GridKind) -> Self {
        Self { name, kind }
    }

    /// Construct a leaf cell.
    fn cell(name: String) -> Self {
        Self::new(name, GridKind::Cell)
    }

    /// Construct a row container.
    fn row(name: String) -> Self {
        Self::new(name, GridKind::Row)
    }

    /// Construct a column container.
    fn column(name: String) -> Self {
        Self::new(name, GridKind::Column)
    }
}

impl Widget for GridNode {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        matches!(self.kind, GridKind::Cell)
    }

    fn layout(&self) -> Layout {
        match self.kind {
            GridKind::Cell => Layout::column().fixed_width(10).fixed_height(10),
            GridKind::Row => Layout::row(),
            GridKind::Column => Layout::column(),
        }
    }

    fn render(&mut self, _r: &mut crate::render::Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert(&self.name)
    }
}

/// A test utility for creating grids with configurable recursion and subdivisions.
pub struct Grid {
    /// Root node for the grid.
    pub root: NodeId,
    /// Recursion depth.
    recursion: usize,
    /// Number of subdivisions per level.
    divisions: usize,
}

impl Grid {
    /// Create a new grid with specified recursion levels and subdivisions per level.
    pub fn install(core: &mut Core, recursion: usize, divisions: usize) -> Result<Self> {
        let root = build_node(core, 0, 0, recursion, divisions)?;
        Ok(Self {
            root,
            recursion,
            divisions,
        })
    }

    /// Get the expected grid size in cells.
    pub fn expected_size(&self) -> Expanse {
        let cells_per_side = if self.recursion == 0 {
            1
        } else {
            self.divisions.pow(self.recursion as u32)
        };
        let size = cells_per_side as u32 * 10;
        Expanse::new(size, size)
    }

    /// Get the dimensions of the grid (number of cells in x and y).
    pub fn dimensions(&self) -> (usize, usize) {
        let cells_per_side = if self.recursion == 0 {
            1
        } else {
            self.divisions.pow(self.recursion as u32)
        };
        (cells_per_side, cells_per_side)
    }

    /// Helper to find the deepest leaf node at a given position.
    pub fn find_leaf_at(&self, core: &Core, x: u32, y: u32) -> Option<String> {
        let point = Point { x, y };
        let id = core.locate_node(self.root, point).ok().flatten()?;
        let name = core.nodes.get(id)?.name.to_string();
        if name.starts_with("cell_") || name.starts_with("container_") {
            Some(name)
        } else {
            None
        }
    }
}

/// Recursively build grid nodes and apply layout styles.
fn build_node(
    core: &mut Core,
    x: usize,
    y: usize,
    recursion: usize,
    divisions: usize,
) -> Result<NodeId> {
    let name = if recursion == 0 {
        format!("cell_{x}_{y}")
    } else {
        format!("container_{x}_{y}")
    };

    if recursion == 0 {
        return Ok(core.create_detached(GridNode::cell(name)));
    }

    let node_id = core.create_detached(GridNode::column(name));

    let mut children = Vec::new();
    let child_scale = divisions.pow((recursion - 1) as u32);

    for row in 0..divisions {
        let row_name = format!("row_{x}_{y}_{row}");
        let row_node = core.create_detached(GridNode::row(row_name));
        let mut row_children = Vec::new();
        for col in 0..divisions {
            let child_x = x + col * child_scale;
            let child_y = y + row * child_scale;
            let child = build_node(core, child_x, child_y, recursion - 1, divisions)?;
            row_children.push(child);
        }
        core.set_children(row_node, row_children)?;
        children.push(row_node);
    }

    core.set_children(node_id, children)?;

    Ok(node_id)
}
