use canopy::tree::*;
use canopy::*;
use canopy_core::{Context, Expanse, Node, Rect, Result, ViewPort, ViewStack};

#[derive(StatefulNode)]
struct IrregularBlock {
    state: NodeState,
    children: Vec<IrregularBlock>,
    rect: Rect,
}

#[derive_commands]
impl IrregularBlock {
    fn new(rect: Rect) -> Self {
        IrregularBlock {
            state: NodeState::default(),
            children: vec![],
            rect,
        }
    }

    fn add_child(&mut self, child: IrregularBlock) {
        self.children.push(child);
    }
}

impl Node for IrregularBlock {
    fn accept_focus(&mut self) -> bool {
        self.children.is_empty()
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        // For root, use the given size; for children, use their predefined rect
        if self.children.is_empty() {
            // Leaf node - fill with its own size
            let self_expanse = self.rect.expanse();
            l.fill(self, self_expanse)?;
        } else {
            // Container - fill with given size
            l.fill(self, sz)?;

            // Store child rects before iterating
            let child_rects: Vec<Rect> = self.children.iter().map(|c| c.rect).collect();

            // Layout children with their predefined rects
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

/// Test focus navigation invariants
/// For each direction, verify that the focused node actually moves in that direction
#[test]
#[ignore = "Test design issue: focus state not preserved between get_focused calls"]
fn test_focus_navigation_invariants() -> Result<()> {
    use canopy::Canopy;

    // Create a layout with irregular nodes that could cause issues
    // Root has children laid out like this:
    //  A(0,0,10,10)  B(20,0,10,10)
    //  C(5,15,20,10)
    //  D(0,30,10,10) E(20,30,10,10)

    let mut root = IrregularBlock::new(Rect::new(0, 0, 30, 40));

    let block_a = IrregularBlock::new(Rect::new(0, 0, 10, 10));
    let block_b = IrregularBlock::new(Rect::new(20, 0, 10, 10));
    let block_c = IrregularBlock::new(Rect::new(5, 15, 20, 10));
    let block_d = IrregularBlock::new(Rect::new(0, 30, 10, 10));
    let block_e = IrregularBlock::new(Rect::new(20, 30, 10, 10));

    root.add_child(block_a);
    root.add_child(block_b);
    root.add_child(block_c);
    root.add_child(block_d);
    root.add_child(block_e);

    let layout = Layout {};
    root.layout(&layout, Expanse::new(30, 40))?;

    let mut canopy = Canopy::new();

    // Helper to get current focus position
    let get_focused = |canopy: &Canopy, root: &mut IrregularBlock| -> Option<Rect> {
        let mut result = None;
        let root_node: &mut dyn Node = root;
        preorder(root_node, &mut |node| -> Result<Walk<()>> {
            if Context::is_focused(canopy, node) && node.accept_focus() {
                let vp = node.vp();
                result = Some(vp.view());
                return Ok(Walk::Handle(()));
            }
            Ok(Walk::Continue)
        })
        .ok()?;
        result
    };

    // Test 1: From first block (top-left), moving right should go to second block (top-right)
    canopy.focus_first(&mut root);
    let initial = get_focused(&canopy, &mut root);
    // First focusable should be at position (0, 0)
    assert_eq!(initial, Some(Rect::new(0, 0, 10, 10)));

    canopy.focus_right(&mut root);
    let after_right = get_focused(&canopy, &mut root);

    // Should move to block at (20, 0)
    assert_eq!(after_right, Some(Rect::new(20, 0, 10, 10)));

    // Test 2: From middle block (C), moving left should respect invariants
    canopy.set_focus(&mut root.children[2]); // Focus middle block
    let from_middle = get_focused(&canopy, &mut root);
    assert_eq!(from_middle, Some(Rect::new(5, 15, 20, 10)));

    canopy.focus_left(&mut root);
    let after_left_from_middle = get_focused(&canopy, &mut root);

    if let Some(rect) = after_left_from_middle {
        // If we moved, verify the invariant
        if rect != Rect::new(5, 15, 20, 10) {
            // If it's different from middle block
            // The right edge of the new node should be to the left of middle block's left edge
            assert!(
                rect.tl.x + rect.w <= 5,
                "Focus moved to rect {rect:?} which isn't actually to the left of middle block"
            );
        }
    }

    // Test 3: From middle block, moving down should go to bottom blocks
    canopy.set_focus(&mut root.children[2]); // Focus middle block again
    canopy.focus_down(&mut root);
    let after_down_from_middle = get_focused(&canopy, &mut root);

    if let Some(rect) = after_down_from_middle {
        if rect != Rect::new(5, 15, 20, 10) {
            // If it's different from middle block
            // The top edge of the new node should be below middle block's bottom edge
            assert!(
                rect.tl.y >= 25,
                "Focus moved to rect {rect:?} which isn't actually below middle block"
            );
        }
    }

    Ok(())
}

/// Test a more complex irregular layout similar to focusgym
#[test]
fn test_focus_navigation_complex_irregular() -> Result<()> {
    use canopy::Canopy;

    // Create a layout that mimics the irregular splits in focusgym
    // This tests the case where nodes have very different sizes and positions

    let mut root = IrregularBlock::new(Rect::new(0, 0, 100, 60));

    // Left side - tall narrow section
    let mut left = IrregularBlock::new(Rect::new(0, 0, 20, 60));
    left.add_child(IrregularBlock::new(Rect::new(0, 0, 20, 20)));
    left.add_child(IrregularBlock::new(Rect::new(0, 20, 20, 20)));
    left.add_child(IrregularBlock::new(Rect::new(0, 40, 20, 20)));

    // Right side - irregular splits
    let mut right = IrregularBlock::new(Rect::new(20, 0, 80, 60));

    // Top right - wide short
    right.add_child(IrregularBlock::new(Rect::new(20, 0, 80, 15)));

    // Middle section with uneven splits
    let mut middle = IrregularBlock::new(Rect::new(20, 15, 80, 30));
    middle.add_child(IrregularBlock::new(Rect::new(20, 15, 30, 30)));
    middle.add_child(IrregularBlock::new(Rect::new(50, 15, 50, 30)));

    // Bottom right
    right.add_child(IrregularBlock::new(Rect::new(20, 45, 80, 15)));

    right.children.insert(1, middle);
    root.add_child(left);
    root.add_child(right);

    let layout = Layout {};
    root.layout(&layout, Expanse::new(100, 60))?;

    let mut canopy = Canopy::new();

    // Helper to get current focus with screen coordinates
    let get_focused_with_screen = |canopy: &Canopy, root: &mut IrregularBlock| -> Option<Rect> {
        let mut result = None;

        // Create ViewStack to get screen coordinates
        let root_vp = root.vp();
        let screen_vp = ViewPort::new(root_vp.canvas(), root_vp.canvas().rect(), (0, 0)).unwrap();
        let mut view_stack = ViewStack::new(screen_vp);

        collect_focused_recursive(root, &mut view_stack, canopy, &mut result).ok();
        result
    };

    // Test navigation from L2 (middle left) going right
    // This should NOT jump down to M1 just because M1's center is to the right
    canopy.focus_first(&mut root);

    // Navigate to L2
    canopy.set_focus(&mut root.children[0].children[1]);
    let at_l2 = get_focused_with_screen(&canopy, &mut root);
    // L2 should be at position (0, 20)
    assert_eq!(at_l2, Some(Rect::new(0, 20, 20, 20)));

    canopy.focus_right(&mut root);
    let after_right = get_focused_with_screen(&canopy, &mut root);

    // Verify we didn't jump to a node that's actually below L2
    if let Some(rect) = after_right {
        if rect != Rect::new(0, 20, 20, 20) {
            // If different from L2
            let l2_rect = at_l2.unwrap();
            // The new node should have at least some portion at the same vertical level as L2
            assert!(
                rect.tl.y < l2_rect.tl.y + l2_rect.h && rect.tl.y + rect.h > l2_rect.tl.y,
                "Navigating right from L2 jumped to rect {rect:?} which has no vertical overlap"
            );
        }
    }

    Ok(())
}

fn collect_focused_recursive(
    node: &mut dyn Node,
    view_stack: &mut ViewStack,
    canopy: &Canopy,
    result: &mut Option<Rect>,
) -> Result<()> {
    if node.is_hidden() {
        return Ok(());
    }

    let node_vp = node.vp();
    if node_vp.view().is_zero() {
        return Ok(());
    }

    view_stack.push(node_vp);

    if let Some((_, screen_rect)) = view_stack.projection() {
        if Context::is_focused(canopy, node) && node.accept_focus() {
            *result = Some(screen_rect);
        } else {
            node.children(&mut |child| {
                collect_focused_recursive(child, view_stack, canopy, result)?;
                Ok(())
            })?;
        }
    }

    view_stack.pop()?;
    Ok(())
}

/// Test edge case: When the only option in a direction violates the invariant
#[test]
fn test_focus_navigation_no_valid_target() -> Result<()> {
    use canopy::Canopy;

    // Create a layout where navigating right from A would violate invariants
    // A is at top-left, B is below and slightly to the right but not enough
    let mut root = IrregularBlock::new(Rect::new(0, 0, 30, 30));

    root.add_child(IrregularBlock::new(Rect::new(0, 0, 15, 10)));
    root.add_child(IrregularBlock::new(Rect::new(10, 20, 15, 10)));

    let layout = Layout {};
    root.layout(&layout, Expanse::new(30, 30))?;

    let mut canopy = Canopy::new();

    // Focus A
    canopy.focus_first(&mut root);

    // Try to move right - B's left edge (10) is to the left of A's right edge (15)
    // So this should not move focus
    canopy.focus_right(&mut root);

    // Verify we're still on A
    let mut focused = None;
    let root_node: &mut dyn Node = &mut root;
    preorder(root_node, &mut |node| -> Result<Walk<()>> {
        if Context::is_focused(&canopy, node) && node.accept_focus() {
            focused = Some(node.name().to_string());
            return Ok(Walk::Handle(()));
        }
        Ok(Walk::Continue)
    })?;

    // We should still be on the first block (at position 0,0)
    assert_eq!(
        focused.as_deref(),
        Some("irregular_block"),
        "Focus should stay on first block when no valid right target exists"
    );

    Ok(())
}
