//! Scroll target resolution and frame scrollbar tests.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use canopy::{
    Context, ContextExt, EventOutcome, Loader, NodeId, NodeName, ViewContext, Widget,
    error::Result,
    event::{Event, key, mouse},
    geom::{Point, PointI32, Rect, Size},
    layout::{
        CanvasContext, Direction, Edges, Layout, LayoutOverride, MeasureConstraints, Measurement,
    },
    style::Color,
    testing::harness::Harness,
};

use crate::{
    Columns, Frame, List, Scroll, Selectable, Tabs,
    scrollbar::{Axis, ScrollTarget, scroll_target},
};

/// Builds a subtree under the scene root and returns the nodes a test uses.
type Build = Box<dyn FnOnce(&mut dyn Context) -> Result<Vec<NodeId>>>;

/// Root that builds a test subtree when mounted.
struct Scene {
    /// Subtree builder, consumed on mount.
    build: Option<Build>,
    /// Nodes the builder returned.
    nodes: Rc<RefCell<Vec<NodeId>>>,
}

impl Widget for Scene {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        if let Some(build) = self.build.take() {
            *self.nodes.borrow_mut() = build(c)?;
        }
        Ok(())
    }
}

impl Loader for Scene {}

/// A leaf with a fixed canvas.
struct Surface {
    /// Canvas size in content cells.
    canvas: Size,
}

impl Widget for Surface {
    fn canvas(&self, _view: Size, _ctx: &CanvasContext) -> Size {
        self.canvas
    }

    fn name(&self) -> NodeName {
        NodeName::convert("surface")
    }
}

/// A container that supplies only a layout.
struct Boxed(Layout);

impl Widget for Boxed {
    fn layout(&self) -> Layout {
        self.0
    }

    fn name(&self) -> NodeName {
        NodeName::convert("boxed")
    }
}

/// Render a scene of the given size and return the nodes its builder chose.
fn scene(
    width: u32,
    height: u32,
    build: impl FnOnce(&mut dyn Context) -> Result<Vec<NodeId>> + 'static,
) -> Result<(Harness, Vec<NodeId>)> {
    let nodes = Rc::new(RefCell::new(Vec::new()));
    let root = Scene {
        build: Some(Box::new(build)),
        nodes: Rc::clone(&nodes),
    };
    let mut harness = Harness::builder(root).size(width, height).build()?;
    harness
        .canopy
        .style_mut()
        .rules()
        .fg("frame/thumb", Color::Red)
        .fg("frame/thumb/active", Color::Green)
        .apply();
    harness.render()?;
    let nodes = nodes.borrow().clone();
    Ok((harness, nodes))
}

/// Add a fill-sized surface with `canvas` under `parent`.
fn surface(c: &mut dyn Context, parent: NodeId, canvas: Size) -> Result<NodeId> {
    let node = c.add_child_to(parent, Surface { canvas })?;
    c.set_layout_of(node, Layout::fill())?;
    Ok(node.into())
}

/// Add a layout-only container under `parent`.
fn boxed(c: &mut dyn Context, parent: NodeId, layout: Layout) -> Result<NodeId> {
    Ok(c.add_child_to(parent, Boxed(layout))?.into())
}

/// Add a frame holding a fill-sized surface, returning both nodes.
fn framed_surface(c: &mut dyn Context, canvas: Size) -> Result<Vec<NodeId>> {
    let root = c.node_id();
    let frame: NodeId = c.add_child_to(root, Frame::new())?.into();
    let body = surface(c, frame, canvas)?;
    Ok(vec![frame, body])
}

/// Resolve the target beneath `owner`.
fn target(harness: &Harness, owner: NodeId, axis: Axis) -> Option<ScrollTarget> {
    harness
        .canopy
        .with_root_view(|ctx| scroll_target(ctx, owner, axis))
        .expect("targets resolve")
}

/// Return a node's scroll offset.
fn scroll(harness: &Harness, node: NodeId) -> Point {
    harness
        .canopy
        .with_root_view(|ctx| ctx.view_of(node).expect("live node").scroll)
}

