//! The block cursor, driven through the reader's keys, pointer, and
//! commands, as a host drives it.

use std::ops::Range;

use canopy::{
    geom::PointI32,
    input::key::{self, Key},
    testing::harness::Harness,
};
use proptest::{collection::vec, prelude::*};

use super::*;
use crate::{
    executor::{ExecutorOptions, ManualExecutor, MarkdownExecutor},
    surface::CursorScroll,
    testing,
    units::EditKind,
};

/// A document of a heading, a paragraph, two list items, a tall code block,
/// and a last paragraph.
const DOCUMENT: &str = "## One\n\nalpha [link](https://a.test)\n\n- item a\n- item b\n\n```\nc1\nc2\nc3\nc4\nc5\nc6\nc7\nc8\nc9\nc10\n```\n\nomega\n";

/// A mounted reader over `text`, resting in `resting`, `width` by
/// `height` cells, with no status row.
pub(super) fn reader(
    text: &str,
    resting: Mode,
    width: u32,
    height: u32,
) -> (Harness, ManualExecutor) {
    let (executor, manual) = MarkdownExecutor::manual(ExecutorOptions {
        worker_count: 1,
        ..ExecutorOptions::default()
    });
    let reader = Markdown::new(
        text.to_owned(),
        MarkdownOptions {
            executor,
            resting_mode: resting,
            external_search: true,
            clipboard: true,
            status_row: false,
            coalesce: Duration::ZERO,
            ..MarkdownOptions::default()
        },
    )
    .unwrap();
    let harness = Harness::builder(reader)
        .register::<Markdown>()
        .script("markdown-defaults", "markdown.default_bindings()")
        .size(width, height)
        .build()
        .unwrap();
    (harness, manual)
}

/// Render, poll the reader, and render again.
pub(super) fn service(harness: &mut Harness) {
    harness.render().unwrap();
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            Widget::poll(reader, ctx).map(|_| ())
        })
        .unwrap();
    harness.render().unwrap();
}

/// Run the reader's work until it shows its document, and focus its view.
pub(super) fn settle(harness: &mut Harness, manual: &ManualExecutor) {
    for _ in 0..16 {
        service(harness);
        manual.run_until_idle();
        service(harness);
        if harness.with_root_widget(|reader: &mut Markdown| reader.ready()) {
            service(harness);
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    ctx.set_focus(reader.navigation_node().unwrap())
                })
                .unwrap();
            return;
        }
    }
    panic!("the reader did not settle");
}

/// The marked unit's index and rows, while the block cursor shows.
pub(super) fn marked(harness: &mut Harness) -> Option<(usize, Range<u32>)> {
    harness.with_root_widget(|reader: &mut Markdown| {
        let mut state = reader.shared.borrow_mut();
        let (_, rows) = state.marked_unit()?;
        Some((state.marked_index()?, rows))
    })
}

/// The rows of each unit of the shown document.
pub(super) fn extents(harness: &mut Harness) -> Vec<Range<u32>> {
    harness.with_root_widget(|reader: &mut Markdown| {
        let mut state = reader.shared.borrow_mut();
        state
            .cursor_map()
            .map_or_else(Vec::new, |(_, rows)| rows.to_vec())
    })
}

/// The first row in view, and the rows in view.
pub(super) fn view(harness: &mut Harness) -> (u32, u32) {
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            let view = ctx
                .view_of(reader.nodes.as_ref().unwrap().rendered)
                .unwrap();
            Ok((view.scroll.y, view.content.h))
        })
        .unwrap()
}

/// The document rows in view that the bar marks, read from the screen.
pub(super) fn bar_rows(harness: &mut Harness) -> Vec<u32> {
    let (top, height) = view(harness);
    (0..height)
        .filter(|&y| {
            harness
                .buf()
                .get(Point::new(0, y))
                .is_some_and(|cell| cell.ch == '▌')
        })
        .map(|y| top + y)
        .collect()
}

/// The document rows of `rows` that the view shows.
pub(super) fn visible(harness: &mut Harness, rows: &Range<u32>) -> Vec<u32> {
    let (top, height) = view(harness);
    (rows.start.max(top)..rows.end.min(top + height)).collect()
}

/// Press `key` and service the reader.
pub(super) fn press(harness: &mut Harness, key: &str) {
    harness.key(Key::parse_spec(key).unwrap()).unwrap();
    service(harness);
}

#[test]
fn j_and_k_step_through_units_and_the_bar_marks_the_marked_unit() {
    let (mut harness, manual) = reader(DOCUMENT, Mode::Interact, 40, 40);
    settle(&mut harness, &manual);
    let extents = extents(&mut harness);
    assert_eq!(extents.len(), 6);
    assert_eq!(marked(&mut harness).map(|(unit, _)| unit), Some(0));
    for expected in [1, 2, 3, 4, 5, 5] {
        press(&mut harness, "j");
        let (unit, rows) = marked(&mut harness).unwrap();
        assert_eq!(unit, expected);
        assert_eq!(bar_rows(&mut harness), visible(&mut harness, &rows));
    }
    press(&mut harness, "k");
    assert_eq!(marked(&mut harness).map(|(unit, _)| unit), Some(4));
    press(&mut harness, "g");
    assert_eq!(marked(&mut harness).map(|(unit, _)| unit), Some(0));
    press(&mut harness, "G");
    assert_eq!(marked(&mut harness).map(|(unit, _)| unit), Some(5));
}

