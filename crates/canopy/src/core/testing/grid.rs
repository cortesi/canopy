//! Grid test utility for creating configurable grid layouts.

use crate::{
    Canopy, Context, ContextExt, NodeId, ViewContext,
    error::Result,
    geom::Size,
    layout::{Layout, LayoutOverride},
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

    fn name(&self) -> NodeName {
        NodeName::convert(&self.name)
    }
}

/// A test utility for creating grids with configurable recursion and
/// subdivisions.
pub struct Grid {
    /// Root node for the grid.
    pub root: NodeId,
    /// Recursion depth.
    recursion: usize,
    /// Number of subdivisions per level.
    divisions: usize,
}

impl Grid {
    /// Build and prepare a grid with the root sized to hold it.
    pub fn install(canopy: &mut Canopy, recursion: usize, divisions: usize) -> Result<Self> {
        let grid = canopy.with_root_context(|context| {
            let grid_root = build_node(context, 0, 0, recursion, divisions)?;
            let root = context.root_id();
            context.set_children(root, vec![grid_root])?;
            context.set_layout_override(root, Layout::fill().into())?;
            context.set_layout_override(
                grid_root,
                LayoutOverride::new().flex_horizontal(1).flex_vertical(1),
            )?;
            Ok(Self {
                root: grid_root,
                recursion,
                divisions,
            })
        })?;
        canopy.set_root_size(grid.expected_size())?;
        canopy.turn(crate::Work::Prepare)?;
        Ok(grid)
    }

    /// Return the number of cells along one side of the grid.
    fn cells_per_side(&self) -> usize {
        if self.recursion == 0 {
            1
        } else {
            self.divisions.pow(self.recursion as u32)
        }
    }

    /// Get the expected grid size in cells.
    pub fn expected_size(&self) -> Size {
        let size = self.cells_per_side() as u32 * 10;
        Size::new(size, size)
    }

    /// Get the dimensions of the grid (number of cells in x and y).
    pub fn dimensions(&self) -> (usize, usize) {
        (self.cells_per_side(), self.cells_per_side())
    }
}

/// Recursively build grid nodes and apply layout styles.
fn build_node(
    core: &mut dyn Context,
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
        return Ok(core.create_detached(GridNode::cell(name))?.into());
    }

    let node_id: NodeId = core.create_detached(GridNode::column(name))?.into();

    let mut children = Vec::new();
    let child_scale = divisions.pow((recursion - 1) as u32);

    for row in 0..divisions {
        let row_name = format!("row_{x}_{y}_{row}");
        let row_node: NodeId = core.create_detached(GridNode::row(row_name))?.into();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canopy, CanopyBuilder, ViewContext, core::context::CoreViewContext, geom::Point};

    /// Name the deepest node under `point`, as a pointer event would find it.
    fn locate_name(canopy: &Canopy, root: NodeId, point: Point) -> Result<Option<String>> {
        let context = CoreViewContext::new(&canopy.core, root);
        Ok(canopy.core.locate_node(root, point)?.map(|node| {
            context
                .path_of(root, node)
                .pop()
                .expect("node path should contain a name")
        }))
    }

    #[test]
    fn test_locate_single_cell_grid() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        let grid = Grid::install(&mut canopy, 0, 2)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Size::new(10, 10));

        let test_points = vec![
            ((5, 5), "cell_0_0"),
            ((0, 0), "cell_0_0"),
            ((9, 0), "cell_0_0"),
            ((0, 9), "cell_0_0"),
            ((9, 9), "cell_0_0"),
        ];

        for (point, expected) in test_points {
            let found = locate_name(
                &canopy,
                grid.root,
                Point {
                    x: point.0,
                    y: point.1,
                },
            )?;
            assert_eq!(found, Some(expected.to_string()));
        }

        Ok(())
    }

    #[test]
    fn test_locate_2x2_grid() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        let grid = Grid::install(&mut canopy, 1, 2)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Size::new(20, 20));

        let test_points = vec![
            ((5, 5), "cell_0_0"),
            ((15, 5), "cell_1_0"),
            ((5, 15), "cell_0_1"),
            ((15, 15), "cell_1_1"),
        ];

        for (point, expected) in test_points {
            let found = locate_name(
                &canopy,
                grid.root,
                Point {
                    x: point.0,
                    y: point.1,
                },
            )?;
            assert_eq!(found, Some(expected.to_string()));
        }

        Ok(())
    }

    #[test]
    fn test_locate_3x3_grid() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        let grid = Grid::install(&mut canopy, 1, 3)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Size::new(30, 30));

        for row in 0..3 {
            for col in 0..3 {
                let x = col as u32 * 10 + 5;
                let y = row as u32 * 10 + 5;
                let expected = format!("cell_{col}_{row}");
                let found = locate_name(&canopy, grid.root, Point { x, y })?;
                assert_eq!(found, Some(expected));
            }
        }

        Ok(())
    }

    #[test]
    fn test_locate_nested_grid() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        let grid = Grid::install(&mut canopy, 2, 2)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Size::new(40, 40));

        let corner_tests = vec![
            (Point { x: 5, y: 5 }, "cell_0_0"),
            (Point { x: 35, y: 5 }, "cell_3_0"),
            (Point { x: 5, y: 35 }, "cell_0_3"),
            (Point { x: 35, y: 35 }, "cell_3_3"),
        ];

        for (point, expected) in corner_tests {
            let found = locate_name(&canopy, grid.root, point)?;
            assert_eq!(found, Some(expected.to_string()));
        }

        Ok(())
    }

    #[test]
    fn test_grid_boundary_conditions() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        let grid = Grid::install(&mut canopy, 1, 2)?;

        let result = locate_name(&canopy, grid.root, Point { x: 100, y: 100 })?;
        assert_eq!(result, None);

        Ok(())
    }

    #[test]
    fn test_grid_dimensions() {
        // Test various grid configurations
        let test_cases = vec![
            (0, 2, (1, 1)),   // recursion=0: 1x1 grid
            (1, 2, (2, 2)),   // recursion=1, divisions=2: 2x2 grid
            (2, 2, (4, 4)),   // recursion=2, divisions=2: 4x4 grid
            (3, 2, (8, 8)),   // recursion=3, divisions=2: 8x8 grid
            (1, 3, (3, 3)),   // recursion=1, divisions=3: 3x3 grid
            (2, 3, (9, 9)),   // recursion=2, divisions=3: 9x9 grid
            (3, 3, (27, 27)), // recursion=3, divisions=3: 27x27 grid
            (1, 4, (4, 4)),   // recursion=1, divisions=4: 4x4 grid
            (2, 4, (16, 16)), // recursion=2, divisions=4: 16x16 grid
        ];

        for (recursion, divisions, expected) in test_cases {
            let mut canopy = crate::CanopyBuilder::new()
                .build()
                .expect("an empty application builds");
            let grid =
                Grid::install(&mut canopy, recursion, divisions).expect("Failed to build grid");
            let dimensions = grid.dimensions();
            assert_eq!(
                dimensions, expected,
                "Grid({recursion}, {divisions}) should have dimensions {expected:?}, got {dimensions:?}"
            );

            // Also verify that dimensions match expected_size
            let expected_size = grid.expected_size();
            let expected_pixels = (expected.0 as u32 * 10, expected.1 as u32 * 10);
            assert_eq!(
                (expected_size.w, expected_size.h),
                expected_pixels,
                "Grid({recursion}, {divisions}) expected_size should match dimensions * 10"
            );
        }
    }
}