/// Return whether `node` holds mouse capture.
fn captured(harness: &mut Harness, node: NodeId) -> Result<bool> {
    harness
        .canopy
        .with_context(node, |c| Ok(c.has_mouse_capture()))
}

/// Return the rows of column `x`, below `height`, that show `glyph`.
fn rows_with(harness: &Harness, x: u32, height: u32, glyph: char) -> Vec<u32> {
    (0..height)
        .filter(|&y| {
            harness
                .buf()
                .get(Point { x, y })
                .is_some_and(|cell| cell.ch == glyph)
        })
        .collect()
}

/// Return the columns of row `y`, below `width`, that show `glyph`.
fn columns_with(harness: &Harness, y: u32, width: u32, glyph: char) -> Vec<u32> {
    (0..width)
        .filter(|&x| {
            harness
                .buf()
                .get(Point { x, y })
                .is_some_and(|cell| cell.ch == glyph)
        })
        .collect()
}

/// Return the foreground color drawn at a cell.
fn color_at(harness: &Harness, x: u32, y: u32) -> Option<Color> {
    harness.buf().get(Point { x, y }).map(|cell| cell.style.fg)
}

/// Build a mouse event at a screen location.
fn pointer(action: mouse::Action, x: i32, y: i32) -> mouse::MouseEvent {
    let button = match action {
        mouse::Action::Down | mouse::Action::Up | mouse::Action::Drag => mouse::Button::Left,
        _ => mouse::Button::None,
    };
    mouse::MouseEvent {
        action,
        button,
        modifiers: key::Empty,
        location: PointI32 { x, y },
    }
}

#[test]
fn the_target_is_the_one_overflowing_node_beneath_wrappers() -> Result<()> {
    let (harness, nodes) = scene(20, 10, |c| {
        let root = c.node_id();
        let owner = boxed(c, root, Layout::fill())?;
        let wrapper = boxed(c, owner, Layout::fill())?;
        let inner = surface(c, wrapper, Size::new(5, 50))?;
        Ok(vec![owner, inner])
    })?;
    let (owner, inner) = (nodes[0], nodes[1]);
    assert_eq!(
        target(&harness, owner, Axis::Vertical),
        Some(ScrollTarget {
            node: inner,
            viewport: Rect::new(0, 0, 20, 10),
        })
    );
    assert_eq!(target(&harness, owner, Axis::Horizontal), None);
    Ok(())
}

#[test]
fn sibling_targets_are_ambiguous_at_every_enclosing_level() -> Result<()> {
    let (mut harness, nodes) = scene(20, 10, |c| {
        let root = c.node_id();
        let owner = boxed(c, root, Layout::fill())?;
        let pair = boxed(c, owner, Layout::fill())?;
        let first = surface(c, pair, Size::new(1, 50))?;
        let second = surface(c, pair, Size::new(1, 50))?;
        surface(c, owner, Size::new(1, 1))?;
        Ok(vec![owner, first, second])
    })?;
    let (owner, first, second) = (nodes[0], nodes[1], nodes[2]);
    assert_eq!(target(&harness, owner, Axis::Vertical), None);

    harness
        .canopy
        .with_root_context(|c| c.set_hidden_of(second, true).map(|_| ()))?;
    harness.render()?;
    assert_eq!(
        target(&harness, owner, Axis::Vertical).map(|found| found.node),
        Some(first)
    );
    Ok(())
}

