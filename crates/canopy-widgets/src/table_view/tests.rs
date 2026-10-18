use canopy::{
    geom::{PointI32, Size},
    input::key,
    testing::harness::Harness,
};

use super::*;
use crate::table::{TableAlignment, TableColumn, TableLayoutOptions, TableRow, prepare};
fn table(dataset: &str, revision: u64, rows: usize) -> Arc<PreparedTable> {
    Arc::new(
        prepare(
            Arc::new(
                TableData::new(
                    dataset,
                    revision,
                    vec![
                        TableColumn::new("a", "Alpha"),
                        TableColumn::new("b", "Beta"),
                        TableColumn::new("c", "Gamma"),
                    ],
                    (0..rows)
                        .map(|r| {
                            TableRow::new(
                                format!("r{r}"),
                                vec![
                                    format!("a{r}").into(),
                                    format!("b{r}").into(),
                                    format!("c{r}").into(),
                                ],
                            )
                        })
                        .collect(),
                )
                .unwrap(),
            ),
            TableLayoutOptions {
                widths: vec![5, 5, 5],
                ..Default::default()
            },
        )
        .unwrap(),
    )
}
fn harness(rows: usize, width: u32, height: u32) -> Result<Harness> {
    Harness::builder(TableView::new(table("d", 1, rows), Default::default()))
        .register::<TableView>()
        .size(width, height)
        .build()
}
fn with<R>(
    h: &mut Harness,
    f: impl FnOnce(&mut TableView, &mut dyn Context) -> Result<R>,
) -> Result<R> {
    h.with_root_widget_context(f)
}
fn mouse(action: mouse::Action, x: i32, y: i32) -> mouse::MouseEvent {
    mouse::MouseEvent {
        action,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: PointI32 { x, y },
    }
}
#[test]
fn mounted_default_keys_scroll_one_body_and_keep_chrome_fixed() -> Result<()> {
    let mut h = harness(30, 12, 6)?;
    h.render()?;
    assert!(h.tbuf().contains_text("Alpha"));
    for _ in 0..12 {
        h.key('j')?;
    }
    h.key(KeyCode::End)?;
    h.render()?;
    with(&mut h, |v, c| {
        assert_eq!(v.cursor_index(), Some((12, 2)));
        let body = c.view_of(v.body.unwrap().into()).unwrap();
        assert!(body.scroll.x > 0);
        assert!(body.scroll.y > 0);
        assert_eq!(c.children_of(c.node_id()).len(), 1);
        Ok(())
    })?;
    assert!(
        h.tbuf().contains_text("Gamma"),
        "header tracks horizontal offset"
    );
    assert!(h.tbuf().contains_text("c12"));
    h.key(key::Ctrl + KeyCode::Home)?;
    h.key(KeyCode::Home)?;
    h.render()?;
    with(&mut h, |v, c| {
        assert_eq!(v.cursor_index(), Some((0, 0)));
        assert_eq!(
            c.view_of(v.body.unwrap().into()).unwrap().scroll,
            Point::ZERO
        );
        Ok(())
    })?;
    Ok(())
}
#[test]
fn shift_rectangles_both_directions_copy_full_displayed_snapshot() -> Result<()> {
    let mut h = harness(4, 25, 6)?;
    h.key(key::Shift + KeyCode::Right)?;
    h.key(key::Shift + KeyCode::Down)?;
    with(&mut h, |v, c| {
        assert_eq!(v.selection().rows().len(), 2);
        assert_eq!(v.selection().columns().len(), 2);
        v.copy_selection(c, false)?;
        let events = v.take_events(1);
        let TableEvent::Copy { text, revision, .. } = &events[0] else {
            panic!("copy")
        };
        assert_eq!(text, "a0\tb0\na1\tb1\n");
        assert_eq!(*revision, 1);
        Ok(())
    })?;
    h.key(KeyCode::End)?;
    h.key(key::Shift + KeyCode::Left)?;
    h.key(key::Shift + KeyCode::Up)?;
    with(&mut h, |v, c| {
        v.copy_selection(c, false)?;
        let TableEvent::Copy { text, .. } = v.take_events(1).remove(0) else {
            panic!("copy")
        };
        assert_eq!(text, "b0\tc0\nb1\tc1\n");
        Ok(())
    })?;
    Ok(())
}
#[test]
fn publishing_reconciles_keys_selection_top_row_and_rejects_stale_data() -> Result<()> {
    let mut h = harness(20, 13, 5)?;
    with(&mut h, |v, c| {
        v.goto(c, 10, 2)?;
        v.select_column(c)?;
        Ok(())
    })?;
    h.render()?;
    let replacement = Arc::new(
        prepare(
            Arc::new(
                TableData::new(
                    "d",
                    2,
                    vec![TableColumn::new("b", "B"), TableColumn::new("a", "A")],
                    (0..20)
                        .rev()
                        .map(|r| {
                            TableRow::new(
                                format!("r{r}"),
                                vec![format!("new{r}").into(), "other".into()],
                            )
                        })
                        .collect(),
                )
                .unwrap(),
            ),
            TableLayoutOptions {
                widths: vec![5, 5],
                ..Default::default()
            },
        )
        .unwrap(),
    );
    with(&mut h, |v, c| {
        let old_top = c.view_of(v.body.unwrap().into()).unwrap().scroll.y;
        let old_key = v.table.data().rows()[old_top as usize].key.clone();
        v.publish(c, replacement)?;
        assert_eq!(
            v.cursor(),
            Some(CellKey {
                row: "r9".into(),
                column: "b".into()
            })
        );
        let top = c.view_of(v.body.unwrap().into()).unwrap().scroll.y;
        assert_eq!(v.table.data().rows()[top as usize].key, old_key);
        v.copy_selection(c, false)?;
        let TableEvent::Copy { text, revision, .. } = v.take_events(1).remove(0) else {
            panic!("copy")
        };
        assert_eq!(revision, 2);
        assert!(text.starts_with("new19\n"));
        assert!(v.publish(c, table("d", 1, 20)).is_err());
        assert_eq!(v.table.data().revision(), 2);
        v.publish(c, table("other", 0, 2))?;
        assert_eq!(v.cursor_index(), Some((0, 0)));
        assert!(v.selection.is_empty());
        Ok(())
    })?;
    Ok(())
}
#[test]
fn queue_and_copy_limits_are_atomic_and_activation_is_revision_addressed() -> Result<()> {
    let mut h = Harness::builder(TableView::new(
        table("d", 7, 2),
        TableViewOptions {
            event_capacity: 1,
            event_bytes: 100,
            copy_bytes: 3,
            ..Default::default()
        },
    ))
    .size(20, 5)
    .build()?;
    with(&mut h, |v, c| {
        assert!(v.copy_all(c).is_err());
        assert_eq!(v.pending_events(), (0, 0));
        v.activate(c)?;
        assert!(v.activate(c).is_err());
        assert_eq!(v.pending_events().0, 1);
        let TableEvent::Activate { revision, cell, .. } = v.take_events(1).remove(0) else {
            panic!("activate")
        };
        assert_eq!(revision, 7);
        assert_eq!(cell.row.as_ref(), "r0");
        assert_eq!(v.pending_events(), (0, 0));
        v.copy_cell(c)?;
        let TableEvent::Copy { text, .. } = v.take_events(1).remove(0) else {
            panic!("copy")
        };
        assert_eq!(text, "a0");
        Ok(())
    })?;
    Ok(())
}
#[test]
fn pointer_drag_has_rectangular_membership_and_releases_capture() -> Result<()> {
    let mut h = harness(5, 25, 7)?;
    h.render()?;
    h.mouse(mouse(mouse::Action::Down, 3, 1))?;
    h.mouse(mouse(mouse::Action::Drag, 9, 3))?;
    with(&mut h, |v, c| {
        assert!(c.has_mouse_capture());
        assert_eq!(v.selection.rows().len(), 3);
        assert_eq!(v.selection.columns().len(), 2);
        Ok(())
    })?;
    h.mouse(mouse(mouse::Action::Up, 9, 3))?;
    with(&mut h, |v, c| {
        assert!(!c.has_mouse_capture());
        assert!(!v.drag);
        Ok(())
    })?;
    Ok(())
}
#[test]
fn wheel_moves_view_without_moving_cursor() -> Result<()> {
    let mut h = harness(40, 15, 6)?;
    h.render()?;
    let mut event = mouse(mouse::Action::ScrollDown, 5, 2);
    event.button = mouse::Button::None;
    h.mouse(event)?;
    h.render()?;
    with(&mut h, |v, c| {
        assert_eq!(v.cursor_index(), Some((0, 0)));
        assert_eq!(c.view_of(v.body.unwrap().into()).unwrap().scroll.y, 3);
        Ok(())
    })?;
    Ok(())
}
#[test]
fn narrow_and_empty_axes_keep_navigation_and_header_copy() -> Result<()> {
    let mut h = harness(2, 3, 2)?;
    h.key('j')?;
    h.key('l')?;
    h.render()?;
    with(&mut h, |v, _| {
        assert_eq!(v.cursor_index(), Some((1, 1)));
        Ok(())
    })?;
    for width in 0..=4 {
        for height in 0..=3 {
            h.canopy.set_screen_size(Size::new(width, height))?;
            h.render()?;
        }
    }
    let prepared = Arc::new(prepare(
        Arc::new(TableData::builder("empty", 1, ["Only"]).build()?),
        Default::default(),
    )?);
    with(&mut h, |v, c| {
        v.publish(c, prepared)?;
        v.move_cursor(c, i32::MAX, i32::MIN, true)?;
        assert!(v.cursor().is_none());
        v.copy_all(c)?;
        let TableEvent::Copy { text, .. } = v.take_events(1).remove(0) else {
            panic!("copy")
        };
        assert_eq!(text, "Only\n");
        Ok(())
    })?;
    Ok(())
}
#[test]
fn coordinate_prompt_tab_and_all_four_boundaries_are_deterministic() -> Result<()> {
    let mut h = harness(3, 25, 6)?;
    h.key(':')?;
    for c in "3,2".chars() {
        h.key(c)?;
    }
    h.key(KeyCode::Enter)?;
    h.key(KeyCode::Tab)?;
    h.key(KeyCode::Tab)?;
    with(&mut h, |v, _| {
        assert_eq!(v.cursor_index(), Some((2, 2)));
        Ok(())
    })?;
    h.key(key::Ctrl + KeyCode::Home)?;
    h.key(KeyCode::Home)?;
    h.key(KeyCode::Left)?;
    h.key(KeyCode::Up)?;
    h.key(KeyCode::BackTab)?;
    with(&mut h, |v, _| {
        assert_eq!(v.cursor_index(), Some((0, 0)));
        Ok(())
    })?;
    Ok(())
}
#[test]
fn vi_letters_jump_to_edges_and_shifted_letters_extend() -> Result<()> {
    let mut h = harness(3, 25, 6)?;
    h.key('G')?;
    h.key('$')?;
    with(&mut h, |v, _| {
        assert_eq!(v.cursor_index(), Some((2, 2)));
        Ok(())
    })?;
    h.key('g')?;
    h.key('0')?;
    with(&mut h, |v, _| {
        assert_eq!(v.cursor_index(), Some((0, 0)));
        assert!(v.selection().is_empty());
        Ok(())
    })?;
    h.key('J')?;
    h.key('L')?;
    with(&mut h, |v, _| {
        assert_eq!(v.cursor_index(), Some((1, 1)));
        assert_eq!(v.selection().rows().len(), 2);
        assert_eq!(v.selection().columns().len(), 2);
        Ok(())
    })?;
    Ok(())
}

