//! Integration tests for focus behavior.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Core, NodeId, ViewContext,
        commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
        error::{Error, Result},
        geom::{Direction, Expanse},
        layout::{Layout, Sizing},
        render::Render,
        state::NodeName,
        testing::grid::Grid,
        widget::Widget,
    };

    struct FocusLeaf {
        name: &'static str,
    }

    impl FocusLeaf {
        fn new(name: &'static str) -> Self {
            Self { name }
        }
    }

    impl CommandNode for FocusLeaf {
        fn commands() -> Vec<CommandSpec> {
            Vec::new()
        }

        fn dispatch(
            &mut self,
            _c: &mut dyn canopy::Context,
            _cmd: &CommandInvocation,
        ) -> Result<ReturnValue> {
            Ok(ReturnValue::Void)
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

    fn attach_grid(core: &mut Core, grid_root: NodeId, size: Expanse) -> Result<()> {
        core.set_children(core.root, vec![grid_root])?;
        core.with_layout_of(core.root, |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;
        core.with_layout_of(grid_root, |layout| {
            layout.width = Sizing::Flex(1);
            layout.height = Sizing::Flex(1);
        })?;
        core.update_layout(size)?;
        Ok(())
    }

    fn get_focused_cell(core: &Core) -> Option<String> {
        core.focus
            .and_then(|id| core.nodes.get(id).map(|n| n.name.to_string()))
            .filter(|name| name.starts_with("cell_"))
    }

    fn test_snake_navigation(grid: &Grid, canopy: &mut Canopy, size: Expanse) -> Result<()> {
        attach_grid(&mut canopy.core, grid.root, size)?;
        let (grid_width, grid_height) = grid.dimensions();
        let total_cells = grid_width * grid_height;

        canopy.core.focus_first(grid.root);
        let initial = get_focused_cell(&canopy.core);
        if initial != Some("cell_0_0".to_string()) {
            return Err(Error::Focus(format!(
                "Expected to start at cell_0_0, but started at {initial:?}"
            )));
        }

        let mut visited_cells: Vec<String> = Vec::new();
        let mut position_errors: Vec<String> = Vec::new();

        for row in 0..grid_height {
            if row % 2 == 0 {
                for col in 0..grid_width {
                    let cell = get_focused_cell(&canopy.core);
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
                        let before = get_focused_cell(&canopy.core);
                        canopy.core.focus_dir(grid.root, Direction::Right);
                        let after = get_focused_cell(&canopy.core);

                        if before == after {
                            return Err(Error::Focus(format!(
                                "Failed to move right from row {row}, col {col} (stuck at {before:?})"
                            )));
                        }
                    }
                }
            } else {
                for col in (0..grid_width).rev() {
                    let cell = get_focused_cell(&canopy.core);
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
                        let before = get_focused_cell(&canopy.core);
                        canopy.core.focus_dir(grid.root, Direction::Left);
                        let after = get_focused_cell(&canopy.core);

                        if before == after {
                            return Err(Error::Focus(format!(
                                "Failed to move left from row {row}, col {col} (stuck at {before:?})"
                            )));
                        }
                    }
                }
            }

            if row < grid_height - 1 {
                let before = get_focused_cell(&canopy.core);
                canopy.core.focus_dir(grid.root, Direction::Down);
                let after = get_focused_cell(&canopy.core);

                if before == after {
                    return Err(Error::Focus(format!(
                        "Failed to move down after row {row} (stuck at {before:?})"
                    )));
                }
            }
        }

        if visited_cells.len() != total_cells {
            return Err(Error::Focus(format!(
                "Only visited {} out of {} cells ({:.1}% coverage)",
                visited_cells.len(),
                total_cells,
                (visited_cells.len() as f64 / total_cells as f64) * 100.0
            )));
        }

        if !position_errors.is_empty() {
            return Err(Error::Focus(format!(
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
        let grid = Grid::install(&mut canopy.core, 1, 2)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Expanse::new(20, 20));
        attach_grid(&mut canopy.core, grid.root, grid_size)?;

        canopy.core.focus_first(grid.root);
        assert_eq!(get_focused_cell(&canopy.core), Some("cell_0_0".to_string()));

        canopy.core.focus_dir(grid.root, Direction::Right);
        assert_eq!(get_focused_cell(&canopy.core), Some("cell_1_0".to_string()));

        canopy.core.focus_dir(grid.root, Direction::Down);
        assert_eq!(get_focused_cell(&canopy.core), Some("cell_1_1".to_string()));

        canopy.core.focus_dir(grid.root, Direction::Left);
        assert_eq!(get_focused_cell(&canopy.core), Some("cell_0_1".to_string()));

        canopy.core.focus_dir(grid.root, Direction::Up);
        assert_eq!(get_focused_cell(&canopy.core), Some("cell_0_0".to_string()));

        Ok(())
    }

    #[test]
    fn test_focus_snake_navigation_3x3() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy.core, 1, 3)?;
        let grid_size = grid.expected_size();
        test_snake_navigation(&grid, &mut canopy, grid_size)
    }

    #[test]
    fn test_focus_snake_navigation_4x4() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy.core, 2, 2)?;
        let grid_size = grid.expected_size();
        test_snake_navigation(&grid, &mut canopy, grid_size)
    }

    #[test]
    fn test_focus_moves_off_zero_view_nodes() -> Result<()> {
        let mut canopy = Canopy::new();
        let first = canopy.core.add(FocusLeaf::new("first"));
        let second = canopy.core.add(FocusLeaf::new("second"));

        canopy
            .core
            .set_children(canopy.core.root, vec![first, second])?;
        canopy.core.with_layout_of(canopy.core.root, |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;
        canopy.core.with_layout_of(first, |layout| {
            *layout = Layout::column().fixed_width(10).fixed_height(5);
        })?;
        canopy.core.with_layout_of(second, |layout| {
            *layout = Layout::fill();
        })?;

        canopy.core.update_layout(Expanse::new(10, 10))?;
        canopy.core.set_focus(first);

        canopy.core.with_layout_of(first, |layout| {
            *layout = layout.fixed_height(0);
        })?;
        canopy.core.update_layout(Expanse::new(10, 10))?;

        assert_eq!(canopy.core.focus, Some(second));
        Ok(())
    }
}
