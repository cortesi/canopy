use canopy::{
    NodeId,
    error::Result,
    geom::{Expanse, Point},
    layout::Sizing,
    testing::harness::Harness,
};

use crate::focusgym::{FocusGym, setup_bindings};

fn setup_harness(size: Expanse) -> Result<Harness> {
    let mut harness = Harness::builder(FocusGym::new())
        .size(size.w, size.h)
        .build()?;
    setup_bindings(&mut harness.canopy)?;
    harness.render()?;
    Ok(harness)
}

fn root_block_id(harness: &Harness) -> NodeId {
    harness
        .canopy
        .core
        .nodes
        .get(harness.root)
        .and_then(|node| node.children.first().copied())
        .expect("root block not initialized")
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
    let harness = setup_harness(Expanse::new(60, 14))?;
    let root_block = root_block_id(&harness);
    let core = &harness.canopy.core;
    let parent = core.nodes[root_block].view.outer;
    let children = core.nodes[root_block].children.clone();
    assert_eq!(children.len(), 2);
    for child in children {
        let view = core.nodes[child].view.outer;
        assert_eq!(view.h, parent.h);
        assert_eq!(view.tl.y, parent.tl.y);
    }
    Ok(())
}

#[test]
fn test_vertical_children_fill_width_and_height() -> Result<()> {
    let mut harness = setup_harness(Expanse::new(60, 14))?;
    let root_block = root_block_id(&harness);
    let core = &harness.canopy.core;
    let left = core.nodes[root_block]
        .children
        .first()
        .copied()
        .expect("missing left child");

    harness.key('s')?;

    let core = &harness.canopy.core;
    let parent = core.nodes[left].view.outer;
    let children = core.nodes[left].children.clone();
    assert_eq!(children.len(), 2);
    let mut max_bottom = parent.tl.y;
    for child in children {
        let view = core.nodes[child].view.outer;
        assert_eq!(view.w, parent.w);
        max_bottom = max_bottom.max(view.tl.y + view.h as i32);
    }
    assert_eq!(max_bottom, parent.tl.y + parent.h as i32);
    Ok(())
}

#[test]
fn test_flex_grow_and_shrink_commands_update_style() -> Result<()> {
    let mut harness = setup_harness(Expanse::new(60, 14))?;
    let root_block = root_block_id(&harness);
    let core = &harness.canopy.core;
    let left = core.nodes[root_block]
        .children
        .first()
        .copied()
        .expect("missing left child");

    let weight_before = match core.nodes[left].layout.width {
        Sizing::Flex(weight) => weight,
        _ => 1,
    };

    harness.key(']')?;
    harness.key('}')?;

    let core = &harness.canopy.core;
    let weight_after = match core.nodes[left].layout.width {
        Sizing::Flex(weight) => weight,
        _ => 1,
    };

    assert!(weight_after > weight_before);

    Ok(())
}

#[test]
fn test_flex_grow_affects_layout() -> Result<()> {
    let mut harness = setup_harness(Expanse::new(60, 14))?;
    let root_block = root_block_id(&harness);
    let core = &harness.canopy.core;
    let left = core.nodes[root_block]
        .children
        .first()
        .copied()
        .expect("missing left child");
    let right = core.nodes[root_block]
        .children
        .get(1)
        .copied()
        .expect("missing right child");

    let left_before = core.nodes[left].view.outer.w;
    let right_before = core.nodes[right].view.outer.w;
    assert!(left_before.abs_diff(right_before) <= 1);

    harness.key(']')?;

    let core = &harness.canopy.core;
    let left_after = core.nodes[left].view.outer.w;
    let right_after = core.nodes[right].view.outer.w;
    assert!(left_after > right_after);
    Ok(())
}

#[test]
fn test_flex_adjust_refuses_at_min_size() -> Result<()> {
    let mut harness = setup_harness(Expanse::new(2, 2))?;
    let root_block = root_block_id(&harness);
    let core = &harness.canopy.core;
    let left = core.nodes[root_block]
        .children
        .first()
        .copied()
        .expect("missing left child");

    let view = core.nodes[left].view.outer;
    assert!(view.w <= 1 || view.h <= 1);

    let weight_before = match core.nodes[left].layout.width {
        Sizing::Flex(weight) => weight,
        _ => 1,
    };

    harness.key('[')?;
    harness.key('}')?;

    let core = &harness.canopy.core;
    let weight_after = match core.nodes[left].layout.width {
        Sizing::Flex(weight) => weight,
        _ => 1,
    };
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
    let root_block = root_block_id(&harness);
    let core = &harness.canopy.core;
    let left = core.nodes[root_block]
        .children
        .first()
        .copied()
        .expect("missing left child");
    let left_view = core.nodes[left].view.outer;
    let right = core.nodes[root_block]
        .children
        .get(1)
        .copied()
        .expect("missing right child");
    let right_view = core.nodes[right].view.outer;

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
    let root_block = root_block_id(&harness);
    let core = &harness.canopy.core;
    let left = core.nodes[root_block]
        .children
        .first()
        .copied()
        .expect("missing left child");
    let right = core.nodes[root_block]
        .children
        .get(1)
        .copied()
        .expect("missing right child");
    assert_eq!(core.focus, Some(left));

    harness.key('x')?;

    let core = &harness.canopy.core;
    assert_eq!(core.nodes[root_block].children.len(), 1);
    assert_eq!(core.focus, Some(right));
    Ok(())
}

#[test]
fn test_separators_remain_continuous_after_nested_splits() -> Result<()> {
    let mut harness = setup_harness(Expanse::new(40, 12))?;
    harness.key('s')?;

    let root_block = root_block_id(&harness);
    let right = harness.canopy.core.nodes[root_block]
        .children
        .get(1)
        .copied()
        .expect("missing right child");
    harness.canopy.core.set_focus(right);
    harness.key('s')?;

    let core = &harness.canopy.core;
    let left = core.nodes[root_block]
        .children
        .first()
        .copied()
        .expect("missing left child");
    let left_view = core.nodes[left].view.outer;
    let right_view = core.nodes[right].view.outer;

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
