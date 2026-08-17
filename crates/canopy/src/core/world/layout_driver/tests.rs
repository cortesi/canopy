//! Tests for the layout driver.

use std::sync::{Arc, Mutex};

use proptest::prelude::*;
use rand::{RngExt, SeedableRng, rngs::StdRng};

use super::{
    LayoutPass, align_offset, allocate_flex_shares, clamp_outer, clamp_scroll, constraint_for_axis,
};
use crate::{
    NodeId,
    core::world::{
        Core,
        test_support::{LayoutWidget, TestWidget, assert_error_context, fixed_leaf, wrap_node},
    },
    error::{Error, NodeOperationKind, Result},
    geom::{Point, Size},
    layout::{
        Align, Constraint, Direction, Direction as LayoutDirection, Display, Edges, Layout,
        MeasureConstraints, Measurement, Sizing,
    },
};

/// Attach one node as the only child of the root.
fn attach_root_child(core: &mut Core, child: NodeId) -> Result<()> {
    core.set_children(core.root, vec![child])
}

/// A widget that always measures to one cell.
fn simple_widget() -> TestWidget {
    TestWidget::new(|_constraints| Measurement::Fixed(Size::new(1, 1))).0
}

#[test]
fn clamp_outer_no_bounds() {
    let layout = Layout::column();
    let size = Size::new(5, 7);
    assert_eq!(clamp_outer(size, layout), size);
}

#[test]
fn clamp_outer_min_only() {
    let mut layout = Layout::column();
    layout.min_width = Some(10);
    layout.min_height = Some(2);
    assert_eq!(clamp_outer(Size::new(5, 1), layout), Size::new(10, 2));
}

#[test]
fn clamp_outer_max_only() {
    let mut layout = Layout::column();
    layout.max_width = Some(3);
    layout.max_height = Some(4);
    assert_eq!(clamp_outer(Size::new(5, 7), layout), Size::new(3, 4));
}

#[test]
fn constraint_for_axis_flex_is_exact() {
    let c = constraint_for_axis(Sizing::Flex(1), 10, None, None, 0, false);
    assert_eq!(c, Constraint::Exact(10));
}

#[test]
fn constraint_for_axis_min_equals_max_is_exact() {
    let c = constraint_for_axis(Sizing::Measure, 10, Some(6), Some(6), 2, false);
    assert_eq!(c, Constraint::Exact(4));
}

#[test]
fn constraint_for_axis_max_caps_available() {
    let c = constraint_for_axis(Sizing::Measure, 10, None, Some(6), 2, false);
    assert_eq!(c, Constraint::AtMost(4));
}

#[test]
fn leaf_measure_adds_padding() -> Result<()> {
    let mut core = Core::new();
    let child = fixed_leaf(&mut core, 5, 5)?;
    attach_root_child(&mut core, child)?;
    core.set_layout_of(child, Layout::column().padding(Edges::all(1)))?;
    core.update_layout(Size::new(50, 50))?;
    let node = &core.nodes[child];
    assert_eq!(node.rect.w, 7);
    assert_eq!(node.rect.h, 7);
    assert_eq!(node.content_size, Size::new(5, 5));
    Ok(())
}

#[test]
fn leaf_padding_consumes_all() -> Result<()> {
    let mut core = Core::new();
    let child = fixed_leaf(&mut core, 1, 1)?;
    attach_root_child(&mut core, child)?;
    core.set_layout_of(child, Layout::fill().padding(Edges::all(1)))?;
    core.update_layout(Size::new(1, 1))?;
    let node = &core.nodes[child];
    assert_eq!(node.content_size, Size::new(0, 0));
    Ok(())
}

#[test]
fn flex_axis_constraints_are_exact() -> Result<()> {
    let mut core = Core::new();
    let (widget, calls) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let child = core.create_detached(widget)?;
    attach_root_child(&mut core, child)?;
    core.set_layout_of(child, Layout::column().flex_horizontal(1))?;
    core.update_layout(Size::new(10, 5))?;
    let calls = calls.lock().unwrap();
    assert!(!calls.is_empty());
    assert_eq!(calls[0].width, Constraint::Exact(10));
    Ok(())
}

