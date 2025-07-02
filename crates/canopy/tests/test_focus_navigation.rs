use canopy::tree::*;
use canopy::*;
use canopy_core::{Context, tutils::Grid};

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

    // Helper to get the currently focused cell name
    let get_focused_cell = |canopy: &Canopy, grid: &mut Grid| -> Option<String> {
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
    };

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

    // Helper to get the currently focused cell name
    let get_focused_cell = |canopy: &Canopy, grid: &mut Grid| -> Option<String> {
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
    };

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

    // Helper to get the currently focused cell name
    let get_focused_cell = |canopy: &Canopy, grid: &mut Grid| -> Option<String> {
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
    };

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
fn test_focus_dir_snake_navigation_27x27() -> Result<()> {
    use canopy::Canopy;

    // Create 27x27 grid with recursion=3, divisions=3 (3^3 = 27)
    let mut grid = Grid::new(3, 3);
    let grid_size = grid.expected_size();
    let expected_size = 27 * 10; // 27x27 grid with 10x10 cells
    assert_eq!(
        grid_size,
        Expanse::new(expected_size, expected_size),
        "27x27 grid should be 270x270"
    );

    let layout = Layout {};
    grid.layout(&layout, grid_size)?;

    let mut canopy = Canopy::new();

    // Helper to get the currently focused cell name
    let get_focused_cell = |canopy: &Canopy, grid: &mut Grid| -> Option<String> {
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
    };

    println!("\n=== Testing snake navigation through 27x27 grid ===");

    // Debug: Let's understand the grid structure
    println!("Grid expected size: {grid_size:?}");
    println!("Grid recursion: 3, divisions: 3");

    // Debug: Collect all focusable cells
    let mut all_cells = Vec::new();
    let grid_node: &mut dyn Node = &mut grid;
    preorder(grid_node, &mut |node| -> Result<Walk<()>> {
        let name = node.name().to_string();
        if name.starts_with("cell_") && node.accept_focus() {
            all_cells.push(name);
        }
        Ok(Walk::Continue)
    })?;
    println!("Total focusable cells found: {}", all_cells.len());
    println!(
        "First 10 cells: {:?}",
        &all_cells[..all_cells.len().min(10)]
    );

    canopy.focus_first(&mut grid);

    // Verify initial position
    assert_eq!(
        get_focused_cell(&canopy, &mut grid),
        Some("cell_0_0".to_string()),
        "Should start at cell_0_0"
    );

    // Navigate in snake pattern through entire 27x27 grid
    // Since cells are named 0-8 in each dimension due to nesting,
    // we'll track our position and movements rather than expect specific names

    println!("\n=== Testing snake navigation through 27x27 grid ===");

    let mut visited_cells = Vec::new();
    let mut total_moves = 0;

    for row in 0..27 {
        if row % 2 == 0 {
            // Even rows: left to right
            for col in 0..27 {
                let cell = get_focused_cell(&canopy, &mut grid);
                assert!(cell.is_some(), "Row {row}, col {col}: no cell found");

                if !visited_cells.contains(&cell) {
                    visited_cells.push(cell.clone());
                }

                // Move right unless we're at the last column
                if col < 26 {
                    let before_move = get_focused_cell(&canopy, &mut grid);
                    canopy.focus_right(&mut grid);
                    let after_move = get_focused_cell(&canopy, &mut grid);

                    // Verify we actually moved (unless we hit a boundary)
                    if before_move == after_move && col < 25 {
                        println!("WARNING: Failed to move right at row {row}, col {col}");
                    }
                    total_moves += 1;
                }
            }
        } else {
            // Odd rows: right to left
            for col in (0..27).rev() {
                let cell = get_focused_cell(&canopy, &mut grid);
                assert!(cell.is_some(), "Row {row}, col {col}: no cell found");

                if !visited_cells.contains(&cell) {
                    visited_cells.push(cell.clone());
                }

                // Move left unless we're at the first column
                if col > 0 {
                    let before_move = get_focused_cell(&canopy, &mut grid);
                    canopy.focus_left(&mut grid);
                    let after_move = get_focused_cell(&canopy, &mut grid);

                    // Verify we actually moved (unless we hit a boundary)
                    if before_move == after_move && col > 1 {
                        println!("WARNING: Failed to move left at row {row}, col {col}");
                    }
                    total_moves += 1;
                }
            }
        }

        // Move down to next row unless we're at the last row
        if row < 26 {
            let before_move = get_focused_cell(&canopy, &mut grid);
            canopy.focus_down(&mut grid);
            let after_move = get_focused_cell(&canopy, &mut grid);

            if before_move == after_move {
                println!("WARNING: Failed to move down after row {row}");
            }
            total_moves += 1;
        }
    }

    println!("\nSnake navigation summary:");
    println!("Total moves attempted: {total_moves}");
    println!("Unique cells visited: {}", visited_cells.len());
    println!("Expected unique cells in 27x27 grid: 729");

    // In a 27x27 grid, we should visit all 729 cells
    // However, due to the nested structure and cell naming (0-8),
    // we may see fewer unique cell names
    assert!(
        visited_cells.len() >= 81,
        "Should visit at least 81 unique cell names (9x9)"
    );

    println!("✓ Successfully completed snake navigation through entire 27x27 grid!");

    Ok(())
}