#[test]
fn only_the_visible_tab_page_is_a_target() -> Result<()> {
    let (mut harness, nodes) = scene(20, 10, |c| {
        let root = c.node_id();
        let owner = boxed(c, root, Layout::fill())?;
        let tabs = c.add_child_to(owner, Tabs::new())?;
        let pages = c.with_widget_mut(tabs, |tabs: &mut Tabs, c| {
            let canvas = Size::new(1, 50);
            let first = tabs.add_tab(c, "One", Surface { canvas })?;
            let second = tabs.add_tab(c, "Two", Surface { canvas })?;
            Ok([NodeId::from(first), NodeId::from(second)])
        })?;
        Ok(vec![owner, tabs.into(), pages[0], pages[1]])
    })?;
    let (owner, tabs, first, second) = (nodes[0], nodes[1], nodes[2], nodes[3]);
    let found = target(&harness, owner, Axis::Vertical).expect("the first page");
    assert_eq!(found.node, first);
    assert_eq!(
        found.viewport.tl.y, 1,
        "the tab bar is outside the viewport"
    );

    harness
        .canopy
        .with_root_context(|c| c.with_widget_mut(tabs, |tabs: &mut Tabs, c| tabs.select(c, 1)))?;
    harness.render()?;
    assert_eq!(
        target(&harness, owner, Axis::Vertical).map(|found| found.node),
        Some(second)
    );
    Ok(())
}

#[test]
fn a_nested_owner_hides_its_subtree_from_enclosing_owners() -> Result<()> {
    let (harness, nodes) = scene(20, 10, |c| {
        let root = c.node_id();
        let owner = boxed(c, root, Layout::fill())?;
        let frame: NodeId = c.add_child_to(owner, Frame::new())?.into();
        let inner = surface(c, frame, Size::new(1, 50))?;
        let other = surface(c, owner, Size::new(1, 50))?;
        Ok(vec![owner, frame, inner, other])
    })?;
    let (owner, frame, inner, other) = (nodes[0], nodes[1], nodes[2], nodes[3]);
    assert_eq!(
        target(&harness, owner, Axis::Vertical).map(|found| found.node),
        Some(other)
    );
    assert_eq!(
        target(&harness, frame, Axis::Vertical).map(|found| found.node),
        Some(inner)
    );
    Ok(())
}

#[test]
fn a_detached_owner_resolves_no_target_from_cached_views() -> Result<()> {
    let (mut harness, nodes) = scene(20, 10, |c| {
        let root = c.node_id();
        let owner = boxed(c, root, Layout::fill())?;
        surface(c, owner, Size::new(1, 50))?;
        Ok(vec![owner])
    })?;
    let owner = nodes[0];
    assert!(target(&harness, owner, Axis::Vertical).is_some());

    harness.canopy.with_root_context(|c| c.detach(owner))?;
    let cached = harness
        .canopy
        .with_root_view(|ctx| ctx.view_of(owner).expect("detached node"));
    assert!(
        !cached.is_empty(),
        "the detached view keeps its last layout"
    );
    assert_eq!(target(&harness, owner, Axis::Vertical), None);
    Ok(())
}

/// Add a 30 by 30 cell surface with a 60 by 60 cell canvas under `parent`.
fn oversized_surface(c: &mut dyn Context, parent: NodeId) -> Result<NodeId> {
    let node = c.add_child_to(
        parent,
        Surface {
            canvas: Size::new(60, 60),
        },
    )?;
    c.set_layout_of(node, Layout::column().fixed_width(30).fixed_height(30))?;
    Ok(node.into())
}

#[test]
fn viewports_are_clipped_by_every_ancestor_and_the_screen() -> Result<()> {
    let (harness, nodes) = scene(20, 10, |c| {
        let root = c.node_id();
        let owner = boxed(c, root, Layout::fill())?;
        let clip = boxed(
            c,
            owner,
            Layout::column()
                .fixed_width(12)
                .fixed_height(5)
                .padding(Edges::all(1)),
        )?;
        oversized_surface(c, clip)?;
        Ok(vec![clip])
    })?;
    assert_eq!(
        target(&harness, nodes[0], Axis::Vertical).map(|found| found.viewport),
        Some(Rect::new(1, 1, 10, 3)),
        "the clip box's content bounds its child"
    );

    let (harness, nodes) = scene(20, 10, |c| {
        let root = c.node_id();
        let owner = boxed(c, root, Layout::fill())?;
        oversized_surface(c, owner)?;
        Ok(vec![owner])
    })?;
    assert_eq!(
        target(&harness, nodes[0], Axis::Horizontal).map(|found| found.viewport),
        Some(Rect::new(0, 0, 20, 10)),
        "the screen bounds a surface larger than itself"
    );
    Ok(())
}

