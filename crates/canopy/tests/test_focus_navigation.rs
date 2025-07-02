use canopy::tree::*;
use canopy::*;
use canopy_core::{Context, tutils::Grid};

/// Helper function to get the currently focused cell name
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

/// Test snake navigation on a grid, expecting 100% coverage
/// Returns Ok(()) if all cells were visited, Err with details if not
fn test_snake_navigation(grid: &mut Grid) -> Result<()> {
    use canopy::Canopy;

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

    println!("✓ focus_dir works correctly for simple 2x2 grid");

    Ok(())
}

#[test]
fn test_focus_dir_deep_grid() -> Result<()> {
    use canopy::Canopy;

    // Test with 4x4 grid (recursion=2, divisions=2) which creates a full grid of cells
    let mut grid = Grid::new(2, 2);
    let grid_size = grid.expected_size();
    assert_eq!(grid_size, Expanse::new(40, 40), "4x4 grid should be 40x40");

    let layout = Layout {};
    grid.layout(&layout, grid_size)?;

    let mut canopy = Canopy::new();

    // Test that focus_dir now works correctly!
    canopy.focus_first(&mut grid);
    assert_eq!(
        get_focused_cell(&canopy, &mut grid),
        Some("cell_0_0".to_string())
    );

    // Test navigating through the first row
    println!("\n=== Testing focus_right navigation in 4x4 grid ===");
    let mut positions = vec!["cell_0_0".to_string()];

    for i in 1..4 {
        canopy.focus_right(&mut grid);
        let pos = get_focused_cell(&canopy, &mut grid);
        if let Some(p) = &pos {
            positions.push(p.clone());
            assert_eq!(p, &format!("cell_{i}_0"), "Should move to cell_{i}_0");
        }
    }

    println!("Successfully navigated first row: {positions:?}");

    // Test moving down from the last cell in the row
    canopy.focus_down(&mut grid);
    assert_eq!(
        get_focused_cell(&canopy, &mut grid),
        Some("cell_3_1".to_string()),
        "Should move down to cell_3_1"
    );

    // Test moving left
    canopy.focus_left(&mut grid);
    assert_eq!(
        get_focused_cell(&canopy, &mut grid),
        Some("cell_2_1".to_string()),
        "Should move left to cell_2_1"
    );

    // Test moving up
    canopy.focus_up(&mut grid);
    assert_eq!(
        get_focused_cell(&canopy, &mut grid),
        Some("cell_2_0".to_string()),
        "Should move up to cell_2_0"
    );

    println!("\n✓ focus_dir now works correctly in deeply nested grids!");

    Ok(())
}

#[test]
fn test_focus_dir_zigzag_navigation() -> Result<()> {
    use canopy::Canopy;

    // Test zigzag navigation through entire grid
    let mut grid = Grid::new(2, 2); // 4x4 grid
    let grid_size = grid.expected_size();
    assert_eq!(grid_size, Expanse::new(40, 40), "4x4 grid should be 40x40");

    let layout = Layout {};
    grid.layout(&layout, grid_size)?;

    let mut canopy = Canopy::new();

    println!("\n=== Testing zigzag navigation through 4x4 grid ===");
    canopy.focus_first(&mut grid);

    // Navigate in a zigzag pattern through the entire grid
    // Row 0: left to right
    for col in 0..4 {
        assert_eq!(
            get_focused_cell(&canopy, &mut grid),
            Some(format!("cell_{col}_0")),
            "Row 0, should be at cell_{col}_0"
        );
        if col < 3 {
            canopy.focus_right(&mut grid);
        }
    }

    // Move down to row 1
    canopy.focus_down(&mut grid);

    // Row 1: right to left
    for col in (0..4).rev() {
        assert_eq!(
            get_focused_cell(&canopy, &mut grid),
            Some(format!("cell_{col}_1")),
            "Row 1, should be at cell_{col}_1"
        );
        if col > 0 {
            canopy.focus_left(&mut grid);
        }
    }

    // Move down to row 2
    canopy.focus_down(&mut grid);

    // Row 2: left to right
    for col in 0..4 {
        assert_eq!(
            get_focused_cell(&canopy, &mut grid),
            Some(format!("cell_{col}_2")),
            "Row 2, should be at cell_{col}_2"
        );
        if col < 3 {
            canopy.focus_right(&mut grid);
        }
    }

    // Move down to row 3
    canopy.focus_down(&mut grid);

    // Row 3: right to left
    for col in (0..4).rev() {
        assert_eq!(
            get_focused_cell(&canopy, &mut grid),
            Some(format!("cell_{col}_3")),
            "Row 3, should be at cell_{col}_3"
        );
        if col > 0 {
            canopy.focus_left(&mut grid);
        }
    }

    println!("✓ Successfully completed zigzag navigation through entire 4x4 grid!");

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
    // This test is expected to fail due to container boundary issues
    let mut grid = Grid::new(3, 2);
    test_snake_navigation(&mut grid).unwrap()
}