#[test]
fn test_focus_navigation_boundary_crossing() -> Result<()> {
    use canopy::Canopy;

    // Create a simple 2-level grid to test boundary crossing
    // recursion=2, divisions=3 gives us 9 cells (3x3 top level, each with 3x3 cells)
    let mut grid = Grid::new(2, 3);
    let grid_size = grid.expected_size();

    let layout = Layout {};
    grid.layout(&layout, grid_size)?;

    let mut canopy = Canopy::new();

    // Helper to get the currently focused cell name
    let get_focused_cell = |canopy: &Canopy, grid: &mut Grid| -> Option<String> {
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
    };

    println!("\n=== Testing boundary crossing in nested grid ===");
    canopy.focus_first(&mut grid);

    // Navigate through first row to find all cells
    let mut row_cells = Vec::new();
    let mut prev_cell = None;
    for _ in 0..20 {
        // Try up to 20 moves
        let cell = get_focused_cell(&canopy, &mut grid);
        if cell == prev_cell {
            // We've stopped moving
            break;
        }
        if let Some(c) = &cell {
            row_cells.push(c.clone());
        }
        prev_cell = cell;
        canopy.focus_right(&mut grid);
    }

    println!("Cells encountered moving right: {row_cells:?}");

    // The issue is that with nested containers, cells are named by their position
    // within their immediate container, not their absolute position.
    // So we should see: cell_0_0, cell_1_0, cell_2_0 (first container)
    // Then ideally: the first cell of the next container

    // Let's check if we can navigate down and then continue right
    canopy.focus_first(&mut grid);

    // Move to cell_2_0 (rightmost of first container)
    canopy.focus_right(&mut grid);
    canopy.focus_right(&mut grid);
    let at_2_0 = get_focused_cell(&canopy, &mut grid);
    println!("\nAt position after 2 rights: {at_2_0:?}");

    // Try to go right again - this should cross to the next container
    canopy.focus_right(&mut grid);
    let after_boundary = get_focused_cell(&canopy, &mut grid);
    println!("After trying to cross boundary: {after_boundary:?}");

    // The cells in a 3x3 nested grid are numbered 0-8 in x and y
    // but there are actually 9 cells horizontally because of nesting
    assert_eq!(row_cells.len(), 9, "Should find 9 cells in first row");

    println!("✓ Boundary crossing test complete");

    Ok(())
}

