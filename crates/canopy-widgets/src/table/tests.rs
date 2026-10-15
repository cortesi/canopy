use canopy::style::{AttrSet, Color, Paint};

use super::*;

fn fixture() -> Arc<TableData> {
    Arc::new(
        TableData::builder("dataset", 1, ["First", "First"])
            .row("a", ["", "a\tb"])
            .row("b", ["line\nnext", "say \"yes\""])
            .build()
            .unwrap(),
    )
}
#[test]
fn validates_axes_shape_and_utf8_styles() {
    assert!(
        TableData::builder("d", 0, ["x"])
            .row("r", ["a"])
            .row("r", ["b"])
            .build()
            .is_err()
    );
    assert!(
        TableData::new(
            "d",
            0,
            vec![TableColumn::new("x", "a"), TableColumn::new("x", "b")],
            vec![]
        )
        .is_err()
    );
    assert!(
        TableData::builder("d", 0, ["x"])
            .row("r", ["a", "b"])
            .build()
            .is_err()
    );
    let span = |range| TableSpan {
        range,
        style: Style {
            fg: Paint::solid(Color::White),
            bg: Paint::solid(Color::Black),
            attrs: AttrSet::default(),
        },
    };
    assert!(TableCell::styled("界", vec![span(1..3)]).is_err());
    assert!(TableCell::styled("abcd", vec![span(1..3), span(2..4)]).is_err());
    assert!(TableCell::styled("界", vec![span(0..3)]).is_ok());
    assert_eq!(fixture().columns().len(), 2, "labels need not be unique");
}
#[test]
fn width_sampling_is_bounded_deterministic_and_prefix_only() {
    let data = Arc::new(
        TableData::builder("d", 1, ["a", "b"])
            .row("r", ["界界界", "long-value"])
            .row("s", ["very very long", "even longer"])
            .build()
            .unwrap(),
    );
    let options = TableLayoutOptions {
        min_width: 1,
        max_width: 20,
        sample_rows: 1,
        sample_bytes: 9,
        ..Default::default()
    };
    let a = prepare(data.clone(), options.clone()).unwrap();
    let b = prepare(data, options).unwrap();
    assert_eq!(a.widths(), b.widths());
    assert_eq!(a.geometry_key(), b.geometry_key());
    assert!(a.sampled_bytes() <= 9);
    assert_eq!(a.widths(), [4, 1]);
    assert!(
        prepare(
            fixture(),
            TableLayoutOptions {
                widths: vec![1],
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        prepare(
            fixture(),
            TableLayoutOptions {
                widths: vec![0, 1],
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn geometry_and_hits_have_independent_hand_checked_boundaries() {
    let table = prepare(
        fixture(),
        TableLayoutOptions {
            widths: vec![3, 4],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(table.size(), Size::new(8, 2));
    assert_eq!(table.column_at(2), Some(0));
    assert_eq!(table.column_at(3), None);
    assert_eq!(table.column_at(4), Some(1));
    assert_eq!(table.column_at(8), None);
    let area = Rect::new(10, 20, 8, 3);
    let w = TableWindow::default();
    assert_eq!(
        table.hit_test(area, w, Point { x: 10, y: 20 }),
        Some(TableHit::Header(0))
    );
    assert_eq!(table.hit_test(area, w, Point { x: 13, y: 21 }), None);
    assert_eq!(
        table.hit_test(area, w, Point { x: 14, y: 22 }),
        Some(TableHit::Cell { row: 1, column: 1 })
    );
    let w = TableWindow {
        offset: Point { x: 4, y: 1 },
        header: false,
        row_numbers: true,
    };
    assert_eq!(
        table.hit_test(area, w, Point { x: 10, y: 20 }),
        Some(TableHit::Row(1))
    );
    assert_eq!(
        table.hit_test(area, w, Point { x: 12, y: 20 }),
        Some(TableHit::Cell { row: 1, column: 1 })
    );
    assert_eq!(table.hit_test(area, w, Point { x: 12, y: 21 }), None);
}
#[test]
fn exact_tsv_quotes_fields_orders_by_display_and_retains_empty_values() {
    let data = fixture();
    let selection = TableSelection::new(
        Arc::from([data.rows()[1].key.clone(), data.rows()[0].key.clone()]),
        data.column_keys(),
    );
    let table = prepare(data, Default::default()).unwrap();
    let expected = "First\tFirst\n\t\"a\tb\"\n\"line\nnext\"\t\"say \"\"yes\"\"\"\n";
    assert_eq!(
        table.tsv(&selection, true, expected.len()).unwrap(),
        expected
    );
    assert!(table.tsv(&selection, true, expected.len() - 1).is_err());
    assert_eq!(
        table.tsv(&selection, false, 1000).unwrap(),
        "\t\"a\tb\"\n\"line\nnext\"\t\"say \"\"yes\"\"\"\n"
    );
}
#[test]
fn header_only_and_zero_column_tables_copy_without_synthetic_body_rows() {
    let table = prepare(
        Arc::new(TableData::builder("d", 1, ["", "label"]).build().unwrap()),
        Default::default(),
    )
    .unwrap();
    let selection = TableSelection::new(table.data().row_keys(), table.data().column_keys());
    assert_eq!(table.tsv(&selection, true, 20).unwrap(), "\tlabel\n");
    assert_eq!(table.tsv(&selection, false, 20).unwrap(), "");
    let empty = prepare(
        Arc::new(TableData::new("d", 1, vec![], vec![TableRow::new("r", vec![])]).unwrap()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(empty.tsv(&TableSelection::default(), true, 0).unwrap(), "");
}
#[test]
fn membership_intersects_surviving_keys() {
    let data = fixture();
    let mut selection = TableSelection::new(data.row_keys(), data.column_keys());
    let replacement = TableData::new(
        "dataset",
        2,
        vec![TableColumn::new("1", "new")],
        vec![TableRow::new("b", vec!["changed".into()])],
    )
    .unwrap();
    selection.reconcile(&replacement);
    assert_eq!(selection.rows(), [Arc::<str>::from("b")]);
    assert_eq!(selection.columns(), [Arc::<str>::from("1")]);
    selection.reconcile(&TableData::builder("dataset", 3, ["none"]).build().unwrap());
    assert!(selection.is_empty());
}

#[test]
fn membership_deduplicates_each_axis_in_first_seen_order() {
    let selection = TableSelection::new(
        Arc::from(["b".into(), "a".into(), "b".into()]),
        Arc::from(["y".into(), "y".into(), "x".into()]),
    );
    assert_eq!(selection.rows(), [Arc::<str>::from("b"), "a".into()]);
    assert_eq!(selection.columns(), [Arc::<str>::from("y"), "x".into()]);
    assert!(selection.contains("b", "x"));
    assert!(!selection.contains("missing", "x"));
}
