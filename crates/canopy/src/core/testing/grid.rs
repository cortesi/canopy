//! Grid test utility for creating configurable grid layouts
//!
//! This module provides a flexible grid structure that can be used for testing
//! layout and positioning code. The grid supports configurable recursion levels
//! and subdivisions, making it easy to create complex nested grid structures.

use crate::{
    Context, Layout,
    commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
    error::Result,
    geom::{Expanse, Rect},
    node::Node,
    render::Render,
    state::{NodeName, NodeState, StatefulNode},
    tree::{Locate, locate},
};

/// A node in the grid that can be either a leaf cell or a container with children
pub enum GridNode {
    /// A leaf cell in the grid
    Cell {
        /// Node state.
        state: NodeState,
        /// Node name string.
        name_str: String,
    },
    /// A container node with child nodes
    Container {
        /// Node state.
        state: NodeState,
        /// Node name string.
        name_str: String,
        /// Child nodes.
        children: Vec<Self>,
    },
}

impl GridNode {
    /// Create a new leaf cell at the given coordinates
    pub fn new_cell(x: usize, y: usize) -> Self {
        let name = format!("cell_{x}_{y}");
        Self::Cell {
            state: NodeState::default(),
            name_str: name,
        }
    }

    /// Create a new container or cell based on recursion level
    /// The x, y parameters represent the position in the final grid (leaf-level coordinates)
    /// The scale parameter represents the size of this node in leaf cells (divisions^recursion)
    pub fn new_container_scaled(
        x: usize,
        y: usize,
        recursion: usize,
        divisions: usize,
        scale: usize,
    ) -> Self {
        let name = if recursion == 0 {
            format!("cell_{x}_{y}")
        } else {
            format!("container_{x}_{y}")
        };

        if recursion == 0 {
            // Base case - create a leaf cell
            Self::Cell {
                state: NodeState::default(),
                name_str: name,
            }
        } else {
            // Recursive case - create children
            let mut children = Vec::new();
            let child_scale = scale / divisions;

            for row in 0..divisions {
                for col in 0..divisions {
                    // Calculate the child's position in the overall grid
                    let child_x = x + col * child_scale;
                    let child_y = y + row * child_scale;
                    children.push(Self::new_container_scaled(
                        child_x,
                        child_y,
                        recursion - 1,
                        divisions,
                        child_scale,
                    ));
                }
            }

            Self::Container {
                state: NodeState::default(),
                name_str: name,
                children,
            }
        }
    }

    /// Create a new container or cell based on recursion level (uses new_container_scaled internally)
    pub fn new_container(x: usize, y: usize, recursion: usize, divisions: usize) -> Self {
        // For backwards compatibility, calculate the scale
        let scale = divisions.pow(recursion as u32);
        Self::new_container_scaled(x, y, recursion, divisions, scale)
    }

    /// Create a new root node for the grid
    pub fn new_root(recursion: usize, divisions: usize) -> Self {
        if recursion == 0 {
            // Single cell at root
            Self::new_cell(0, 0)
        } else {
            // Container at root
            let mut children = Vec::new();
            let total_scale = divisions.pow(recursion as u32);
            let child_scale = total_scale / divisions;

            for row in 0..divisions {
                for col in 0..divisions {
                    children.push(Self::new_container_scaled(
                        col * child_scale,
                        row * child_scale,
                        recursion - 1,
                        divisions,
                        child_scale,
                    ));
                }
            }

            Self::Container {
                state: NodeState::default(),
                name_str: "grid".to_string(),
                children,
            }
        }
    }

    /// Return a reference to the node state.
    fn state(&self) -> &NodeState {
        match self {
            Self::Cell { state, .. } => state,
            Self::Container { state, .. } => state,
        }
    }

    /// Return a mutable reference to the node state.
    fn state_mut(&mut self) -> &mut NodeState {
        match self {
            Self::Cell { state, .. } => state,
            Self::Container { state, .. } => state,
        }
    }

    /// Return the node name string.
    fn name_str(&self) -> &str {
        match self {
            Self::Cell { name_str, .. } => name_str,
            Self::Container { name_str, .. } => name_str,
        }
    }
}

impl Node for GridNode {
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        if let Self::Container { children, .. } = self {
            for child in children {
                f(child)?;
            }
        }
        Ok(())
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        self.fill(sz)?;

        if let Self::Container { children, .. } = self {
            let divisions = (children.len() as f64).sqrt() as usize;
            let cell_width = sz.w / divisions as u32;
            let cell_height = sz.h / divisions as u32;

            for (i, child) in children.iter_mut().enumerate() {
                let row = i / divisions;
                let col = i % divisions;

                let x = col as u32 * cell_width;
                let y = row as u32 * cell_height;

                // Last cell in each row/column gets remaining space
                let width = if col == divisions - 1 {
                    sz.w - x
                } else {
                    cell_width
                };
                let height = if row == divisions - 1 {
                    sz.h - y
                } else {
                    cell_height
                };

                // Use the Layout trait's place_ method to properly position the child
                l.place(child, Rect::new(x, y, width, height))?;
            }
        }