#[test]
fn a_frame_track_covers_only_the_rows_beside_its_target() -> Result<()> {
    let (mut harness, nodes) = scene(20, 10, |c| {
        let root = c.node_id();
        let frame: NodeId = c.add_child_to(root, Frame::new())?.into();
        let column = boxed(c, frame, Layout::fill())?;
        let bar = Layout::column().flex_horizontal(1).fixed_height(1);
        boxed(c, column, bar)?;
        let body = surface(c, column, Size::new(1, 100))?;
        boxed(c, column, bar)?;
        Ok(vec![body])
    })?;
    let body = nodes[0];
    // The header takes row 1, the body rows 2 through 7, and the footer row 8.
    assert_eq!(rows_with(&harness, 19, 10, '█'), [2]);

    harness
        .canopy
        .with_root_context(|c| c.scroll_to_of(body, 0, u32::MAX).map(|_| ()))?;
    harness.render()?;
    assert_eq!(rows_with(&harness, 19, 10, '█'), [7]);
    Ok(())
}

#[test]
fn a_sidebar_beside_the_target_removes_only_the_vertical_track() -> Result<()> {
    let (harness, _) = scene(20, 10, |c| {
        let root = c.node_id();
        let frame: NodeId = c.add_child_to(root, Frame::new())?.into();
        let row = boxed(c, frame, Layout::fill().direction(Direction::Row))?;
        surface(c, row, Size::new(100, 100))?;
        boxed(c, row, Layout::column().fixed_width(3).flex_vertical(1))?;
        Ok(Vec::new())
    })?;
    assert_eq!(rows_with(&harness, 19, 10, '█'), Vec::<u32>::new());
    let thumb = columns_with(&harness, 9, 20, '▄');
    assert!(
        !thumb.is_empty(),
        "the body still reaches the bottom border"
    );
    assert!(
        thumb.iter().all(|x| (1..=15).contains(x)),
        "the bottom track stays beside the body, got {thumb:?}"
    );
    Ok(())
}

#[test]
fn wheel_input_scrolls_the_target_only_from_its_track() -> Result<()> {
    let (mut harness, nodes) = scene(20, 10, |c| framed_surface(c, Size::new(1, 100)))?;
    let body = nodes[1];

    harness.mouse(pointer(mouse::Action::ScrollDown, 19, 4))?;
    assert_eq!(scroll(&harness, body), Point { x: 0, y: 3 });
    for (x, y) in [(10, 0), (19, 0), (19, 9)] {
        harness.mouse(pointer(mouse::Action::ScrollDown, x, y))?;
        assert_eq!(
            scroll(&harness, body),
            Point { x: 0, y: 3 },
            "the title row and corners are not track cells: ({x}, {y})"
        );
    }
    harness.mouse(pointer(mouse::Action::ScrollUp, 19, 4))?;
    assert_eq!(scroll(&harness, body), Point::ZERO);
    Ok(())
}

#[test]
fn dragging_a_thumb_scrolls_its_target_and_releases_capture() -> Result<()> {
    let (mut harness, nodes) = scene(20, 10, |c| framed_surface(c, Size::new(1, 64)))?;
    let (frame, body) = (nodes[0], nodes[1]);
    assert_eq!(color_at(&harness, 19, 1), Some(Color::Red));

    // A press below the thumb moves the thumb under the pointer.
    harness.mouse(pointer(mouse::Action::Down, 19, 8))?;
    assert_eq!(scroll(&harness, body), Point { x: 0, y: 56 });
    assert!(captured(&mut harness, frame)?);
    assert_eq!(color_at(&harness, 19, 8), Some(Color::Green));

    harness.mouse(pointer(mouse::Action::Drag, 19, 1))?;
    assert_eq!(scroll(&harness, body), Point::ZERO);
    harness.mouse(pointer(mouse::Action::Drag, 30, 50))?;
    assert_eq!(scroll(&harness, body), Point { x: 0, y: 56 });

    harness.mouse(pointer(mouse::Action::Up, 30, 50))?;
    assert!(!captured(&mut harness, frame)?);
    assert_eq!(color_at(&harness, 19, 8), Some(Color::Red));
    Ok(())
}

