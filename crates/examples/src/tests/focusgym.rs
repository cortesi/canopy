use canopy::{
    TermBuf,
    error::{Error, Result},
    geom::RectI32,
    prelude::*,
    testing::harness::Harness,
};

use super::{Mount, root_harness};
use crate::focusgym::{Block, FocusGym, binding_setup};

fn setup_harness(size: Size) -> Result<Harness> {
    root_harness(FocusGym::new(), binding_setup, size, Mount::Replace)
}

fn with_root_block<R>(
    harness: &mut Harness,
    f: impl FnOnce(&mut dyn Context, NodeId) -> Result<R>,
) -> Result<R> {
    harness.with_root_context(|_root: &mut FocusGym, ctx| {
        let root_block = (ctx as &dyn ViewContext)
            .unique_child::<Block>()?
            .ok_or_else(|| Error::NotFound("root block".into()))?;
        f(ctx, NodeId::from(root_block))
    })
}

fn root_children(harness: &mut Harness) -> Result<Vec<NodeId>> {
    with_root_block(harness, |ctx, root| Ok(ctx.children_of(root)))
}

fn root_children_pair(harness: &mut Harness) -> Result<(NodeId, NodeId)> {
    let children = root_children(harness)?;
    let left = children
        .first()
        .copied()
        .ok_or_else(|| Error::NotFound("left child".into()))?;
    let right = children
        .get(1)
        .copied()
        .ok_or_else(|| Error::NotFound("right child".into()))?;
    Ok((left, right))
}

/// Return the flex weight of the root block's left child.
fn left_flex_weight(harness: &mut Harness) -> Result<u32> {
    let (left, _) = root_children_pair(harness)?;
    with_root_block(harness, move |ctx, _root| {
        Ok(match layout_of(ctx, left)?.width {
            Sizing::Flex(weight) => weight,
            _ => 1,
        })
    })
}

/// Return the outer view rects of the root block's two children.
fn outer_pair(harness: &mut Harness) -> Result<(RectI32, RectI32)> {
    let (left, right) = root_children_pair(harness)?;
    with_root_block(harness, move |ctx, _root| {
        Ok((
            outer_of(ctx, left, "left node")?,
            outer_of(ctx, right, "right node")?,
        ))
    })
}

fn outer_of(ctx: &dyn Context, node: NodeId, label: &str) -> Result<RectI32> {
    ctx.view_of(node)
        .map(|view| view.outer)
        .ok_or_else(|| Error::NotFound(label.to_string()))
}

fn layout_of(ctx: &dyn Context, node: NodeId) -> Result<Layout> {
    (ctx as &dyn ViewContext)
        .layout_of(node)
        .ok_or_else(|| Error::NotFound("layout".into()))
}

/// Find the blank column that separates two side-by-side blocks, if one exists.
fn find_separator_column(buf: &TermBuf, left_view: RectI32, right_view: RectI32) -> Option<u32> {
    let start_x = left_view.tl.x.max(0) as u32;
    let end_x = right_view.tl.x.max(0) as u32;
    let mut found = None;
    for x in start_x..=end_x {
        let mut all_space = true;
        let mut has_neighbors = false;
        for y in 0..buf.size().h {
            let cell = buf.get(Point { x, y }).unwrap();
            if cell.ch != ' ' {
                all_space = false;
                break;
            }
            let left_ok = x > 0
                && buf
                    .get(Point { x: x - 1, y })
                    .is_some_and(|c| c.ch == '\u{2588}');
            let right_ok = x + 1 < buf.size().w
                && buf
                    .get(Point { x: x + 1, y })
                    .is_some_and(|c| c.ch == '\u{2588}');
            if left_ok && right_ok {
                has_neighbors = true;
            }
        }
        if all_space && has_neighbors {
            found = Some(x);
            break;
        }
    }
    found
}

#[test]
fn test_horizontal_children_fill_height() -> Result<()> {
    let mut harness = setup_harness(Size::new(60, 14))?;
    let (parent, children) = with_root_block(&mut harness, |ctx, root| {
        let parent = outer_of(ctx, root, "root block")?;
        let mut child_views = Vec::new();
        for child in ctx.children_of(root) {
            child_views.push(outer_of(ctx, child, "child node")?);
        }
        Ok((parent, child_views))
    })?;

    assert_eq!(children.len(), 2);
    for view in children {
        assert_eq!(view.h, parent.h);
        assert_eq!(view.tl.y, parent.tl.y);
    }
    Ok(())
}

#[test]
fn test_vertical_children_fill_width_and_height() -> Result<()> {
    let mut harness = setup_harness(Size::new(60, 14))?;

    harness.key('s')?;

    let (parent, children) = with_root_block(&mut harness, |ctx, root| {
        let left = ctx
            .children_of(root)
            .first()
            .copied()
            .ok_or_else(|| Error::NotFound("left child".into()))?;
        let parent = outer_of(ctx, left, "left node")?;
        let mut child_views = Vec::new();
        for child in ctx.children_of(left) {
            child_views.push(outer_of(ctx, child, "child node")?);
        }
        Ok((parent, child_views))
    })?;

    assert_eq!(children.len(), 2);
    let mut max_bottom = parent.tl.y;
    for view in children {
        assert_eq!(view.w, parent.w);
        max_bottom = max_bottom.max(view.tl.y + view.h as i32);
    }
    assert_eq!(max_bottom, parent.tl.y + parent.h as i32);
    Ok(())
}

