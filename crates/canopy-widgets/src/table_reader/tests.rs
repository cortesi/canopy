use canopy::{style::PartialStyle, testing::harness::Harness};

use super::*;
use crate::{
    regex_search::{cell_matches, smart_matcher},
    table::{TableColumn, TableRow},
};

/// A table of `rows` rows whose cells name their row and column.
fn data(columns: &[&str], rows: usize) -> Arc<TableData> {
    Arc::new(
        TableData::new(
            "t",
            1,
            columns
                .iter()
                .map(|label| TableColumn::new(*label, *label))
                .collect(),
            (0..rows)
                .map(|row| {
                    TableRow::new(
                        row.to_string(),
                        columns
                            .iter()
                            .map(|label| format!("{label}{row}").into())
                            .collect(),
                    )
                })
                .collect(),
        )
        .unwrap(),
    )
}

fn harness(data: Arc<TableData>, width: u32, height: u32) -> Harness {
    Harness::builder(TableReader::new(data))
        .register::<TableReader>()
        .size(width, height)
        .build()
        .unwrap()
}

fn with<R>(h: &mut Harness, f: impl FnOnce(&mut TableReader, &mut dyn Context) -> R) -> R {
    h.with_root_widget_context(|reader: &mut TableReader, ctx| Ok(f(reader, ctx)))
        .unwrap()
}

/// Install the matches of `pattern`.
fn search(h: &mut Harness, pattern: &str) {
    with(h, |reader, ctx| {
        let matches = cell_matches(&smart_matcher(pattern).unwrap(), reader.data());
        reader.set_matches(ctx, matches);
    });
    h.render().unwrap();
}

fn scroll(h: &mut Harness) -> u32 {
    with(h, |_, ctx| ctx.view().scroll.y)
}

#[test]
fn the_header_stays_while_the_rows_scroll() {
    let mut h = harness(data(&["Key", "Value"], 40), 30, 8);
    h.render().unwrap();
    let lines = h.tbuf().lines();
    assert!(lines[0].contains("Key") && lines[0].contains("Value"));
    assert!(lines[2].contains("Key0"));
    with(&mut h, |reader, ctx| {
        reader.scroll(ctx, ScrollOp::Lines(ScrollDirection::Down, 5))
    });
    h.render().unwrap();
    let lines = h.tbuf().lines();
    assert!(lines[0].contains("Key"), "{lines:?}");
    assert!(lines[2].contains("Key5"), "{lines:?}");
    // A page moves by the rows beneath the header, less one.
    with(&mut h, |reader, ctx| reader.scroll(ctx, ScrollOp::pages(1)));
    h.render().unwrap();
    assert!(
        h.tbuf().lines()[2].contains("Key10"),
        "{:?}",
        h.tbuf().lines()
    );
    // The last row reaches the bottom of the view.
    with(&mut h, |reader, ctx| {
        reader.scroll(ctx, ScrollOp::To(Point::new(0, u32::MAX)))
    });
    h.render().unwrap();
    assert!(
        h.tbuf().lines()[7].contains("Key39"),
        "{:?}",
        h.tbuf().lines()
    );
}

#[test]
fn a_wide_table_names_its_columns_and_pages_through_them() {
    let columns = ["Alpha", "Bravo", "Charlie", "Delta", "Echo"];
    let mut h = harness(data(&columns, 3), 24, 8);
    h.render().unwrap();
    let lines = h.tbuf().lines();
    let shown = |lines: &[String]| {
        columns
            .iter()
            .filter(|label| lines[1].contains(*label))
            .count()
    };
    let first = shown(&lines);
    assert!(first < columns.len());
    assert!(
        lines[0].contains(&format!("{first} of 5 columns")),
        "{lines:?}"
    );
    assert!(lines[1].contains('›'));
    with(&mut h, |reader, ctx| reader.shift_columns(ctx, 1));
    h.render().unwrap();
    let lines = h.tbuf().lines();
    assert!(lines[0].contains("columns 2–"), "{lines:?}");
    assert!(lines[1].trim_start().starts_with('‹'), "{lines:?}");
    assert!(!lines[1].contains("Alpha"));
    // Paging past the last column stops where the rest shows.
    for _ in 0..10 {
        with(&mut h, |reader, ctx| reader.shift_columns(ctx, 1));
    }
    h.render().unwrap();
    assert!(h.tbuf().lines()[1].contains("Echo"));
    let last = with(&mut h, |reader, _| reader.first_column());
    assert!(last < columns.len());
    with(&mut h, |reader, ctx| reader.shift_columns(ctx, -10));
    assert_eq!(with(&mut h, |reader, _| reader.first_column()), 0);
}