#[test]
fn a_tall_unit_pages_before_the_cursor_moves_on() {
    let (mut harness, manual) = reader(DOCUMENT, Mode::Interact, 40, 7);
    settle(&mut harness, &manual);
    let extents = extents(&mut harness);
    let code = extents[4].clone();
    let (_, height) = view(&mut harness);
    assert!(
        code.len() as u32 > height,
        "the code is taller than the view"
    );
    for _ in 0..4 {
        press(&mut harness, "j");
    }
    // Entered going down, the code shows its top at the top of the view.
    assert_eq!(marked(&mut harness).map(|(unit, _)| unit), Some(4));
    assert_eq!(view(&mut harness).0, code.start);
    let mut tops = vec![code.start];
    while marked(&mut harness).map(|(unit, _)| unit) == Some(4) {
        press(&mut harness, "j");
        tops.push(view(&mut harness).0);
    }
    // Each step is a page less two rows, until the end of the code shows.
    let step = height - 2;
    for pair in tops.windows(2).take(tops.len() - 2) {
        assert!(pair[1] - pair[0] <= step && pair[1] > pair[0], "{tops:?}");
    }
    assert_eq!(tops[tops.len() - 2] + height, code.end, "{tops:?}");
    assert_eq!(marked(&mut harness).map(|(unit, _)| unit), Some(5));
    // Going up, the code shows its bottom at the bottom of the view.
    press(&mut harness, "k");
    assert_eq!(marked(&mut harness).map(|(unit, _)| unit), Some(4));
    assert_eq!(view(&mut harness).0 + height, code.end);
    press(&mut harness, "k");
    assert_eq!(view(&mut harness).0 + height, code.end - step);
}

#[test]
fn free_scrolls_keep_the_cursor_on_a_unit_in_view() {
    let (mut harness, manual) = reader(DOCUMENT, Mode::Interact, 40, 7);
    settle(&mut harness, &manual);
    press(&mut harness, "PageDown");
    let (top, height) = view(&mut harness);
    assert!(top > 0);
    let (unit, rows) = marked(&mut harness).unwrap();
    let first = extents(&mut harness)
        .iter()
        .position(|rows| rows.start < top + height && rows.end > top)
        .unwrap();
    assert_eq!(unit, first);
    assert!(!visible(&mut harness, &rows).is_empty());
    // The wheel scrolls freely too.
    harness
        .mouse(mouse::MouseEvent {
            action: mouse::Action::ScrollDown,
            button: mouse::Button::None,
            modifiers: key::Empty,
            location: PointI32 { x: 4, y: 1 },
        })
        .unwrap();
    service(&mut harness);
    let (_, rows) = marked(&mut harness).unwrap();
    assert!(!visible(&mut harness, &rows).is_empty());
}

#[test]
fn a_click_marks_the_unit_of_its_row() {
    let (mut harness, manual) = reader(DOCUMENT, Mode::Interact, 40, 40);
    settle(&mut harness, &manual);
    let extents = extents(&mut harness);
    let row = extents[3].start;
    harness
        .mouse(mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32 {
                x: 1,
                y: row as i32,
            },
        })
        .unwrap();
    service(&mut harness);
    assert_eq!(marked(&mut harness).map(|(unit, _)| unit), Some(3));
}

#[test]
fn a_current_search_match_moves_the_cursor_to_its_unit() {
    let (mut harness, manual) = reader(DOCUMENT, Mode::Interact, 40, 7);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.set_search(ctx, "omega".into(), false)
        })
        .unwrap();
    settle(&mut harness, &manual);
    assert_eq!(marked(&mut harness).map(|(unit, _)| unit), Some(5));
    let (_, rows) = marked(&mut harness).unwrap();
    assert!(!visible(&mut harness, &rows).is_empty());
}

#[test]
fn select_block_at_marks_a_unit_now_or_once_it_shows() {
    let (mut harness, manual) = reader(DOCUMENT, Mode::Scroll, 40, 40);
    let byte = DOCUMENT.find("item b").unwrap();
    // Before the document shows, the reader marks the unit once it does.
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.select_block_at(ctx, byte))
        .unwrap();
    settle(&mut harness, &manual);
    assert_eq!(marked(&mut harness).map(|(unit, _)| unit), Some(3));
    let unit = harness.with_root_widget(|reader: &mut Markdown| reader.selected_block().unwrap());
    assert_eq!(&DOCUMENT[unit.range.clone()], "- item b");
    assert_eq!(unit.kind, EditKind::ListItem);
    // A byte between units marks the next one.
    let blank = DOCUMENT.find("\n\n- item").unwrap() + 1;
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.select_block_at(ctx, blank))
        .unwrap();
    service(&mut harness);
    assert_eq!(marked(&mut harness).map(|(unit, _)| unit), Some(2));
    // Outside the block cursor, no unit is selected.
    press(&mut harness, "Escape");
    assert!(harness.with_root_widget(|reader: &mut Markdown| reader.selected_block().is_none()));
}

#[test]
fn tab_carries_the_cursor_to_an_object_and_j_drops_it() {
    let (mut harness, manual) = reader(DOCUMENT, Mode::Interact, 40, 40);
    settle(&mut harness, &manual);
    press(&mut harness, "Tab");
    let (unit, object) = harness.with_root_widget(|reader: &mut Markdown| {
        let mut state = reader.shared.borrow_mut();
        (state.marked_index(), state.overlays.active_object)
    });
    assert_eq!(unit, Some(1), "the link lies in the paragraph");
    assert!(object.is_some());
    press(&mut harness, "j");
    let object = harness
        .with_root_widget(|reader: &mut Markdown| reader.shared.borrow().overlays.active_object);
    assert_eq!(object, None, "a move to another unit drops the object");
    // Tab from the list goes on to the code card, the next object.
    press(&mut harness, "Tab");
    assert_eq!(marked(&mut harness).map(|(unit, _)| unit), Some(4));
}

/// Step the object cursor by `delta`, staying at the ends of the document,
/// and return the marked unit and whether an object is active.
fn object_step(harness: &mut Harness, delta: i32) -> (Option<usize>, Option<u64>) {
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            let node = reader.nodes.as_ref().unwrap().rendered;
            ctx.with_widget_mut(node, |surface: &mut RenderedSurface, ctx| {
                surface.object_next(ctx, delta, false)
            })
        })
        .unwrap();
    service(harness);
    harness.with_root_widget(|reader: &mut Markdown| {
        let mut state = reader.shared.borrow_mut();
        (state.marked_index(), state.overlays.active_object)
    })
}

