use std::mem::size_of;

use canopy::{
    ViewContext, Widget,
    error::Result,
    geom::Size,
    layout::{CanvasContext, Layout},
    style::PartialStyle,
    testing::harness::Harness,
};

use super::*;
use crate::{
    document::{ParseLimits, parse_document},
    source_store::SourceStore,
};

fn document(text: &str) -> Arc<DocumentSnapshot> {
    let source = SourceStore::static_text(text).unwrap().snapshot();
    Arc::new(parse_document(&source, &ParseLimits::default(), None))
}
fn prepared(text: &str, width: u32) -> PresentationSnapshot {
    PresentationSnapshot::prepare(
        document(text),
        GeometryKey {
            width,
            spacing: 0,
            ..Default::default()
        },
        None,
    )
}
fn row_text(p: &PresentationSnapshot, row: u32) -> String {
    p.row(row)
        .unwrap()
        .runs
        .iter()
        .map(|r| r.text.as_str())
        .collect()
}

#[test]
fn heading_inline_roles_link_hits_and_copy_have_independent_expected_values() {
    let p = prepared(
        "# Title\n\nalpha **bold** [site](https://example.com)\n",
        40,
    );
    assert_eq!(row_text(&p, 0), "Title");
    assert_eq!(row_text(&p, 1), "━━━━━");
    assert_eq!(row_text(&p, 2), "alpha bold site");
    assert!(
        p.row(2)
            .unwrap()
            .runs
            .iter()
            .any(|r| r.role == "markdown/strong" && r.text == "bold")
    );
    let hit = p.hit(Point { x: 13, y: 2 }).unwrap();
    assert!(hit.object.is_some());
    assert_eq!(
        hit.reading,
        Some(ReadingPosition {
            unit_id: p.document.units[1].id,
            offset: 11
        })
    );
    let selection = ReadingRange {
        start: ReadingPosition {
            unit_id: p.document.units[0].id,
            offset: 0,
        },
        end: ReadingPosition {
            unit_id: p.document.units[1].id,
            offset: 15,
        },
    };
    assert_eq!(
        p.document.copy_range(&selection).as_deref(),
        Some("Title\n\nalpha bold site")
    );
}

#[test]
fn word_wrap_hanging_lists_and_quote_rails_survive_continuations() {
    let p = prepared("> - alpha beta gamma delta\n", 18);
    let rows = (0..p.height()).map(|i| row_text(&p, i)).collect::<Vec<_>>();
    assert!(rows.len() >= 2);
    assert!(rows[0].contains('•'));
    assert!(rows.iter().all(|row| row.contains('│')));
    assert!(rows.iter().any(|row| row.contains("alpha")));
    assert!(rows.iter().any(|row| row.contains("delta")));
}

#[test]
fn soft_breaks_wrap_as_prose_while_hard_breaks_keep_logical_rows() {
    let p = prepared("soft\nline\n\nhard  \nbreak\n", 40);
    assert_eq!(row_text(&p, 0), "soft line");
    assert_eq!(row_text(&p, 1), "hard");
    assert_eq!(row_text(&p, 2), "break");
    assert_eq!(p.document.units[0].text, "soft\nline");
}

#[test]
fn narrow_unicode_tabs_and_controls_progress_and_never_overflow() {
    for width in [0, 1, 2, 3, 4, 8, 20] {
        let p = prepared("界👩‍💻e\u{301}\t\u{1b}[31m a\u{202e}b\n", width);
        assert!(p.height() < 100);
        for y in 0..p.height() {
            for run in &p.row(y).unwrap().runs {
                assert!(run.x.saturating_add(run.width) <= width, "{width}: {run:?}");
                assert!(!run.text.chars().any(|c| c.is_control() || c == '\u{202e}'));
            }
        }
    }
    let p = prepared("a界b\n", 3);
    let hit = p.hit(Point { x: 1, y: 1 }).unwrap().reading.unwrap();
    assert_eq!(
        hit.offset, 1,
        "second cell of CJK maps to the whole grapheme"
    );
}

#[test]
fn code_preview_limits_lines_marks_clipping_and_keeps_atomic_payload() {
    let p = prepared("```unknown\nabcdefghijk\n2\n3\n4\n5\n6\n7\n```\n", 10);
    assert_eq!(
        p.height(),
        8,
        "label, six payload lines, omission indicator"
    );
    assert!(row_text(&p, 1).ends_with('…'));
    assert_eq!(row_text(&p, 7), "1 more ");
    let id = p.document.units[0].id;
    let range = ReadingRange {
        start: ReadingPosition {
            unit_id: id,
            offset: 1,
        },
        end: ReadingPosition {
            unit_id: id,
            offset: 2,
        },
    };
    assert_eq!(
        p.document.copy_range(&range).as_deref(),
        Some("abcdefghijk\n2\n3\n4\n5\n6\n7\n")
    );
    assert!(p.intersects(&range, id, &(0..1), true));
}