#[test]
fn search_marks_cell_text_and_steps_between_matches() {
    let mut h = harness(data(&["Key", "Value"], 60), 30, 10);
    search(&mut h, "Value1");
    // Value1 and Value10 through Value19 all match.
    let (matches, position) = with(&mut h, |reader, _| {
        (reader.search_matches(), reader.search_position())
    });
    assert_eq!((matches, position), (11, 1));
    let ground = |h: &Harness, role| PartialStyle::new().bg(h.canopy.style().resolve(role).bg);
    let (current, other) = (
        ground(&h, view_search::CURRENT),
        ground(&h, view_search::MATCH),
    );
    assert!(h.tbuf().contains_text_style("Value1", &current));
    assert!(!h.tbuf().contains_text_style("Key1", &current));
    assert!(!h.tbuf().contains_text_style("Key1", &other));
    // A step reveals the match near the middle of the rows.
    for _ in 0..5 {
        with(&mut h, |reader, ctx| reader.search_next(ctx, 1));
    }
    h.render().unwrap();
    let lines = h.tbuf().lines();
    let y = lines
        .iter()
        .position(|line| line.contains("Value14"))
        .unwrap();
    assert!((4..=6).contains(&y), "{y} in {lines:?}");
    assert!(scroll(&mut h) > 0);
    // Each row that holds a match marks the scrollbar once.
    let marks = with(&mut h, |reader, ctx| {
        reader.scroll_marks(ScrollAxis::Vertical, ctx.view().content.size())
    });
    assert_eq!(marks.len(), 11);
    with(&mut h, |reader, ctx| reader.clear_search(ctx));
    h.render().unwrap();
    assert!(!h.tbuf().contains_text_style("Value14", &current));
}

#[test]
fn a_match_in_a_cut_column_brings_its_column_into_view() {
    let columns = ["Alpha", "Bravo", "Charlie", "Delta", "Echo"];
    let mut h = harness(data(&columns, 3), 24, 8);
    search(&mut h, "Echo2");
    assert_eq!(with(&mut h, |reader, _| reader.search_position()), 1);
    assert!(with(&mut h, |reader, _| reader.first_column()) > 0);
    let current = PartialStyle::new().bg(h.canopy.style().resolve(view_search::CURRENT).bg);
    assert!(h.tbuf().contains_text_style("Echo2", &current));
}

#[test]
fn a_new_table_drops_the_search_and_starts_at_its_first_column() {
    let columns = ["Alpha", "Bravo", "Charlie", "Delta", "Echo"];
    let mut h = harness(data(&columns, 3), 24, 8);
    search(&mut h, "Echo");
    with(&mut h, |reader, _| reader.set_data(data(&["Key"], 2)));
    h.render().unwrap();
    assert_eq!(
        with(&mut h, |reader, _| (
            reader.search_matches(),
            reader.first_column()
        )),
        (0, 0)
    );
    assert!(h.tbuf().contains_text("Key1"));
}

#[test]
fn a_search_before_layout_keeps_the_columns_that_fit() {
    // The matches arrive before the reader has a width, as when a host
    // shows a new table and runs its held query at once.
    let mut h = harness(data(&["Key", "Value", "Notes"], 30), 60, 8);
    with(&mut h, |reader, ctx| {
        let matches = cell_matches(&smart_matcher("Notes20").unwrap(), reader.data());
        reader.set_matches(ctx, matches);
    });
    h.render().unwrap();
    assert_eq!(with(&mut h, |reader, _| reader.first_column()), 0);
    assert!(h.tbuf().contains_text("Notes20"));
    assert!(h.tbuf().contains_text("Key20"));
}