#[test]
fn remeasure_when_min_width_expands_measured() -> Result<()> {
    let mut core = Core::new();
    let (widget, calls) = TestWidget::new(|c| {
        let width = match c.width {
            Constraint::Exact(n) => n,
            _ => 4,
        };
        let height = if width >= 10 { 2 } else { 4 };
        Measurement::Fixed(Size::new(width, height))
    });
    let child = core.create_detached(widget)?;
    attach_root_child(&mut core, child)?;
    core.set_layout_of(child, Layout::column().min_width(10))?;
    core.update_layout(Size::new(20, 20))?;
    let calls = calls.lock().unwrap();
    assert!(calls.iter().any(|c| c.width == Constraint::Exact(10)));
    let node = &core.nodes[child];
    assert_eq!(node.content_size.h, 2);
    Ok(())
}

#[test]
fn remeasure_when_min_width_expands_flex() -> Result<()> {
    let mut core = Core::new();
    let (widget, calls) = TestWidget::new(|c| {
        let width = match c.width {
            Constraint::Exact(n) => n,
            _ => 0,
        };
        Measurement::Fixed(Size::new(width, width.max(1)))
    });
    let child = core.create_detached(widget)?;
    attach_root_child(&mut core, child)?;
    core.set_layout_of(
        child,
        Layout::column()
            .flex_horizontal(1)
            .padding(Edges::all(1))
            .min_width(30),
    )?;
    core.update_layout(Size::new(10, 10))?;
    let calls = calls.lock().unwrap();
    assert!(calls.iter().any(|c| c.width == Constraint::Exact(8)));
    assert!(calls.iter().any(|c| c.width == Constraint::Exact(28)));
    Ok(())
}

#[test]
fn wrap_no_children() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(parent, Layout::column().padding(Edges::all(1)))?;
    core.update_layout(Size::new(20, 20))?;
    let node = &core.nodes[parent];
    assert_eq!(node.content_size, Size::new(0, 0));
    assert_eq!(node.rect.w, 2);
    assert_eq!(node.rect.h, 2);
    Ok(())
}

#[test]
fn wrap_sum_main_max_cross() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 3)));
    let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 2)));
    let child1 = core.create_detached(c1)?;
    let child2 = core.create_detached(c2)?;
    let child3 = core.create_detached(c3)?;
    core.set_children(parent, vec![child1, child2, child3])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(parent, Layout::column().gap(1))?;
    core.update_layout(Size::new(50, 50))?;
    let node = &core.nodes[parent];
    assert_eq!(node.content_size, Size::new(4, 8));
    Ok(())
}

#[test]
fn wrap_includes_child_padding() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let child = fixed_leaf(&mut core, 3, 1)?;
    core.set_children(parent, vec![child])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(parent, Layout::column())?;
    core.set_layout_of(child, Layout::column().padding(Edges::all(1)))?;
    core.update_layout(Size::new(50, 50))?;
    let node = &core.nodes[parent];
    assert_eq!(node.content_size, Size::new(5, 3));
    Ok(())
}

#[test]
fn wrap_flex_child_treated_as_measure_when_parent_not_exact() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let child = fixed_leaf(&mut core, 2, 4)?;
    core.set_children(parent, vec![child])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(parent, Layout::column())?;
    core.with_layout_of(child, |layout| {
        layout.height = Sizing::Flex(1);
    })?;
    core.update_layout(Size::new(20, 20))?;
    let node = &core.nodes[parent];
    assert_eq!(node.content_size.h, 4);
    Ok(())
}

#[test]
fn wrap_flex_child_behaves_as_flex_when_parent_exact() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let (child1_widget, calls1) = TestWidget::new(|c| {
        let width = match c.width {
            Constraint::Exact(n) => n,
            Constraint::AtMost(n) => n,
            Constraint::Unbounded => 0,
        };
        Measurement::Fixed(Size::new(width, width))
    });
    let (child2_widget, calls2) = TestWidget::new(|c| {
        let width = match c.width {
            Constraint::Exact(n) => n,
            Constraint::AtMost(n) => n,
            Constraint::Unbounded => 0,
        };
        Measurement::Fixed(Size::new(width, width))
    });
    let child1 = core.create_detached(child1_widget)?;
    let child2 = core.create_detached(child2_widget)?;
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(parent, Layout::row().flex_horizontal(1))?;
    core.with_layout_of(child1, |layout| {
        layout.width = Sizing::Flex(1);
    })?;
    core.with_layout_of(child2, |layout| {
        layout.width = Sizing::Flex(1);
    })?;
    core.update_layout(Size::new(10, 10))?;
    let calls1 = calls1.lock().unwrap();
    let calls2 = calls2.lock().unwrap();
    assert!(calls1.iter().any(|c| c.width == Constraint::Exact(5)));
    assert!(calls2.iter().any(|c| c.width == Constraint::Exact(5)));
    let parent_node = &core.nodes[parent];
    assert_eq!(parent_node.content_size.h, 5);
    Ok(())
}

