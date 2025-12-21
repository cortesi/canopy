//! Integration tests for focus behavior.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, Layout, derive_commands,
        error::{Error, Result},
        geom::{Expanse, Rect},
        node::Node,
        state::{NodeState, StatefulNode},
        testing::grid::Grid,
        tree::*,
    };

    /// Helper function to get the currently focused cell name in a Grid
    fn get_focused_cell(canopy: &Canopy, grid: &mut Grid) -> Option<String> {
        let mut focused = None;
        let grid_node: &mut dyn Node = grid;
        preorder(grid_node, &mut |node| -> Result<Walk<()>> {
            if Context::is_focused(canopy, node) {
                let name = node.name().to_string();
                if name.starts_with("cell_") {
                    focused = Some(name);
                    return Ok(Walk::Handle(()));
                }
            }
            Ok(Walk::Continue)
        })
        .ok()?;
        focused
    }

    /// Test helper to perform snake navigation through a grid and verify all cells are visited
    fn test_snake_navigation(grid: &mut Grid) -> Result<()> {
        let (grid_width, grid_height) = grid.dimensions();
        let total_cells = grid_width * grid_height;

        let grid_size = grid.expected_size();
        let layout = Layout {};
        grid.layout(&layout, grid_size)?;

        let mut canopy = Canopy::new();
        canopy.focus_first(grid);

        // Verify we start at the expected position
        let initial = get_focused_cell(&canopy, grid);
        if initial != Some("cell_0_0".to_string()) {
            return Err(Error::Focus(format!(
                "Expected to start at cell_0_0, but started at {initial:?}"
            )));
        }

        let mut visited_cells: Vec<String> = Vec::new();
        let mut position_errors: Vec<String> = Vec::new();

        for row in 0..grid_height {
            if row % 2 == 0 {
                // Even rows: left to right
                for col in 0..grid_width {
                    let cell = get_focused_cell(&canopy, grid);
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

                    // Move right unless we're at the last column
                    if col < grid_width - 1 {
                        let before = get_focused_cell(&canopy, grid);
                        canopy.focus_right(grid);
                        let after = get_focused_cell(&canopy, grid);

                        if before == after {
                            // Add debug info
                            eprintln!("DEBUG: Failed to move right from row {row}, col {col}");
                            eprintln!("  Before: {before:?}");
                            eprintln!("  After: {after:?}");
                            return Err(Error::Focus(format!(
                                "Failed to move right from row {row}, col {col} (stuck at {before:?})"
                            )));
                        }
                    }
                }
            } else {
                // Odd rows: right to left
                for col in (0..grid_width).rev() {
                    let cell = get_focused_cell(&canopy, grid);
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

                    // Move left unless we're at the first column
                    if col > 0 {
                        let before = get_focused_cell(&canopy, grid);
                        canopy.focus_left(grid);
                        let after = get_focused_cell(&canopy, grid);

                        if before == after {
                            return Err(Error::Focus(format!(
                                "Failed to move left from row {row}, col {col} (stuck at {before:?})"
                            )));
                        }
                    }
                }
            }

            // Move down to next row unless we're at the last row
            if row < grid_height - 1 {
                let before = get_focused_cell(&canopy, grid);
                canopy.focus_down(grid);
                let after = get_focused_cell(&canopy, grid);

                if before == after {
                    return Err(Error::Focus(format!(
                        "Failed to move down after row {row} (stuck at {before:?})"
                    )));
                }
            }
        }

        // Check if we visited all cells
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

    // Grid-based focus navigation tests

    #[test]
    fn test_focus_dir_simple_grid() -> Result<()> {
        use canopy::Canopy;

        // Test 1: Simple 2x2 grid - focus_dir should work correctly
        let mut grid = Grid::new(1, 2);
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Expanse::new(20, 20), "2x2 grid should be 20x20");

        let layout = Layout {};
        grid.layout(&layout, grid_size)?;

        let mut canopy = Canopy::new();

        // Test 2x2 grid navigation
        canopy.focus_first(&mut grid);
        assert_eq!(
            get_focused_cell(&canopy, &mut grid),
            Some("cell_0_0".to_string()),
            "Initial focus should be on cell_0_0"
        );

        // Test right navigation
        canopy.focus_right(&mut grid);
        assert_eq!(
            get_focused_cell(&canopy, &mut grid),
            Some("cell_1_0".to_string()),
            "After moving right, should be at cell_1_0"
        );

        // Test down navigation from cell_1_0
        canopy.focus_down(&mut grid);
        assert_eq!(
            get_focused_cell(&canopy, &mut grid),
            Some("cell_1_1".to_string()),
            "After moving down from cell_1_0, should be at cell_1_1"
        );

        // Test left navigation
        canopy.focus_left(&mut grid);
        assert_eq!(
            get_focused_cell(&canopy, &mut grid),
            Some("cell_0_1".to_string()),
            "After moving left from cell_1_1, should be at cell_0_1"
        );

        // Test up navigation
        canopy.focus_up(&mut grid);
        assert_eq!(
            get_focused_cell(&canopy, &mut grid),
            Some("cell_0_0".to_string()),
            "After moving up from cell_0_1, should be at cell_0_0"
        );

        Ok(())
    }

    #[test]
    fn test_snake_navigation_2x2_grid() -> Result<()> {
        let mut grid = Grid::new(1, 2);
        test_snake_navigation(&mut grid)
    }

    #[test]
    fn test_snake_navigation_4x4_grid() -> Result<()> {
        let mut grid = Grid::new(2, 2);
        test_snake_navigation(&mut grid)
    }

    #[test]
    fn test_snake_navigation_3x3_grid() -> Result<()> {
        let mut grid = Grid::new(1, 3);
        test_snake_navigation(&mut grid)
    }

    #[test]
    fn test_snake_navigation_9x9_grid() -> Result<()> {
        let mut grid = Grid::new(2, 3);
        test_snake_navigation(&mut grid)
    }

    #[test]
    fn test_snake_navigation_8x8_grid() {
        let mut grid = Grid::new(3, 2);
        test_snake_navigation(&mut grid).unwrap()
    }

    // Irregular layout tests

    #[derive(canopy::StatefulNode)]
    struct IrregularBlock {
        state: NodeState,
        children: Vec<Self>,
        rect: Rect,
        name_str: String,
    }

    #[derive_commands]
    impl IrregularBlock {
        fn new(name: &str, rect: Rect) -> Self {
            Self {
                state: NodeState::default(),
                children: vec![],
                rect,
                name_str: name.to_string(),
            }
        }

        fn add_child(&mut self, child: Self) {
            self.children.push(child);
        }
    }

    impl Node for IrregularBlock {
        fn accept_focus(&mut self) -> bool {
            self.children.is_empty()
        }

        fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
            if self.children.is_empty() {
                let self_expanse = self.rect.expanse();
                self.fill(self_expanse)?;
            } else {
                self.fill(sz)?;
                let child_rects: Vec<Rect> = self.children.iter().map(|c| c.rect).collect();
                for (child, rect) in self.children.iter_mut().zip(child_rects.iter()) {
                    l.place(child, *rect)?;
                }
            }
            Ok(())
        }

        fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
            for child in &mut self.children {
                f(child)?;
            }
            Ok(())
        }
    }

    // Helper function to get focused node name for irregular blocks
    fn get_focused_name(node: &mut IrregularBlock, canopy: &Canopy) -> Option<String> {
        if Context::is_focused(canopy, node) && node.accept_focus() {
            Some(node.name_str.clone())
        } else {
            for child in &mut node.children {
                if let Some(name) = get_focused_name(child, canopy) {
                    return Some(name);
                }
            }
            None
        }
    }

    // Helper to set focus on a specific node by name
    fn set_focus_on(node: &mut IrregularBlock, canopy: &mut Canopy, target: &str) -> bool {
        if node.name_str == target && node.accept_focus() {
            canopy.set_focus(node);
            true
        } else {
            for child in &mut node.children {
                if set_focus_on(child, canopy, target) {
                    return true;
                }
            }
            false
        }
    }

    /// Test that focus navigation doesn't skip tall nodes when moving from middle nodes
    #[test]
    fn test_focus_skip_issue_reproduction() -> Result<()> {
        use canopy::Canopy;

        // Layout visualization:
        // |------------|--------------------------|
        // |            |  top_right (20,0,80,12)  |
        // |  left_tall |--------------------------|
        // | (0,0,30,60)|  mid_left  |  mid_right  |
        // |            | (30,12,35,36)(65,12,35,36)|
        // |            |--------------------------|
        // |            |  bottom (20,48,80,12)    |
        // |------------|--------------------------|

        let mut root = IrregularBlock::new("root", Rect::new(0, 0, 100, 60));

        // Left side: single tall cell that spans the entire height
        let left_tall = IrregularBlock::new("left_tall", Rect::new(0, 0, 30, 60));

        // Right side container
        let mut right_container = IrregularBlock::new("right_container", Rect::new(30, 0, 70, 60));

        // Right side children
        let top_right = IrregularBlock::new("top_right", Rect::new(30, 0, 70, 12));
        let mid_left = IrregularBlock::new("mid_left", Rect::new(30, 12, 35, 36));
        let mid_right = IrregularBlock::new("mid_right", Rect::new(65, 12, 35, 36));
        let bottom = IrregularBlock::new("bottom", Rect::new(30, 48, 70, 12));

        right_container.add_child(top_right);
        right_container.add_child(mid_left);
        right_container.add_child(mid_right);
        right_container.add_child(bottom);

        root.add_child(left_tall);
        root.add_child(right_container);

        // Layout
        let layout = Layout {};
        let size = Expanse::new(100, 60);
        root.layout(&layout, size)?;

        let mut canopy = Canopy::new();

        // Test case: Focus on mid_left and try to move left
        // This should go to left_tall, not skip it
        set_focus_on(&mut root, &mut canopy, "mid_left");

        let before_move = get_focused_name(&mut root, &canopy);
        assert_eq!(before_move.as_deref(), Some("mid_left"));

        canopy.focus_left(&mut root);
        let after_left = get_focused_name(&mut root, &canopy);

        assert_eq!(
            after_left.as_deref(),
            Some("left_tall"),
            "Focus should move to left_tall which completely covers the left vantage of mid_left"
        );

        Ok(())
    }

    /// Test focus navigation with nodes that have extreme size differences
    #[test]
    fn test_focus_navigation_extreme_sizes() -> Result<()> {
        use canopy::Canopy;

        let mut root = IrregularBlock::new("root", Rect::new(0, 0, 100, 50));

        // Tiny node on the left (5% width)
        let tiny = IrregularBlock::new("tiny", Rect::new(0, 0, 5, 50));

        // Huge node in the middle (85% width)
        let huge = IrregularBlock::new("huge", Rect::new(5, 0, 85, 50));

        // Medium node on the right (10% width)
        let medium = IrregularBlock::new("medium", Rect::new(90, 0, 10, 50));

        root.add_child(tiny);
        root.add_child(huge);
        root.add_child(medium);

        let layout = Layout {};
        root.layout(&layout, Expanse::new(100, 50))?;

        let mut canopy = Canopy::new();

        // Test navigation doesn't skip the huge middle node
        canopy.focus_first(&mut root);

        canopy.focus_right(&mut root);
        let focused_name = get_focused_name(&mut root, &canopy);
        assert_eq!(
            focused_name,
            Some("huge".to_string()),
            "Should focus huge node, not skip to medium"
        );

        // Now test from huge to medium
        canopy.focus_right(&mut root);
        let focused_name = get_focused_name(&mut root, &canopy);
        assert_eq!(
            focused_name,
            Some("medium".to_string()),
            "Should now focus medium node"
        );

        Ok(())
    }

    /// Test that focus navigation doesn't allow diagonal movement
    #[test]
    fn test_no_diagonal_focus_movement() -> Result<()> {
        use canopy::Canopy;

        // Create a layout that reproduces the diagonal movement issue
        // Two nodes side by side, but with different vertical centers
        //
        // |---------|---------|
        // | A       | B       |
        // |(0,0,    |(50,20,  |
        // | 50,60)  | 50,40)  |
        // |         |---------|
        // |         |
        // |---------|
        //
        // From B, pressing down should NOT go to A even though A's center is lower

        let mut root = IrregularBlock::new("root", Rect::new(0, 0, 100, 60));

        // Node A: tall on the left, extends below B
        let node_a = IrregularBlock::new("node_a", Rect::new(0, 0, 50, 60));

        // Node B: shorter on the right, starts lower
        let node_b = IrregularBlock::new("node_b", Rect::new(50, 20, 50, 40));

        root.add_child(node_a);
        root.add_child(node_b);

        let layout = Layout {};
        root.layout(&layout, Expanse::new(100, 60))?;

        let mut canopy = Canopy::new();

        // Test case: From B, pressing down should do nothing (not move to A)
        set_focus_on(&mut root, &mut canopy, "node_b");
        assert_eq!(
            get_focused_name(&mut root, &canopy).as_deref(),
            Some("node_b")
        );

        canopy.focus_down(&mut root);
        let after_down = get_focused_name(&mut root, &canopy);

        assert_eq!(
            after_down.as_deref(),
            Some("node_b"),
            "Focus should stay on node_b when pressing down, not move diagonally to node_a"
        );

        Ok(())
    }

    /// Test complex overlapping nodes where B is shorter than its neighbors
    #[test]
    fn test_focus_skip_complex_overlap() -> Result<()> {
        use canopy::Canopy;

        // Layout:
        // |-------|--------|--------|
        // | A     | B      | C      |
        // |(0,0,  |(25,0,  |(50,0,  |
        // | 25,20)| 25,10) | 50,20) |
        // |-------|--------|--------|
        // | D              | E      |
        // |(0,20,50,30)    |(50,20, |
        // |                | 50,30) |
        // |----------------|--------|

        let mut root = IrregularBlock::new("root", Rect::new(0, 0, 100, 50));

        // Top row - three nodes with B being shorter
        let node_a = IrregularBlock::new("node_a", Rect::new(0, 0, 25, 20));
        let node_b = IrregularBlock::new("node_b", Rect::new(25, 0, 25, 10)); // Shorter!
        let node_c = IrregularBlock::new("node_c", Rect::new(50, 0, 50, 20));

        // Bottom row - D spans A and B width, E aligns with C
        let node_d = IrregularBlock::new("node_d", Rect::new(0, 20, 50, 30));
        let node_e = IrregularBlock::new("node_e", Rect::new(50, 20, 50, 30));

        root.add_child(node_a);
        root.add_child(node_b);
        root.add_child(node_c);
        root.add_child(node_d);
        root.add_child(node_e);

        let layout = Layout {};
        root.layout(&layout, Expanse::new(100, 50))?;

        let mut canopy = Canopy::new();

        // Test case 1: From node_c, moving left should go to node_b (not skip to node_a)
        set_focus_on(&mut root, &mut canopy, "node_c");
        assert_eq!(
            get_focused_name(&mut root, &canopy).as_deref(),
            Some("node_c")
        );

        canopy.focus_left(&mut root);
        let after_left_from_c = get_focused_name(&mut root, &canopy);

        // Should go to B which is directly to the left, even though it's shorter
        assert_eq!(
            after_left_from_c.as_deref(),
            Some("node_b"),
            "Should focus node_b which is directly left of node_c"
        );

        // Test case 2: From node_e, moving left should consider node_d
        set_focus_on(&mut root, &mut canopy, "node_e");
        canopy.focus_left(&mut root);
        let after_left_from_e = get_focused_name(&mut root, &canopy);

        assert_eq!(
            after_left_from_e.as_deref(),
            Some("node_d"),
            "Should focus node_d which covers the left side"
        );

        Ok(())
    }

    /// Test more complex diagonal movement scenarios
    #[test]
    fn test_diagonal_movement_complex() -> Result<()> {
        use canopy::Canopy;

        // Create a more complex layout with multiple potential diagonal movements
        //
        // |-------|-------|-------|
        // | A     | B     | C     |
        // |(0,0,  |(40,10,|(70,0,  |
        // | 30,30)| 20,20)| 30,40)|
        // |-------|-------|       |
        // | D             |       |
        // |(0,35,40,25)   |       |
        // |---------------|-------|
        //
        // Test various movements that should NOT happen:
        // - From B down should not go to D (no horizontal overlap)
        // - From C down should not go to D (no horizontal overlap)
        // - From D up should only go to A (only A has horizontal overlap)

        let mut root = IrregularBlock::new("root", Rect::new(0, 0, 100, 60));

        let node_a = IrregularBlock::new("node_a", Rect::new(0, 0, 30, 30));
        let node_b = IrregularBlock::new("node_b", Rect::new(40, 10, 20, 20));
        let node_c = IrregularBlock::new("node_c", Rect::new(70, 0, 30, 40));
        let node_d = IrregularBlock::new("node_d", Rect::new(0, 35, 40, 25));

        root.add_child(node_a);
        root.add_child(node_b);
        root.add_child(node_c);
        root.add_child(node_d);

        let layout = Layout {};
        root.layout(&layout, Expanse::new(100, 60))?;

        let mut canopy = Canopy::new();

        // Test 1: From B pressing down - should NOT reach D
        set_focus_on(&mut root, &mut canopy, "node_b");
        canopy.focus_down(&mut root);
        let from_b_down = get_focused_name(&mut root, &canopy);

        assert_eq!(
            from_b_down.as_deref(),
            Some("node_b"),
            "From B, pressing down should not move to D (D is not directly below B)"
        );

        // Test 2: From C pressing down - should NOT reach D (no horizontal overlap)
        set_focus_on(&mut root, &mut canopy, "node_c");
        canopy.focus_down(&mut root);
        let from_c_down = get_focused_name(&mut root, &canopy);

        assert_eq!(
            from_c_down.as_deref(),
            Some("node_c"),
            "From C, pressing down should not move to D (no horizontal overlap)"
        );

        // Test 3: From D pressing up - should go to A (leftmost valid option)
        set_focus_on(&mut root, &mut canopy, "node_d");
        canopy.focus_up(&mut root);
        let from_d_up = get_focused_name(&mut root, &canopy);

        // Should go to A since it's directly above part of D
        assert_eq!(
            from_d_up.as_deref(),
            Some("node_a"),
            "From D, pressing up should move to A"
        );

        Ok(())
    }
}