#[test]
fn the_object_cursor_steps_both_ways_from_the_marked_unit() {
    let (mut harness, manual) = reader(DOCUMENT, Mode::Interact, 40, 40);
    settle(&mut harness, &manual);
    // The link lies in unit 1 and the code card is unit 4.
    let (unit, link) = object_step(&mut harness, 1);
    assert_eq!(unit, Some(1));
    let (unit, code) = object_step(&mut harness, 1);
    assert_eq!(unit, Some(4));
    assert_ne!(code, link);
    // Past the last object, the cursor stays.
    assert_eq!(object_step(&mut harness, 1), (Some(4), code));
    assert_eq!(object_step(&mut harness, -1), (Some(1), link));
    assert_eq!(object_step(&mut harness, -1), (Some(1), link));
    // With no active object, a step starts from the marked unit: forward at
    // its first object at or after it, backward at its last before it.
    for (marked, delta, expected) in [(2, 1, code), (2, -1, link), (5, -1, code)] {
        press(&mut harness, "Escape");
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                let byte = reader.shared.borrow_mut().cursor_map().unwrap().0[marked]
                    .range
                    .start;
                reader.select_block_at(ctx, byte)
            })
            .unwrap();
        service(&mut harness);
        assert_eq!(
            object_step(&mut harness, delta).1,
            expected,
            "{marked} {delta}"
        );
    }
    // A reader that never marked a unit steps back from the last object.
    let (mut harness, manual) = reader(DOCUMENT, Mode::Scroll, 40, 40);
    settle(&mut harness, &manual);
    assert_eq!(object_step(&mut harness, -1), (Some(4), code));
}

/// Readers in a column, for focus traversal.
struct Pair {
    /// The readers to mount.
    readers: Vec<Markdown>,
}

impl Widget for Pair {
    fn name(&self) -> NodeName {
        NodeName::convert("pair")
    }
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        for reader in self.readers.drain(..) {
            let node = ctx.add_child(ctx.node_id(), reader)?;
            ctx.set_layout_override(node.into(), Layout::fill().into())?;
        }
        Ok(())
    }
}

impl Register for Pair {
    fn register(setup: &mut Setup) -> Result<()> {
        Markdown::register(setup)
    }
}

#[test]
fn the_object_cursor_leaves_each_end_toward_its_own_side() {
    let (executor, manual) = MarkdownExecutor::manual(ExecutorOptions::default());
    let readers = (0..3)
        .map(|_| {
            Markdown::new(
                "[one](https://one.test)\n".into(),
                MarkdownOptions {
                    executor: executor.clone(),
                    resting_mode: Mode::Interact,
                    status_row: false,
                    coalesce: Duration::ZERO,
                    ..MarkdownOptions::default()
                },
            )
            .unwrap()
        })
        .collect();
    let mut harness = Harness::builder(Pair { readers })
        .register::<Pair>()
        .script("markdown-defaults", "markdown.default_bindings()")
        .size(30, 12)
        .build()
        .unwrap();
    let mut readers = Vec::new();
    for _ in 0..16 {
        harness.render().unwrap();
        readers = harness
            .with_root_widget_context(|_: &mut Pair, ctx| {
                let nodes = ctx.find_nodes("**/markdown")?;
                for node in &nodes {
                    ctx.with_widget_mut(*node, |reader: &mut Markdown, ctx| {
                        Widget::poll(reader, ctx).map(|_| ())
                    })?;
                }
                Ok(nodes)
            })
            .unwrap();
        manual.run_until_idle();
    }
    assert_eq!(readers.len(), 3);
    let second = readers[1];
    // From the middle reader's only object, Tab leaves for the next reader,
    // and shift-Tab for a node before it.
    let mut left = Vec::new();
    for delta in [-1, 1] {
        let focused = harness
            .with_root_widget_context(|_: &mut Pair, ctx| {
                let surface = ctx.with_widget(second, |reader: &Markdown| {
                    Ok(reader.navigation_node().unwrap())
                })?;
                ctx.set_focus(surface)?;
                ctx.with_widget_mut(second, |reader: &mut Markdown, ctx| {
                    reader.shared.borrow_mut().overlays.active_object = None;
                    reader.next_object(ctx, delta)?;
                    reader.next_object(ctx, delta)
                })?;
                Ok(ctx.focused_node())
            })
            .unwrap();
        assert_ne!(focused, None);
        left.push(focused);
    }
    assert_eq!(left[1], Some(readers[2]), "Tab leaves for the next reader");
    assert_ne!(left[0], left[1], "shift-Tab leaves the other way");
}

#[test]
fn enter_opens_the_marked_units_card_and_y_copies_its_source() {
    let (mut harness, manual) = reader(DOCUMENT, Mode::Interact, 40, 40);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    press(&mut harness, "j");
    // A list item copies its exact source.
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.copy_item(ctx))
        .unwrap();
    settle(&mut harness, &manual);
    let requests = harness.with_root_widget(|reader: &mut Markdown| reader.take_requests(8));
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0].action, HostAction::CopyText("- item a".into()));
    // Enter on the code unit opens its card.
    press(&mut harness, "j");
    press(&mut harness, "j");
    press(&mut harness, "Enter");
    let inspected = harness.with_root_widget(|reader: &mut Markdown| reader.inspector_open());
    assert!(inspected, "the code card opens");
}

