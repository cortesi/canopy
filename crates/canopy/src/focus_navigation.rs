use canopy_core::{
    Context, Direction, Node, NodeId, Rect, Result, ViewPort, ViewStack, tree::walk_to_root,
};

/// Information about a focusable node
#[derive(Debug, Clone)]
pub(crate) struct FocusableNode {
    pub(crate) id: NodeId,
    pub(crate) rect: Rect,
}

/// Collect all focusable nodes with their screen rectangles
pub fn collect_focusable_nodes(root: &mut dyn Node) -> Result<Vec<FocusableNode>> {
    let mut nodes = Vec::new();

    // Create initial ViewStack
    let root_vp = root.vp();
    let screen_vp = ViewPort::new(root_vp.canvas(), root_vp.canvas().rect(), (0, 0))?;
    let mut view_stack = ViewStack::new(screen_vp);

    collect_focusable_recursive(root, &mut view_stack, &mut nodes)?;
    Ok(nodes)
}

fn collect_focusable_recursive(
    node: &mut dyn Node,
    view_stack: &mut ViewStack,
    nodes: &mut Vec<FocusableNode>,
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
    if let Some((_, screen_rect)) = view_stack.projection() {
        if node.accept_focus() {
            nodes.push(FocusableNode {
                id: node.id(),
                rect: screen_rect,
            });
        }

        // Process children
        node.children(&mut |child| {
            collect_focusable_recursive(child, view_stack, nodes)?;
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
    // Debug: print current rect
    #[cfg(test)]
    {
        eprintln!("find_focus_target: current_rect = {current_rect:?}, direction = {direction:?}");
        eprintln!("Total candidates: {}", candidates.len());
    }

    // Filter out the current node and nodes that don't make sense for the direction
    let valid_candidates: Vec<&FocusableNode> = candidates
        .iter()
        .filter(|n| &n.id != current_id)
        .filter(|n| {
            let in_dir = is_in_direction(&current_rect, &n.rect, direction);
            #[cfg(test)]
            {
                if !in_dir
                    && matches!(direction, Direction::Right)
                    && n.rect.tl.x > current_rect.tl.x
                {
                    eprintln!("  Candidate at {:?} rejected by is_in_direction", n.rect);
                }
            }
            in_dir
        })
        .collect();

    #[cfg(test)]
    eprintln!(
        "Valid candidates after filtering: {}",
        valid_candidates.len()
    );

    if valid_candidates.is_empty() {
        return None;
    }

    // Find the best candidate based on direction

    valid_candidates
        .into_iter()
        .min_by_key(|n| distance_score(&current_rect, &n.rect, direction))
        .map(|n| n.id.clone())
}

/// Check if target is in the specified direction from source
fn is_in_direction(source: &Rect, target: &Rect, direction: Direction) -> bool {
    // Don't consider the exact same rectangle
    if source == target {
        return false;
    }

    match direction {
        Direction::Right => {
            // Accept nodes that are:
            // 1. Strictly to the right (no overlap)
            // 2. On the same row (overlapping Y) but extending further right
            if target.tl.x >= source.tl.x + source.w {
                // Strictly to the right
                true
            } else {
                // Check if on same row and extends further right
                let same_row = (target.tl.y < source.tl.y + source.h)
                    && (target.tl.y + target.h > source.tl.y);
                let extends_right = target.tl.x + target.w > source.tl.x + source.w;
                same_row && extends_right
            }
        }
        Direction::Left => {
            // Accept nodes that are:
            // 1. Strictly to the left (no overlap)
            // 2. On the same row (overlapping Y) but extending further left
            if target.tl.x + target.w <= source.tl.x {
                // Strictly to the left
                true
            } else {
                // Check if on same row and extends further left
                let same_row = (target.tl.y < source.tl.y + source.h)
                    && (target.tl.y + target.h > source.tl.y);
                let extends_left = target.tl.x < source.tl.x;
                same_row && extends_left
            }
        }
        Direction::Down => {
            // Accept nodes that are:
            // 1. Strictly below (no overlap)
            // 2. On the same column (overlapping X) but extending further down
            if target.tl.y >= source.tl.y + source.h {
                // Strictly below
                true
            } else {
                // Check if in same column and extends further down
                let same_col = (target.tl.x < source.tl.x + source.w)
                    && (target.tl.x + target.w > source.tl.x);
                let extends_down = target.tl.y + target.h > source.tl.y + source.h;
                same_col && extends_down
            }
        }
        Direction::Up => {
            // Accept nodes that are:
            // 1. Strictly above (no overlap)
            // 2. On the same column (overlapping X) but extending further up
            if target.tl.y + target.h <= source.tl.y {
                // Strictly above
                true
            } else {
                // Check if in same column and extends further up
                let same_col = (target.tl.x < source.tl.x + source.w)
                    && (target.tl.x + target.w > source.tl.x);
                let extends_up = target.tl.y < source.tl.y;
                same_col && extends_up
            }
        }
    }
}

/// Calculate a distance score for ranking candidates
/// Lower scores are better
fn distance_score(source: &Rect, target: &Rect, direction: Direction) -> u32 {
    match direction {
        Direction::Right => {
            // For rightward movement in a grid:
            // 1. Prefer nodes on the same row (small y difference)
            // 2. Among those, prefer the closest one to the right
            let y_diff = source.tl.y.abs_diff(target.tl.y) as u32;
            let x_dist = target.tl.x.saturating_sub(source.tl.x) as u32;

            // If on same row (y_diff == 0), prioritize by x distance
            // Otherwise, prioritize by row difference, then x distance
            if y_diff == 0 {
                x_dist
            } else {
                // Not on same row - add a large penalty
                100000 + y_diff * 1000 + x_dist
            }
        }
        Direction::Left => {
            // For leftward movement in a grid:
            // 1. Prefer nodes on the same row (small y difference)
            // 2. Among those, prefer the closest one to the left
            let y_diff = source.tl.y.abs_diff(target.tl.y) as u32;
            let x_dist = source.tl.x.saturating_sub(target.tl.x) as u32;

            // If on same row (y_diff == 0), prioritize by x distance
            // Otherwise, prioritize by row difference, then x distance
            if y_diff == 0 {
                x_dist
            } else {
                // Not on same row - add a large penalty
                100000 + y_diff * 1000 + x_dist
            }
        }
        Direction::Down => {
            // For downward movement in a grid:
            // 1. Prefer nodes in the same column (small x difference)
            // 2. Among those, prefer the closest one below
            let x_diff = source.tl.x.abs_diff(target.tl.x) as u32;
            let y_dist = target.tl.y.saturating_sub(source.tl.y) as u32;

            // If in same column (x_diff == 0), prioritize by y distance
            // Otherwise, prioritize by column difference, then y distance
            if x_diff == 0 {
                y_dist
            } else {
                // Not in same column - add a penalty, but less than for horizontal movement
                // since we often want to find the closest node below even if not perfectly aligned
                10000 + x_diff * 100 + y_dist
            }
        }
        Direction::Up => {
            // For upward movement in a grid:
            // 1. Prefer nodes in the same column (small x difference)
            // 2. Among those, prefer the closest one above
            let x_diff = source.tl.x.abs_diff(target.tl.x) as u32;
            let y_dist = source.tl.y.saturating_sub(target.tl.y) as u32;

            // If in same column (x_diff == 0), prioritize by y distance
            // Otherwise, prioritize by column difference, then y distance
            if x_diff == 0 {
                y_dist
            } else {
                // Not in same column - add a penalty
                10000 + x_diff * 100 + y_dist
            }
        }
    }
}

/// Find the currently focused node and its screen rectangle
pub fn find_focused_node(
    ctx: &dyn Context,
    root: &mut dyn Node,
    focusable_nodes: &[FocusableNode],
) -> Option<(NodeId, Rect)> {
    for node in focusable_nodes {
        let mut is_focused = false;
        walk_to_root(root, &node.id, &mut |n| {
            if ctx.is_focused(n) {
                is_focused = true;
            }
            Ok(())
        })
        .ok()?;

        if is_focused {
            return Some((node.id.clone(), node.rect));
        }
    }
    None
}