struct Preview {
    table: Arc<PreparedTable>,
    stats: Rc<RefCell<TablePaintStats>>,
}
impl Widget for Preview {
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn render(&mut self, r: &mut Render, c: &dyn ViewContext) -> Result<()> {
        *self.stats.borrow_mut() = self.table.paint(
            r,
            c.view().outer_rect_local(),
            TableWindow {
                offset: Point::ZERO,
                header: true,
                row_numbers: true,
            },
            None,
            &TableSelection::default(),
        )?;
        Ok(())
    }
}
#[test]
fn shared_preview_and_mounted_grid_paint_the_same_cells_and_hit_boundaries() -> Result<()> {
    let prepared = table("d", 1, 3);
    let stats = Rc::new(RefCell::new(Default::default()));
    let mut preview = Harness::builder(Preview {
        table: prepared.clone(),
        stats: stats.clone(),
    })
    .size(19, 4)
    .build()?;
    let mut mounted = Harness::builder(TableView::new(prepared.clone(), Default::default()))
        .size(20, 5)
        .build()?;
    preview.render()?;
    mounted.render()?;
    for y in 0..4 {
        for x in 0..19 {
            let p = Point { x, y };
            assert_eq!(
                preview.buf().get(p).unwrap().ch,
                mounted.buf().get(p).unwrap().ch,
                "at {p:?}"
            );
        }
    }
    assert_eq!(stats.borrow().cells, 9);
    assert_eq!(
        prepared.hit_test(
            Rect::new(0, 0, 19, 4),
            TableWindow {
                offset: Point::ZERO,
                header: true,
                row_numbers: true
            },
            Point { x: 8, y: 2 }
        ),
        Some(TableHit::Cell { row: 1, column: 1 })
    );
    mounted.mouse(mouse(mouse::Action::Down, 8, 2))?;
    mounted.mouse(mouse(mouse::Action::Up, 8, 2))?;
    with(&mut mounted, |v, _| {
        assert_eq!(v.cursor_index(), Some((1, 1)));
        Ok(())
    })?;
    Ok(())
}
/// Fixed viewport geometry removes row-number digit width from the comparison.
struct ScalePreview {
    table: Arc<PreparedTable>,
    window: TableWindow,
    stats: Rc<RefCell<TablePaintStats>>,
}
impl Widget for ScalePreview {
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn render(&mut self, r: &mut Render, c: &dyn ViewContext) -> Result<()> {
        *self.stats.borrow_mut() = self.table.paint(
            r,
            c.view().outer_rect_local(),
            self.window,
            None,
            &TableSelection::default(),
        )?;
        Ok(())
    }
}
#[test]
fn visible_paint_work_is_independent_of_dataset_rows() -> Result<()> {
    for rows in [500, 5_000, 50_000] {
        let prepared = table("d", 1, rows);
        for top in [0, rows / 2, rows - 10] {
            let stats = Rc::new(RefCell::new(Default::default()));
            let mut h = Harness::builder(ScalePreview {
                table: prepared.clone(),
                window: TableWindow {
                    offset: Point {
                        x: 6,
                        y: top as u32,
                    },
                    header: true,
                    row_numbers: false,
                },
                stats: stats.clone(),
            })
            .size(8, 5)
            .build()?;
            h.render()?;
            let observed = *stats.borrow();
            assert_eq!(observed.columns_visited, 2, "{rows} rows, top {top}");
            assert_eq!(
                observed.rows_visited, 8,
                "exactly four visible rows in each of two columns"
            );
            assert_eq!(observed.cells, 8);
            assert_eq!(observed.labels, 2);
            h.render()?;
            assert_eq!(*stats.borrow(), observed);
        }
    }
    Ok(())
}
#[test]
fn mounted_navigation_reuses_preparation_and_visits_only_visible_rows() -> Result<()> {
    for rows in [500, 5_000, 50_000] {
        let prepared = table("d", 1, rows);
        let sample_bytes = prepared.sampled_bytes();
        let mut h = Harness::builder(TableView::new(
            prepared.clone(),
            TableViewOptions {
                row_numbers: false,
                ..Default::default()
            },
        ))
        .size(13, 7)
        .build()?;
        with(&mut h, |v, c| v.goto(c, rows / 2, 2))?;
        h.render()?;
        for _ in 0..16 {
            h.key('j')?;
            h.key('k')?;
            h.key('l')?;
            h.key('h')?;
        }
        h.render()?;
        with(&mut h, |v, c| {
            assert!(
                Arc::ptr_eq(v.table(), &prepared),
                "navigation keeps the prepared snapshot"
            );
            assert_eq!(v.table().sampled_bytes(), sample_bytes);
            assert_eq!(c.children_of(c.node_id()).len(), 1);
            let stats = v.paint_stats();
            assert!(
                stats.columns_visited <= 3,
                "at most two cells and a trailing separator"
            );
            assert_eq!(stats.rows_visited, stats.columns_visited * 5);
            assert!(
                stats.rows_visited - stats.cells <= 5,
                "only a trailing separator can lack cell content"
            );
            assert!(
                stats.cells <= 10,
                "{rows} rows still paint at most ten body cells"
            );
            Ok(())
        })?;
    }
    Ok(())
}
#[test]
fn styled_wide_graphemes_controls_and_clipped_hits_preserve_full_copy() -> Result<()> {
    use canopy::style::{AttrSet, Color, Paint, Style};

    use crate::table::{TableCell, TableSpan};
    let value = "界\tX\n\u{1b}";
    let cell = TableCell::styled(
        value,
        vec![TableSpan {
            range: 0..3,
            style: Style {
                fg: Paint::solid(Color::Red),
                bg: Paint::solid(Color::Blue),
                attrs: AttrSet::default(),
            },
        }],
    )?;
    let prepared = Arc::new(prepare(
        Arc::new(TableData::new(
            "d",
            1,
            vec![TableColumn::new("c", "wide")],
            vec![TableRow::new("r", vec![cell])],
        )?),
        TableLayoutOptions {
            widths: vec![4],
            ..Default::default()
        },
    )?);
    let mut h = Harness::builder(TableView::new(prepared, Default::default()))
        .size(7, 4)
        .build()?;
    h.render()?;
    assert_eq!(h.buf().get(Point { x: 2, y: 1 }).unwrap().ch, '界');
    assert_eq!(
        h.buf().get(Point { x: 2, y: 1 }).unwrap().style.fg,
        Color::Red
    );
    with(&mut h, |v, c| {
        v.copy_cell(c)?;
        let TableEvent::Copy { text, .. } = v.take_events(1).remove(0) else {
            panic!("copy")
        };
        assert_eq!(text, value);
        c.scroll_node(v.body.unwrap().into(), ScrollOp::To(Point { x: 1, y: 0 }))?;
        Ok(())
    })?;
    h.render()?;
    Ok(())
}
#[test]
fn geometry_publication_can_preserve_top_key_beyond_old_canvas_extent() -> Result<()> {
    let mut h = harness(3, 15, 5)?;
    h.render()?;
    let data = Arc::new(TableData::new(
        "d",
        2,
        vec![TableColumn::new("a", "A")],
        (0..30)
            .rev()
            .map(|r| TableRow::new(format!("r{r}"), vec!["value".into()]))
            .collect(),
    )?);
    let prepared = Arc::new(prepare(data, Default::default())?);
    with(&mut h, |v, c| v.publish(c, prepared))?;
    h.render()?;
    with(&mut h, |v, c| {
        let body = c.view_of(v.body.unwrap().into()).unwrap();
        assert_eq!(
            v.table.data().rows()[body.scroll.y as usize].key.as_ref(),
            "r2",
            "last row clamps while retaining the closest possible neighborhood"
        );
        Ok(())
    })?;
    Ok(())
}