#[test]
fn geometry_reuses_blocks_but_glyph_changes_reprepare() {
    let document = document("# Reuse\n\nparagraph\n");
    let a = PresentationSnapshot::prepare(document.clone(), GeometryKey::default(), None);
    let b = PresentationSnapshot::prepare(document.clone(), a.geometry, Some(&a));
    assert_eq!(b.stats.prepared_blocks, 0);
    assert_eq!(b.stats.reused_blocks, 2);
    assert!(Arc::ptr_eq(&a.blocks[0].rows, &b.blocks[0].rows));
    assert_eq!(
        a.derived_bytes(),
        b.derived_bytes(),
        "reused layouts retain the same conservative accounting"
    );
    let c = PresentationSnapshot::prepare(
        document,
        GeometryKey {
            glyphs: GlyphPreset::Ascii,
            ..a.geometry
        },
        Some(&a),
    );
    assert_eq!(c.stats.reused_blocks, 0);
    assert_eq!(row_text(&c, 1), "-----");
}

#[test]
fn derived_layout_accounting_excludes_the_retained_large_source_payload() {
    let p = prepared(&format!("```\n{}\n```\n", "x".repeat(400_000)), 80);
    assert!(p.document.source.len() > 400_000);
    assert!(p.derived_bytes() > size_of::<PresentationSnapshot>());
    assert!(
        p.derived_bytes() < 64 * 1024,
        "a clipped card's derived layout excludes its retained semantic/source payload"
    );
}

#[cfg(feature = "syntax")]
#[test]
fn cumulative_syntax_payload_budget_obeys_zero_and_n_minus_one_n_n_plus_one() {
    let payload = "let a = 1;\n";
    let doc = document(&format!("```rust\n{payload}```\n\n```rust\n{payload}```\n"));
    let total = 2 * payload.len();
    for limit in [0, total - 1, total, total + 1] {
        let p = PresentationSnapshot::prepare(
            doc.clone(),
            GeometryKey {
                width: 120,
                syntax_limit: limit,
                spacing: 0,
                ..Default::default()
            },
            None,
        );
        let reserved = if limit == 0 {
            0
        } else if limit < total {
            payload.len()
        } else {
            total
        };
        assert_eq!(p.stats.syntax_bytes, reserved);
        let limited = p.blocks[1].rows[0]
            .runs
            .iter()
            .any(|run| run.text.contains("plain: syntax limit"));
        assert_eq!(limited, limit < total);
        let styled = p.blocks[1]
            .rows
            .iter()
            .flat_map(|r| &r.runs)
            .any(|run| run.syntax.is_some());
        assert_eq!(styled, limit >= total);
        let cached = PresentationSnapshot::prepare(doc.clone(), p.geometry, Some(&p));
        assert_eq!(cached.stats.syntax_bytes, reserved);
        assert_eq!(cached.stats.reused_blocks, 2);
    }
}

#[test]
fn source_anchors_use_verified_source_bytes_and_hidden_syntax_falls_back() {
    let p = prepared(
        "# Heading\n\n**bold** and [label](https://example.com)\n\nplain words wrap here\n",
        16,
    );
    assert_eq!(p.source_at_row(0), Some(2));
    let formatted = &p.document.units[1];
    let row = p.row_for_unit(formatted.id, 5);
    assert_eq!(p.source_at_row(row), Some(formatted.source.start));
    assert_eq!(
        p.row_for_source(formatted.source.start + 2),
        p.row_for_unit(formatted.id, 0)
    );
    let plain = &p.document.units[2];
    let row = p.row_for_unit(plain.id, 12);
    let source = p.source_at_row(row).unwrap();
    assert!(p.document.source.text().is_char_boundary(source));
    assert_eq!(p.row_for_source(source), row);
}

#[test]
fn ascii_preset_uses_ascii_for_all_generated_decoration() {
    let doc = document("# Title\n\n- item\n\n```unknown\n1234567890123456789\n```\n");
    let p = PresentationSnapshot::prepare(
        doc,
        GeometryKey {
            width: 18,
            glyphs: GlyphPreset::Ascii,
            spacing: 0,
            ..Default::default()
        },
        None,
    );
    for row in 0..p.height() {
        for run in &p.row(row).unwrap().runs {
            if run.reading.is_none() {
                assert!(run.text.is_ascii(), "{}", run.text);
            }
        }
    }
}

struct Preview {
    prepared: PresentationSnapshot,
    overlays: PaintOverlays,
    last: PaintStats,
}
impl Widget for Preview {
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn canvas(&self, content: Size, _: &CanvasContext) -> Size {
        Size::new(content.w, self.prepared.height().max(content.h))
    }
    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        self.last = self.prepared.paint(
            render,
            Rect::new(view.scroll.x, view.scroll.y, view.content.w, view.content.h),
            &self.overlays,
        )?;
        Ok(())
    }
}
fn mounted(
    prepared: PresentationSnapshot,
    overlays: PaintOverlays,
    width: u32,
    height: u32,
) -> Harness {
    Harness::builder(Preview {
        prepared,
        overlays,
        last: PaintStats::default(),
    })
    .configure(|setup| {
        setup.widget_styles(style_rules);
        setup.widget_styles(palette_style_rules);
        Ok(())
    })
    .size(width, height)
    .build()
    .unwrap()
}