#[test]
fn a_thumb_without_travel_is_drawn_but_never_dragged() -> Result<()> {
    // Two extra rows over a one-row track leave the thumb nowhere to go.
    let (mut harness, nodes) = scene(20, 3, |c| framed_surface(c, Size::new(1, 3)))?;
    let (frame, body) = (nodes[0], nodes[1]);
    assert_eq!(rows_with(&harness, 19, 3, '█'), [1]);
    harness.mouse(pointer(mouse::Action::Down, 19, 1))?;
    assert!(!captured(&mut harness, frame)?);
    assert_eq!(scroll(&harness, body), Point::ZERO);
    harness.mouse(pointer(mouse::Action::ScrollDown, 19, 1))?;
    assert_eq!(
        scroll(&harness, body),
        Point { x: 0, y: 2 },
        "the wheel still reaches the end"
    );
    Ok(())
}

#[test]
fn another_capture_owner_ends_a_drag_and_keeps_its_capture() -> Result<()> {
    let (mut harness, nodes) = scene(20, 10, |c| framed_surface(c, Size::new(1, 64)))?;
    let (frame, body) = (nodes[0], nodes[1]);
    harness.mouse(pointer(mouse::Action::Down, 19, 8))?;
    assert!(captured(&mut harness, frame)?);

    harness
        .canopy
        .with_context(body, |c| c.capture_mouse().map(|_| ()))?;
    let outcome = harness.canopy.with_context(frame, |c| {
        c.with_widget_mut(frame, |frame: &mut Frame, c| {
            frame.on_event(&Event::Mouse(pointer(mouse::Action::Drag, 19, 1)), c)
        })
    })?;
    assert_eq!(outcome, EventOutcome::Ignore);
    assert!(captured(&mut harness, body)?, "the new owner keeps capture");
    assert_eq!(scroll(&harness, body), Point { x: 0, y: 56 });

    harness.render()?;
    assert_eq!(color_at(&harness, 19, 8), Some(Color::Red));
    Ok(())
}

#[test]
fn a_drag_follows_range_changes_and_ends_when_its_target_leaves() -> Result<()> {
    let (mut harness, nodes) = scene(20, 10, |c| framed_surface(c, Size::new(1, 64)))?;
    let (frame, body) = (nodes[0], nodes[1]);
    harness.mouse(pointer(mouse::Action::Down, 19, 8))?;

    harness.with_widget(body, |surface: &mut Surface| {
        surface.canvas = Size::new(1, 128);
    });
    harness.canopy.with_context(body, |c| {
        c.invalidate_layout();
        Ok(())
    })?;
    harness.render()?;
    harness.mouse(pointer(mouse::Action::Drag, 19, 8))?;
    assert_eq!(scroll(&harness, body), Point { x: 0, y: 120 });
    assert!(captured(&mut harness, frame)?);

    harness
        .canopy
        .with_root_context(|c| c.remove_subtree(body))?;
    harness.render()?;
    harness.mouse(pointer(mouse::Action::Drag, 19, 4))?;
    assert!(!captured(&mut harness, frame)?);
    Ok(())
}