#[test]
fn copy_capability_agrees_with_keys_commands_and_direct_calls() -> Result<()> {
    let mut h = harness(2, 20, 5)?;
    with(&mut h, |v, c| {
        v.set_copy_enabled(false);
        assert_eq!(v.key_outcome(key::Empty + 'y', c), EventOutcome::Ignore);
        assert_eq!(v.key_outcome(key::Ctrl + 'c', c), EventOutcome::Ignore);
        assert!(matches!(v.copy_status(c)?, CommandStatus::Disabled(_)));
        assert!(v.copy_all(c).is_err());
        assert_eq!(v.pending_events(), (0, 0));
        v.set_copy_enabled(true);
        assert_eq!(v.key_outcome(key::Empty + 'y', c), EventOutcome::Handle);
        Ok(())
    })?;
    Ok(())
}
#[test]
fn scrollbar_escape_releases_capture_without_moving_cursor() -> Result<()> {
    let mut h = harness(50, 15, 6)?;
    h.render()?;
    h.mouse(mouse(mouse::Action::Down, 14, 3))?;
    with(&mut h, |v, c| {
        assert!(c.has_mouse_capture());
        assert!(v.scroll_drag);
        assert_eq!(v.cursor_index(), Some((0, 0)));
        Ok(())
    })?;
    h.key(KeyCode::Esc)?;
    with(&mut h, |v, c| {
        assert!(!c.has_mouse_capture());
        assert!(!v.scroll_drag);
        Ok(())
    })?;
    Ok(())
}
#[test]
fn missing_keys_clamp_axes_and_clear_empty_selection_membership() -> Result<()> {
    let mut h = harness(8, 22, 6)?;
    with(&mut h, |v, c| {
        v.goto(c, 8, 3)?;
        v.select_column(c)?;
        let prepared = Arc::new(prepare(
            Arc::new(TableData::new(
                "d",
                2,
                vec![TableColumn::new("a", "A")],
                vec![TableRow::new("r0", vec!["remaining".into()])],
            )?),
            Default::default(),
        )?);
        v.publish(c, prepared)?;
        assert_eq!(v.cursor_index(), Some((0, 0)));
        assert!(v.selection.is_empty());
        Ok(())
    })?;
    Ok(())
}

