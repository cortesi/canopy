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