#[test]
fn wrap_gap_counts_only_visible_children() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let child1 = core.create_detached(c1)?;
    let child2 = core.create_detached(c2)?;
    let child3 = core.create_detached(c3)?;
    core.set_children(parent, vec![child1, child2, child3])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(parent, Layout::column().gap(2))?;
    core.set_layout_of(child2, Layout::column().none())?;
    core.update_layout(Size::new(20, 20))?;
    let node = &core.nodes[parent];
    assert_eq!(node.content_size.h, 4);
    Ok(())
}

#[test]
fn flex_shares_sum_equals_remaining() {
    let shares = allocate_flex_shares(17, &[1, 2, 3, 4]);
    let sum: u32 = shares.iter().sum();
    assert_eq!(sum, 17);
}

#[test]
fn flex_shares_proportional_sanity() {
    let shares = allocate_flex_shares(5, &[3, 7]);
    assert_eq!(shares, vec![2, 3]);
}

#[test]
fn flex_shares_stable_tie_break() {
    let shares = allocate_flex_shares(2, &[1, 1, 1]);
    assert_eq!(shares, vec![1, 1, 0]);
}

fn boundary_layout_u32() -> impl Strategy<Value = u32> {
    prop_oneof![
        Just(0),
        Just(1),
        Just(2),
        Just(u32::MAX - 1),
        Just(u32::MAX),
        any::<u32>(),
    ]
}

