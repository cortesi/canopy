//! Grid test utility for creating configurable grid layouts.

use taffy::{
    geometry::Line,
    style::{Display, GridPlacement, TrackSizingFunction},
    style_helpers::{FromFlex, line},
};

use crate::{
    NodeId, ViewContext,
    core::Core,
    derive_commands,
    error::Result,
    geom::{Expanse, Point, Rect},
    state::NodeName,
    widget::Widget,
};

/// A grid node widget used for testing.
struct GridWidget {
    /// Node name for identification.
    name: String,
    /// Whether this node is a focusable leaf.
    leaf: bool,
}

#[derive_commands]
impl GridWidget {
    /// Construct a test grid widget.
    fn new(name: String, leaf: bool) -> Self {
        Self { name, leaf }
    }
}

impl Widget for GridWidget {
    fn accept_focus(&self) -> bool {
        self.leaf
    }

    fn render(
        &mut self,
        _r: &mut crate::render::Render,
        _area: Rect,
        _ctx: &dyn ViewContext,
    ) -> Result<()> {
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

    /// Get the expected grid size in pixels.
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

    let node_id = core.add(GridWidget::new(name, recursion == 0));

    if recursion == 0 {
        return Ok(node_id);
    }

    let mut children = Vec::new();
    let child_scale = divisions.pow((recursion - 1) as u32);

    for row in 0..divisions {
        for col in 0..divisions {
            let child_x = x + col * child_scale;
            let child_y = y + row * child_scale;
            let child = build_node(core, child_x, child_y, recursion - 1, divisions)?;
            children.push((row, col, child));
        }
    }

    core.set_children(node_id, children.iter().map(|(_, _, id)| *id).collect())?;

    core.build(node_id).style(|style| {
        style.display = Display::Grid;
        style.grid_template_columns = vec![TrackSizingFunction::from_flex(1.0); divisions];
        style.grid_template_rows = vec![TrackSizingFunction::from_flex(1.0); divisions];
    });

    for (row, col, child) in children {
        core.build(child).style(|style| {
            style.grid_row = line::<Line<GridPlacement>>((row + 1) as i16);
            style.grid_column = line::<Line<GridPlacement>>((col + 1) as i16);
        });
    }

    Ok(node_id)
}
