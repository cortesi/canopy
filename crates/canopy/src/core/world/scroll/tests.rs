//! Scroll, reveal ordering, and default action tests.

use super::{super::test_support::*, *};
use crate::{
    Context, ModalBindings, ModalOptions, ViewContext,
    core::context::CoreContext,
    geom::Size,
    layout::{CanvasContext, Direction, Edges, Layout, Measurement},
    widget::Widget,
};

/// Screen size shared by most reveal tests: a four-row viewport.
const SCREEN: Size = Size::new(10, 4);

/// A focusable leaf with a fixed outer size.
struct Leaf(Size);

impl Widget for Leaf {
    fn layout(&self) -> Layout {
        Layout::column()
            .fixed_width(self.0.w)
            .fixed_height(self.0.h)
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }
}

/// A viewport whose canvas spans its children, with an optional anchor.
struct Viewport {
    /// Layout of the viewport.
    layout: Layout,
    /// Rectangle the reveal anchor hook reports.
    anchor: Option<Rect>,
}

impl Widget for Viewport {
    fn layout(&self) -> Layout {
        self.layout
    }

    fn canvas(&self, view: Size, ctx: &CanvasContext) -> Size {
        let extent = ctx.children_extent();
        Size::new(extent.w.max(view.w), extent.h.max(view.h))
    }

    fn reveal_anchor(&self, _view: Size) -> Option<Rect> {
        self.anchor
    }
}

/// Attach a fill-sized viewport under the root.
fn root_viewport(core: &mut Core, anchor: Option<Rect>) -> Result<NodeId> {
    let viewport = core.create_detached(Viewport {
        layout: Layout::fill(),
        anchor,
    })?;
    attach_root_child(core, viewport)?;
    Ok(viewport)
}

/// Attach a widget under `parent`.
fn add(core: &mut Core, parent: NodeId, widget: impl Widget + 'static) -> Result<NodeId> {
    let node = core.create_detached(widget)?;
    core.attach(parent, node)?;
    Ok(node)
}

/// Attach a one-cell leaf under `parent`.
fn cell(core: &mut Core, parent: NodeId) -> Result<NodeId> {
    add(core, parent, Leaf(Size::new(1, 1)))
}

/// Attach an unfocusable one-column block of `rows` rows under `parent`.
fn spacer(core: &mut Core, parent: NodeId, rows: u32) -> Result<NodeId> {
    add(
        core,
        parent,
        LayoutWidget(Layout::column().fixed_width(1).fixed_height(rows)),
    )
}

/// Return a node's vertical scroll offset.
fn offset(core: &Core, node: NodeId) -> u32 {
    core.nodes[node].scroll.y
}

/// Attach a fill-sized surface whose canvas is 100 cells on each axis.
fn scroll_surface(core: &mut Core) -> Result<NodeId> {
    let (widget, _) =
        TestWidget::with_canvas(|_c| Measurement::Wrap, |_view, _ctx| Size::new(100, 100));
    let child = core.create_detached(widget)?;
    attach_root_child(core, child)?;
    core.set_layout_of(child, Layout::fill())?;
    Ok(child)
}

#[test]
fn default_scrolling_moves_each_axis_and_stops_at_the_canvas_edges() -> Result<()> {
    let mut core = Core::new();
    let child = scroll_surface(&mut core)?;
    core.update_layout(SCREEN)?;
    let step = |core: &mut Core, x, y| {
        core.apply_default_action(child, DefaultAction::Scroll(PointI32 { x, y }))
    };

    assert!(!step(&mut core, 0, -3), "the top edge cannot move up");
    assert!(!step(&mut core, -3, 0), "the left edge cannot move left");
    assert!(step(&mut core, 0, 3));
    assert!(step(&mut core, 3, 0));
    assert_eq!(core.nodes[child].scroll, Point { x: 3, y: 3 });
    assert!(step(&mut core, 0, i32::MAX));
    assert_eq!(core.nodes[child].scroll, Point { x: 3, y: 96 });
    assert!(!step(&mut core, 0, 3), "the bottom edge cannot move down");
    assert!(step(&mut core, i32::MIN, 0));
    assert_eq!(core.nodes[child].scroll, Point { x: 0, y: 96 });
    Ok(())
}

