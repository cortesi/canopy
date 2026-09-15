//! Width fraction layout tests.

use std::sync::{Arc, Mutex};

use crate::{
    core::{
        id::NodeId,
        world::{
            Core,
            test_support::{LayoutWidget, TestWidget, attach_root_child},
        },
    },
    error::Result,
    geom::Size,
    layout::{
        Constraint, Direction, Fraction, Layout, MeasureConstraints, MeasureOverflow, Sizing,
    },
};

/// A third of a parent's width budget.
const THIRD: Fraction = Fraction::new(1, 3);

/// Recorded measurement constraints.
type Seen = Arc<Mutex<Vec<MeasureConstraints>>>;

/// Attach a row with one-cell gaps under the root.
fn gapped_row(core: &mut Core) -> Result<NodeId> {
    let row = core.create_detached(LayoutWidget(
        Layout::fill().direction(Direction::Row).gap(1),
    ))?;
    attach_root_child(core, row)?;
    Ok(row)
}

/// Attach a child with a natural width of 50 cells, capped at a third of the
/// parent's budget, and record its measurements.
fn capped_child(core: &mut Core, parent: NodeId, bounds: Layout) -> Result<(NodeId, Seen)> {
    let (widget, seen) = TestWidget::new(|c| c.clamp(Size::new(50, 1)));
    let node = core.create_detached(widget)?;
    core.attach(parent, node)?;
    core.set_layout_of(
        node,
        bounds.height(Sizing::Flex(1)).max_width_fraction(THIRD),
    )?;
    Ok((node, seen))
}

/// Attach a fill-sized sibling under `parent`.
fn filler(core: &mut Core, parent: NodeId) -> Result<NodeId> {
    let node = core.create_detached(LayoutWidget(Layout::fill()))?;
    core.attach(parent, node)?;
    Ok(node)
}

#[test]
fn a_width_fraction_resolves_once_against_the_row_less_its_gaps() -> Result<()> {
    let mut core = Core::new();
    let row = gapped_row(&mut core)?;
    let (capped, seen) = capped_child(&mut core, row, Layout::column())?;
    let rest = filler(&mut core, row)?;
    core.update_layout(Size::new(31, 4))?;

    // Thirty-one columns less one gap leave a budget of thirty.
    assert_eq!(core.nodes[capped].rect.w, 10);
    assert_eq!(core.nodes[rest].rect.w, 20);
    let widths = seen
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.width)
        .collect::<Vec<_>>();
    assert!(!widths.is_empty());
    assert!(
        widths
            .iter()
            .all(|width| matches!(width, Constraint::AtMost(10) | Constraint::Exact(10))),
        "every measurement keeps the parent-relative cap: {widths:?}"
    );
    Ok(())
}

#[test]
fn width_fractions_join_absolute_bounds_and_saturate_on_tiny_budgets() -> Result<()> {
    for (screen, bounds, expected) in [
        (31, Layout::column().min_width(12), 12),
        (31, Layout::column().max_width(5), 5),
        (2, Layout::column(), 0),
    ] {
        let mut core = Core::new();
        let row = gapped_row(&mut core)?;
        let (capped, _) = capped_child(&mut core, row, bounds)?;
        filler(&mut core, row)?;
        core.update_layout(Size::new(screen, 4))?;
        assert_eq!(
            core.nodes[capped].rect.w, expected,
            "{bounds:?} at {screen}"
        );
    }
    Ok(())
}

#[test]
fn the_root_resolves_a_width_fraction_against_the_screen() -> Result<()> {
    let mut core = Core::new();
    let root = core.root;
    core.set_layout_of(root, Layout::fill().max_width_fraction(Fraction::new(1, 2)))?;
    core.update_layout(Size::new(41, 4))?;
    assert_eq!(core.nodes[root].rect.w, 20);
    Ok(())
}

#[test]
fn unbounded_measurement_reports_natural_width_before_the_fraction_applies() -> Result<()> {
    let mut core = Core::new();
    let shrink = core.create_detached(LayoutWidget(
        Layout::row()
            .height(Sizing::Flex(1))
            .overflow_x(MeasureOverflow::Unbounded),
    ))?;
    attach_root_child(&mut core, shrink)?;
    let (capped, seen) = capped_child(&mut core, shrink, Layout::column())?;
    core.update_layout(Size::new(80, 4))?;

    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .any(|c| c.width == Constraint::Unbounded),
        "the row measures its child without a width bound"
    );
    assert_eq!(
        core.nodes[shrink].rect.w, 50,
        "the row takes the natural width"
    );
    assert_eq!(
        core.nodes[capped].rect.w, 16,
        "the fraction applies to the row's finite width"
    );
    Ok(())
}