#[test]
fn header_and_gutter_selection_focus_the_body_for_keyboard_actions() -> Result<()> {
    let mut h = Harness::builder(crate::Container::stack().focusable())
        .register::<TableView>()
        .size(25, 7)
        .build()?;
    h.canopy.with_root_context(|c| {
        c.add_child(
            c.node_id(),
            TableView::new(table("d", 1, 5), Default::default()),
        )?;
        Ok(())
    })?;
    h.render()?;
    for (x, y, selected_rows, selected_columns) in [(3, 0, 5, 1), (0, 2, 1, 3)] {
        h.canopy
            .with_root_context(|c| c.set_focus(c.node_id()).map(|_| ()))?;
        h.mouse(mouse(mouse::Action::Down, x, y))?;
        h.with_unique(|v: &mut TableView, c| {
            assert_eq!(c.focused_node(), v.initial_focus());
            assert_eq!(v.selection().rows().len(), selected_rows);
            assert_eq!(v.selection().columns().len(), selected_columns);
            Ok(())
        })?;
        h.key('y')?;
        h.with_unique(|v: &mut TableView, _| {
            assert!(matches!(
                v.take_events(1).as_slice(),
                [TableEvent::Copy { .. }]
            ));
            Ok(())
        })?;
    }
    Ok(())
}