proptest! {
    #[test]
    fn flex_shares_preserve_remaining_space(remaining in 0u32..1000, weights in prop::collection::vec(1u32..20, 0..12)) {
        let shares = allocate_flex_shares(remaining, &weights);
        prop_assert_eq!(shares.len(), weights.len());
        let expected = if weights.is_empty() { 0 } else { remaining };
        prop_assert_eq!(shares.iter().sum::<u32>(), expected);
        prop_assert!(shares.iter().all(|share| *share <= remaining));
    }

    #[test]
    fn clamp_scroll_stays_inside_canvas(
        scroll_x in 0u32..200,
        scroll_y in 0u32..200,
        view_w in 0u32..100,
        view_h in 0u32..100,
        canvas_w in 0u32..150,
        canvas_h in 0u32..150,
    ) {
        let mut scroll = Point { x: scroll_x, y: scroll_y };
        let view = Size::new(view_w, view_h);
        let canvas = Size::new(canvas_w, canvas_h);

        clamp_scroll(&mut scroll, view, canvas);

        let max_x = if view.w == 0 { 0 } else { canvas.w.saturating_sub(view.w) };
        let max_y = if view.h == 0 { 0 } else { canvas.h.saturating_sub(view.h) };
        prop_assert!(scroll.x <= max_x);
        prop_assert!(scroll.y <= max_y);
    }

    #[test]
    fn hidden_display_and_padding_layout_properties(
        hidden in any::<bool>(),
        display_none in any::<bool>(),
        padding in 0u32..8,
        screen_w in 0u32..40,
        screen_h in 0u32..40,
    ) {
        let mut core = Core::new();
        let parent = wrap_node(&mut core)?;
        let child = fixed_leaf(&mut core, 3, 2)?;

        prop_assert!(core.set_children(parent, vec![child]).is_ok());
        prop_assert!(attach_root_child(&mut core, parent).is_ok());
        let parent_layout = Layout::fill().padding(Edges::all(padding));
        prop_assert!(core.set_layout_of(parent, parent_layout).is_ok());
        let child_layout = if display_none {
            Layout::fill().none()
        } else {
            Layout::fill()
        };
        let child_layout_set = core.set_layout_of(child, child_layout).is_ok();
        prop_assert!(child_layout_set);
        core.set_hidden(child, hidden)?;

        prop_assert!(core.update_layout(Size::new(screen_w, screen_h)).is_ok());

        let parent_node = &core.nodes[parent];
        let expected_content = Size::new(
            parent_node.rect.w.saturating_sub(parent_node.layout.padding.horizontal()),
            parent_node.rect.h.saturating_sub(parent_node.layout.padding.vertical()),
        );
        prop_assert_eq!(parent_node.content_size, expected_content);

        let child_node = &core.nodes[child];
        if hidden || display_none {
            prop_assert_eq!(child_node.rect.w, 0);
            prop_assert_eq!(child_node.rect.h, 0);
            prop_assert_eq!(child_node.canvas, Size::ZERO);
            prop_assert!(child_node.view.is_zero());
        } else {
            prop_assert!(child_node.rect.w <= parent_node.content_size.w);
            prop_assert!(child_node.rect.h <= parent_node.content_size.h);
        }
    }

    #[test]
    fn sequential_layout_boundaries_preserve_alignment_and_order(
        is_row in any::<bool>(),
        main_align in 0u8..3,
        cross_align in 0u8..3,
        padding in 0u32..4,
        gap in boundary_layout_u32(),
        screen_w in boundary_layout_u32(),
        screen_h in boundary_layout_u32(),
        hide_last in any::<bool>(),
        remove_last in any::<bool>(),
        overflow in any::<bool>(),
    ) {
        let direction = if is_row { Direction::Row } else { Direction::Column };
        let align = |value| match value {
            0 => Align::Start,
            1 => Align::Center,
            _ => Align::End,
        };
        let (horizontal, vertical) = if is_row {
            (align(main_align), align(cross_align))
        } else {
            (align(cross_align), align(main_align))
        };
        let mut layout = Layout::fill()
            .direction(direction)
            .padding(Edges::all(padding))
            .gap(gap)
            .align_horizontal(horizontal)
            .align_vertical(vertical);
        let equivalent = Layout::fill()
            .align_vertical(vertical)
            .gap(gap)
            .align_horizontal(horizontal)
            .padding(Edges::all(padding))
            .direction(direction);
        prop_assert_eq!(layout, equivalent);
        if overflow {
            layout = layout.overflow_x().overflow_y();
        }

        let mut core = Core::new();
        let parent = wrap_node(&mut core)?;
        let (first_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 2)));
        let (second_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 4)));
        let (last_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
        let first = core.create_detached(first_widget)?;
        let second = core.create_detached(second_widget)?;
        let last = core.create_detached(last_widget)?;
        core.set_children(parent, vec![first, second, last])?;
        attach_root_child(&mut core, parent)?;
        core.set_layout_of(parent, layout)?;
        core.set_layout_of(first, Layout::column().fixed_width(3).fixed_height(2))?;
        let second_layout = if is_row {
            Layout::column().flex_horizontal(1).fixed_height(4)
        } else {
            Layout::column().fixed_width(5).flex_vertical(1)
        };
        core.set_layout_of(second, second_layout)?;
        core.set_layout_of(
            last,
            if remove_last {
                Layout::column().fixed_width(2).fixed_height(1).none()
            } else {
                Layout::column().fixed_width(2).fixed_height(1)
            },
        )?;
        core.set_hidden(last, hide_last)?;
        core.update_layout(Size::new(screen_w, screen_h))?;

        let visible = [first, second, last]
            .into_iter()
            .filter(|node| !core.nodes[*node].hidden && core.nodes[*node].layout.display == Display::Block)
            .collect::<Vec<_>>();
        let content = core.nodes[parent].content_size;
        let available_main = direction.main_size(content);
        let available_cross = direction.cross_size(content);
        let children_main = visible.iter().fold(0u32, |total, node| {
            total.saturating_add(direction.main_size(core.nodes[*node].rect.expanse()))
        });
        let gap_total = gap.saturating_mul(u32::try_from(visible.len().saturating_sub(1)).unwrap_or(u32::MAX));
        let group_main = children_main.saturating_add(gap_total);
        let mut expected_main = align_offset(group_main, available_main, align(main_align));

        for node in visible {
            let rect = core.nodes[node].rect;
            let actual_main = if is_row { rect.tl.x } else { rect.tl.y };
            let actual_cross = if is_row { rect.tl.y } else { rect.tl.x };
            prop_assert_eq!(actual_main, expected_main);
            prop_assert_eq!(
                actual_cross,
                align_offset(direction.cross_size(rect.expanse()), available_cross, align(cross_align))
            );
            expected_main = expected_main
                .saturating_add(direction.main_size(rect.expanse()))
                .saturating_add(gap);
        }
    }

    #[test]
    fn generated_invalid_layouts_never_mutate_nodes(kind in 0u8..3) {
        let mut core = Core::new();
        let node = core.create_detached(LayoutWidget(Layout::column()))?;
        let before = core.nodes[node].layout;
        let invalid = match kind {
            0 => Layout::column().flex_vertical(0),
            1 => Layout::column().min_height(u32::MAX).max_height(u32::MAX - 1),
            _ => Layout::column().padding(Edges::new(u32::MAX, 0, 1, 0)),
        };
        prop_assert!(matches!(core.set_layout_of(node, invalid), Err(Error::InvalidLayout(_))));
        prop_assert_eq!(core.nodes[node].layout, before);
    }
}

