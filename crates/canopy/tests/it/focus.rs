//! Focus traversal integration tests.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, CanopyBuilder, ContextExt, NodeId, NodeName, ViewContext, Widget,
        error::Result,
        geom::Size,
        layout::Layout,
        runtime::TurnInput,
        testing::grid::Grid,
        tree::{FocusDirection, FocusScope},
    };

    /// Return the name of the focused grid cell, if a cell holds focus.
    fn focused_cell(canopy: &Canopy) -> Option<String> {
        canopy.with_root_view(|context| {
            let root = context.root_id();
            let focused = context.focused_within(root)?;
            let mut path = context.path_of(root, focused);
            path.pop().filter(|name| name.starts_with("cell_"))
        })
    }

    /// Focus the first focusable node in a subtree.
    fn focus_first(canopy: &mut Canopy, root: NodeId) -> Result<()> {
        canopy.with_root_context(|context| context.focus_first(FocusScope::Node(root)).map(|_| ()))
    }

    /// Move focus one step in a direction within a subtree.
    fn focus_dir(canopy: &mut Canopy, root: NodeId, direction: FocusDirection) -> Result<()> {
        canopy.with_root_context(|context| {
            context
                .focus_move(FocusScope::Node(root), direction)
                .map(|_| ())
        })
    }

    struct FocusLeaf {
        name: &'static str,
    }

    impl FocusLeaf {
        fn new(name: &'static str) -> Self {
            Self { name }
        }
    }

    impl Widget for FocusLeaf {
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }

        fn name(&self) -> NodeName {
            NodeName::convert(self.name)
        }
    }

    fn test_snake_navigation(grid: &Grid, canopy: &mut Canopy) -> Result<()> {
        let (grid_width, grid_height) = grid.dimensions();
        let total_cells = grid_width * grid_height;

        focus_first(canopy, grid.root)?;
        assert_eq!(
            focused_cell(canopy),
            Some("cell_0_0".to_string()),
            "navigation starts at cell_0_0"
        );

        let mut visited_cells: Vec<String> = Vec::new();
        let mut position_errors: Vec<String> = Vec::new();

        for row in 0..grid_height {
            // Rows alternate direction, so even rows run left to right and odd
            // rows right to left.
            let forward = row % 2 == 0;
            let step = if forward {
                FocusDirection::Right
            } else {
                FocusDirection::Left
            };
            let cols: Vec<usize> = if forward {
                (0..grid_width).collect()
            } else {
                (0..grid_width).rev().collect()
            };

            for (index, col) in cols.iter().copied().enumerate() {
                let cell = focused_cell(canopy);
                let expected_cell = format!("cell_{col}_{row}");

                match &cell {
                    Some(actual_cell) => {
                        if !visited_cells.contains(actual_cell) {
                            visited_cells.push(actual_cell.clone());
                        }
                        if actual_cell != &expected_cell {
                            position_errors.push(format!(
                                "Row {row}, col {col}: expected {expected_cell}, got {actual_cell}"
                            ));
                        }
                    }
                    None => {
                        position_errors
                            .push(format!("Row {row}, col {col}: no focused cell found"));
                    }
                }

                if index + 1 < cols.len() {
                    let before = focused_cell(canopy);
                    focus_dir(canopy, grid.root, step)?;
                    let after = focused_cell(canopy);

                    assert_ne!(
                        before, after,
                        "failed to move {step:?} from row {row}, col {col}"
                    );
                }
            }

            if row < grid_height - 1 {
                let before = focused_cell(canopy);
                focus_dir(canopy, grid.root, FocusDirection::Down)?;
                let after = focused_cell(canopy);

                assert_ne!(before, after, "failed to move down after row {row}");
            }
        }

        assert_eq!(
            visited_cells.len(),
            total_cells,
            "navigation visits every cell"
        );
        assert!(
            position_errors.is_empty(),
            "{} position errors occurred:\n{}",
            position_errors.len(),
            position_errors[..5.min(position_errors.len())].join("\n")
        );

        Ok(())
    }

    #[test]
    fn test_focus_dir_simple_grid() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        let grid = Grid::install(&mut canopy, 1, 2)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Size::new(20, 20));

        focus_first(&mut canopy, grid.root)?;
        assert_eq!(focused_cell(&canopy), Some("cell_0_0".to_string()));

        focus_dir(&mut canopy, grid.root, FocusDirection::Right)?;
        assert_eq!(focused_cell(&canopy), Some("cell_1_0".to_string()));

        focus_dir(&mut canopy, grid.root, FocusDirection::Down)?;
        assert_eq!(focused_cell(&canopy), Some("cell_1_1".to_string()));

        focus_dir(&mut canopy, grid.root, FocusDirection::Left)?;
        assert_eq!(focused_cell(&canopy), Some("cell_0_1".to_string()));

        focus_dir(&mut canopy, grid.root, FocusDirection::Up)?;
        assert_eq!(focused_cell(&canopy), Some("cell_0_0".to_string()));

        Ok(())
    }

    #[test]
    fn test_focus_snake_navigation_3x3() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        let grid = Grid::install(&mut canopy, 1, 3)?;
        test_snake_navigation(&grid, &mut canopy)
    }

    #[test]
    fn test_focus_snake_navigation_4x4() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        let grid = Grid::install(&mut canopy, 2, 2)?;
        test_snake_navigation(&grid, &mut canopy)
    }

    #[test]
    fn test_focus_moves_off_zero_view_nodes() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        let (first, second) = canopy.with_root_context(|context| {
            let first = context.create_detached(FocusLeaf::new("first"))?;
            let second = context.create_detached(FocusLeaf::new("second"))?;
            let root = context.root_id();
            context.set_children(root, vec![first.into(), second.into()])?;
            context.set_layout_override(
                root,
                Layout::column().flex_horizontal(1).flex_vertical(1).into(),
            )?;
            context.set_layout_override(
                first.into(),
                Layout::column().fixed_width(10).fixed_height(5).into(),
            )?;
            context.set_layout_override(second.into(), Layout::fill().into())?;
            context.set_focus(first.into())?;
            Ok((first, second))
        })?;

        canopy.set_screen_size(Size::new(10, 10))?;
        canopy.turn(TurnInput::Prepare)?;
        canopy.with_root_context(|context| {
            context.set_layout_override(
                first.into(),
                Layout::column().fixed_width(10).fixed_height(0).into(),
            )
        })?;
        canopy.set_screen_size(Size::new(10, 10))?;
        canopy.turn(TurnInput::Prepare)?;

        assert_eq!(
            canopy.with_root_view(|context| context.focused_within(context.root_id())),
            Some(second.into())
        );
        Ok(())
    }
}