struct ClippedPreview {
    table: Arc<PreparedTable>,
    area: Rect,
    offset: Point,
}
impl Widget for ClippedPreview {
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn render(&mut self, render: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        self.table.paint(
            render,
            self.area,
            TableWindow {
                offset: self.offset,
                header: true,
                row_numbers: false,
            },
            None,
            &TableSelection::default(),
        )?;
        Ok(())
    }
}

#[test]
fn separator_paints_when_next_column_is_outside_the_clip() -> Result<()> {
    for (offset, width) in [(0, 6), (5, 1)] {
        let mut h = Harness::builder(ClippedPreview {
            table: table("d", 1, 2),
            area: Rect::new(2, 0, width, 3),
            offset: Point::new(offset, 0),
        })
        .size(10, 3)
        .build()?;
        h.render()?;
        for y in 0..3 {
            assert_eq!(h.buf().get(Point::new(2 + width - 1, y)).unwrap().ch, '│');
        }
    }
    Ok(())
}

#[test]
fn large_canvas_offsets_translate_before_adding_the_render_origin() -> Result<()> {
    let mut column = TableColumn::new("c", "H");
    column.alignment = TableAlignment::Right;
    let prepared = Arc::new(prepare(
        Arc::new(TableData::new(
            "d",
            1,
            vec![column],
            vec![TableRow::new("r", vec!["V".into()])],
        )?),
        TableLayoutOptions {
            widths: vec![u32::MAX],
            ..Default::default()
        },
    )?);
    let mut h = Harness::builder(ClippedPreview {
        table: prepared,
        area: Rect::new(10, 0, 4, 2),
        offset: Point::new(u32::MAX - 4, 0),
    })
    .size(14, 2)
    .build()?;
    h.render()?;
    assert_eq!(h.buf().get(Point::new(13, 0)).unwrap().ch, 'H');
    assert_eq!(h.buf().get(Point::new(13, 1)).unwrap().ch, 'V');
    Ok(())
}