#[test]
fn escape_clears_then_leaves_only_a_view_that_rests_in_scroll() {
    for resting in [Mode::Interact, Mode::Scroll] {
        let (mut harness, manual) = reader(DOCUMENT, resting, 40, 40);
        settle(&mut harness, &manual);
        press(&mut harness, "i");
        press(&mut harness, "Tab");
        press(&mut harness, "Escape");
        let (mode, object) = harness.with_root_widget(|reader: &mut Markdown| {
            let state = reader.shared.borrow();
            (state.mode, state.overlays.active_object)
        });
        assert_eq!((mode, object), (Mode::Interact, None), "{resting:?}");
        press(&mut harness, "Escape");
        let mode = harness.with_root_widget(|reader: &mut Markdown| reader.shared.borrow().mode);
        assert_eq!(mode, resting);
        let bars = bar_rows(&mut harness);
        assert_eq!(bars.is_empty(), resting == Mode::Scroll, "{resting:?}");
    }
}

#[test]
fn a_resize_keeps_the_marked_unit_where_it_shows() {
    let (mut harness, manual) = reader(DOCUMENT, Mode::Interact, 40, 9);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    press(&mut harness, "j");
    press(&mut harness, "j");
    let before = harness.with_root_widget(|reader: &mut Markdown| reader.selected_block());
    harness.canopy.set_screen_size(Size::new(14, 9)).unwrap();
    settle(&mut harness, &manual);
    let after = harness.with_root_widget(|reader: &mut Markdown| reader.selected_block());
    assert_eq!(after.map(|unit| unit.range), before.map(|unit| unit.range));
    let (_, rows) = marked(&mut harness).unwrap();
    assert!(!visible(&mut harness, &rows).is_empty());
}

#[test]
fn a_shorter_view_scrolls_to_keep_the_marked_unit() {
    // The width stays, so the layout stays, and only the view shrinks.
    let (mut harness, manual) = reader(DOCUMENT, Mode::Interact, 40, 40);
    settle(&mut harness, &manual);
    for _ in 0..5 {
        press(&mut harness, "j");
    }
    harness.canopy.set_screen_size(Size::new(40, 6)).unwrap();
    settle(&mut harness, &manual);
    let (unit, rows) = marked(&mut harness).unwrap();
    assert_eq!(unit, 5);
    assert!(!visible(&mut harness, &rows).is_empty());
}

#[test]
fn a_new_layout_scrolls_to_the_marked_unit_it_hides() {
    // A narrow width wraps the heading over the whole view.
    let text = "> [[wiki]] aaa a\tb\n> ===\n\n> - ^lone\n>\n>   # a";
    let (mut harness, manual) = reader(text, Mode::Scroll, 16, 4);
    settle(&mut harness, &manual);
    press(&mut harness, "i");
    press(&mut harness, "j");
    let before = harness.with_root_widget(|reader: &mut Markdown| reader.selected_block());
    harness.canopy.set_screen_size(Size::new(3, 10)).unwrap();
    settle(&mut harness, &manual);
    let after = harness.with_root_widget(|reader: &mut Markdown| reader.selected_block());
    assert_eq!(after, before);
    let (_, rows) = marked(&mut harness).unwrap();
    assert!(!visible(&mut harness, &rows).is_empty());
}

#[test]
fn select_block_at_scrolls_to_a_unit_out_of_view() {
    let (mut harness, manual) = reader(DOCUMENT, Mode::Interact, 40, 6);
    settle(&mut harness, &manual);
    let byte = DOCUMENT.find("omega").unwrap();
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.select_block_at(ctx, byte))
        .unwrap();
    service(&mut harness);
    let (unit, rows) = marked(&mut harness).unwrap();
    assert_eq!(unit, 5);
    assert!(!visible(&mut harness, &rows).is_empty());
}

/// Scroll the view to `top` as a host does, without the cursor, ask for
/// the reveal that a resize asks for, and poll the reader.
fn reveal_from(harness: &mut Harness, top: u32) {
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            let node = reader.nodes.as_ref().unwrap().rendered;
            ctx.scroll_node(node, ScrollOp::To(Point { x: 0, y: top }))?;
            reader.shared.borrow_mut().cursor_scroll = Some(CursorScroll::Reveal);
            Widget::poll(reader, ctx).map(|_| ())
        })
        .unwrap();
}

#[test]
fn a_reveal_after_a_resize_shows_the_end_of_a_tall_unit_it_enters() {
    // Paragraphs after the code let the view scroll past it.
    let text = format!("{DOCUMENT}{}", "\nmore\n".repeat(12));
    let (mut harness, manual) = reader(&text, Mode::Interact, 40, 7);
    settle(&mut harness, &manual);
    for _ in 0..4 {
        press(&mut harness, "j");
    }
    let (unit, code) = marked(&mut harness).unwrap();
    assert_eq!(unit, 4);
    let (_, height) = view(&mut harness);
    // Hidden below the view, the code shows its top.
    reveal_from(&mut harness, 0);
    assert_eq!(view(&mut harness).0, code.start);
    // Hidden above the view, it shows its bottom.
    reveal_from(&mut harness, code.end + 2);
    assert_eq!(view(&mut harness).0 + height, code.end);
    // While a row of it shows, the view stays.
    reveal_from(&mut harness, code.start + 1);
    assert_eq!(view(&mut harness).0, code.start + 1);
}