#[test]
fn a_match_past_the_view_keeps_the_leading_columns_that_fit_beside_it() {
    let columns = ["Alpha", "Bravo", "Charlie", "Delta", "Echo"];
    let mut h = harness(data(&columns, 3), 24, 8);
    h.render().unwrap();
    search(&mut h, "Delta1");
    let first = with(&mut h, |reader, _| reader.first_column());
    assert!(first > 0 && first <= 3, "{first}");
    // The column before the first shown would leave no room for the match.
    let earlier = with(&mut h, |reader, ctx| {
        reader
            .prepare(inner(ctx.view().content.w), first - 1)
            .columns()
    });
    assert!(!earlier.contains(&3), "{earlier:?}");
}

#[test]
fn a_reload_keeps_the_current_match_and_the_view() {
    let mut h = harness(data(&["Key", "Value"], 60), 30, 10);
    search(&mut h, "Value1");
    for _ in 0..4 {
        with(&mut h, |reader, ctx| reader.search_next(ctx, 1));
    }
    h.render().unwrap();
    let (current, offset) = with(&mut h, |reader, ctx| {
        (reader.current_match().cloned(), ctx.view().scroll.y)
    });
    assert_eq!(current.as_ref().map(|found| found.row), Some(Some(13)));
    // The new revision holds one more row above the match.
    let next = data(&["Key", "Value"], 61);
    with(&mut h, |reader, _| {
        reader.set_data(next.clone());
        let found = cell_matches(&smart_matcher("Value1").unwrap(), &next);
        reader.restore_matches(found, current.as_ref());
    });
    h.render().unwrap();
    let (position, scroll) = with(&mut h, |reader, ctx| {
        (reader.search_position(), ctx.view().scroll.y)
    });
    assert_eq!((position, scroll), (5, offset));
    // With none current before, none is after.
    with(&mut h, |reader, _| {
        let found = cell_matches(&smart_matcher("Value1").unwrap(), &next);
        reader.restore_matches(found, None);
    });
    assert_eq!(with(&mut h, |reader, _| reader.search_position()), 0);
}

#[test]
fn interact_selects_a_cell_and_moves_it_with_the_intents() {
    let mut h = harness(data(&["Key", "Value"], 40), 30, 8);
    h.render().unwrap();
    with(&mut h, |reader, ctx| {
        reader.scroll(ctx, ScrollOp::Lines(ScrollDirection::Down, 5))
    });
    h.render().unwrap();
    // The selection starts at the first cell in view.
    with(&mut h, |reader, ctx| reader.interact(ctx));
    h.render().unwrap();
    assert_eq!(with(&mut h, |reader, _| reader.cursor()), Some((5, 0)));
    let cursor = PartialStyle::new().bg(h.canopy.style().resolve(CURSOR).bg);
    assert!(h.tbuf().contains_text_style("Key5", &cursor));
    assert!(!h.tbuf().contains_text_style("Key6", &cursor));
    let nav = |h: &mut Harness, nav: NavIntent| {
        with(h, |reader, ctx| reader.on_intent(nav.name(), ctx).unwrap());
        h.render().unwrap();
    };
    nav(&mut h, NavIntent::Right);
    nav(&mut h, NavIntent::Down);
    assert_eq!(with(&mut h, |reader, _| reader.cursor()), Some((6, 1)));
    assert_eq!(
        with(&mut h, |reader, _| reader.selected_text()),
        Some("Value6".to_owned())
    );
    assert!(h.tbuf().contains_text_style("Value6", &cursor));
    // The rows follow a selection that leaves the view, and no further.
    for _ in 0..10 {
        nav(&mut h, NavIntent::Down);
    }
    let lines = h.tbuf().lines();
    assert_eq!(lines.last().map(|line| line.contains("Key16")), Some(true));
    assert_eq!(scroll(&mut h), 11);
    nav(&mut h, NavIntent::Last);
    assert!(h.tbuf().lines()[7].contains("Key39"));
    nav(&mut h, NavIntent::First);
    assert_eq!(with(&mut h, |reader, _| reader.cursor()), Some((0, 1)));
    assert_eq!(scroll(&mut h), 0);
    // A page moves the selection and the rows together.
    nav(&mut h, NavIntent::PageDown);
    let (cursor_row, offset) = (
        with(&mut h, |reader, _| reader.cursor()).unwrap().0,
        scroll(&mut h),
    );
    assert_eq!((cursor_row, offset), (5, 5));
    let status = with(&mut h, |reader, ctx| reader.status(ctx));
    assert_eq!(status[0].spans[0].text, "row 6 · Value");
    with(&mut h, |reader, ctx| reader.leave_interact(ctx));
    h.render().unwrap();
    assert!(!h.tbuf().contains_text_style("Value5", &cursor));
    nav(&mut h, NavIntent::Down);
    assert_eq!(scroll(&mut h), 6, "the rows move again");
}