#[test]
fn an_immovable_default_step_preserves_a_pending_reveal() -> Result<()> {
    let mut core = Core::new();
    let child = scroll_surface(&mut core)?;
    core.update_layout(SCREEN)?;
    let nearest = RevealAlign::Nearest;

    CoreContext::new(&mut core, child).reveal_area(Rect::new(0, 40, 1, 1), nearest);
    assert!(!core.apply_default_action(child, DefaultAction::Scroll(PointI32 { x: 0, y: -3 })));
    core.update_layout(SCREEN)?;
    assert_eq!(core.nodes[child].scroll, Point { x: 0, y: 37 });

    CoreContext::new(&mut core, child).reveal_area(Rect::new(0, 80, 1, 1), nearest);
    assert!(core.apply_default_action(child, DefaultAction::Scroll(PointI32 { x: 0, y: 3 })));
    core.update_layout(SCREEN)?;
    assert_eq!(
        core.nodes[child].scroll,
        Point { x: 0, y: 40 },
        "a step that moves supersedes the older reveal"
    );
    Ok(())
}

#[test]
fn scrolling_another_node_rejects_missing_and_detached_targets() -> Result<()> {
    let mut core = Core::new();
    let child = scroll_surface(&mut core)?;
    let detached = core.create_detached(simple_widget())?;
    core.update_layout(SCREEN)?;
    let root = core.root;
    let mut ctx = CoreContext::new(&mut core, root);

    assert!(ctx.scroll_to_of(child, 5, 200)?.changed());
    assert_eq!(
        ctx.view_of(child).map(|view| view.scroll),
        Some(Point { x: 5, y: 96 })
    );
    assert!(!ctx.scroll_to_of(child, 5, 96)?.changed());
    assert!(matches!(
        ctx.scroll_to_of(detached, 1, 1),
        Err(Error::NodeDetached(_))
    ));
    core.remove_subtree(detached)?;
    assert!(matches!(
        CoreContext::new(&mut core, root).scroll_to_of(detached, 1, 1),
        Err(Error::NodeNotFound(_))
    ));
    assert!(matches!(
        core.reveal_node(detached, RevealAlign::Nearest),
        Err(Error::NodeNotFound(_))
    ));
    Ok(())
}

#[test]
fn reveal_offsets_take_the_nearest_edge_or_center_without_oscillation() {
    use RevealAlign::{Center, Nearest};
    for (offset, view, start, length, align, expected) in [
        (0, 5, 10, 2, Nearest, 7),
        (10, 5, 3, 2, Nearest, 3),
        (4, 5, 6, 2, Nearest, 4),
        (0, 5, 10, 20, Nearest, 10),
        (40, 5, 10, 20, Nearest, 25),
        (15, 5, 10, 20, Nearest, 15),
        (0, u32::MAX, 0, u32::MAX, Nearest, 0),
        (0, 10, u32::MAX - 1, 10, Nearest, u32::MAX - 1),
        (0, 10, 20, 2, Center, 16),
        (0, 10, 3, 2, Center, 0),
        (40, 5, 10, 20, Center, 25),
        (0, 4, u32::MAX - 1, 1, Center, u32::MAX - 3),
    ] {
        let revealed = reveal_offset(offset, view, start, length, align);
        assert_eq!(
            revealed, expected,
            "{offset} {view} {start} {length} {align:?}"
        );
        assert_eq!(
            reveal_offset(revealed, view, start, length, align),
            revealed
        );
    }
}

#[test]
fn area_anchor_and_node_requests_apply_in_call_order() -> Result<()> {
    let mut core = Core::new();
    let viewport = root_viewport(&mut core, Some(Rect::new(0, 30, 1, 1)))?;
    spacer(&mut core, viewport, 50)?;
    let target = cell(&mut core, viewport)?;
    core.update_layout(SCREEN)?;
    let nearest = RevealAlign::Nearest;

    core.reveal_node(target, nearest)?;
    CoreContext::new(&mut core, viewport).reveal_area(Rect::new(0, 20, 1, 1), nearest);
    core.update_layout(SCREEN)?;
    assert_eq!(offset(&core, viewport), 20, "the later area wins");

    CoreContext::new(&mut core, viewport).reveal_area(Rect::new(0, 20, 1, 1), nearest);
    CoreContext::new(&mut core, viewport).reveal_anchor(nearest);
    core.update_layout(SCREEN)?;
    assert_eq!(offset(&core, viewport), 27, "the anchor replaces the area");

    CoreContext::new(&mut core, viewport).reveal_anchor(nearest);
    core.reveal_node(target, nearest)?;
    core.update_layout(SCREEN)?;
    assert_eq!(offset(&core, viewport), 47, "the later node request wins");

    core.reveal_node(target, nearest)?;
    CoreContext::new(&mut core, viewport).reveal_anchor(nearest);
    core.update_layout(SCREEN)?;
    assert_eq!(offset(&core, viewport), 30, "the later anchor wins");
    Ok(())
}

