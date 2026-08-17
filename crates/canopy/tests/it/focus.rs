//! Focus traversal integration tests.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, ViewContext, Widget,
        commands::{CommandNode, CommandSpec},
        error::{Error, Result},
        geom::{Direction, Size},
        layout::Layout,
        render::Render,
        state::NodeName,
        testing::grid::Grid,
    };

    use crate::common::{focus_dir, focus_first, focused_cell};

    struct FocusLeaf {
        name: &'static str,
    }

    impl FocusLeaf {
        fn new(name: &'static str) -> Self {
            Self { name }
        }
    }

    impl CommandNode for FocusLeaf {
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Widget for FocusLeaf {
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }

        fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert(self.name)
        }
    }

    fn test_snake_navigation(grid: &Grid, canopy: &mut Canopy) -> Result<()> {
        let (grid_width, grid_height) = grid.dimensions();
        let total_cells = grid_width * grid_height;

        focus_first(canopy, grid.root)?;
        let initial = focused_cell(canopy);
        if initial != Some("cell_0_0".to_string()) {
            return Err(Error::Invariant(format!(
                "Expected to start at cell_0_0, but started at {initial:?}"
            )));
        }

        let mut visited_cells: Vec<String> = Vec::new();
        let mut position_errors: Vec<String> = Vec::new();

        for row in 0..grid_height {
            if row % 2 == 0 {
                for col in 0..grid_width {
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

                    if col < grid_width - 1 {
                        let before = focused_cell(canopy);
                        focus_dir(canopy, grid.root, Direction::Right)?;
                        let after = focused_cell(canopy);

                        if before == after {
                            return Err(Error::Invariant(format!(
                                "Failed to move right from row {row}, col {col} (stuck at {before:?})"
                            )));
                        }
                    }
                }
            } else {
                for col in (0..grid_width).rev() {
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

                    if col > 0 {
                        let before = focused_cell(canopy);
                        focus_dir(canopy, grid.root, Direction::Left)?;
                        let after = focused_cell(canopy);

                        if before == after {
                            return Err(Error::Invariant(format!(
                                "Failed to move left from row {row}, col {col} (stuck at {before:?})"
                            )));
                        }
                    }
                }
            }

            if row < grid_height - 1 {
                let before = focused_cell(canopy);
                focus_dir(canopy, grid.root, Direction::Down)?;
                let after = focused_cell(canopy);

                if before == after {
                    return Err(Error::Invariant(format!(
                        "Failed to move down after row {row} (stuck at {before:?})"
                    )));
                }
            }
        }

        if visited_cells.len() != total_cells {
            return Err(Error::Invariant(format!(
                "Only visited {} out of {} cells ({:.1}% coverage)",
                visited_cells.len(),
                total_cells,
                (visited_cells.len() as f64 / total_cells as f64) * 100.0
            )));
        }

        if !position_errors.is_empty() {
            return Err(Error::Invariant(format!(
                "Navigation completed but {} position errors occurred:\n{}",
                position_errors.len(),
                position_errors[..5.min(position_errors.len())].join("\n")
            )));
        }

        Ok(())
    }

    #[test]
    fn test_focus_dir_simple_grid() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy, 1, 2)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Size::new(20, 20));

        focus_first(&mut canopy, grid.root)?;
        assert_eq!(focused_cell(&canopy), Some("cell_0_0".to_string()));

        focus_dir(&mut canopy, grid.root, Direction::Right)?;
        assert_eq!(focused_cell(&canopy), Some("cell_1_0".to_string()));

        focus_dir(&mut canopy, grid.root, Direction::Down)?;
        assert_eq!(focused_cell(&canopy), Some("cell_1_1".to_string()));

        focus_dir(&mut canopy, grid.root, Direction::Left)?;
        assert_eq!(focused_cell(&canopy), Some("cell_0_1".to_string()));

        focus_dir(&mut canopy, grid.root, Direction::Up)?;
        assert_eq!(focused_cell(&canopy), Some("cell_0_0".to_string()));

        Ok(())
    }

    #[test]
    fn test_focus_snake_navigation_3x3() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy, 1, 3)?;
        test_snake_navigation(&grid, &mut canopy)
    }

    #[test]
    fn test_focus_snake_navigation_4x4() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy, 2, 2)?;
        test_snake_navigation(&grid, &mut canopy)
    }

    #[test]
    fn test_focus_moves_off_zero_view_nodes() -> Result<()> {
        let mut canopy = Canopy::new();
        let first = canopy.create_detached(FocusLeaf::new("first"))?;
        let second = canopy.create_detached(FocusLeaf::new("second"))?;
        canopy.with_root_context(|context| {
            let root = context.root_id();
            context.set_children_of(root, vec![first.into(), second.into()])?;
            context.set_layout_of(root, Layout::column().flex_horizontal(1).flex_vertical(1))?;
            context.set_layout_of(first, Layout::column().fixed_width(10).fixed_height(5))?;
            context.set_layout_of(second, Layout::fill())?;
            context.set_focus(first.into())?;
            Ok(())
        })?;

        canopy.set_root_size(Size::new(10, 10))?;
        canopy.with_root_context(|context| {
            context.with_layout_of(first.into(), &mut |layout| {
                *layout = layout.fixed_height(0);
            })
        })?;
        canopy.set_root_size(Size::new(10, 10))?;

        assert_eq!(
            canopy.with_root_view(|context| context.focused_leaf(context.root_id())),
            Some(second.into())
        );
        Ok(())
    }
}