#[test]
fn shift_click_keeps_its_anchor_and_the_gutter_stops_at_the_body() -> Result<()> {
    let mut h = harness(20, 25, 7)?;
    h.render()?;
    // The gutter is three cells wide and the body starts on row one.
    h.mouse(mouse(mouse::Action::Down, 4, 1))?;
    h.mouse(mouse(mouse::Action::Up, 4, 1))?;
    h.mouse(mouse::MouseEvent {
        modifiers: key::Shift,
        ..mouse(mouse::Action::Down, 10, 3)
    })?;
    h.mouse(mouse(mouse::Action::Up, 10, 3))?;
    h.key(key::Shift + KeyCode::Down)?;
    with(&mut h, |v, _| {
        assert_eq!(v.cursor_index(), Some((3, 1)));
        assert_eq!(v.selection().rows().len(), 4, "rows extend from the click");
        assert_eq!(v.selection().columns().len(), 2);
        Ok(())
    })?;
    // The bottom row holds the horizontal scrollbar, not a row number.
    h.mouse(mouse(mouse::Action::Down, 0, 6))?;
    with(&mut h, |v, _| {
        assert_eq!(v.cursor_index(), Some((3, 1)));
        assert_eq!(v.selection().rows().len(), 4);
        Ok(())
    })?;
    Ok(())
}