#[test]
fn rendering_a_stale_drag_keeps_capture_until_the_next_event() -> Result<()> {
    let (mut harness, nodes) = scene(20, 10, |c| framed_surface(c, Size::new(1, 64)))?;
    let (frame, body) = (nodes[0], nodes[1]);
    harness.mouse(pointer(mouse::Action::Down, 19, 8))?;

    harness
        .canopy
        .with_root_context(|c| c.set_hidden_of(body, true).map(|_| ()))?;
    harness.render()?;
    assert!(rows_with(&harness, 19, 10, '█').is_empty());
    assert!(
        captured(&mut harness, frame)?,
        "rendering cannot release capture"
    );

    // Hiding a node clears its scroll offset, so the thumb returns at the top.
    harness
        .canopy
        .with_root_context(|c| c.set_hidden_of(body, false).map(|_| ()))?;
    harness.render()?;
    assert_eq!(
        color_at(&harness, 19, 1),
        Some(Color::Red),
        "a cancelled drag never returns to its thumb"
    );
    harness.mouse(pointer(mouse::Action::Drag, 19, 5))?;
    assert!(!captured(&mut harness, frame)?);
    assert_eq!(
        scroll(&harness, body),
        Point::ZERO,
        "the event that ends a drag does not scroll"
    );
    Ok(())
}

/// A focusable list row whose height the test shares and changes.
struct Tall(Rc<Cell<u32>>);

impl Selectable for Tall {
    fn set_selected(&mut self, _selected: bool) {}
}

impl Widget for Tall {
    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.clamp(Size::new(1, self.0.get()))
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }
}

/// Add a list of `rows` rows under `parent`, each as tall as `height`.
fn tall_list(
    c: &mut dyn Context,
    parent: NodeId,
    rows: usize,
    height: &Rc<Cell<u32>>,
) -> Result<NodeId> {
    let list = c.add_child_to(parent, List::<Tall>::new())?;
    c.with_widget_mut(list, |list: &mut List<Tall>, c| {
        for _ in 0..rows {
            list.append(c, Tall(Rc::clone(height)))?;
        }
        Ok(())
    })?;
    Ok(list.into())
}

/// Select a list's last row.
fn select_last(harness: &mut Harness, list: NodeId) -> Result<()> {
    harness.canopy.with_root_context(|c| {
        c.with_widget_mut(list, |list: &mut List<Tall>, c| list.select_last(c))
    })
}

#[test]
fn a_list_reveals_its_selection_with_row_heights_changed_in_the_same_turn() -> Result<()> {
    let height = Rc::new(Cell::new(1));
    let rows = Rc::clone(&height);
    let (mut harness, nodes) = scene(10, 4, move |c| {
        let root = c.node_id();
        Ok(vec![tall_list(c, root, 5, &rows)?])
    })?;
    let list = nodes[0];

    height.set(3);
    select_last(&mut harness, list)?;
    harness.render()?;
    // Five rows of three cells end at row 15, and the view shows four rows.
    assert_eq!(scroll(&harness, list), Point { x: 0, y: 11 });
    Ok(())
}

#[test]
fn a_list_inside_a_scroll_container_reveals_through_both_views() -> Result<()> {
    let (mut harness, nodes) = scene(10, 6, |c| {
        let root = c.node_id();
        let container: NodeId = c.add_child_to(root, Scroll::vertical())?.into();
        boxed(
            c,
            container,
            Layout::column().flex_horizontal(1).fixed_height(10),
        )?;
        let list = tall_list(c, container, 10, &Rc::new(Cell::new(1)))?;
        c.set_layout_override_of(list, LayoutOverride::new().fixed_height(4))?;
        Ok(vec![container, list])
    })?;
    let (container, list) = (nodes[0], nodes[1]);
    // Startup focus reveals the first row, so return both views to the top.
    harness.canopy.with_root_context(|c| {
        c.scroll_to_of(list, 0, 0)?;
        c.scroll_to_of(container, 0, 0).map(|_| ())
    })?;
    harness.render()?;
    assert_eq!(scroll(&harness, container), Point::ZERO);

    select_last(&mut harness, list)?;
    harness.render()?;
    // The list shows rows 6 through 9. The container then shows the list's
    // last visible row, which sits at row 13 of its canvas.
    assert_eq!(scroll(&harness, list), Point { x: 0, y: 6 });
    assert_eq!(scroll(&harness, container), Point { x: 0, y: 8 });
    Ok(())
}

