use canopy::tree::*;
use canopy::*;
use canopy_core::{Context, Expanse, Node, Rect, Result, ViewPort, ViewStack, tutils::Grid};

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

// IrregularBlock for testing irregular layouts
#[derive(StatefulNode)]
struct IrregularTestBlock {
    state: NodeState,
    children: Vec<IrregularTestBlock>,
    rect: Rect,
    name_str: String,
}

#[derive_commands]
impl IrregularTestBlock {
    fn new(name: &str, rect: Rect) -> Self {
        IrregularTestBlock {
            state: NodeState::default(),
            children: vec![],
            rect,
            name_str: name.to_string(),
        }
    }

    fn add_child(&mut self, child: IrregularTestBlock) {
        self.children.push(child);
    }
}

impl Node for IrregularTestBlock {
    fn accept_focus(&mut self) -> bool {
        self.children.is_empty()
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        if self.children.is_empty() {
            let self_expanse = self.rect.expanse();
            l.fill(self, self_expanse)?;
        } else {
            l.fill(self, sz)?;
            let child_rects: Vec<Rect> = self.children.iter().map(|c| c.rect).collect();
            for (child, rect) in self.children.iter_mut().zip(child_rects.iter()) {
                l.place_(child, *rect)?;
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

/// Creates an irregular layout that reproduces the focus skipping issue
/// This simulates the focusgym-like scenario where nodes can be skipped
#[test]
fn test_focus_skip_issue_reproduction() -> Result<()> {
    use canopy::Canopy;

    // Create a complex irregular layout similar to focusgym
    // The key issue: when we have a tall node on the left that spans multiple smaller nodes on the right,
    // focus navigation might skip over it when moving left from a middle-right node

    // Layout visualization:
    // |------------|--------------------------|
    // |            |  top_right (20,0,80,12)  |
    // |  left_tall |--------------------------|
    // | (0,0,30,60)|  mid_left  |  mid_right  |
    // |            | (30,12,35,36)(65,12,35,36)|
    // |            |--------------------------|
    // |            |  bottom (20,48,80,12)    |
    // |------------|--------------------------|

    let mut root = IrregularTestBlock::new("root", Rect::new(0, 0, 100, 60));

    // Left side: single tall cell that spans the entire height
    let left_tall = IrregularTestBlock::new("left_tall", Rect::new(0, 0, 30, 60));

    // Right side container
    let mut right_container = IrregularTestBlock::new("right_container", Rect::new(30, 0, 70, 60));

    // Right side children
    let top_right = IrregularTestBlock::new("top_right", Rect::new(30, 0, 70, 12));
    let mid_left = IrregularTestBlock::new("mid_left", Rect::new(30, 12, 35, 36));
    let mid_right = IrregularTestBlock::new("mid_right", Rect::new(65, 12, 35, 36));
    let bottom = IrregularTestBlock::new("bottom", Rect::new(30, 48, 70, 12));

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

    // Helper to get focused node name
    fn get_focused_name_recursive(
        node: &mut IrregularTestBlock,
        canopy: &Canopy,
    ) -> Option<String> {
        if Context::is_focused(canopy, node) && node.accept_focus() {
            Some(node.name_str.clone())
        } else {
            for child in &mut node.children {
                if let Some(name) = get_focused_name_recursive(child, canopy) {
                    return Some(name);
                }
            }
            None
        }
    }

    // Test case: Focus on mid_left and try to move left
    // This should go to left_tall, not skip it
    canopy.focus_first(&mut root);

    // Navigate to mid_left - we need to find it in the tree structure
    fn set_focus_on_named(
        node: &mut IrregularTestBlock,
        canopy: &mut Canopy,
        target_name: &str,
    ) -> bool {
        if node.name_str == target_name && node.accept_focus() {
            canopy.set_focus(node);
            return true;
        }
        for child in &mut node.children {
            if set_focus_on_named(child, canopy, target_name) {
                return true;
            }
        }
        false
    }

    set_focus_on_named(&mut root, &mut canopy, "mid_left");

    let before_move = get_focused_name_recursive(&mut root, &canopy);
    assert_eq!(before_move.as_deref(), Some("mid_left"));
    println!("Focused on: {before_move:?}");

    canopy.focus_left(&mut root);
    let after_left = get_focused_name_recursive(&mut root, &canopy);
    println!("After focus_left: {after_left:?}");

    // The bug: focus might skip left_tall and jump to something else or stay on mid_left
    // Expected: Should move to left_tall since it completely covers the left vantage
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

    // Create a layout with extreme size differences
    // This tests whether the algorithm correctly handles huge disparities in node sizes

    let mut root = IrregularTestBlock::new("root", Rect::new(0, 0, 100, 50));

    // Tiny node on the left (5% width)
    let tiny = IrregularTestBlock::new("tiny", Rect::new(0, 0, 5, 50));

    // Huge node in the middle (85% width)
    let huge = IrregularTestBlock::new("huge", Rect::new(5, 0, 85, 50));

    // Medium node on the right (10% width)
    let medium = IrregularTestBlock::new("medium", Rect::new(90, 0, 10, 50));

    root.add_child(tiny);
    root.add_child(huge);
    root.add_child(medium);

    let layout = Layout {};
    root.layout(&layout, Expanse::new(100, 50))?;

    let mut canopy = Canopy::new();

    // Test navigation doesn't skip the huge middle node
    canopy.focus_first(&mut root);

    canopy.focus_right(&mut root);

    // Use our helper function
    fn get_focused_name(node: &mut IrregularTestBlock, canopy: &Canopy) -> Option<String> {
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

/// Test the exact focus skipping scenario with more complex overlapping nodes
#[test]
fn test_focus_skip_complex_overlap() -> Result<()> {
    use canopy::Canopy;

    // Create a layout where multiple nodes overlap in complex ways
    // This tests the edge case where the center-based algorithm might fail

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

    let mut root = IrregularTestBlock::new("root", Rect::new(0, 0, 100, 50));

    // Top row - three nodes with B being shorter
    let node_a = IrregularTestBlock::new("node_a", Rect::new(0, 0, 25, 20));
    let node_b = IrregularTestBlock::new("node_b", Rect::new(25, 0, 25, 10)); // Shorter!
    let node_c = IrregularTestBlock::new("node_c", Rect::new(50, 0, 50, 20));

    // Bottom row - D spans A and B width, E aligns with C
    let node_d = IrregularTestBlock::new("node_d", Rect::new(0, 20, 50, 30));
    let node_e = IrregularTestBlock::new("node_e", Rect::new(50, 20, 50, 30));

    root.add_child(node_a);
    root.add_child(node_b);
    root.add_child(node_c);
    root.add_child(node_d);
    root.add_child(node_e);

    let layout = Layout {};
    root.layout(&layout, Expanse::new(100, 50))?;

    let mut canopy = Canopy::new();

    // Helper to get focused node name
    fn get_focused_name(node: &mut IrregularTestBlock, canopy: &Canopy) -> Option<String> {
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

    // Helper to set focus on a specific node
    fn set_focus_on(node: &mut IrregularTestBlock, canopy: &mut Canopy, target: &str) -> bool {
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

    // Test case 1: From node_c, moving left should go to node_b (not skip to node_a)
    set_focus_on(&mut root, &mut canopy, "node_c");
    assert_eq!(
        get_focused_name(&mut root, &canopy).as_deref(),
        Some("node_c")
    );

    canopy.focus_left(&mut root);
    let after_left_from_c = get_focused_name(&mut root, &canopy);
    println!("From C, focus_left goes to: {after_left_from_c:?}");

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
    println!("From E, focus_left goes to: {after_left_from_e:?}");

    assert_eq!(
        after_left_from_e.as_deref(),
        Some("node_d"),
        "Should focus node_d which covers the left side"
    );

    // Test case 3: From node_d moving up - should it go to A or B?
    set_focus_on(&mut root, &mut canopy, "node_d");
    canopy.focus_up(&mut root);
    let after_up_from_d = get_focused_name(&mut root, &canopy);
    println!("From D, focus_up goes to: {after_up_from_d:?}");

    // The algorithm should pick the node with the smallest vertical distance
    // Both A and B are above D, but the algorithm might behave unexpectedly

    Ok(())
}

/// Test focus navigation with deeply nested irregular structures
#[test]
fn test_focus_navigation_deep_nesting() -> Result<()> {
    use canopy::Canopy;

    // Create a deeply nested structure where nodes at different depths can be focused
    // This tests whether the algorithm correctly handles nodes at varying tree depths

    let mut root = IrregularTestBlock::new("root", Rect::new(0, 0, 100, 100));

    // Level 1: Two containers side by side
    let mut left_container = IrregularTestBlock::new("left_container", Rect::new(0, 0, 40, 100));
    let mut right_container = IrregularTestBlock::new("right_container", Rect::new(40, 0, 60, 100));

    // Left container has a single deep cell
    let left_leaf = IrregularTestBlock::new("left_leaf", Rect::new(0, 0, 40, 100));
    left_container.add_child(left_leaf);

    // Right container has nested structure
    let mut right_top = IrregularTestBlock::new("right_top_container", Rect::new(40, 0, 60, 50));
    let right_top_leaf = IrregularTestBlock::new("right_top_leaf", Rect::new(40, 0, 60, 50));
    right_top.add_child(right_top_leaf);

    let mut right_bottom =
        IrregularTestBlock::new("right_bottom_container", Rect::new(40, 50, 60, 50));

    // Right bottom has even more nesting
    let mut rb_left = IrregularTestBlock::new("rb_left_container", Rect::new(40, 50, 30, 50));
    let rb_left_leaf = IrregularTestBlock::new("rb_left_leaf", Rect::new(40, 50, 30, 50));
    rb_left.add_child(rb_left_leaf);

    let rb_right_leaf = IrregularTestBlock::new("rb_right_leaf", Rect::new(70, 50, 30, 50));

    right_bottom.add_child(rb_left);
    right_bottom.add_child(rb_right_leaf);

    right_container.add_child(right_top);
    right_container.add_child(right_bottom);

    root.add_child(left_container);
    root.add_child(right_container);

    let layout = Layout {};
    root.layout(&layout, Expanse::new(100, 100))?;

    let mut canopy = Canopy::new();

    // Helper functions
    fn get_focused_name(node: &mut IrregularTestBlock, canopy: &Canopy) -> Option<String> {
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

    fn set_focus_on(node: &mut IrregularTestBlock, canopy: &mut Canopy, target: &str) -> bool {
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

    // Test navigation across different nesting levels
    // From rb_left_leaf (deeply nested) going left should find left_leaf
    set_focus_on(&mut root, &mut canopy, "rb_left_leaf");
    assert_eq!(
        get_focused_name(&mut root, &canopy).as_deref(),
        Some("rb_left_leaf")
    );

    canopy.focus_left(&mut root);
    let after_left = get_focused_name(&mut root, &canopy);
    println!("From rb_left_leaf, focus_left goes to: {after_left:?}");

    assert_eq!(
        after_left.as_deref(),
        Some("left_leaf"),
        "Should navigate from deeply nested node to left_leaf"
    );

    Ok(())
}

/// Test that focus navigation doesn't allow diagonal movement
/// When pressing down, focus should only move to nodes that are actually below,
/// not to nodes that are to the side
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

    let mut root = IrregularTestBlock::new("root", Rect::new(0, 0, 100, 60));

    // Node A: tall on the left, extends below B
    let node_a = IrregularTestBlock::new("node_a", Rect::new(0, 0, 50, 60));

    // Node B: shorter on the right, starts lower
    let node_b = IrregularTestBlock::new("node_b", Rect::new(50, 20, 50, 40));

    root.add_child(node_a);
    root.add_child(node_b);

    let layout = Layout {};
    root.layout(&layout, Expanse::new(100, 60))?;

    let mut canopy = Canopy::new();

    // Helper functions
    fn get_focused_name(node: &mut IrregularTestBlock, canopy: &Canopy) -> Option<String> {
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

    fn set_focus_on(node: &mut IrregularTestBlock, canopy: &mut Canopy, target: &str) -> bool {
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

    // Test case 1: From B, pressing down should do nothing (not move to A)
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

    // Test case 2: From A, pressing up should do nothing (when at top)
    set_focus_on(&mut root, &mut canopy, "node_a");
    canopy.focus_up(&mut root);
    let after_up = get_focused_name(&mut root, &canopy);

    assert_eq!(
        after_up.as_deref(),
        Some("node_a"),
        "Focus should stay on node_a when pressing up at the top"
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

    let mut root = IrregularTestBlock::new("root", Rect::new(0, 0, 100, 60));

    let node_a = IrregularTestBlock::new("node_a", Rect::new(0, 0, 30, 30));
    let node_b = IrregularTestBlock::new("node_b", Rect::new(40, 10, 20, 20));
    let node_c = IrregularTestBlock::new("node_c", Rect::new(70, 0, 30, 40));
    let node_d = IrregularTestBlock::new("node_d", Rect::new(0, 35, 40, 25));

    root.add_child(node_a);
    root.add_child(node_b);
    root.add_child(node_c);
    root.add_child(node_d);

    let layout = Layout {};
    root.layout(&layout, Expanse::new(100, 60))?;

    let mut canopy = Canopy::new();

    // Helper functions
    fn get_focused_name(node: &mut IrregularTestBlock, canopy: &Canopy) -> Option<String> {
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

    fn set_focus_on(node: &mut IrregularTestBlock, canopy: &mut Canopy, target: &str) -> bool {
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

    // Test 1: From B pressing down - should NOT reach D
    set_focus_on(&mut root, &mut canopy, "node_b");
    canopy.focus_down(&mut root);
    let from_b_down = get_focused_name(&mut root, &canopy);
    println!("From B, pressing down: {from_b_down:?}");

    assert_eq!(
        from_b_down.as_deref(),
        Some("node_b"),
        "From B, pressing down should not move to D (D is not directly below B)"
    );

    // Test 2: From C pressing down - should NOT reach D (no horizontal overlap)
    set_focus_on(&mut root, &mut canopy, "node_c");
    canopy.focus_down(&mut root);
    let from_c_down = get_focused_name(&mut root, &canopy);
    println!("From C, pressing down: {from_c_down:?}");

    assert_eq!(
        from_c_down.as_deref(),
        Some("node_c"),
        "From C, pressing down should not move to D (no horizontal overlap)"
    );

    // Test 3: From D pressing up - should go to A (leftmost valid option)
    set_focus_on(&mut root, &mut canopy, "node_d");
    canopy.focus_up(&mut root);
    let from_d_up = get_focused_name(&mut root, &canopy);
    println!("From D, pressing up: {from_d_up:?}");

    // Should go to A since it's directly above part of D
    assert_eq!(
        from_d_up.as_deref(),
        Some("node_a"),
        "From D, pressing up should move to A"
    );

    Ok(())
}