#[test]
fn invalid_layout_mutations_are_rejected_without_change() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let before = core.nodes[parent].layout;

    let invalid = [
        Layout::column().width(Sizing::Flex(0)),
        Layout::column().min_width(2).max_width(1),
        Layout::column().padding(Edges::new(0, u32::MAX, 0, 1)),
    ];
    for layout in invalid {
        assert!(matches!(
            core.set_layout_of(parent, layout),
            Err(Error::InvalidLayout(_))
        ));
        assert_eq!(core.nodes[parent].layout, before);
    }
    Ok(())
}

#[test]
fn invalid_widget_layouts_are_rejected_before_publication() -> Result<()> {
    let mut core = Core::new();
    let node_count = core.nodes.len();
    let invalid = Layout::row().flex_horizontal(0);

    assert!(matches!(
        core.create_detached(LayoutWidget(invalid)),
        Err(Error::InvalidLayout(_))
    ));
    assert_eq!(core.nodes.len(), node_count);

    let target = core.create_detached(LayoutWidget(Layout::column()))?;
    let before = core.nodes[target].layout;
    assert!(matches!(
        core.replace_subtree(target, LayoutWidget(invalid)),
        Err(Error::InvalidLayout(_))
    ));
    assert_eq!(core.nodes[target].layout, before);
    Ok(())
}

#[test]
fn positions_monotonic_main() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
    let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
    let child1 = core.create_detached(c1)?;
    let child2 = core.create_detached(c2)?;
    let child3 = core.create_detached(c3)?;
    core.set_children(parent, vec![child1, child2, child3])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(parent, Layout::row().flex_horizontal(1).gap(1))?;
    core.update_layout(Size::new(20, 5))?;
    let p1 = core.nodes[child1].rect.tl.x;
    let p2 = core.nodes[child2].rect.tl.x;
    let p3 = core.nodes[child3].rect.tl.x;
    assert!(p1 <= p2 && p2 <= p3);
    Ok(())
}

#[test]
fn no_overlaps_with_min_expansion() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let child1 = core.create_detached(c1)?;
    let child2 = core.create_detached(c2)?;
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(parent, Layout::row().flex_horizontal(1))?;
    core.with_layout_of(child1, |layout| {
        layout.width = Sizing::Flex(1);
        layout.min_width = Some(10);
    })?;
    core.with_layout_of(child2, |layout| {
        layout.width = Sizing::Flex(1);
        layout.min_width = Some(10);
    })?;
    core.update_layout(Size::new(5, 5))?;
    let first = &core.nodes[child1];
    let second = &core.nodes[child2];
    assert_eq!(second.rect.tl.x, first.rect.tl.x + first.rect.w);
    Ok(())
}

#[test]
fn overflow_positions_consistent() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 1)));
    let child1 = core.create_detached(c1)?;
    let child2 = core.create_detached(c2)?;
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(parent, Layout::row().flex_horizontal(1).gap(1))?;
    core.update_layout(Size::new(5, 5))?;
    assert_eq!(core.nodes[child2].rect.tl.x, 5);
    Ok(())
}

#[test]
fn canvas_clamped_at_least_view() -> Result<()> {
    let mut core = Core::new();
    let (widget, _) =
        TestWidget::with_canvas(|_c| Measurement::Wrap, |_view, _ctx| Size::new(1, 1));
    let child = core.create_detached(widget)?;
    attach_root_child(&mut core, child)?;
    core.set_layout_of(child, Layout::fill())?;
    core.update_layout(Size::new(5, 5))?;
    let node = &core.nodes[child];
    assert_eq!(node.canvas, Size::new(5, 5));
    Ok(())
}

#[test]
fn offset_clamped_when_canvas_shrinks() -> Result<()> {
    let mut core = Core::new();
    let canvas = Arc::new(Mutex::new(Size::new(20, 20)));
    let canvas_clone = Arc::clone(&canvas);
    let (widget, _) = TestWidget::with_canvas(
        |_c| Measurement::Wrap,
        move |_view, _ctx| *canvas_clone.lock().unwrap(),
    );
    let child = core.create_detached(widget)?;
    attach_root_child(&mut core, child)?;
    core.set_layout_of(child, Layout::fill())?;
    if let Some(node) = core.nodes.get_mut(child) {
        node.scroll = Point { x: 15, y: 15 };
    }
    core.update_layout(Size::new(10, 10))?;
    assert_eq!(core.nodes[child].scroll, Point { x: 10, y: 10 });
    *canvas.lock().unwrap() = Size::new(12, 12);
    core.update_layout(Size::new(10, 10))?;
    assert_eq!(core.nodes[child].scroll, Point { x: 2, y: 2 });
    Ok(())
}