#[test]
fn space_pages_down_like_page_down() -> Result<()> {
    let mut cursors = Vec::new();
    for page in [KeyCode::PageDown, KeyCode::Char(' ')] {
        let mut h = harness(30, 12, 6)?;
        h.render()?;
        h.key(page)?;
        cursors.push(with(&mut h, |v, _| Ok(v.cursor_index()))?);
    }
    assert!(cursors[0].is_some_and(|(row, _)| row > 0), "{cursors:?}");
    assert_eq!(cursors[0], cursors[1]);
    Ok(())
}

#[test]
fn a_wrapping_view_shows_every_value_in_full() -> Result<()> {
    let notes = "one two three four five six seven eight nine ten eleven twelve";
    let data = Arc::new(
        TableData::builder("w", 1, ["Key", "Notes"])
            .row("a", ["k1", notes])
            .row("b", ["k2", "short"])
            .row("c", ["k3", "also short"])
            .build()
            .unwrap(),
    );
    let prepared = Arc::new(prepare(
        data,
        TableLayoutOptions {
            wrap: true,
            ..Default::default()
        },
    )?);
    let mut h = Harness::builder(TableView::new(prepared, Default::default()))
        .register::<TableView>()
        .size(30, 12)
        .build()?;
    h.render()?;
    let screen = h.tbuf().lines().join("\n");
    for word in notes.split(' ') {
        assert!(screen.contains(word), "{word} is cut from:\n{screen}");
    }
    assert!(
        screen.contains('─'),
        "rules separate wrapped rows:\n{screen}"
    );
    // A page moves by the rows that fit, so the cursor lands on a row.
    h.key(key::KeyCode::PageDown)?;
    let cursor = with(&mut h, |view, _| Ok(view.cursor_index()))?;
    assert!(cursor.is_some_and(|(row, _)| row > 0), "{cursor:?}");
    Ok(())
}
