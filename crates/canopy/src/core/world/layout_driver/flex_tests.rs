//! Bounded flex allocation tests.

use proptest::prelude::*;

use super::{FlexItem, allocate_flex};
use crate::{
    NodeId,
    core::world::{
        Core,
        test_support::{LayoutWidget, TestWidget, attach_root_child, wrap_node},
    },
    error::Result,
    geom::Size,
    layout::{Constraint, Direction, Fraction, Layout, LayoutOverride, Measurement},
};

/// A flex item of `weight` with no bounds.
fn weight(weight: u32) -> FlexItem {
    FlexItem {
        weight,
        ..FlexItem::default()
    }
}

/// Attach a fill-sized row under the root, with `children` of the given
/// layouts.
fn row_of(core: &mut Core, layouts: &[Layout]) -> Result<Vec<NodeId>> {
    let row = core.create_detached(LayoutWidget(Layout::fill().direction(Direction::Row)))?;
    attach_root_child(core, row)?;
    let mut children = Vec::new();
    for layout in layouts {
        let child = core.create_detached(LayoutWidget(*layout))?;
        core.attach(row, child)?;
        children.push(child);
    }
    Ok(children)
}

/// Return the widths of `nodes`.
fn widths(core: &Core, nodes: &[NodeId]) -> Vec<u32> {
    nodes.iter().map(|node| core.nodes[*node].rect.w).collect()
}

#[test]
fn a_fixed_child_takes_its_size_and_flex_siblings_share_the_rest() -> Result<()> {
    let mut core = Core::new();
    let children = row_of(
        &mut core,
        &[
            Layout::fill().fixed_width(4),
            Layout::fill(),
            Layout::fill(),
        ],
    )?;
    core.update_layout(Size::new(20, 3))?;
    assert_eq!(widths(&core, &children), [4, 8, 8]);
    Ok(())
}

#[test]
fn a_fixed_override_on_a_flex_layout_takes_its_size() -> Result<()> {
    let mut core = Core::new();
    let children = row_of(&mut core, &[Layout::fill(), Layout::fill()])?;
    core.set_layout_override(children[0], LayoutOverride::new().fixed_width(3))?;
    core.update_layout(Size::new(20, 3))?;
    assert_eq!(widths(&core, &children), [3, 17]);
    Ok(())
}

#[test]
fn a_max_below_the_share_frees_the_rest() -> Result<()> {
    let mut core = Core::new();
    let children = row_of(&mut core, &[Layout::fill().max_width(3), Layout::fill()])?;
    core.update_layout(Size::new(20, 3))?;
    assert_eq!(widths(&core, &children), [3, 17]);
    Ok(())
}

#[test]
fn a_min_above_the_share_takes_the_min() -> Result<()> {
    let mut core = Core::new();
    let children = row_of(
        &mut core,
        &[Layout::fill().min_width(12), Layout::fill(), Layout::fill()],
    )?;
    core.update_layout(Size::new(20, 3))?;
    assert_eq!(widths(&core, &children), [12, 4, 4]);
    Ok(())
}

#[test]
fn a_width_fraction_frees_the_rest_of_its_share() -> Result<()> {
    let mut core = Core::new();
    let children = row_of(
        &mut core,
        &[
            Layout::fill().max_width_fraction(Fraction::new(1, 4)),
            Layout::fill(),
        ],
    )?;
    core.update_layout(Size::new(20, 3))?;
    assert_eq!(widths(&core, &children), [5, 15]);
    Ok(())
}

#[test]
fn minimums_that_exceed_the_space_overflow() -> Result<()> {
    let mut core = Core::new();
    let children = row_of(
        &mut core,
        &[
            Layout::fill().min_width(8),
            Layout::fill().min_width(9),
            Layout::fill(),
        ],
    )?;
    core.update_layout(Size::new(10, 3))?;
    assert_eq!(widths(&core, &children), [8, 9, 0]);
    assert_eq!(
        allocate_flex(
            10,
            &[
                FlexItem {
                    min: Some(8),
                    ..weight(1)
                },
                FlexItem {
                    min: Some(9),
                    ..weight(1)
                },
            ]
        ),
        [8, 9]
    );
    Ok(())
}

#[test]
fn a_measured_row_and_its_layout_agree() -> Result<()> {
    let mut core = Core::new();
    let row = wrap_node(&mut core)?;
    attach_root_child(&mut core, row)?;
    core.set_layout_of(row, Layout::row().flex_horizontal(1))?;
    let capped = core.create_detached(LayoutWidget(Layout::fill().max_width(3)))?;
    // Twenty-one cells of text: three lines at seven columns, four at five.
    let (text, _) = TestWidget::new(|c| {
        let width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n.max(1),
            Constraint::Unbounded => 21,
        };
        Measurement::Fixed(Size::new(width, 21u32.div_ceil(width)))
    });
    let text = core.create_detached(text)?;
    core.set_children(row, vec![capped, text])?;
    core.set_layout_of(text, Layout::fill())?;
    core.update_layout(Size::new(10, 10))?;

    assert_eq!(widths(&core, &[capped, text]), [3, 7]);
    assert_eq!(core.nodes[row].content_size.h, 3);
    assert_eq!(core.nodes[text].rect.h, 3);
    Ok(())
}

#[test]
fn bounded_items_follow_the_weights_of_the_rest() {
    let items = [
        FlexItem {
            max: Some(2),
            ..weight(1)
        },
        weight(1),
        weight(2),
    ];
    assert_eq!(allocate_flex(20, &items), [2, 6, 12]);

    let items = [
        FlexItem {
            min: Some(8),
            ..weight(1)
        },
        FlexItem {
            max: Some(4),
            ..weight(1)
        },
        weight(1),
    ];
    assert_eq!(allocate_flex(10, &items), [8, 1, 1]);
}

#[test]
fn unbounded_items_split_by_weight() {
    assert_eq!(allocate_flex(5, &[weight(3), weight(7)]), [2, 3]);
    assert_eq!(
        allocate_flex(2, &[weight(1), weight(1), weight(1)]),
        [1, 1, 0]
    );
    assert_eq!(allocate_flex(0, &[weight(1), weight(2)]), [0, 0]);
}

proptest! {
    #[test]
    fn unbounded_items_give_every_cell_to_some_child(
        remaining in 0u32..1000,
        weights in prop::collection::vec(1u32..20, 1..12),
    ) {
        let items: Vec<FlexItem> = weights.iter().map(|w| weight(*w)).collect();
        let shares = allocate_flex(remaining, &items);
        prop_assert_eq!(shares.iter().sum::<u32>(), remaining);
    }

    #[test]
    fn bounded_items_keep_their_bounds(
        remaining in 0u32..200,
        bounds in prop::collection::vec((1u32..5, 0u32..30, 0u32..30), 1..8),
    ) {
        let items: Vec<FlexItem> = bounds
            .iter()
            .map(|(w, a, b)| FlexItem {
                weight: *w,
                min: Some((*a).min(*b)),
                max: Some((*a).max(*b)),
            })
            .collect();
        let shares = allocate_flex(remaining, &items);
        let total: u32 = shares.iter().sum();
        let mins: u32 = items.iter().filter_map(|item| item.min).sum();
        let maxes: u32 = items.iter().filter_map(|item| item.max).sum();
        for (share, item) in shares.iter().zip(&items) {
            prop_assert!(item.min.is_none_or(|min| *share >= min));
            prop_assert!(item.max.is_none_or(|max| *share <= max));
        }
        prop_assert_eq!(total, remaining.clamp(mins, maxes.max(mins)));
    }
}