#[test]
fn offset_clamped_when_view_grows() -> Result<()> {
    let mut core = Core::new();
    let canvas = Arc::new(Mutex::new(Size::new(20, 20)));
    let canvas_clone = Arc::clone(&canvas);
    let (widget, _) = TestWidget::with_canvas(
        |_c| Measurement::Wrap,
        move |_view, _ctx| *canvas_clone.lock().unwrap(),
    );
    let child = core.create_detached(widget)?;
    attach_root_child(&mut core, child)?;
    core.set_layout_of(child, Layout::fill())?;
    if let Some(node) = core.nodes.get_mut(child) {
        node.scroll = Point { x: 15, y: 15 };
    }
    core.update_layout(Size::new(5, 5))?;
    assert_eq!(core.nodes[child].scroll, Point { x: 15, y: 15 });
    core.update_layout(Size::new(10, 10))?;
    assert_eq!(core.nodes[child].scroll, Point { x: 10, y: 10 });
    Ok(())
}

#[test]
fn zero_view_clamps_scroll() -> Result<()> {
    let mut core = Core::new();
    let canvas = Arc::new(Mutex::new(Size::new(10, 10)));
    let canvas_clone = Arc::clone(&canvas);
    let (widget, _) = TestWidget::with_canvas(
        |_c| Measurement::Wrap,
        move |_view, _ctx| *canvas_clone.lock().unwrap(),
    );
    let child = core.create_detached(widget)?;
    attach_root_child(&mut core, child)?;
    core.set_layout_of(child, Layout::fill())?;
    if let Some(node) = core.nodes.get_mut(child) {
        node.scroll = Point { x: 5, y: 5 };
    }
    core.update_layout(Size::new(0, 0))?;
    assert_eq!(core.nodes[child].scroll, Point { x: 0, y: 0 });
    Ok(())
}

#[test]
fn extreme_padding_is_rejected_without_mutation() -> Result<()> {
    let mut core = Core::new();
    let child = fixed_leaf(&mut core, u32::MAX, u32::MAX)?;
    attach_root_child(&mut core, child)?;
    let before = core.nodes[child].layout;
    assert!(matches!(
        core.set_layout_of(child, Layout::fill().padding(Edges::all(u32::MAX))),
        Err(Error::InvalidLayout(_))
    ));
    assert_eq!(core.nodes[child].layout, before);
    Ok(())
}

#[test]
fn child_screen_origin_signed() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) =
        TestWidget::with_canvas(|_c| Measurement::Wrap, |_view, _ctx| Size::new(20, 10));
    let parent = core.create_detached(parent_widget)?;
    let child = fixed_leaf(&mut core, 2, 2)?;
    core.set_children(parent, vec![child])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(parent, Layout::fill())?;
    core.set_layout_of(child, Layout::column().fixed_width(2).fixed_height(2))?;
    if let Some(node) = core.nodes.get_mut(parent) {
        node.scroll = Point { x: 5, y: 0 };
    }
    core.update_layout(Size::new(10, 10))?;
    let child_view = core.nodes[child].view;
    assert_eq!(child_view.outer.tl.x, -5);
    Ok(())
}

#[test]
fn content_rect_respects_padding() -> Result<()> {
    let mut core = Core::new();
    let child = fixed_leaf(&mut core, 5, 5)?;
    attach_root_child(&mut core, child)?;
    core.set_layout_of(child, Layout::column().padding(Edges::all(1)))?;
    core.update_layout(Size::new(20, 20))?;
    let view = core.nodes[child].view;
    assert_eq!(view.content.tl.x, view.outer.tl.x + 1);
    assert_eq!(view.content.tl.y, view.outer.tl.y + 1);
    assert_eq!(view.content.w, view.outer.w.saturating_sub(2));
    assert_eq!(view.content.h, view.outer.h.saturating_sub(2));
    Ok(())
}

