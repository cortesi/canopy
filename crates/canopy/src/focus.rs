use canopy_core::{
    Context, Direction, Node, NodeId, Point, Rect, Result, ViewPort, ViewStack,
    tree::{Walk, preorder},
};

/// Information about a focusable node
#[derive(Debug, Clone)]
pub struct FocusableNode {
    pub id: NodeId,
    pub rect: Rect,
}

/// Collect all focusable nodes with their screen rectangles
pub fn collect_focusable_nodes(root: &mut dyn Node) -> Result<Vec<FocusableNode>> {
    let mut nodes = Vec::new();

    // Create initial ViewStack - same as render_traversal
    let root_vp = root.vp();
    let screen_vp = ViewPort::new(root_vp.canvas(), root_vp.canvas().rect(), (0, 0))?;
    let mut view_stack = ViewStack::new(screen_vp);

    collect_focusable_recursive(root, &mut view_stack, &mut nodes, Point::zero())?;

    Ok(nodes)
}

fn collect_focusable_recursive(
    node: &mut dyn Node,
    view_stack: &mut ViewStack,
    nodes: &mut Vec<FocusableNode>,
    parent_screen_pos: Point,
) -> Result<()> {
    if node.is_hidden() {
        return Ok(());
    }

    let node_vp = node.vp();
    if node_vp.view().is_zero() {
        return Ok(());
    }

    // Convert node position to parent-relative coordinates
    // If the position is already parent-relative (from place_()), it will be small
    // If the position is absolute (from place()), we need to subtract parent position
    let relative_pos = if node_vp.position().x < parent_screen_pos.x
        || node_vp.position().y < parent_screen_pos.y
    {
        // Position is smaller than parent position, so it's already parent-relative
        node_vp.position()
    } else {
        // Position might be absolute, convert to parent-relative
        Point {
            x: node_vp.position().x.saturating_sub(parent_screen_pos.x),
            y: node_vp.position().y.saturating_sub(parent_screen_pos.y),
        }
    };

    // Create a new viewport with parent-relative position
    let relative_vp = ViewPort::new(node_vp.canvas(), node_vp.view(), relative_pos)?;

    // Push the viewport onto the stack
    view_stack.push(relative_vp);

    // Get screen rect from projection
    if let Some((_, screen_rect)) = view_stack.projection() {
        if node.accept_focus() {
            nodes.push(FocusableNode {
                id: node.id(),
                rect: screen_rect,
            });
        }

        // Process children
        // The screen_rect gives us the absolute screen position of this node
        // We should use this as the parent position for children
        let node_screen_pos = screen_rect.tl;
        node.children(&mut |child| {
            collect_focusable_recursive(child, view_stack, nodes, node_screen_pos)?;
            Ok(())
        })?;
    }

    // Pop viewport
    view_stack.pop()?;

    Ok(())
}