#[test]
fn a_new_layout_keeps_the_marked_unit_on_its_screen_row() {
    // A long paragraph above the marked unit wraps to fewer rows when the
    // view widens. The marked unit holds its screen row; the top of the
    // view would hold its own.
    let text = format!(
        "{}{}\n\nmarked\n\n{}",
        "short\n\n".repeat(12),
        "word ".repeat(40),
        "after\n\n".repeat(12)
    );
    let (mut harness, manual) = reader(&text, Mode::Interact, 30, 14);
    settle(&mut harness, &manual);
    let byte = text.find("marked").unwrap();
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.select_block_at(ctx, byte))
        .unwrap();
    service(&mut harness);
    // Put the marked unit in the middle of the view, below the paragraph.
    let (_, rows) = marked(&mut harness).unwrap();
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.scroll_view(
                ctx,
                ScrollOp::To(Point {
                    x: 0,
                    y: rows.start - 8,
                }),
            )
        })
        .unwrap();
    service(&mut harness);
    let (top, _) = view(&mut harness);
    let (unit, rows) = marked(&mut harness).unwrap();
    let row = rows.start - top;
    assert_eq!(row, 8);
    harness.canopy.set_screen_size(Size::new(60, 14)).unwrap();
    settle(&mut harness, &manual);
    let (top, _) = view(&mut harness);
    let (after, rows) = marked(&mut harness).unwrap();
    assert_eq!(after, unit);
    assert_eq!(
        rows.start - top,
        row,
        "the marked unit keeps its screen row"
    );
}

#[test]
fn without_a_margin_the_marked_rows_take_the_cursor_ground() {
    let (mut harness, manual) = reader("one\n\ntwo\n", Mode::Interact, 4, 8);
    settle(&mut harness, &manual);
    let (_, rows) = marked(&mut harness).unwrap();
    let ground = harness
        .canopy
        .style()
        .resolve("markdown/block")
        .resolve_solid()
        .unwrap()
        .bg;
    // The gutter column takes each row's ground, and no text.
    for y in 0..6 {
        let cell = harness.buf().get(Point::new(2, y)).unwrap();
        assert_eq!(cell.style.bg == ground, rows.contains(&y), "row {y}");
    }
    assert!(bar_rows(&mut harness).is_empty());
}

#[test]
fn a_document_without_units_marks_nothing() {
    for text in ["", "[a]: https://a.test\n"] {
        let (mut harness, manual) = reader(text, Mode::Interact, 30, 8);
        settle(&mut harness, &manual);
        for key in ["j", "k", "g", "G", "Tab", "Escape"] {
            press(&mut harness, key);
        }
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| reader.select_block_at(ctx, 0))
            .unwrap();
        service(&mut harness);
        assert_eq!(marked(&mut harness), None, "{text:?}");
        assert!(bar_rows(&mut harness).is_empty());
        let unit = harness.with_root_widget(|reader: &mut Markdown| reader.selected_block());
        assert_eq!(unit, None);
    }
}

#[test]
fn state_reports_the_marked_unit() {
    let (mut harness, manual) = reader(DOCUMENT, Mode::Interact, 40, 40);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    let state = harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.state(ctx))
        .unwrap();
    let ArgValue::Map(fields) = state else {
        panic!("state is a map");
    };
    let ArgValue::Map(block) = &fields["block"] else {
        panic!("a unit is marked: {:?}", fields["block"]);
    };
    assert_eq!(block["kind"], ArgValue::String("Paragraph".into()));
    let start = DOCUMENT.find("alpha").unwrap() as u64;
    let ArgValue::Map(range) = &block["range"] else {
        panic!("the unit has a range");
    };
    assert_eq!(range["start"], ArgValue::UInt(start));
}

/// The model of the block cursor: a plain statement of its rules, apart
/// from the reader's code. It takes the rows of each unit, the view, and one
/// operation, and gives the mode, the marked unit, and the first row in
/// view.
mod model {
    use std::ops::Range;

    use crate::options::Mode;