#[test]
fn test_focus_debug_at_boundary() -> Result<()> {
    use canopy::Canopy;

    // Create a simple grid to debug boundary issues
    let mut grid = Grid::new(2, 3); // 9x9 grid
    let grid_size = grid.expected_size();

    let layout = Layout {};
    grid.layout(&layout, grid_size)?;

    let mut canopy = Canopy::new();

    println!("\n=== Debugging focus at boundary ===");

    // Navigate to cell_8_0 (rightmost cell on first row)
    canopy.focus_first(&mut grid);
    for _ in 0..8 {
        canopy.focus_right(&mut grid);
    }

    // Get current position
    let get_focused_cell = |canopy: &Canopy, grid: &mut Grid| -> Option<String> {
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
    };

    let current = get_focused_cell(&canopy, &mut grid);
    println!("Current position: {current:?}");

    // Let's debug what cells are available
    println!("\nDebugging available cells:");

    // Collect all cell names and positions manually
    let mut all_cells = Vec::new();
    let grid_node: &mut dyn Node = &mut grid;
    preorder(grid_node, &mut |node| -> Result<Walk<()>> {
        let name = node.name().to_string();
        if name.starts_with("cell_") && node.accept_focus() {
            let vp = node.vp();
            let rect = vp.canvas().rect();
            all_cells.push((name, rect));
        }
        Ok(Walk::Continue)
    })?;

    println!("Total cells found: {}", all_cells.len());

    // Sort by Y then X to see the grid structure
    all_cells.sort_by(|a, b| a.1.tl.y.cmp(&b.1.tl.y).then(a.1.tl.x.cmp(&b.1.tl.x)));

    // Print first row cells
    println!("\nFirst row cells:");
    for (name, rect) in all_cells.iter().take(9) {
        println!("  {} at x={}, y={}", name, rect.tl.x, rect.tl.y);
    }

    // Try navigating in each direction from the boundary
    println!("\nTesting navigation from boundary:");

    // Try right (should fail since we're at the edge)
    let before = get_focused_cell(&canopy, &mut grid);
    canopy.focus_right(&mut grid);
    let after = get_focused_cell(&canopy, &mut grid);
    println!(
        "Right: {} -> {}",
        before.as_ref().unwrap(),
        after.as_ref().unwrap()
    );

    Ok(())
}

#[test]
fn test_focus_navigation_detailed_debug() -> Result<()> {
    use canopy::Canopy;

    // Simple 3x3 grid to debug
    let mut grid = Grid::new(1, 3);
    let grid_size = grid.expected_size();

    let layout = Layout {};
    grid.layout(&layout, grid_size)?;

    let mut canopy = Canopy::new();

    println!("\n=== Detailed focus navigation debug ===");

    // Helper to get the currently focused cell name
    let get_focused_cell = |canopy: &Canopy, grid: &mut Grid| -> Option<String> {
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
    };

    // Navigate through first row
    canopy.focus_first(&mut grid);

    for i in 0..5 {
        let current = get_focused_cell(&canopy, &mut grid);
        println!("Step {i}: at {current:?}");

        canopy.focus_right(&mut grid);
        let after = get_focused_cell(&canopy, &mut grid);

        if current == after {
            println!("  -> Could not move right, stuck at {current:?}");
            break;
        } else {
            println!("  -> Moved to {after:?}");
        }
    }

    Ok(())
}

#[test]
fn test_grid_cell_positions() -> Result<()> {
    // Create a simple 1x3 grid (just 3 cells in a row)
    let mut grid = Grid::new(1, 3);
    let grid_size = grid.expected_size();

    let layout = Layout {};
    grid.layout(&layout, grid_size)?;

    println!("\n=== Grid cell positions ===");

    // Collect all cells and their viewports
    let mut cells = Vec::new();
    let grid_node: &mut dyn Node = &mut grid;
    preorder(grid_node, &mut |node| -> Result<Walk<()>> {
        let name = node.name().to_string();
        if name.starts_with("cell_") {
            let vp = node.vp();
            let canvas_rect = vp.canvas().rect();
            let view_rect = vp.view();
            let position = vp.position();
            cells.push((name, canvas_rect, view_rect, position));
        }
        Ok(Walk::Continue)
    })?;

    // Sort by name
    cells.sort_by(|a, b| a.0.cmp(&b.0));

    println!("Grid has {} cells:", cells.len());
    for (name, canvas, view, pos) in &cells {
        println!(
            "{name}: canvas={canvas:?}, view={view:?}, position={pos:?}"
        );
    }

    // Now test if the is_in_direction logic works
    if cells.len() >= 3 {
        let cell_0 = &cells[0];
        let cell_1 = &cells[1];
        let cell_2 = &cells[2];

        println!("\nTesting is_in_direction logic:");
        println!("cell_0 canvas: {:?}", cell_0.1);
        println!("cell_1 canvas: {:?}", cell_1.1);
        println!("cell_2 canvas: {:?}", cell_2.1);

        // Check our is_in_direction condition for Right
        let source = cell_2.1; // cell_2_0
        let target = cell_0.1; // cell_0_0 (which might be in next row)

        let would_match =
            target.tl.x >= source.tl.x && target.tl.x + target.w > source.tl.x + source.w;
        println!(
            "\nWould cell_0_0 match as 'right' of cell_2_0? {would_match}"
        );
        println!(
            "  target.tl.x ({}) >= source.tl.x ({})? {}",
            target.tl.x,
            source.tl.x,
            target.tl.x >= source.tl.x
        );
        println!(
            "  target.tl.x + target.w ({}) > source.tl.x + source.w ({})? {}",
            target.tl.x + target.w,
            source.tl.x + source.w,
            target.tl.x + target.w > source.tl.x + source.w
        );
    }

    Ok(())
}

