use canopy_core::{Context, Direction, Node, NodeId, Rect, Result, ViewPort, ViewStack};

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
        stack: &mut ViewStack,
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

        stack.push(vp);

        if let Some((_, screen_rect)) = stack.projection() {
            if node.accept_focus() {
                if focused.is_none() && ctx.is_focused(node) {
                    *focused = Some((node.id(), screen_rect));
                }
                acc.push(FocusableNode {
                    id: node.id(),
                    rect: screen_rect,
                });
            }

            node.children(&mut |child| {
                recurse(ctx, child, stack, acc, focused)?;
                Ok(())
            })?;
        }

        stack.pop()?;
        Ok(())
    }

    let root_vp = root.vp();
    let screen_vp = ViewPort::new(root_vp.canvas(), root_vp.canvas().rect(), (0, 0))?;
    let mut stack = ViewStack::new(screen_vp);

    recurse(ctx, root, &mut stack, &mut nodes, &mut focused)?;
    Ok((nodes, focused))
}

/// Find the best focus target in the specified direction
pub fn find_focus_target(
    current_rect: Rect,
    direction: Direction,
    candidates: &[FocusableNode],
    current_id: &NodeId,
) -> Option<NodeId> {
    // Centre of the currently focused node
    let cur_cx = current_rect.tl.x as i32 + current_rect.w as i32 / 2;
    let cur_cy = current_rect.tl.y as i32 + current_rect.h as i32 / 2;

    // Filter and sort candidates by how well they match the requested direction.
    let mut candidates: Vec<&FocusableNode> = candidates
        .iter()
        .filter(|n| &n.id != current_id)
        .filter(|n| {
            let cx = n.rect.tl.x as i32 + n.rect.w as i32 / 2;
            let cy = n.rect.tl.y as i32 + n.rect.h as i32 / 2;
            match direction {
                Direction::Right => cx > cur_cx,
                Direction::Left => cx < cur_cx,
                Direction::Down => cy > cur_cy,
                Direction::Up => cy < cur_cy,
            }
        })
        .collect();

    candidates.sort_by_key(|n| {
        let cx = n.rect.tl.x as i32 + n.rect.w as i32 / 2;
        let cy = n.rect.tl.y as i32 + n.rect.h as i32 / 2;
        match direction {
            Direction::Right => ((cy - cur_cy).abs(), cx - cur_cx),
            Direction::Left => ((cy - cur_cy).abs(), cur_cx - cx),
            Direction::Down => ((cx - cur_cx).abs(), cy - cur_cy),
            Direction::Up => ((cx - cur_cx).abs(), cur_cy - cy),
        }
    });

    candidates.first().map(|n| n.id.clone())
}