/// Add columns under the scene root.
fn add_columns(c: &mut dyn Context) -> Result<NodeId> {
    let root = c.node_id();
    Ok(c.add_child_to(root, Columns::new())?.into())
}

/// Return the characters of screen column `x` over `height` rows.
fn column_text(harness: &Harness, x: u32, height: u32) -> String {
    (0..height)
        .map(|y| {
            harness
                .buf()
                .get(Point { x, y })
                .map_or(' ', |cell| cell.ch)
        })
        .collect()
}

/// Return the characters of screen column `x`, with blank cells as spaces.
fn trimmed_column_text(harness: &Harness, x: u32, height: u32) -> String {
    column_text(harness, x, height).replace('\0', " ")
}

#[test]
fn dividers_follow_each_pane_and_track_its_overflow() -> Result<()> {
    // Twenty content columns less one gap leave panes of ten and nine.
    let (harness, _) = scene(21, 6, |c| {
        let columns = add_columns(c)?;
        surface(c, columns, Size::new(1, 1))?;
        surface(c, columns, Size::new(1, 60))?;
        Ok(Vec::new())
    })?;
    assert_eq!(column_text(&harness, 10, 6), "││││││");
    assert_eq!(column_text(&harness, 20, 6), "█│││││");

    let (harness, _) = scene(21, 6, |c| {
        let columns = add_columns(c)?;
        surface(c, columns, Size::new(1, 60))?;
        surface(c, columns, Size::new(1, 1))?;
        Ok(Vec::new())
    })?;
    assert_eq!(column_text(&harness, 10, 6), "█│││││");
    assert_eq!(
        trimmed_column_text(&harness, 20, 6).trim(),
        "",
        "the trailing column stays blank while its pane fits"
    );
    Ok(())
}

#[test]
fn a_pane_header_keeps_a_plain_divider() -> Result<()> {
    let (harness, _) = scene(21, 6, |c| {
        let columns = add_columns(c)?;
        let pane = boxed(c, columns, Layout::fill())?;
        boxed(c, pane, Layout::column().flex_horizontal(1).fixed_height(1))?;
        surface(c, pane, Size::new(1, 50))?;
        surface(c, columns, Size::new(1, 1))?;
        Ok(Vec::new())
    })?;
    assert_eq!(column_text(&harness, 10, 6), "│█││││");
    Ok(())
}

#[test]
fn wheel_input_and_drags_on_a_divider_scroll_the_pane_on_its_left() -> Result<()> {
    let (mut harness, nodes) = scene(21, 6, |c| {
        let columns = add_columns(c)?;
        let left = surface(c, columns, Size::new(1, 60))?;
        let right = surface(c, columns, Size::new(1, 60))?;
        Ok(vec![columns, left, right])
    })?;
    let (columns, left, right) = (nodes[0], nodes[1], nodes[2]);

    harness.mouse(pointer(mouse::Action::ScrollDown, 10, 3))?;
    assert_eq!(scroll(&harness, left), Point { x: 0, y: 3 });
    assert_eq!(scroll(&harness, right), Point::ZERO);

    harness.mouse(pointer(mouse::Action::Down, 20, 5))?;
    assert!(captured(&mut harness, columns)?);
    assert_eq!(scroll(&harness, right), Point { x: 0, y: 54 });
    harness.mouse(pointer(mouse::Action::Drag, 20, 0))?;
    assert_eq!(scroll(&harness, right), Point::ZERO);
    harness.mouse(pointer(mouse::Action::Up, 20, 0))?;
    assert!(!captured(&mut harness, columns)?);
    assert_eq!(scroll(&harness, left), Point { x: 0, y: 3 });
    Ok(())
}

