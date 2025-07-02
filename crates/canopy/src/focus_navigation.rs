use canopy_core::{Context, Direction, Node, NodeId, Rect, Result};

/// Information about a focusable node
#[derive(Debug, Clone)]
pub(crate) struct FocusableNode {
    pub(crate) id: NodeId,
    pub(crate) rect: Rect,
}

/// Collect all focusable nodes with their screen rectangles
pub fn collect_focusable_nodes(
    ctx: &dyn Context,
    root: &mut dyn Node,
) -> Result<(Vec<FocusableNode>, Option<(NodeId, Rect)>)> {
    let mut nodes = Vec::new();
    let mut focused = None;

    fn recurse(
        ctx: &dyn Context,
        node: &mut dyn Node,
        offset: (i16, i16),
        acc: &mut Vec<FocusableNode>,
        focused: &mut Option<(NodeId, Rect)>,
    ) -> Result<()> {
        if node.is_hidden() {
            return Ok(());
        }

        let vp = node.vp();
        if vp.view().is_zero() {
            return Ok(());
        }

        let pos = (
            offset.0 + vp.position().x as i16,
            offset.1 + vp.position().y as i16,
        );
        let screen = vp.view().shift(pos.0, pos.1);

        if node.accept_focus() {
            if focused.is_none() && ctx.is_focused(node) {
                *focused = Some((node.id(), screen));
            }
            acc.push(FocusableNode {
                id: node.id(),
                rect: screen,
            });
        }

        node.children(&mut |child| {
            recurse(ctx, child, pos, acc, focused)?;
            Ok(())
        })?;

        Ok(())
    }

    recurse(ctx, root, (0, 0), &mut nodes, &mut focused)?;
    Ok((nodes, focused))
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