    /// What the model tracks of the reader.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) struct State {
        /// The rendered view's mode.
        pub(super) mode: Mode,
        /// The marked unit.
        pub(super) marked: Option<usize>,
        /// The first row in view.
        pub(super) top: u32,
    }

    /// The rows of each unit, and the view over them.
    pub(super) struct Scene<'a> {
        /// The rows of each unit.
        pub(super) extents: &'a [Range<u32>],
        /// The rows in view.
        pub(super) height: u32,
        /// The rows of the canvas.
        pub(super) canvas: u32,
    }

    /// An operation whose outcome the model gives.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum Op {
        /// `j`.
        Down,
        /// `k`.
        Up,
        /// `g`.
        First,
        /// `G`.
        Last,
        /// `PageDown`, or `Space`.
        PageDown,
        /// `PageUp`.
        PageUp,
        /// The wheel, down.
        WheelDown,
        /// The wheel, up.
        WheelUp,
        /// `i`.
        Interact,
    }

    impl Scene<'_> {
        /// The highest first row the view can take.
        fn last_top(&self) -> u32 {
            self.canvas.saturating_sub(self.height)
        }

        /// Whether the view from `top` shows a row of unit `unit`.
        pub(super) fn shows(&self, unit: usize, top: u32) -> bool {
            let rows = &self.extents[unit];
            let bottom = top + self.height;
            (rows.start..rows.end).any(|row| row >= top && row < bottom)
        }

        /// The units that have rows.
        fn with_rows(&self) -> Vec<usize> {
            (0..self.extents.len())
                .filter(|&unit| !self.extents[unit].is_empty())
                .collect()
        }

        /// Where the view goes to show `unit`, entering it going `down`.
        fn show(&self, unit: usize, top: u32, down: bool) -> u32 {
            let rows = &self.extents[unit];
            let tall = rows.end - rows.start > self.height;
            let wanted = match (tall, down) {
                (true, true) => rows.start,
                (true, false) => rows.end - self.height,
                (false, _) if rows.start < top => rows.start,
                (false, _) if rows.end > top + self.height => rows.end - self.height,
                (false, _) => top,
            };
            wanted.min(self.last_top())
        }

        /// Keep the cursor on a unit in view after the view moved freely.
        fn keep(&self, mut state: State) -> State {
            if state.mode != Mode::Interact || self.height == 0 {
                return state;
            }
            if state.marked.is_some_and(|unit| self.shows(unit, state.top)) {
                return state;
            }
            let units = self.with_rows();
            if let Some(&unit) = units.iter().find(|&&unit| self.shows(unit, state.top)) {
                state.marked = Some(unit);
            } else if let Some(&unit) = units
                .iter()
                .find(|&&unit| self.extents[unit].start >= state.top)
                .or(units.last())
            {
                let down = self.extents[unit].start >= state.top;
                state.marked = Some(unit);
                state.top = self.show(unit, state.top, down);
            }
            state
        }

        /// Apply `op` to `state`.
        pub(super) fn apply(&self, state: State, op: Op) -> State {
            let mut next = state;
            let page = self.height.saturating_sub(1).max(1);
            let scroll = |top: i64| top.clamp(0, i64::from(self.last_top())) as u32;
            let top = i64::from(state.top);
            match (op, state.mode) {
                (Op::PageDown, _) => next.top = scroll(top + i64::from(page)),
                (Op::PageUp, _) => next.top = scroll(top - i64::from(page)),
                (Op::WheelDown, _) => next.top = scroll(top + 3),
                (Op::WheelUp, _) => next.top = scroll(top - 3),
                (Op::Interact, _) => next.mode = Mode::Interact,
                (Op::Down, Mode::Scroll) => next.top = scroll(top + 1),
                (Op::Up, Mode::Scroll) => next.top = scroll(top - 1),
                (Op::First, Mode::Scroll) => next.top = 0,
                (Op::Last, Mode::Scroll) => next.top = self.last_top(),
                (Op::First | Op::Last, Mode::Interact) => {
                    let units = self.with_rows();
                    let unit = if op == Op::First {
                        units.first()
                    } else {
                        units.last()
                    };
                    if let Some(&unit) = unit {
                        next.marked = Some(unit);
                        next.top = self.show(unit, state.top, op == Op::First);
                    }
                }
                (Op::Down | Op::Up, Mode::Interact) => {
                    let Some(marked) = state.marked else {
                        return self.keep(next);
                    };
                    let rows = &self.extents[marked];
                    let step = self.height.saturating_sub(2).max(1);
                    if op == Op::Down && rows.end > state.top + self.height {
                        next.top = (state.top + step).min(rows.end - self.height);
                    } else if op == Op::Up && rows.start < state.top {
                        next.top = state.top.saturating_sub(step).max(rows.start);
                    } else {
                        let units = self.with_rows();
                        let neighbour = if op == Op::Down {
                            units.iter().find(|&&unit| unit > marked)
                        } else {
                            units.iter().rev().find(|&&unit| unit < marked)
                        };
                        if let Some(&unit) = neighbour {
                            next.marked = Some(unit);
                            next.top = self.show(unit, state.top, op == Op::Down);
                        }
                    }
                }
            }
            self.keep(next)
        }
    }

    /// The mode that `Esc` leaves, with nothing else to clear.
    pub(super) fn escape(state: State, resting: Mode) -> State {
        State {
            mode: if state.mode == Mode::Interact && resting == Mode::Scroll {
                Mode::Scroll
            } else {
                state.mode
            },
            ..state
        }
    }
}

/// One operation of the model-based test.
#[derive(Clone, Debug)]
enum Action {
    /// An operation whose outcome the model gives.
    Model(model::Op),
    /// `Space`, which pages down.
    Space,
    /// A press on a row of the view.
    Click(u32),
    /// A new screen size.
    Resize(u32, u32),
    /// A search whose first match becomes current.
    Search,
    /// A new text from the host.
    Replace(String),
    /// `Tab`, or `shift-Tab`.
    Tab(bool),
    /// `Esc`.
    Escape,
}

/// Generate an action.
fn action() -> impl Strategy<Value = Action> {
    use model::Op;
    prop_oneof![
        6 => prop_oneof![Just(Op::Down), Just(Op::Up)].prop_map(Action::Model),
        3 => prop_oneof![
            Just(Op::First),
            Just(Op::Last),
            Just(Op::PageDown),
            Just(Op::PageUp),
            Just(Op::WheelDown),
            Just(Op::WheelUp),
            Just(Op::Interact),
        ]
        .prop_map(Action::Model),
        1 => Just(Action::Space),
        1 => (0..12u32).prop_map(Action::Click),
        1 => (2..60u32, 2..14u32).prop_map(|(width, height)| Action::Resize(width, height)),
        1 => Just(Action::Search),
        1 => testing::document().prop_map(Action::Replace),
        1 => any::<bool>().prop_map(Action::Tab),
        1 => Just(Action::Escape),
    ]
}

/// Read the model's state from the reader.
fn observed(harness: &mut Harness) -> model::State {
    let (top, _) = view(harness);
    harness.with_root_widget(|reader: &mut Markdown| {
        let mut state = reader.shared.borrow_mut();
        model::State {
            mode: state.mode,
            marked: state.marked_index(),
            top,
        }
    })
}

/// The view's height and the canvas's.
fn sizes(harness: &mut Harness) -> (u32, u32) {
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            let view = ctx
                .view_of(reader.nodes.as_ref().unwrap().rendered)
                .unwrap();
            Ok((view.content.h, view.canvas.h))
        })
        .unwrap()
}