#[test]
fn test_snake_navigation_27x27_grid() {
    // This test is expected to fail due to container boundary issues
    let mut grid = Grid::new(3, 3);
    test_snake_navigation(&mut grid).unwrap()
}

#[test]
fn test_grid_structure_understanding() -> Result<()> {
    // This test helps understand how Grid structures work
    println!("\n=== Understanding Grid structure ===");

    // Simple grid: 3x3
    let simple = Grid::new(1, 3);
    let (w1, h1) = simple.dimensions();
    println!(
        "Grid(1, 3) creates a {}x{} grid ({:?} pixels)",
        w1,
        h1,
        simple.expected_size()
    );

    // Nested grid: 9x9
    let nested = Grid::new(2, 3);
    let (w2, h2) = nested.dimensions();
    println!(
        "Grid(2, 3) creates a {}x{} grid ({:?} pixels)",
        w2,
        h2,
        nested.expected_size()
    );

    // Deep grid: 27x27
    let mut deep = Grid::new(3, 3);
    let (w3, h3) = deep.dimensions();
    println!(
        "Grid(3, 3) creates a {}x{} grid ({:?} pixels)",
        w3,
        h3,
        deep.expected_size()
    );

    // Layout the deep grid and count focusable cells
    let layout = Layout {};
    deep.layout(&layout, deep.expected_size())?;

    let mut cell_count = 0;
    let deep_node: &mut dyn Node = &mut deep;
    preorder(deep_node, &mut |node| -> Result<Walk<()>> {
        if node.name().to_string().starts_with("cell_") && node.accept_focus() {
            cell_count += 1;
        }
        Ok(Walk::Continue)
    })?;

    println!("Grid(3, 3) has {cell_count} focusable cells");
    println!("Note: Grid creates a hierarchical structure");
    println!("Cells are named by their absolute position in the grid (0-26 for each dimension)");

    Ok(())
}

/// Helper function to collect focusable nodes with their screen coordinates
fn collect_nodes_recursive(
    node: &mut dyn Node,
    view_stack: &mut ViewStack,
    nodes: &mut Vec<(String, Rect, Rect)>,
) -> Result<()> {
    if node.is_hidden() {
        return Ok(());
    }

    let node_vp = node.vp();
    if node_vp.view().is_zero() {
        return Ok(());
    }

    // Push viewport
    view_stack.push(node_vp);

    // Get screen rect from projection
    if let Some((canvas_rect, screen_rect)) = view_stack.projection() {
        if node.accept_focus() && node.name().to_string().starts_with("cell_") {
            nodes.push((node.name().to_string(), canvas_rect, screen_rect));
        }

        // Process children
        node.children(&mut |child| {
            collect_nodes_recursive(child, view_stack, nodes)?;
            Ok(())
        })?;
    }

    // Pop viewport
    view_stack.pop()?;

    Ok(())
}

#[test]
fn test_focus_navigation_with_viewstack() -> Result<()> {
    use canopy::Canopy;

    // Test that ViewStack correctly calculates screen coordinates
    let mut grid = Grid::new(2, 2); // 4x4 grid
    let grid_size = grid.expected_size();

    let layout = Layout {};
    grid.layout(&layout, grid_size)?;

    println!("\n=== Testing ViewStack screen coordinates ===");

    // Manually collect focusable nodes with screen coordinates
    let mut nodes_info = Vec::new();

    // Create initial ViewStack
    let root_vp = grid.vp();
    let screen_vp = ViewPort::new(root_vp.canvas(), root_vp.canvas().rect(), (0, 0))?;
    let mut view_stack = ViewStack::new(screen_vp);

    collect_nodes_recursive(&mut grid, &mut view_stack, &mut nodes_info)?;

    // Sort by y then x
    nodes_info.sort_by(|a, b| a.2.tl.y.cmp(&b.2.tl.y).then(a.2.tl.x.cmp(&b.2.tl.x)));

    println!("Found {} focusable nodes:", nodes_info.len());

    // Verify screen coordinates are correct
    // In a 4x4 grid, cells should be at positions (0,0), (10,0), (20,0), (30,0), etc.
    let first_row: Vec<_> = nodes_info.iter().filter(|(_, _, r)| r.tl.y == 0).collect();

    println!("First row has {} cells", first_row.len());
    for (i, (name, _, screen)) in first_row.iter().enumerate() {
        assert_eq!(
            screen.tl.x as usize,
            i * 10,
            "Cell {name} should be at x={}",
            i * 10
        );
    }

    // Test navigation works through the first row
    let mut canopy = Canopy::new();
    canopy.focus_first(&mut grid);

    for i in 0..4 {
        assert_eq!(
            get_focused_cell(&canopy, &mut grid),
            Some(format!("cell_{i}_0"))
        );
        if i < 3 {
            canopy.focus_right(&mut grid);
        }
    }

    println!("✓ ViewStack and navigation working correctly");

    Ok(())
}