#[test]
fn columns_keep_their_panes_from_an_enclosing_frame() -> Result<()> {
    let (harness, _) = scene(22, 8, |c| {
        let root = c.node_id();
        let frame: NodeId = c.add_child_to(root, Frame::new())?.into();
        let columns = c.add_child_to(frame, Columns::new())?;
        surface(c, columns.into(), Size::new(1, 60))?;
        Ok(Vec::new())
    })?;
    assert!(rows_with(&harness, 21, 8, '█').is_empty());
    assert_eq!(rows_with(&harness, 20, 8, '█'), [1]);
    Ok(())
}

#[test]
fn focus_column_wraps_through_displayed_panes() -> Result<()> {
    let (mut harness, nodes) = scene(30, 4, |c| {
        let columns = add_columns(c)?;
        let mut nodes = vec![columns];
        for _ in 0..3 {
            let pane = boxed(c, columns, Layout::fill())?;
            let leaf = c.add_child_to(pane, Tall(Rc::new(Cell::new(1))))?;
            nodes.extend([pane, leaf.into()]);
        }
        Ok(nodes)
    })?;
    let columns = nodes[0];
    let (middle, leaves) = (nodes[3], [nodes[2], nodes[4], nodes[6]]);
    let focus_column = |harness: &mut Harness, delta| -> Result<Option<NodeId>> {
        harness.canopy.with_root_context(|c| {
            c.with_widget_mut(columns, |columns: &mut Columns, c| {
                columns.focus_column(c, delta)
            })
        })?;
        harness.render()?;
        Ok(harness.canopy.with_root_view(|ctx| ctx.focused_node()))
    };

    harness
        .canopy
        .with_root_context(|c| c.set_focus(leaves[0]).map(|_| ()))?;
    assert_eq!(focus_column(&mut harness, 1)?, Some(leaves[1]));
    assert_eq!(focus_column(&mut harness, -2)?, Some(leaves[2]));

    harness
        .canopy
        .with_root_context(|c| c.set_hidden_of(middle, true).map(|_| ()))?;
    harness.render()?;
    assert_eq!(
        focus_column(&mut harness, 1)?,
        Some(leaves[0]),
        "a hidden pane is skipped"
    );

    let (mut empty, nodes) = scene(10, 4, |c| Ok(vec![add_columns(c)?]))?;
    empty.canopy.with_root_context(|c| {
        c.with_widget_mut(nodes[0], |columns: &mut Columns, c| {
            columns.focus_column(c, 1)
        })
    })?;
    Ok(())
}

#[test]
fn a_resize_during_a_drag_ends_it_at_the_next_event() -> Result<()> {
    let (mut harness, nodes) = scene(20, 10, |c| framed_surface(c, Size::new(1, 64)))?;
    let (frame, body) = (nodes[0], nodes[1]);
    harness.mouse(pointer(mouse::Action::Down, 19, 8))?;
    assert!(captured(&mut harness, frame)?);

    // A shorter window changes the track the drag started on.
    harness.canopy.set_root_size(Size::new(20, 6))?;
    harness.render()?;
    assert!(
        captured(&mut harness, frame)?,
        "rendering cannot release capture"
    );
    let offset = scroll(&harness, body);
    harness.mouse(pointer(mouse::Action::Drag, 19, 1))?;
    assert!(!captured(&mut harness, frame)?);
    assert_eq!(
        scroll(&harness, body),
        offset,
        "the event that ends a drag does not scroll"
    );
    Ok(())
}

#[test]
fn a_sidebar_inside_a_pane_leaves_its_divider_plain() -> Result<()> {
    let (harness, _) = scene(21, 6, |c| {
        let columns = add_columns(c)?;
        let pane = boxed(c, columns, Layout::fill().direction(Direction::Row))?;
        surface(c, pane, Size::new(1, 60))?;
        boxed(c, pane, Layout::column().fixed_width(2).flex_vertical(1))?;
        surface(c, columns, Size::new(1, 1))?;
        Ok(Vec::new())
    })?;
    assert_eq!(column_text(&harness, 10, 6), "││││││");
    Ok(())
}