/// Check what holds after every step: the marked unit shows, and the bar,
/// or the cursor's ground without a margin, marks exactly its rows in view.
fn check_screen(harness: &mut Harness, width: u32) -> StdResult<(), TestCaseError> {
    let state = observed(harness);
    let extents = extents(harness);
    let (height, _) = sizes(harness);
    let interact = state.mode == Mode::Interact;
    let rows = match state.marked.and_then(|unit| extents.get(unit)) {
        Some(rows) if interact => rows.clone(),
        _ => 0..0,
    };
    if interact && height > 0 && extents.iter().any(|rows| !rows.is_empty()) {
        prop_assert!(
            !visible(harness, &rows).is_empty(),
            "the marked unit {:?} {rows:?} shows from {}",
            state.marked,
            state.top
        );
    }
    let expected = visible(harness, &rows);
    if width > 3 {
        prop_assert_eq!(bar_rows(harness), expected);
    } else {
        // The gutter takes the ground of the row, and no text.
        let style = harness.canopy.style();
        let solid = |role: &str| style.resolve(role).resolve_solid().unwrap().bg;
        let ground = solid("markdown/block");
        let covers = [
            ground,
            solid("markdown/object"),
            solid(view_search::CURRENT),
            solid(view_search::MATCH),
        ];
        let (top, height) = view(harness);
        for y in 0..height {
            let bg = harness
                .buf()
                .get(Point::new(width - 1, y))
                .unwrap()
                .style
                .bg;
            if expected.contains(&(top + y)) {
                prop_assert!(covers.contains(&bg), "row {} lacks the ground", top + y);
            } else {
                prop_assert!(bg != ground, "row {} takes the ground", top + y);
            }
        }
    }
    Ok(())
}

/// Apply `action` to the reader, and check it against the model.
fn step(
    harness: &mut Harness,
    manual: &ManualExecutor,
    resting: Mode,
    width: &mut u32,
    action: &Action,
) -> StdResult<(), TestCaseError> {
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.dismiss_inspector(ctx)?;
            ctx.set_focus(reader.navigation_node().unwrap())
        })
        .unwrap();
    let before = observed(harness);
    let extents = extents(harness);
    let (height, canvas) = sizes(harness);
    let scene = model::Scene {
        extents: &extents,
        height,
        canvas,
    };
    let unit_start = |harness: &mut Harness| {
        harness.with_root_widget(|reader: &mut Markdown| {
            reader.selected_block().map(|unit| unit.range.start)
        })
    };
    match action {
        Action::Model(op) => {
            let key = match op {
                model::Op::Down => Some("j"),
                model::Op::Up => Some("k"),
                model::Op::First => Some("g"),
                model::Op::Last => Some("G"),
                model::Op::PageDown => Some("PageDown"),
                model::Op::PageUp => Some("PageUp"),
                model::Op::Interact => Some("i"),
                model::Op::WheelDown | model::Op::WheelUp => None,
            };
            match key {
                Some(key) => press(harness, key),
                None => {
                    harness
                        .mouse(mouse::MouseEvent {
                            action: if *op == model::Op::WheelDown {
                                mouse::Action::ScrollDown
                            } else {
                                mouse::Action::ScrollUp
                            },
                            button: mouse::Button::None,
                            modifiers: key::Empty,
                            location: PointI32 { x: 0, y: 0 },
                        })
                        .unwrap();
                    service(harness);
                }
            }
            let expected = scene.apply(before, *op);
            prop_assert_eq!(
                observed(harness),
                expected,
                "{:?} from {:?} over {:?}",
                op,
                before,
                extents
            );
            // Each step moves the cursor or the view, until the end shows.
            if before.mode == Mode::Interact && matches!(op, model::Op::Down | model::Op::Up) {
                let after = observed(harness);
                let units: Vec<_> = (0..extents.len())
                    .filter(|&unit| !extents[unit].is_empty())
                    .collect();
                let end = if *op == model::Op::Down {
                    units.last()
                } else {
                    units.first()
                };
                let whole = |unit: usize| {
                    let rows = &extents[unit];
                    rows.start >= before.top && rows.end <= before.top + height
                };
                let at_end = before.marked.is_some()
                    && before.marked == end.copied()
                    && (whole(before.marked.unwrap())
                        || if *op == model::Op::Down {
                            extents[before.marked.unwrap()].end <= before.top + height
                        } else {
                            extents[before.marked.unwrap()].start >= before.top
                        });
                prop_assert!(
                    after != before || at_end || units.is_empty(),
                    "{op:?} moved nothing"
                );
            }
        }
        Action::Space => {
            press(harness, "Space");
            let expected = scene.apply(before, model::Op::PageDown);
            prop_assert_eq!(observed(harness), expected);
        }
        Action::Click(row) => {
            let row = *row % height.max(1);
            // A press on the active object activates it, and a link or
            // footnote may jump, so only the screen checks hold then.
            let armed = before.mode == Mode::Interact
                && harness.with_root_widget(|reader: &mut Markdown| {
                    reader.shared.borrow().overlays.active_object.is_some()
                });
            harness
                .mouse(mouse::MouseEvent {
                    action: mouse::Action::Down,
                    button: mouse::Button::Left,
                    modifiers: key::Empty,
                    location: PointI32 {
                        x: 1,
                        y: row as i32,
                    },
                })
                .unwrap();
            harness
                .mouse(mouse::MouseEvent {
                    action: mouse::Action::Up,
                    button: mouse::Button::Left,
                    modifiers: key::Empty,
                    location: PointI32 {
                        x: 1,
                        y: row as i32,
                    },
                })
                .unwrap();
            service(harness);
            // A second press on the active object opens it, and the
            // inspector covers the view.
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    reader.dismiss_inspector(ctx)
                })
                .unwrap();
            service(harness);
            if armed {
                return check_screen(harness, *width);
            }
            let after = observed(harness);
            prop_assert_eq!(after.top, before.top);
            // A press on an object carries the cursor to the object's unit.
            let object = harness.with_root_widget(|reader: &mut Markdown| {
                let mut state = reader.shared.borrow_mut();
                let id = state.overlays.active_object?;
                let unit = state
                    .presentation
                    .as_ref()?
                    .document
                    .objects
                    .iter()
                    .find(|o| o.id == id)?
                    .unit_id;
                state.unit_of_reading(unit)
            });
            if let Some(unit) = object {
                prop_assert_eq!(after.marked, Some(unit), "the object's unit");
            } else if after.mode == Mode::Interact {
                let unit = extents
                    .iter()
                    .position(|rows| rows.contains(&(before.top + row)));
                prop_assert_eq!(after.marked, unit.or(before.marked));
            }
        }
        Action::Resize(new_width, new_height) => {
            let start = unit_start(harness);
            let shown = before
                .marked
                .is_some_and(|unit| scene.shows(unit, before.top));
            harness
                .canopy
                .set_screen_size(Size::new(new_width + 1, new_height + 1))
                .unwrap();
            *width = *new_width;
            settle(harness, manual);
            if before.mode == Mode::Interact && shown {
                prop_assert_eq!(
                    unit_start(harness),
                    start,
                    "the marked unit holds through a resize"
                );
            }
        }
        Action::Search => {
            let word = harness.with_root_widget(|reader: &mut Markdown| {
                let state = reader.shared.borrow();
                let document = &state.presentation.as_ref()?.document;
                let text = document.units.iter().rev().find_map(|unit| {
                    unit.text
                        .split_whitespace()
                        .find(|word| word.len() > 2)
                        .map(str::to_owned)
                })?;
                Some(text)
            });
            let Some(word) = word else {
                return Ok(());
            };
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    reader.set_search(ctx, word, false)
                })
                .unwrap();
            settle(harness, manual);
            let found = harness.with_root_widget(|reader: &mut Markdown| {
                let mut state = reader.shared.borrow_mut();
                let index = state.overlays.search_current?;
                let unit = state.overlays.search.get(index)?.start.unit_id;
                state.unit_of_reading(unit)
            });
            if let Some(unit) = found {
                prop_assert_eq!(observed(harness).marked, Some(unit), "the match's unit");
            }
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    reader.set_search(ctx, String::new(), false)
                })
                .unwrap();
            settle(harness, manual);
        }
        Action::Replace(text) => {
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    reader
                        .replace(ctx, DocumentInput::Complete(text.clone()))
                        .map(|_| ())
                })
                .unwrap();
            settle(harness, manual);
            let after = observed(harness);
            prop_assert_eq!(after.mode, resting);
            prop_assert_eq!(after.top, 0);
            if resting == Mode::Interact {
                let first = self::extents(harness)
                    .iter()
                    .position(|rows| !rows.is_empty());
                prop_assert_eq!(after.marked, first, "a new text marks its first unit");
            }
        }
        Action::Tab(forward) => {
            press(harness, if *forward { "Tab" } else { "shift-Tab" });
            let object = harness.with_root_widget(|reader: &mut Markdown| {
                let mut state = reader.shared.borrow_mut();
                let id = state.overlays.active_object?;
                let object = state
                    .presentation
                    .as_ref()?
                    .document
                    .objects
                    .iter()
                    .find(|o| o.id == id)?
                    .unit_id;
                state.unit_of_reading(object)
            });
            if let Some(unit) = object {
                let after = observed(harness);
                prop_assert_eq!(after.mode, Mode::Interact);
                prop_assert_eq!(after.marked, Some(unit), "Tab carries the cursor");
            }
        }
        Action::Escape => {
            let object = harness.with_root_widget(|reader: &mut Markdown| {
                reader.shared.borrow().overlays.active_object
            });
            press(harness, "Escape");
            let expected = if object.is_some() {
                before
            } else {
                model::escape(before, resting)
            };
            prop_assert_eq!(observed(harness), expected);
        }
    }
    check_screen(harness, *width)
}