#[test]
fn the_last_focus_change_wins_against_tree_order() -> Result<()> {
    let mut core = Core::new();
    let viewport = root_viewport(&mut core, None)?;
    let first = cell(&mut core, viewport)?;
    spacer(&mut core, viewport, 40)?;
    let last = cell(&mut core, viewport)?;
    core.update_layout(SCREEN)?;

    core.set_focus(first)?;
    CoreContext::new(&mut core, viewport).scroll_to(0, 20);
    core.set_focus(last)?;
    core.set_focus(first)?;
    core.update_layout(SCREEN)?;
    assert_eq!(offset(&core, viewport), 0);

    core.set_focus(last)?;
    core.update_layout(SCREEN)?;
    assert_eq!(offset(&core, viewport), 38);
    Ok(())
}

#[test]
fn independent_viewports_keep_their_own_reveals() -> Result<()> {
    let mut core = Core::new();
    let row = core.create_detached(LayoutWidget(Layout::fill().direction(Direction::Row)))?;
    attach_root_child(&mut core, row)?;
    let mut targets = Vec::new();
    let mut viewports = Vec::new();
    for _ in 0..2 {
        let viewport = add(
            &mut core,
            row,
            Viewport {
                layout: Layout::fill(),
                anchor: None,
            },
        )?;
        spacer(&mut core, viewport, 40)?;
        targets.push(cell(&mut core, viewport)?);
        viewports.push(viewport);
    }
    core.update_layout(SCREEN)?;
    for target in &targets {
        core.reveal_node(*target, RevealAlign::Nearest)?;
    }
    core.update_layout(SCREEN)?;
    assert_eq!(offset(&core, viewports[0]), 37);
    assert_eq!(offset(&core, viewports[1]), 37);
    Ok(())
}

#[test]
fn an_explicit_scroll_that_cannot_move_still_supersedes_older_reveals() -> Result<()> {
    let mut core = Core::new();
    let viewport = root_viewport(&mut core, None)?;
    spacer(&mut core, viewport, 40)?;
    let target = cell(&mut core, viewport)?;
    core.update_layout(SCREEN)?;

    core.reveal_node(target, RevealAlign::Nearest)?;
    assert!(
        !CoreContext::new(&mut core, viewport)
            .scroll_to(0, 0)
            .changed(),
        "an ordering stamp alone is not a change"
    );
    core.update_layout(SCREEN)?;
    assert_eq!(offset(&core, viewport), 0);

    core.reveal_node(target, RevealAlign::Nearest)?;
    core.update_layout(SCREEN)?;
    assert_eq!(offset(&core, viewport), 37, "a later reveal wins again");
    Ok(())
}

#[test]
fn a_retained_request_yields_to_newer_intent_when_its_node_returns() -> Result<()> {
    let mut core = Core::new();
    let viewport = root_viewport(&mut core, None)?;
    spacer(&mut core, viewport, 40)?;
    let target = cell(&mut core, viewport)?;
    core.update_layout(SCREEN)?;

    core.set_hidden(target, true)?;
    core.reveal_node(target, RevealAlign::Nearest)?;
    core.update_layout(SCREEN)?;
    assert!(core.nodes[target].reveal_in_ancestors.is_some());

    CoreContext::new(&mut core, viewport).scroll_to(0, 5);
    core.update_layout(SCREEN)?;
    core.set_hidden(target, false)?;
    core.update_layout(SCREEN)?;
    assert_eq!(offset(&core, viewport), 5);
    assert!(core.nodes[target].reveal_in_ancestors.is_none());
    Ok(())
}