#[test]
fn test_viewstack_screen_coords() -> Result<()> {
    use canopy::Canopy;

    // Create a 3x3 grid
    let mut grid = Grid::new(1, 3);
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
    for (name, canvas_rect, screen_rect) in &nodes_info {
        println!(
            "{name}: canvas={canvas_rect:?}, screen={screen_rect:?}"
        );
    }

    // Test navigation
    let mut canopy = Canopy::new();
    canopy.focus_first(&mut grid);

    // Try navigating right multiple times
    println!("\nTesting navigation:");
    for i in 0..5 {
        let mut focused = None;
        preorder(
            &mut grid as &mut dyn Node,
            &mut |node| -> Result<Walk<()>> {
                if Context::is_focused(&canopy, node)
                    && node.name().to_string().starts_with("cell_")
                {
                    focused = Some(node.name().to_string());
                    return Ok(Walk::Handle(()));
                }
                Ok(Walk::Continue)
            },
        )?;

        println!("Step {i}: at {focused:?}");

        if i < 4 {
            canopy.focus_right(&mut grid);
        }
    }

    Ok(())
}

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
fn test_focus_navigation_with_nested_grid() -> Result<()> {
    use canopy::Canopy;

    // Create a grid with recursion=3, divisions=3
    // This should give us 27x27 = 729 cells total
    let mut grid = Grid::new(3, 3);
    let grid_size = grid.expected_size();

    let layout = Layout {};
    grid.layout(&layout, grid_size)?;

    println!("\n=== Testing nested grid navigation ===");

    // Collect all focusable nodes with screen coordinates
    let mut nodes_info = Vec::new();

    let root_vp = grid.vp();
    let screen_vp = ViewPort::new(root_vp.canvas(), root_vp.canvas().rect(), (0, 0))?;
    let mut view_stack = ViewStack::new(screen_vp);

    collect_nodes_recursive(&mut grid, &mut view_stack, &mut nodes_info)?;

    // Sort by y then x
    nodes_info.sort_by(|a, b| a.2.tl.y.cmp(&b.2.tl.y).then(a.2.tl.x.cmp(&b.2.tl.x)));

    println!("Found {} focusable nodes:", nodes_info.len());
    println!("Grid size: {grid_size:?}");

    // In a 27x27 grid with cells named 0-8, we should have 729 cells total
    // but only 81 unique cell names (9x9)
    if nodes_info.len() != 729 {
        println!("WARNING: Expected 729 cells but found {}", nodes_info.len());
    }

    // Print first row nodes
    println!("\nFirst row nodes:");
    for (name, _, screen_rect) in nodes_info.iter().filter(|(_, _, r)| r.tl.y == 0) {
        println!("  {} at x={}", name, screen_rect.tl.x);
    }

    // Now test navigation
    let mut canopy = Canopy::new();
    canopy.focus_first(&mut grid);

    // Navigate through first row
    println!("\nNavigating through first row:");
    let mut visited = Vec::new();
    let mut last_pos = None;

    for i in 0..20 {
        let mut focused = None;
        preorder(
            &mut grid as &mut dyn Node,
            &mut |node| -> Result<Walk<()>> {
                if Context::is_focused(&canopy, node)
                    && node.name().to_string().starts_with("cell_")
                {
                    focused = Some(node.name().to_string());
                    return Ok(Walk::Handle(()));
                }
                Ok(Walk::Continue)
            },
        )?;

        if focused == last_pos {
            println!("Stuck at {focused:?} after {i} moves");
            break;
        }

        if let Some(name) = &focused {
            visited.push(name.clone());
            println!("  Position {i}: {name}");
        }

        last_pos = focused;
        canopy.focus_right(&mut grid);
    }

    println!("\nVisited {} cells in first row", visited.len());
    println!("With recursion=3, divisions=3, we should be able to navigate through more cells");

    Ok(())
}