#[test]
fn random_tree_no_panics() -> Result<()> {
    let mut core = Core::new();
    let mut rng = StdRng::seed_from_u64(0x5eed);
    let root_child = build_random_tree(&mut core, &mut rng, 3)?;
    attach_root_child(&mut core, root_child)?;
    core.update_layout(Size::new(40, 20))?;

    for node in core.nodes.values() {
        let expected_w = node.rect.w.saturating_sub(node.layout.padding.horizontal());
        let expected_h = node.rect.h.saturating_sub(node.layout.padding.vertical());
        assert_eq!(node.content_size.w, expected_w);
        assert_eq!(node.content_size.h, expected_h);
        assert!(node.canvas.w >= node.content_size.w);
        assert!(node.canvas.h >= node.content_size.h);
        let max_x = node.canvas.w.saturating_sub(node.content_size.w);
        let max_y = node.canvas.h.saturating_sub(node.content_size.h);
        assert!(node.scroll.x <= max_x);
        assert!(node.scroll.y <= max_y);
    }

    for node in core.nodes.values() {
        // For Stack direction, children can overlap, so skip position ordering check
        if node.layout.direction == LayoutDirection::Stack {
            continue;
        }
        let mut last = 0u32;
        for child in &node.children {
            let child = &core.nodes[*child];
            if child.layout.display == Display::None || child.hidden {
                continue;
            }
            let pos = match node.layout.direction {
                LayoutDirection::Row => child.rect.tl.x,
                LayoutDirection::Column => child.rect.tl.y,
                LayoutDirection::Stack => continue,
            };
            assert!(pos >= last);
            last = pos;
        }
    }

    Ok(())
}

fn build_random_tree(core: &mut Core, rng: &mut StdRng, depth: usize) -> Result<NodeId> {
    let node = fixed_leaf(core, 1, 1)?;
    let mut layout = if rng.random_bool(0.5) {
        Layout::row()
    } else {
        Layout::column()
    };
    if rng.random_bool(0.6) {
        layout.width = Sizing::Flex(rng.random_range(1..3));
    }
    if rng.random_bool(0.6) {
        layout.height = Sizing::Flex(rng.random_range(1..3));
    }
    layout.padding = Edges::new(
        rng.random_range(0..3),
        rng.random_range(0..3),
        rng.random_range(0..3),
        rng.random_range(0..3),
    );
    layout.gap = rng.random_range(0..3);
    let width_bounds = [rng.random_range(0..6), rng.random_range(0..6)];
    let height_bounds = [rng.random_range(0..6), rng.random_range(0..6)];
    layout.min_width = rng
        .random_bool(0.3)
        .then_some(width_bounds[0].min(width_bounds[1]));
    layout.max_width = rng
        .random_bool(0.3)
        .then_some(width_bounds[0].max(width_bounds[1]));
    layout.min_height = rng
        .random_bool(0.3)
        .then_some(height_bounds[0].min(height_bounds[1]));
    layout.max_height = rng
        .random_bool(0.3)
        .then_some(height_bounds[0].max(height_bounds[1]));
    core.set_layout_of(node, layout)?;

    if depth > 0 {
        let child_count = rng.random_range(0..=3);
        if child_count > 0 {
            let mut children = Vec::new();
            for _ in 0..child_count {
                children.push(build_random_tree(core, rng, depth - 1)?);
            }
            core.set_children(node, children)?;
        }
    }

    Ok(node)
}

#[test]
fn stack_children_overlap() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 5)));
    let child1 = core.create_detached(c1)?;
    let child2 = core.create_detached(c2)?;
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(parent, Layout::stack())?;
    core.update_layout(Size::new(50, 50))?;

    // Both children should be at the same position (0, 0) by default
    assert_eq!(core.nodes[child1].rect.tl.x, 0);
    assert_eq!(core.nodes[child1].rect.tl.y, 0);
    assert_eq!(core.nodes[child2].rect.tl.x, 0);
    assert_eq!(core.nodes[child2].rect.tl.y, 0);

    // Parent content size should be the max of children
    let parent_node = &core.nodes[parent];
    assert_eq!(parent_node.content_size, Size::new(10, 10));
    Ok(())
}

#[test]
fn sequential_alignment_controls_group_and_cross_axes() -> Result<()> {
    for (direction, screen, expected) in [
        (
            Direction::Row,
            Size::new(20, 10),
            [Point { x: 10, y: 4 }, Point { x: 15, y: 3 }],
        ),
        (
            Direction::Column,
            Size::new(10, 20),
            [Point { x: 3, y: 12 }, Point { x: 2, y: 16 }],
        ),
    ] {
        let mut core = Core::new();
        let parent = wrap_node(&mut core)?;
        let (first_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 2)));
        let (second_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 4)));
        let first = core.create_detached(first_widget)?;
        let second = core.create_detached(second_widget)?;
        core.set_children(parent, vec![first, second])?;
        attach_root_child(&mut core, parent)?;
        core.set_layout_of(
            parent,
            Layout::fill()
                .direction(direction)
                .gap(2)
                .align_horizontal(if direction == Direction::Row {
                    Align::End
                } else {
                    Align::Center
                })
                .align_vertical(if direction == Direction::Column {
                    Align::End
                } else {
                    Align::Center
                }),
        )?;

        core.update_layout(screen)?;
        assert_eq!(core.nodes[first].rect.tl, expected[0]);
        assert_eq!(core.nodes[second].rect.tl, expected[1]);
    }
    Ok(())
}