        Ok(())
    }

    fn render(&mut self, _ctx: &dyn Context, _render: &mut Render) -> Result<()> {
        // Grid nodes don't render anything by default
        Ok(())
    }

    fn accept_focus(&mut self) -> bool {
        // Only leaf cells can accept focus
        matches!(self, Self::Cell { .. })
    }
}

impl CommandNode for GridNode {
    fn commands() -> Vec<CommandSpec> {
        vec![]
    }

    fn dispatch(
        &mut self,
        _ctx: &mut dyn Context,
        _cmd: &CommandInvocation,
    ) -> Result<ReturnValue> {
        Ok(ReturnValue::Void)
    }
}

impl StatefulNode for GridNode {
    fn name(&self) -> NodeName {
        NodeName::convert(self.name_str())
    }

    fn state(&self) -> &NodeState {
        self.state()
    }

    fn state_mut(&mut self) -> &mut NodeState {
        self.state_mut()
    }
}

/// A test utility for creating grids with configurable recursion and subdivisions
///
/// # Examples
/// ```no_run
/// use canopy::testing::grid::Grid;
///
/// // Create a 2x2 grid with 4 cells
/// let grid = Grid::new(1, 2);
///
/// // Create a 4x4 grid with 16 cells
/// let grid = Grid::new(2, 2);
///
/// // Create a 3x3 grid with 9 cells
/// let grid = Grid::new(1, 3);
/// ```
pub struct Grid {
    /// Root node for the grid.
    root: GridNode,
    /// Recursion depth.
    recursion: usize,
    /// Number of subdivisions per level.
    divisions: usize,
}

impl Grid {
    /// Create a new grid with specified recursion levels and subdivisions per level
    ///
    /// # Arguments
    /// * `recursion` - Number of recursive levels (0 = just leaf cells)
    /// * `divisions` - Number of subdivisions in each dimension (e.g., 2 = 2x2 = 4 children)
    ///
    /// # Examples
    /// * `Grid::new(0, 2)` - Single 10x10 cell
    /// * `Grid::new(1, 2)` - 2x2 grid = 4 cells of 10x10 each
    /// * `Grid::new(2, 2)` - 4x4 grid = 16 cells of 10x10 each
    /// * `Grid::new(1, 3)` - 3x3 grid = 9 cells of 10x10 each
    pub fn new(recursion: usize, divisions: usize) -> Self {
        let root = GridNode::new_root(recursion, divisions);
        Self {
            root,
            recursion,
            divisions,
        }
    }

    /// Get the expected grid size in pixels
    pub fn expected_size(&self) -> Expanse {
        // Calculate the actual number of leaf cells
        let cells_per_side = if self.recursion == 0 {
            1
        } else {
            self.divisions.pow(self.recursion as u32)
        };
        let size = cells_per_side as u32 * 10; // Each cell is 10x10
        Expanse::new(size, size)
    }

    /// Get the dimensions of the grid (number of cells in x and y)
    /// Returns (width, height) in cells
    pub fn dimensions(&self) -> (usize, usize) {
        let cells_per_side = if self.recursion == 0 {
            1
        } else {
            self.divisions.pow(self.recursion as u32)
        };
        (cells_per_side, cells_per_side)
    }

    /// Helper to find the deepest leaf node at a given position
    pub fn find_leaf_at(&mut self, x: u32, y: u32) -> Option<String> {
        // Keep track of all nodes we encounter
        let mut nodes = Vec::new();
        locate(self, (x, y), &mut |node| -> Result<Locate<()>> {
            nodes.push(node.name().to_string());
            Ok(Locate::Continue) // Always continue to find all nodes
        })
        .ok()?;

        // Return the last (deepest) node that is a cell or container
        nodes
            .into_iter()
            .rfind(|n| n.starts_with("cell_") || n.starts_with("container_"))
    }
}

impl Node for Grid {
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.root)
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        l.place(&mut self.root, sz.rect())
    }

    fn render(&mut self, ctx: &dyn Context, render: &mut Render) -> Result<()> {
        self.root.render(ctx, render)
    }
}

impl CommandNode for Grid {
    fn commands() -> Vec<CommandSpec> {
        vec![]
    }

    fn dispatch(
        &mut self,
        _ctx: &mut dyn Context,
        _cmd: &CommandInvocation,
    ) -> Result<ReturnValue> {
        Ok(ReturnValue::Void)
    }
}

impl StatefulNode for Grid {
    fn name(&self) -> NodeName {
        NodeName::convert("grid_wrapper")
    }

    fn state(&self) -> &NodeState {
        self.root.state()
    }

    fn state_mut(&mut self) -> &mut NodeState {
        self.root.state_mut()
    }
}