#[test]
fn test_flex_grow_commands_update_layout() -> Result<()> {
    let mut harness = setup_harness(Size::new(60, 14))?;
    let weight_before = left_flex_weight(&mut harness)?;

    harness.key(']')?;

    let weight_after = left_flex_weight(&mut harness)?;

    assert!(weight_after > weight_before);

    Ok(())
}

#[test]
fn test_flex_grow_affects_layout() -> Result<()> {
    let mut harness = setup_harness(Size::new(60, 14))?;
    let (left_before, right_before) = {
        let (left, right) = outer_pair(&mut harness)?;
        (left.w, right.w)
    };
    assert!(left_before.abs_diff(right_before) <= 1);

    harness.key(']')?;

    let (left_after, right_after) = {
        let (left, right) = outer_pair(&mut harness)?;
        (left.w, right.w)
    };
    assert!(left_after > right_after);
    Ok(())
}

#[test]
fn test_flex_adjust_refuses_at_min_size() -> Result<()> {
    let mut harness = setup_harness(Size::new(2, 2))?;
    let (view, _) = outer_pair(&mut harness)?;
    assert!(view.w <= 1 || view.h <= 1);

    // Grow first, so a refused shrink is distinguishable from the `.max(1)`
    // clamp.
    let weight_before = left_flex_weight(&mut harness)?;
    harness.key(']')?;
    let grown = left_flex_weight(&mut harness)?;
    assert_eq!(grown, weight_before + 1);

    harness.key('[')?;
    assert_eq!(
        left_flex_weight(&mut harness)?,
        grown,
        "shrink must be refused while the block is at its minimum size"
    );

    Ok(())
}

#[test]
fn test_screen_edge_is_flush() -> Result<()> {
    let harness = setup_harness(Size::new(40, 12))?;
    let cell = harness.buf().get(Point { x: 0, y: 0 }).unwrap();
    assert_eq!(cell.ch, '\u{2588}');
    Ok(())
}

#[test]
fn test_single_separator_between_root_children() -> Result<()> {
    let mut harness = setup_harness(Size::new(40, 12))?;
    let (left_view, right_view) = outer_pair(&mut harness)?;

    harness.render()?;
    let buf = harness.buf();
    let separator = find_separator_column(buf, left_view, right_view);
    assert!(
        separator.is_some(),
        "expected a single-column separator between root children"
    );

    Ok(())
}

#[test]
fn test_delete_focused_block() -> Result<()> {
    let mut harness = setup_harness(Size::new(60, 14))?;
    let (left, right) = root_children_pair(&mut harness)?;
    let left_focused =
        with_root_block(&mut harness, |ctx, _root| Ok(ctx.is_on_focus_path_of(left)))?;
    assert!(left_focused);

    harness.key('x')?;

    let (count, right_focused) = with_root_block(&mut harness, |ctx, root| {
        let count = ctx.children_of(root).len();
        let right_focused = ctx.is_on_focus_path_of(right);
        Ok((count, right_focused))
    })?;
    assert_eq!(count, 1);
    assert!(right_focused);
    Ok(())
}

#[test]
fn test_separators_remain_continuous_after_nested_splits() -> Result<()> {
    let mut harness = setup_harness(Size::new(40, 12))?;
    harness.key('s')?;

    let (_, right) = root_children_pair(&mut harness)?;
    harness.with_root_context(|_root: &mut FocusGym, ctx| {
        ctx.set_focus(right)?;
        Ok(())
    })?;
    harness.key('s')?;

    let (left_view, right_view) = with_root_block(&mut harness, |ctx, root| {
        let left = ctx
            .children_of(root)
            .first()
            .copied()
            .ok_or_else(|| Error::NotFound("left child".into()))?;
        let left_view = outer_of(ctx, left, "left node")?;
        let right_view = outer_of(ctx, right, "right node")?;
        Ok((left_view, right_view))
    })?;

    harness.render()?;
    let buf = harness.buf();
    let boundary_x = find_separator_column(buf, left_view, right_view)
        .expect("expected a separator column for nested splits");
    for y in 0..buf.size().h {
        let cell = buf.get(Point { x: boundary_x, y }).unwrap();
        assert_eq!(cell.ch, ' ');
    }

    Ok(())
}

#[test]
fn repeated_deletion_preserves_a_root_that_can_split_again() -> Result<()> {
    let mut harness = setup_harness(Size::new(60, 14))?;
    let root = with_root_block(&mut harness, |_ctx, root| Ok(root))?;
    for _ in 0..5 {
        harness.key('x')?;
    }
    with_root_block(&mut harness, |ctx, current| {
        assert_eq!(current, root);
        assert!(ctx.is_attached_of(root));
        assert!(ctx.is_focused_of(root));
        assert!(ctx.children_of(root).is_empty());
        Ok(())
    })?;
    harness.key('s')?;
    assert_eq!(root_children(&mut harness)?.len(), 2);
    Ok(())
}