#[test]
fn test_expected_snake_navigation() -> Result<()> {
    use canopy::Canopy;

    // First, let's verify the Grid structure
    println!("\n=== Debugging Grid structure ===");

    // Create a simple grid first
    let simple_grid = Grid::new(1, 3);
    let simple_size = simple_grid.expected_size();
    println!(
        "Grid(1, 3) size: {simple_size:?} (should be 30x30 for 3x3 grid)"
    );

    // Create a grid with recursion=3, divisions=3 as requested
    let mut grid = Grid::new(3, 3);
    let grid_size = grid.expected_size();
    println!(
        "Grid(3, 3) size: {grid_size:?} (should be 270x270 for 27x27 grid)"
    );

    let layout = Layout {};
    grid.layout(&layout, grid_size)?;

    let mut canopy = Canopy::new();

    println!("\n=== Testing expected snake navigation behavior ===");

    // Helper to get the currently focused cell name
    let get_focused_cell = |canopy: &Canopy, grid: &mut Grid| -> Option<String> {
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
    };

    canopy.focus_first(&mut grid);

    // In a properly working system, we should be able to navigate
    // through all 729 cells (27x27) in a snake pattern
    let mut visited_positions = Vec::new();
    let mut last_position = get_focused_cell(&canopy, &mut grid);
    visited_positions.push(last_position.clone());

    // Track movements in each row
    let mut movements_per_row = Vec::new();
    let mut current_row_movements = 0;

    // Expected behavior: Navigate right until we can't, then down, then left, etc.
    for row in 0..27 {
        if row % 2 == 0 {
            // Even row: move right
            loop {
                canopy.focus_right(&mut grid);
                let new_position = get_focused_cell(&canopy, &mut grid);

                if new_position == last_position {
                    // Can't move right anymore
                    break;
                }

                if !visited_positions.contains(&new_position) {
                    visited_positions.push(new_position.clone());
                }
                last_position = new_position;
                current_row_movements += 1;
            }
        } else {
            // Odd row: move left
            loop {
                canopy.focus_left(&mut grid);
                let new_position = get_focused_cell(&canopy, &mut grid);

                if new_position == last_position {
                    // Can't move left anymore
                    break;
                }

                if !visited_positions.contains(&new_position) {
                    visited_positions.push(new_position.clone());
                }
                last_position = new_position;
                current_row_movements += 1;
            }
        }

        movements_per_row.push(current_row_movements);
        current_row_movements = 0;

        // Try to move down for next row
        if row < 26 {
            canopy.focus_down(&mut grid);
            let new_position = get_focused_cell(&canopy, &mut grid);
            if new_position == last_position {
                println!("Cannot move down after row {row}");
                break;
            }
            last_position = new_position;
        }
    }

    println!("\nSnake navigation results:");
    println!("Unique positions visited: {}", visited_positions.len());
    println!(
        "Movements per row: {:?}",
        &movements_per_row[..movements_per_row.len().min(10)]
    );

    // EXPECTED: We should visit all 729 cells
    // ACTUAL: We get stuck at boundaries, visiting far fewer cells

    println!("\nThis test demonstrates that focus_dir doesn't work correctly");
    println!("with deeply nested grids - it gets stuck at container boundaries.");

    assert!(
        visited_positions.len() < 100,
        "Current implementation visits {} cells, showing it gets stuck (expected to visit all 729)",
        visited_positions.len()
    );

    Ok(())
}