#[test]
fn mounted_visible_paint_is_bounded_and_selection_styles_actual_glyphs() {
    let p = prepared(&("alpha beta\n\n".repeat(1000)), 24);
    let unit = p.document.units[0].id;
    let overlays = PaintOverlays {
        selection: Some(ReadingRange {
            start: ReadingPosition {
                unit_id: unit,
                offset: 0,
            },
            end: ReadingPosition {
                unit_id: unit,
                offset: 5,
            },
        }),
        ..Default::default()
    };
    let mut h = mounted(p, overlays, 24, 4);
    h.render().unwrap();
    assert!(h.tbuf().contains_text("  alpha beta"));
    h.with_root_widget_context(|root: &mut Preview, _| {
        assert_eq!(root.last.rows, 4);
        assert!(root.last.graphemes < 80);
        Ok(())
    })
    .unwrap();
    let selected = h.canopy.style().resolve("markdown/selection");
    assert!(
        h.tbuf()
            .contains_text_style("alpha", &PartialStyle::new().bg(selected.bg))
    );
}

#[test]
fn mounted_table_uses_shared_grid_and_keeps_full_atomic_copy() {
    let p = prepared("| A | B |\n| - | - |\n| one | two |\n", 30);
    assert!(p.blocks[0].table.is_some());
    let id = p.document.units[0].id;
    let range = ReadingRange {
        start: ReadingPosition {
            unit_id: id,
            offset: 0,
        },
        end: ReadingPosition {
            unit_id: id,
            offset: 1,
        },
    };
    assert_eq!(
        p.document.copy_range(&range).as_deref(),
        Some("A\tB\none\ttwo")
    );
    let mut h = mounted(p, PaintOverlays::default(), 30, 5);
    h.render().unwrap();
    assert!(h.tbuf().contains_text("one│two"));
    h.with_root_widget_context(|root: &mut Preview, _| {
        assert_eq!(root.last.tables, 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn scrolled_document_rows_paint_in_viewport_coordinates() {
    use canopy::layout::ScrollOp;
    let p = prepared("first\n\nsecond\n\nthird\n\nfourth\n\nfifth\n", 24);
    let mut h = mounted(p, PaintOverlays::default(), 24, 2);
    h.render().unwrap();
    h.with_root_widget_context(|_: &mut Preview, ctx| {
        ctx.scroll(ScrollOp::To(Point { x: 0, y: 2 }));
        Ok(())
    })
    .unwrap();
    h.render().unwrap();
    assert!(h.tbuf().contains_text("third"));
    assert!(h.tbuf().contains_text("fourth"));
    assert!(!h.tbuf().contains_text("first"));
}

#[test]
fn scrolled_inline_table_does_not_repeat_its_header() {
    use canopy::layout::ScrollOp;
    let p = prepared(
        "| Header | Value |\n| - | - |\n| first | one |\n| second | two |\n| third | three |\n",
        30,
    );
    let mut h = mounted(p, PaintOverlays::default(), 30, 2);
    h.render().unwrap();
    h.with_root_widget_context(|_: &mut Preview, ctx| {
        ctx.scroll(ScrollOp::To(Point { x: 0, y: 3 }));
        Ok(())
    })
    .unwrap();
    h.render().unwrap();
    assert!(h.tbuf().contains_text("second"));
    assert!(h.tbuf().contains_text("third"));
    assert!(!h.tbuf().contains_text("Header"));
}

#[cfg(feature = "syntax")]
#[test]
fn palette_switch_recolors_prepared_code_without_reflow_or_losing_font_attributes() {
    use canopy::style::{Attr, AttrSet, themes};
    let p = prepared("```rust\nlet value = 1; // note\n```\n", 42);
    let rows = p.blocks[0].rows.clone();
    let mut harness = mounted(p, PaintOverlays::default(), 42, 5);
    for palette in [
        themes::default_dark(),
        themes::solarized_light(),
        themes::default_dark(),
    ] {
        harness
            .with_root_widget_context(|_: &mut Preview, ctx| {
                ctx.set_theme(palette);
                Ok(())
            })
            .unwrap();
        harness.render().unwrap();
        let keyword = harness.canopy.style().resolve("syntax/keyword/plain");
        let card = harness.canopy.style().resolve("markdown/card");
        assert!(
            harness
                .tbuf()
                .contains_text_style("let", &PartialStyle::new().fg(keyword.fg).bg(card.bg))
        );
        assert!(harness.tbuf().contains_text_style(
            "// note",
            &PartialStyle::new().attrs(AttrSet::new(Attr::Italic))
        ));
        harness
            .with_root_widget_context(|preview: &mut Preview, _| {
                assert!(Arc::ptr_eq(&rows, &preview.prepared.blocks[0].rows));
                let reused = PresentationSnapshot::prepare(
                    preview.prepared.document.clone(),
                    preview.prepared.geometry,
                    Some(&preview.prepared),
                );
                assert_eq!(reused.stats.reused_blocks, 1);
                assert_eq!(reused.stats.prepared_blocks, 0);
                assert!(Arc::ptr_eq(&rows, &reused.blocks[0].rows));
                Ok(())
            })
            .unwrap();
    }
}