proptest! {
    #![proptest_config(testing::config(16))]

    /// T3: the reader against the model, one operation at a time.
    #[test]
    fn the_block_cursor_follows_the_model(
        text in testing::document(),
        width in 2..60u32,
        height in 1..12u32,
        interact in any::<bool>(),
        actions in vec(action(), 1..24),
    ) {
        let resting = if interact { Mode::Interact } else { Mode::Scroll };
        let (mut harness, manual) = reader(&text, resting, width + 1, height + 1);
        settle(&mut harness, &manual);
        let mut width = width;
        check_screen(&mut harness, width)?;
        for action in &actions {
            step(&mut harness, &manual, resting, &mut width, action)?;
        }
    }

    /// From the first unit, `j` reaches each unit in order, and shows each
    /// row of a tall unit, with two rows shared between views.
    #[test]
    fn j_visits_every_unit_and_every_row_of_a_tall_one(
        text in testing::document(),
        width in 4..60u32,
        height in 1..12u32,
    ) {
        let (mut harness, manual) = reader(&text, Mode::Interact, width + 1, height + 1);
        settle(&mut harness, &manual);
        press(&mut harness, "g");
        let extents = extents(&mut harness);
        let units: Vec<_> = (0..extents.len()).filter(|&unit| !extents[unit].is_empty()).collect();
        let mut visited = Vec::new();
        let mut seen: Vec<Vec<u32>> = vec![Vec::new(); extents.len()];
        let mut previous: Option<(usize, u32)> = None;
        for _ in 0..(extents.iter().map(|rows| rows.len()).sum::<usize>() + units.len() + 2) {
            let Some((unit, rows)) = marked(&mut harness) else {
                break;
            };
            let (top, view_height) = view(&mut harness);
            if visited.last() != Some(&unit) {
                visited.push(unit);
            } else if let Some((last, last_top)) = previous
                && last == unit
                && view_height > 2
                && top != last_top
            {
                prop_assert!(top - last_top <= view_height - 2, "views {last_top} and {top} share too few rows");
            }
            seen[unit].extend(visible(&mut harness, &rows));
            previous = Some((unit, top));
            press(&mut harness, "j");
        }
        prop_assert_eq!(&visited, &units);
        for unit in units {
            let mut rows = seen[unit].clone();
            rows.sort_unstable();
            rows.dedup();
            let all: Vec<u32> = extents[unit].clone().collect();
            prop_assert_eq!(rows, all, "unit {} shows every row", unit);
        }
    }
}
