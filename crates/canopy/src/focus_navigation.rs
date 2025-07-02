use canopy_core::{
    Context, Direction, Node, NodeId, Point, Rect, Result, ViewPort, ViewStack, tree::walk_to_root,
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
    // Filter out the current node and nodes that don't make sense for the direction
    let valid_candidates: Vec<&FocusableNode> = candidates
        .iter()
        .filter(|n| &n.id != current_id)
        .filter(|n| is_in_direction(&current_rect, &n.rect, direction))
        .collect();

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
    // Get centers for better comparison
    let source_center = Point {
        x: source.tl.x + source.w / 2,
        y: source.tl.y + source.h / 2,
    };
    let target_center = Point {
        x: target.tl.x + target.w / 2,
        y: target.tl.y + target.h / 2,
    };

    match direction {
        Direction::Right => {
            // Target should be to the right of source
            target_center.x > source_center.x
        }
        Direction::Left => {
            // Target should be to the left of source
            target_center.x < source_center.x
        }
        Direction::Down => {
            // Target should be below source
            target_center.y > source_center.y
        }
        Direction::Up => {
            // Target should be above source
            target_center.y < source_center.y
        }
    }
}

/// Calculate a distance score for ranking candidates
/// Lower scores are better
fn distance_score(source: &Rect, target: &Rect, direction: Direction) -> u32 {
    let source_center = Point {
        x: source.tl.x + source.w / 2,
        y: source.tl.y + source.h / 2,
    };

    let target_center = Point {
        x: target.tl.x + target.w / 2,
        y: target.tl.y + target.h / 2,
    };

    match direction {
        Direction::Right | Direction::Left => {
            // Primary: horizontal distance
            let h_dist = source_center.x.abs_diff(target_center.x) as u32;

            // Secondary: vertical alignment (prefer aligned items)
            let v_dist = source_center.y.abs_diff(target_center.y) as u32;

            // Weight horizontal distance more heavily
            h_dist * 1000 + v_dist
        }
        Direction::Up | Direction::Down => {
            // Primary: vertical distance
            let v_dist = source_center.y.abs_diff(target_center.y) as u32;

            // Secondary: horizontal alignment (prefer aligned items)
            let h_dist = source_center.x.abs_diff(target_center.x) as u32;

            // Weight vertical distance more heavily
            v_dist * 1000 + h_dist
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