/// Find the best focus target in the specified direction
pub fn find_focus_target(
    current_rect: Rect,
    direction: Direction,
    candidates: &[FocusableNode],
    current_id: &NodeId,
) -> Option<NodeId> {
    // Find the center point of the current rectangle
    let current_center = Point {
        x: current_rect.tl.x + current_rect.w / 2,
        y: current_rect.tl.y + current_rect.h / 2,
    };

    // Filter candidates based on direction
    // A candidate is valid if:
    // 1. Its center is in the correct direction from current's center
    // 2. It has overlap in the perpendicular axis (to prevent diagonal movement)
    let mut valid_candidates: Vec<&FocusableNode> = candidates
        .iter()
        .filter(|n| &n.id != current_id)
        .filter(|n| {
            let candidate_center = Point {
                x: n.rect.tl.x + n.rect.w / 2,
                y: n.rect.tl.y + n.rect.h / 2,
            };

            match direction {
                Direction::Right | Direction::Left => {
                    // For horizontal movement, check:
                    // 1. Center is in the right direction
                    let center_ok = match direction {
                        Direction::Right => candidate_center.x > current_center.x,
                        Direction::Left => candidate_center.x < current_center.x,
                        _ => unreachable!(),
                    };

                    // 2. There's vertical overlap between the rectangles
                    let vertical_overlap = n.rect.tl.y < current_rect.tl.y + current_rect.h
                        && n.rect.tl.y + n.rect.h > current_rect.tl.y;

                    center_ok && vertical_overlap
                }
                Direction::Down | Direction::Up => {
                    // For vertical movement, check:
                    // 1. Center is in the right direction
                    let center_ok = match direction {
                        Direction::Down => candidate_center.y > current_center.y,
                        Direction::Up => candidate_center.y < current_center.y,
                        _ => unreachable!(),
                    };

                    // 2. There's horizontal overlap between the rectangles
                    let horizontal_overlap = n.rect.tl.x < current_rect.tl.x + current_rect.w
                        && n.rect.tl.x + n.rect.w > current_rect.tl.x;

                    center_ok && horizontal_overlap
                }
            }
        })
        .collect();

    // Remove current node if it somehow got through
    valid_candidates.retain(|n| &n.id != current_id);

    if valid_candidates.is_empty() {
        return None;
    }

    // Sort candidates by a score that considers:
    // 1. Edge distance in the primary direction
    // 2. Overlap amount in the secondary direction
    // 3. Center-to-center distance as a tiebreaker
    valid_candidates.sort_by_key(|n| {
        match direction {
            Direction::Right => {
                // Primary: distance from current's right edge to candidate's left edge
                let edge_dist = n
                    .rect
                    .tl
                    .x
                    .saturating_sub(current_rect.tl.x + current_rect.w);

                // Secondary: vertical alignment (less distance = better alignment)
                let vert_center_dist = current_center.y.abs_diff(n.rect.tl.y + n.rect.h / 2);

                // Score: prioritize edge distance, then vertical alignment
                (edge_dist as u64) * 10000 + (vert_center_dist as u64)
            }
            Direction::Left => {
                // Primary: distance from candidate's right edge to current's left edge
                let edge_dist = current_rect.tl.x.saturating_sub(n.rect.tl.x + n.rect.w);

                // Secondary: vertical alignment
                let vert_center_dist = current_center.y.abs_diff(n.rect.tl.y + n.rect.h / 2);

                (edge_dist as u64) * 10000 + (vert_center_dist as u64)
            }
            Direction::Down => {
                // Primary: distance from current's bottom edge to candidate's top edge
                let edge_dist = n
                    .rect
                    .tl
                    .y
                    .saturating_sub(current_rect.tl.y + current_rect.h);

                // Secondary: horizontal alignment
                let horiz_center_dist = current_center.x.abs_diff(n.rect.tl.x + n.rect.w / 2);

                (edge_dist as u64) * 10000 + (horiz_center_dist as u64)
            }
            Direction::Up => {
                // Primary: distance from candidate's bottom edge to current's top edge
                let edge_dist = current_rect.tl.y.saturating_sub(n.rect.tl.y + n.rect.h);

                // Secondary: horizontal alignment
                let horiz_center_dist = current_center.x.abs_diff(n.rect.tl.x + n.rect.w / 2);

                (edge_dist as u64) * 10000 + (horiz_center_dist as u64)
            }
        }
    });

    valid_candidates.first().map(|n| n.id.clone())
}

/// Find the currently focused node and its screen rectangle
pub fn find_focused_node(
    ctx: &dyn Context,
    root: &mut dyn Node,
    focusable_nodes: &[FocusableNode],
) -> Option<(NodeId, Rect)> {
    // Use preorder traversal to find the focused node
    let mut focused_id = None;
    preorder(root, &mut |node| -> Result<Walk<()>> {
        if ctx.is_focused(node) {
            focused_id = Some(node.id());
            return Ok(Walk::Handle(()));
        }
        Ok(Walk::Continue)
    })
    .ok()?;

    // Find the corresponding focusable node info
    if let Some(id) = focused_id {
        for node in focusable_nodes {
            if node.id == id {
                return Some((node.id.clone(), node.rect));
            }
        }
    }

    None
}
