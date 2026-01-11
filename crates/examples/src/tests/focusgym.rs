use canopy::{error::Error, geom::RectI32, prelude::*, testing::harness::Harness};

use crate::focusgym::{Block, FocusGym, setup_bindings};

fn setup_harness(size: Expanse) -> Result<Harness> {
    let mut harness = Harness::builder(FocusGym::new())
        .size(size.w, size.h)
        .build()?;
    setup_bindings(&mut harness.canopy)?;
    harness.render()?;
    Ok(harness)
}

fn with_root_block<R>(
    harness: &mut Harness,
    f: impl FnOnce(&mut dyn Context, NodeId) -> Result<R>,
) -> Result<R> {
    let mut f = Some(f);
    harness.with_root_context(|_root: &mut FocusGym, ctx| {
        let root_block = ctx
            .unique_child::<Block>()?
            .ok_or_else(|| Error::NotFound("root block".into()))?;
        let f = f.take().expect("root block closure already consumed");
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

fn outer_of(ctx: &dyn Context, node: NodeId, label: &str) -> Result<RectI32> {
    ctx.node_view(node)
        .map(|view| view.outer)
        .ok_or_else(|| Error::NotFound(label.to_string()))
}

fn layout_of(ctx: &mut dyn Context, node: NodeId) -> Result<Layout> {
    let mut layout = Layout::default();
    ctx.with_layout_of(node, &mut |node_layout| {
        layout = *node_layout;
    })?;
    Ok(layout)
}

macro_rules! find_separator_column {
    ($buf:expr, $left_view:expr, $right_view:expr) => {{
        let buf = $buf;
        let left_view = $left_view;
        let right_view = $right_view;
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
    }};
}

#[test]
fn test_initial_render_draws_blocks() -> Result<()> {
    let harness = setup_harness(Expanse::new(40, 12))?;
    let buf = harness.buf();
    let size = buf.size();
    let mut found = false;
    for y in 0..size.h {
        for x in 0..size.w {
            let cell = buf.get(Point { x, y }).unwrap();
            if cell.ch == '\u{2588}' {
                found = true;
                break;
            }
        }
        if found {
            break;
        }
    }
    assert!(found, "expected initial render to draw focus blocks");
    Ok(())
}

#[test]
fn test_horizontal_children_fill_height() -> Result<()> {
    let mut harness = setup_harness(Expanse::new(60, 14))?;
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
    let mut harness = setup_harness(Expanse::new(60, 14))?;

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
fn test_flex_grow_and_shrink_commands_update_style() -> Result<()> {
    let mut harness = setup_harness(Expanse::new(60, 14))?;
    let weight_before = with_root_block(&mut harness, |ctx, root| {
        let left = ctx
            .children_of(root)
            .first()
            .copied()
            .ok_or_else(|| Error::NotFound("left child".into()))?;
        let layout = layout_of(ctx, left)?;
        Ok(match layout.width {
            Sizing::Flex(weight) => weight,
            _ => 1,
        })
    })?;

    harness.key(']')?;
    harness.key('}')?;

    let weight_after = with_root_block(&mut harness, |ctx, root| {
        let left = ctx
            .children_of(root)
            .first()
            .copied()
            .ok_or_else(|| Error::NotFound("left child".into()))?;
        let layout = layout_of(ctx, left)?;
        Ok(match layout.width {
            Sizing::Flex(weight) => weight,
            _ => 1,
        })
    })?;

    assert!(weight_after > weight_before);

    Ok(())
}

#[test]
fn test_flex_grow_affects_layout() -> Result<()> {
    let mut harness = setup_harness(Expanse::new(60, 14))?;
    let (left_before, right_before) = with_root_block(&mut harness, |ctx, root| {
        let children = ctx.children_of(root);
        let left = children
            .first()
            .copied()
            .ok_or_else(|| Error::NotFound("left child".into()))?;
        let right = children
            .get(1)
            .copied()
            .ok_or_else(|| Error::NotFound("right child".into()))?;
        let left_view = outer_of(ctx, left, "left node")?.w;
        let right_view = outer_of(ctx, right, "right node")?.w;
        Ok((left_view, right_view))
    })?;
    assert!(left_before.abs_diff(right_before) <= 1);

    harness.key(']')?;

    let (left_after, right_after) = with_root_block(&mut harness, |ctx, root| {
        let children = ctx.children_of(root);
        let left = children
            .first()
            .copied()
            .ok_or_else(|| Error::NotFound("left child".into()))?;
        let right = children
            .get(1)
            .copied()
            .ok_or_else(|| Error::NotFound("right child".into()))?;
        let left_view = outer_of(ctx, left, "left node")?.w;
        let right_view = outer_of(ctx, right, "right node")?.w;
        Ok((left_view, right_view))
    })?;
    assert!(left_after > right_after);
    Ok(())
}

#[test]
fn test_flex_adjust_refuses_at_min_size() -> Result<()> {
    let mut harness = setup_harness(Expanse::new(2, 2))?;
    let (view, weight_before) = with_root_block(&mut harness, |ctx, root| {
        let left = ctx
            .children_of(root)
            .first()
            .copied()
            .ok_or_else(|| Error::NotFound("left child".into()))?;
        let view = outer_of(ctx, left, "left node")?;
        let layout = layout_of(ctx, left)?;
        let weight = match layout.width {
            Sizing::Flex(weight) => weight,
            _ => 1,
        };
        Ok((view, weight))
    })?;

    assert!(view.w <= 1 || view.h <= 1);

    harness.key('[')?;
    harness.key('}')?;

    let weight_after = with_root_block(&mut harness, |ctx, root| {
        let left = ctx
            .children_of(root)
            .first()
            .copied()
            .ok_or_else(|| Error::NotFound("left child".into()))?;
        let layout = layout_of(ctx, left)?;
        Ok(match layout.width {
            Sizing::Flex(weight) => weight,
            _ => 1,
        })
    })?;
    assert!(weight_after >= weight_before);

    Ok(())
}

#[test]
fn test_screen_edge_is_flush() -> Result<()> {
    let harness = setup_harness(Expanse::new(40, 12))?;
    let cell = harness.buf().get(Point { x: 0, y: 0 }).unwrap();
    assert_eq!(cell.ch, '\u{2588}');
    Ok(())
}

#[test]
fn test_single_separator_between_root_children() -> Result<()> {
    let mut harness = setup_harness(Expanse::new(40, 12))?;
    let (left_view, right_view) = with_root_block(&mut harness, |ctx, root| {
        let children = ctx.children_of(root);
        let left = children
            .first()
            .copied()
            .ok_or_else(|| Error::NotFound("left child".into()))?;
        let right = children
            .get(1)
            .copied()
            .ok_or_else(|| Error::NotFound("right child".into()))?;
        let left_view = outer_of(ctx, left, "left node")?;
        let right_view = outer_of(ctx, right, "right node")?;
        Ok((left_view, right_view))
    })?;

    harness.render()?;
    let buf = harness.buf();
    let separator = find_separator_column!(&buf, left_view, right_view);
    assert!(
        separator.is_some(),
        "expected a single-column separator between root children"
    );

    Ok(())
}

#[test]
fn test_delete_focused_block() -> Result<()> {
    let mut harness = setup_harness(Expanse::new(60, 14))?;
    let (left, right) = root_children_pair(&mut harness)?;
    let left_focused = with_root_block(&mut harness, |ctx, _root| {
        Ok(ctx.node_is_on_focus_path(left))
    })?;
    assert!(left_focused);

    harness.key('x')?;

    let (count, right_focused) = with_root_block(&mut harness, |ctx, root| {
        let count = ctx.children_of(root).len();
        let right_focused = ctx.node_is_on_focus_path(right);
        Ok((count, right_focused))
    })?;
    assert_eq!(count, 1);
    assert!(right_focused);
    Ok(())
}

#[test]
fn test_separators_remain_continuous_after_nested_splits() -> Result<()> {
    let mut harness = setup_harness(Expanse::new(40, 12))?;
    harness.key('s')?;

    let (_, right) = root_children_pair(&mut harness)?;
    harness.with_root_context(|_root: &mut FocusGym, ctx| {
        ctx.set_focus(right);
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
    let boundary_x = find_separator_column!(&buf, left_view, right_view)
        .expect("expected a separator column for nested splits");
    for y in 0..buf.size().h {
        let cell = buf.get(Point { x: boundary_x, y }).unwrap();
        assert_eq!(cell.ch, ' ');
    }

    Ok(())
}