#[test]
fn locate_node_prefers_topmost_stack_child() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let child1 = core.create_detached(c1)?;
    let child2 = core.create_detached(c2)?;
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(parent, Layout::stack())?;
    core.update_layout(Size::new(50, 50))?;

    let hit = core.locate_node(core.root, Point { x: 1, y: 1 })?;
    assert_eq!(hit, Some(child2));
    Ok(())
}

#[test]
fn stack_with_center_alignment() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let child = fixed_leaf(&mut core, 10, 10)?;
    core.set_children(parent, vec![child])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(
        parent,
        Layout::fill().direction(Direction::Stack).align_center(),
    )?;
    core.update_layout(Size::new(50, 50))?;

    // Child should be centered in the 50x50 parent
    let child_node = &core.nodes[child];
    assert_eq!(child_node.rect.tl.x, 20); // (50 - 10) / 2
    assert_eq!(child_node.rect.tl.y, 20); // (50 - 10) / 2
    Ok(())
}

#[test]
fn stack_with_end_alignment() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let child = fixed_leaf(&mut core, 10, 10)?;
    core.set_children(parent, vec![child])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(
        parent,
        Layout::fill()
            .direction(Direction::Stack)
            .align_horizontal(Align::End)
            .align_vertical(Align::End),
    )?;
    core.update_layout(Size::new(50, 50))?;

    // Child should be at the end (bottom-right)
    let child_node = &core.nodes[child];
    assert_eq!(child_node.rect.tl.x, 40); // 50 - 10
    assert_eq!(child_node.rect.tl.y, 40); // 50 - 10
    Ok(())
}

#[test]
fn stack_multiple_children_centered() -> Result<()> {
    let mut core = Core::new();
    let parent = wrap_node(&mut core)?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(20, 20)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let child1 = core.create_detached(c1)?;
    let child2 = core.create_detached(c2)?;
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.set_layout_of(
        parent,
        Layout::fill().direction(Direction::Stack).align_center(),
    )?;
    core.update_layout(Size::new(50, 50))?;

    // Both children should be centered independently
    let c1_node = &core.nodes[child1];
    let c2_node = &core.nodes[child2];
    assert_eq!(c1_node.rect.tl.x, 15); // (50 - 20) / 2
    assert_eq!(c1_node.rect.tl.y, 15);
    assert_eq!(c2_node.rect.tl.x, 20); // (50 - 10) / 2
    assert_eq!(c2_node.rect.tl.y, 20);
    Ok(())
}

#[test]
fn measure_errors_include_operation_node_and_path() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(simple_widget())?;
    attach_root_child(&mut core, child)?;
    let path = core.node_path(core.root, child).to_string();
    let constraints = MeasureConstraints {
        width: Constraint::AtMost(1),
        height: Constraint::AtMost(1),
    };

    let error = core
        .with_widget_mut(child, |_widget, core| {
            let mut pass = LayoutPass::new(core);
            pass.measure_cached(child, constraints)
        })?
        .expect_err("measure should fail while the widget is extracted");

    assert!(matches!(
        error,
        Error::NodeOperation {
            kind: NodeOperationKind::Layout,
            ..
        }
    ));
    assert_error_context(&error, "measure", child, &path);
    Ok(())
}

#[test]
fn canvas_errors_include_operation_node_and_path() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(simple_widget())?;
    attach_root_child(&mut core, child)?;
    let path = core.node_path(core.root, child).to_string();

    let error = core
        .with_widget_mut(child, |_widget, core| {
            let pass = LayoutPass::new(core);
            pass.compute_canvas(child, Size::new(1, 1))
        })?
        .expect_err("canvas should fail while the widget is extracted");

    assert!(matches!(
        error,
        Error::NodeOperation {
            kind: NodeOperationKind::Layout,
            ..
        }
    ));
    assert_error_context(&error, "canvas", child, &path);
    Ok(())
}