#[test]
fn a_selection_in_a_cut_column_pages_the_columns_to_it() {
    let columns = ["Alpha", "Bravo", "Charlie", "Delta", "Echo"];
    let mut h = harness(data(&columns, 3), 24, 8);
    h.render().unwrap();
    with(&mut h, |reader, ctx| reader.interact(ctx));
    for _ in 0..4 {
        with(&mut h, |reader, ctx| reader.move_cursor(ctx, 0, 1));
        h.render().unwrap();
    }
    assert_eq!(with(&mut h, |reader, _| reader.cursor()), Some((0, 4)));
    assert!(with(&mut h, |reader, _| reader.first_column()) > 0);
    assert!(h.tbuf().contains_text("Echo0"));
    // A search step selects its match's cell.
    search(&mut h, "Alpha2");
    assert_eq!(with(&mut h, |reader, _| reader.cursor()), Some((2, 0)));
    assert_eq!(with(&mut h, |reader, _| reader.first_column()), 0);
}

/// A table whose first column wraps prose over several lines a row, beside
/// short columns.
fn wrapped(rows: usize) -> Arc<TableData> {
    let columns = ["Notes", "B", "C", "D", "E", "F", "G", "H"];
    Arc::new(
        TableData::new(
            "t",
            1,
            columns
                .iter()
                .map(|label| TableColumn::new(*label, *label))
                .collect(),
            (0..rows)
                .map(|row| {
                    let mut cells = vec![format!("row {row} has a long note that wraps").into()];
                    cells.extend(
                        columns[1..]
                            .iter()
                            .map(|label| format!("{label}{row}").into()),
                    );
                    TableRow::new(row.to_string(), cells)
                })
                .collect(),
        )
        .unwrap(),
    )
}

#[test]
fn a_sideways_step_keeps_the_top_row() {
    let mut h = harness(wrapped(60), 30, 12);
    h.render().unwrap();
    // Scroll a few rows down, to the top of a row.
    let line = with(&mut h, |reader, ctx| {
        let laid = reader.laid(ctx.view().content.w);
        laid.flow.row_line(Some(10)).unwrap() - laid.body_start()
    });
    with(&mut h, |reader, ctx| {
        reader.scroll(ctx, ScrollOp::To(Point::new(0, line)))
    });
    h.render().unwrap();
    assert!(h.tbuf().contains_text("B10"));
    with(&mut h, |reader, ctx| reader.shift_columns(ctx, 1));
    h.render().unwrap();
    let lines = h.tbuf().lines();
    let body = lines
        .iter()
        .skip(3)
        .find(|line| !line.trim().is_empty())
        .unwrap();
    assert!(body.contains("B10"), "{lines:?}");
}

#[test]
fn a_revealed_match_shows_beneath_the_pinned_rows() {
    let columns = ["Alpha", "Bravo", "Charlie", "Delta", "Echo"];
    // Five rows: the label, the header, its rule, and two body rows.
    let mut h = harness(data(&columns, 40), 24, 5);
    search(&mut h, "Alpha30");
    assert!(h.tbuf().contains_text("Alpha30"), "{:?}", h.tbuf().lines());
}

#[test]
fn a_table_without_columns_selects_no_cell() {
    let empty =
        Arc::new(TableData::new("t", 1, Vec::new(), vec![TableRow::new("0", Vec::new())]).unwrap());
    let mut h = harness(empty, 24, 5);
    h.render().unwrap();
    with(&mut h, |reader, ctx| {
        reader.interact(ctx);
        reader.set_cursor(Some((0, 0)));
    });
    assert_eq!(with(&mut h, |reader, _| reader.selected_text()), None);
}

#[test]
fn a_header_match_selects_a_cell_in_its_column() {
    let mut h = harness(data(&["Key", "Value"], 5), 30, 10);
    h.render().unwrap();
    with(&mut h, |reader, ctx| {
        reader.interact(ctx);
        reader.move_cursor(ctx, 2, 0);
    });
    search(&mut h, "Value$");
    assert_eq!(with(&mut h, |reader, _| reader.cursor()), Some((2, 1)));
}