#[test]
fn structural_rollback_restores_a_request_with_its_node() -> Result<()> {
    let mut core = Core::new();
    let viewport = root_viewport(&mut core, None)?;
    spacer(&mut core, viewport, 40)?;
    core.update_layout(SCREEN)?;

    let mut ctx = CoreContext::new(&mut core, viewport);
    ctx.reveal_area(Rect::new(0, 30, 1, 1), RevealAlign::Nearest);
    let failed = ctx.edit_structure(&mut |ctx| {
        ctx.scroll_to(0, 2);
        Err(Error::Internal("abandon the edit".into()))
    });
    assert!(failed.is_err());
    core.update_layout(SCREEN)?;
    assert_eq!(offset(&core, viewport), 27);
    Ok(())
}

#[test]
fn requests_queued_before_the_first_layout_wait_for_geometry() -> Result<()> {
    let mut core = Core::new();
    let viewport = root_viewport(&mut core, None)?;
    spacer(&mut core, viewport, 40)?;
    let target = cell(&mut core, viewport)?;
    spacer(&mut core, viewport, 40)?;
    core.reveal_node(target, RevealAlign::Center)?;
    core.update_layout(SCREEN)?;
    assert_eq!(offset(&core, viewport), 38, "row 40 lands on view row 2");
    Ok(())
}

#[test]
fn node_reveals_pass_through_padded_nested_viewports() -> Result<()> {
    let mut core = Core::new();
    let outer = core.create_detached(Viewport {
        layout: Layout::fill().padding(Edges::all(1)),
        anchor: None,
    })?;
    attach_root_child(&mut core, outer)?;
    spacer(&mut core, outer, 30)?;
    let inner = add(
        &mut core,
        outer,
        Viewport {
            layout: Layout::column()
                .fixed_width(10)
                .fixed_height(5)
                .padding(Edges::all(1)),
            anchor: None,
        },
    )?;
    spacer(&mut core, inner, 40)?;
    let target = cell(&mut core, inner)?;
    core.update_layout(Size::new(20, 10))?;

    core.reveal_node(target, RevealAlign::Nearest)?;
    core.update_layout(Size::new(20, 10))?;
    // The inner view shows three rows. The target then sits at row 33 of the
    // outer canvas, which shows eight rows.
    assert_eq!(offset(&core, inner), 38);
    assert_eq!(offset(&core, outer), 26);
    assert_eq!(core.nodes[target].view.content.tl.y, 8);
    Ok(())
}

#[test]
fn the_active_modal_region_bounds_node_reveals() -> Result<()> {
    let mut core = Core::new();
    let outside_view = root_viewport(&mut core, None)?;
    spacer(&mut core, outside_view, 30)?;
    let region = add(
        &mut core,
        outside_view,
        Viewport {
            layout: Layout::column().fixed_width(5).fixed_height(3),
            anchor: None,
        },
    )?;
    spacer(&mut core, region, 20)?;
    let target = cell(&mut core, region)?;
    let outside = cell(&mut core, outside_view)?;
    core.update_layout(Size::new(20, 10))?;

    core.open_modal(ModalOptions {
        owner: outside_view,
        modal: region,
        initial_focus: target,
        dim_target: None,
        bindings: ModalBindings::Application,
    })?;
    core.reveal_node(outside, RevealAlign::Nearest)?;
    core.update_layout(Size::new(20, 10))?;
    assert_eq!(offset(&core, region), 18, "focus reveals inside the region");
    assert_eq!(
        offset(&core, outside_view),
        0,
        "the walk stops at the region"
    );
    assert!(
        core.nodes[outside].reveal_in_ancestors.is_some(),
        "a node outside the region keeps its request"
    );
    Ok(())
}

#[test]
fn layout_time_focus_recovery_reveals_in_the_same_frame() -> Result<()> {
    let mut core = Core::new();
    let viewport = root_viewport(&mut core, None)?;
    let first = cell(&mut core, viewport)?;
    spacer(&mut core, viewport, 40)?;
    let last = cell(&mut core, viewport)?;
    core.update_layout(SCREEN)?;
    core.set_focus(first)?;
    core.update_layout(SCREEN)?;

    core.set_hidden(first, true)?;
    core.update_layout(SCREEN)?;
    assert_eq!(core.focus, Some(last));
    assert_eq!(offset(&core, viewport), 37);
    assert_eq!(core.nodes[last].view.content.tl.y, 3);
    Ok(())
}
