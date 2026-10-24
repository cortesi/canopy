use std::mem::size_of;

use canopy::{
    ViewContext, Widget,
    error::Result,
    geom::Size,
    layout::{CanvasContext, Layout},
    style::PartialStyle,
    testing::harness::Harness,
};
use canopy_widgets::highlight::palette_style_rules;

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
        &Supplied::default(),
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
    // The title draws in two rows of big text, with no underline.
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
fn enclosing_inline_styles_preserve_link_hit_targets() {
    for text in [
        "**[site](https://example.test)**",
        "*[site](https://example.test)*",
        "~~[site](https://example.test)~~",
    ] {
        let p = prepared(text, 40);
        let object = p.document.objects[0].id;
        let row = p.row(0).unwrap();
        let run = row.runs.iter().find(|run| run.text == "site").unwrap();
        for x in run.x..run.x + run.width {
            assert_eq!(p.hit(Point { x, y: 0 }).unwrap().object, Some(object));
        }
    }
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
fn code_cards_show_every_line_clip_wide_ones_and_keep_atomic_payload() {
    let p = prepared("```unknown\nabcdefghijk\n2\n3\n4\n5\n6\n7\n```\n", 10);
    assert_eq!(p.height(), 8, "the label, then every payload line");
    assert!(row_text(&p, 1).ends_with('…'), "a wide line clips");
    assert_eq!(row_text(&p, 7).trim_end(), "7", "no line is left out");
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
    let span = p.reading_span(&range).unwrap();
    assert!(overlaps(&span, p.unit_index(id).unwrap(), &WHOLE_UNIT));
}

#[test]
fn geometry_reuses_blocks_but_glyph_changes_reprepare() {
    let document = document("# Reuse\n\nparagraph\n");
    let a = PresentationSnapshot::prepare(
        document.clone(),
        GeometryKey::default(),
        None,
        &Supplied::default(),
    );
    let b =
        PresentationSnapshot::prepare(document.clone(), a.geometry, Some(&a), &Supplied::default());
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
        &Supplied::default(),
    );
    assert_eq!(c.stats.reused_blocks, 0);
    let underline = row_text(&c, 1);
    assert!(!underline.is_empty() && underline.chars().all(|c| c == '='));
}

#[test]
fn headings_show_markers_and_underline_the_top_three_levels() {
    let p = prepared(
        "para\n\n## Sub `code`\n\n### Three\n\n#### Four *it*\n\n###### Six\n",
        40,
    );
    let roles = |row: u32| {
        p.row(row)
            .unwrap()
            .runs
            .iter()
            .map(|r| (r.text.clone(), r.role))
            .collect::<Vec<_>>()
    };
    // Each heading after the first block takes an extra row above.
    assert_eq!(row_text(&p, 0), "para");
    assert_eq!(row_text(&p, 1), "");
    assert_eq!(
        roles(2),
        [
            ("## ".into(), "markdown/h2/marker"),
            ("Sub ".into(), "markdown/h2"),
            ("code".into(), "markdown/h2/inline-code")
        ]
    );
    assert_eq!(row_text(&p, 3), "━".repeat(11));
    assert_eq!(row_text(&p, 4), "");
    assert_eq!(row_text(&p, 5), "### Three");
    assert_eq!(row_text(&p, 6), "─".repeat(9));
    assert_eq!(row_text(&p, 7), "");
    assert_eq!(
        roles(8),
        [
            ("#### ".into(), "markdown/h4/marker"),
            ("Four ".into(), "markdown/h4"),
            ("it".into(), "markdown/h4/emphasis")
        ]
    );
    // Below the third level, no underline follows.
    assert_eq!(row_text(&p, 9), "");
    assert_eq!(
        roles(10),
        [
            ("###### ".into(), "markdown/h6/marker"),
            ("Six".into(), "markdown/h6")
        ]
    );
}

#[test]
fn a_first_level_heading_draws_in_big_text_that_reads_as_the_heading() {
    let big = |text: &str, width: u32, mosaic: Tier, glyphs: GlyphPreset| {
        PresentationSnapshot::prepare(
            document(text),
            GeometryKey {
                width,
                spacing: 0,
                mosaic,
                glyphs,
                ..Default::default()
            },
            None,
            &Supplied::default(),
        )
    };
    // Octants draw the title in two rows; every one reads as the heading.
    let p = big("# Title\n", 40, Tier::Full, GlyphPreset::Unicode);
    for row in 0..2 {
        let runs = &p.row(row).unwrap().runs;
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].reading, Some(0..5));
        assert!(!runs[0].text.contains("Title"));
    }
    assert_eq!(p.row_for_unit(p.document.units[0].id, 3), 0);
    // Half blocks take more rows.
    let p = big("# Title\n", 40, Tier::Blocks, GlyphPreset::Unicode);
    assert!(
        row_text(&p, 2)
            .chars()
            .all(|c| matches!(c, '▀' | '▄' | '█' | ' '))
    );
    // A title too wide for the view, and ASCII, keep the text heading.
    let wide = format!("# {}\n", "Wide ".repeat(12));
    let p = big(&wide, 40, Tier::Full, GlyphPreset::Unicode);
    assert!(row_text(&p, 0).starts_with("# Wide"));
    let p = big("# Title\n", 40, Tier::Full, GlyphPreset::Ascii);
    assert_eq!(row_text(&p, 0), "# Title");
}

#[test]
fn a_wrapped_heading_hangs_under_its_text_and_is_underlined_to_its_widest_line() {
    let p = prepared("## alphabet beta gamma\n", 19);
    assert_eq!(row_text(&p, 0).trim_end(), "## alphabet beta");
    assert_eq!(row_text(&p, 1), "   gamma");
    // The trailing space of the wrapped line is not underlined.
    assert_eq!(row_text(&p, 2), "━".repeat(16));
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
            &Supplied::default(),
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
        let cached =
            PresentationSnapshot::prepare(doc.clone(), p.geometry, Some(&p), &Supplied::default());
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
        &Supplied::default(),
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
fn search_overlay_marks_only_matched_prose_graphemes() {
    let p = prepared("alpha beta\n", 24);
    let unit = p.document.units[0].id;
    let at = |offset| ReadingPosition {
        unit_id: unit,
        offset,
    };
    let overlays = PaintOverlays {
        search: vec![ReadingRange {
            start: at(10),
            end: at(6),
        }],
        ..Default::default()
    };
    let mut h = mounted(p, overlays, 24, 2);
    h.render().unwrap();
    let search = h.canopy.style().resolve(view_search::MATCH);
    let marked = PartialStyle::new().bg(search.bg);
    assert!(h.tbuf().contains_text_style("beta", &marked));
    assert!(!h.tbuf().contains_text_style("alpha", &marked));
}

#[test]
fn the_current_match_paints_apart_from_the_others() {
    let p = prepared("alpha beta alpha\n\n```\nalpha code\n```\n", 40);
    let prose = p.document.units[0].id;
    let code = p.document.units[1].id;
    let range = |unit_id, start, end| ReadingRange {
        start: ReadingPosition {
            unit_id,
            offset: start,
        },
        end: ReadingPosition {
            unit_id,
            offset: end,
        },
    };
    let overlays = PaintOverlays {
        search: vec![range(prose, 0, 5), range(prose, 11, 16), range(code, 0, 5)],
        search_current: Some(1),
        ..Default::default()
    };
    let mut h = mounted(p, overlays, 40, 8);
    h.render().unwrap();
    let bg = |role| h.canopy.style().resolve(role).bg.solid_color().unwrap();
    let (current, other) = (bg(view_search::CURRENT), bg(view_search::MATCH));
    assert_ne!(current, other);
    // The background of the cell `skip` columns into the first `text` shown.
    let at = |text: &str, skip: u32| {
        let lines = h.tbuf().lines();
        let (y, line) = lines
            .iter()
            .enumerate()
            .find(|(_, line)| line.contains(text))
            .unwrap();
        let x = line[..line.find(text).unwrap()].chars().count() as u32 + skip;
        h.buf().get(Point::new(x, y as u32)).unwrap().style.bg
    };
    assert_eq!(at("alpha beta alpha", 0), other);
    assert_ne!(at("alpha beta alpha", 6), other);
    assert_eq!(at("alpha beta alpha", 11), current);
    // A match in a code line marks its own text, not the whole card.
    assert_eq!(at("alpha code", 0), other);
    assert_ne!(at("alpha code", 6), other);
}

#[test]
fn object_cursor_shares_one_ground_and_marks_its_rows_in_the_margin() {
    let p = prepared(
        "see [the guide](https://x) now\n\n```\nlet a\n\nlet b\n```\n",
        24,
    );
    let link = p.blocks[0]
        .rows
        .iter()
        .flat_map(|row| &row.runs)
        .find_map(|run| run.object)
        .unwrap();
    let card = p.blocks[1].object.unwrap();
    let card_start = p.blocks[1].start;
    let mut h = mounted(
        p,
        PaintOverlays {
            active_object: Some(link),
            ..Default::default()
        },
        24,
        8,
    );
    h.render().unwrap();
    let object = h.canopy.style().resolve("markdown/object");
    let bar = h.canopy.style().resolve("markdown/object/bar");
    let ground = PartialStyle::new().bg(object.bg.clone());
    let (object, bar) = (
        object.resolve_solid().unwrap(),
        bar.resolve_solid().unwrap(),
    );
    let margin = |h: &Harness, y| h.buf().get(Point { x: 0, y }).unwrap().clone();
    assert!(h.tbuf().contains_text_style("the guide", &ground));
    assert!(!h.tbuf().contains_text_style("see", &ground));
    assert_eq!((margin(&h, 0).ch, margin(&h, 0).style.fg), ('▌', bar.fg));
    assert_eq!(margin(&h, card_start).ch, ' ');

    // A card marks every row, blank code lines included, on the same ground.
    h.with_root_widget_context(|preview: &mut Preview, _| {
        preview.overlays.active_object = Some(card);
        Ok(())
    })
    .unwrap();
    h.render().unwrap();
    assert!(h.tbuf().contains_text_style("let a", &ground));
    assert!(!h.tbuf().contains_text_style("the guide", &ground));
    assert_eq!(margin(&h, 0).ch, ' ');
    // An unlabelled card is its three code lines.
    for y in card_start..card_start + 3 {
        let cell = margin(&h, y);
        assert_eq!((cell.ch, cell.style.bg), ('▌', object.bg), "row {y}");
    }
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
    assert!(h.tbuf().contains_text("one │ two"));
    h.with_root_widget_context(|root: &mut Preview, _| {
        assert_eq!(root.last.tables, 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn table_preview_wraps_long_values_within_the_reading_width() {
    let p = prepared(
        "| Key | Notes |\n| - | - |\n| a | one two three four five six seven |\n| b | short |\n",
        24,
    );
    let block = &p.blocks[0];
    assert!(block.flow.is_some());
    // Header and its rule, three wrapped lines, a row rule, and the short
    // row.
    assert_eq!(block.table_height, 7);
    let mut h = mounted(p, PaintOverlays::default(), 24, 8);
    h.render().unwrap();
    for line in [
        "Key │ Notes",
        "━━━━┿━━━━",
        "a   │ one two three",
        "    │ four five six",
        "────┼────",
        "b   │ short",
    ] {
        assert!(h.tbuf().contains_text(line), "missing {line:?}");
    }
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
    // The label, the header, and its rule precede the first body row.
    h.with_root_widget_context(|_: &mut Preview, ctx| {
        ctx.scroll(ScrollOp::To(Point { x: 0, y: 4 }));
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
                    &Supplied::default(),
                );
                assert_eq!(reused.stats.reused_blocks, 1);
                assert_eq!(reused.stats.prepared_blocks, 0);
                assert!(Arc::ptr_eq(&rows, &reused.blocks[0].rows));
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn table_preview_keeps_every_row_and_marks_cut_columns() {
    let mut source = String::from("| Key | One | Two | Three |\n| - | - | - | - |\n");
    let prose = "a sentence of prose that needs room to stay readable";
    for i in 0..12 {
        source.push_str(&format!("| k{i} | {prose} | {prose} | {prose} |\n"));
    }
    let p = prepared(&source, 50);
    let block = &p.blocks[0];
    let flow = block.flow.as_ref().unwrap();
    assert!(flow.cut());
    assert!(row_text(&p, block.start).contains(" of 4 columns"));
    let mut h = mounted(p, PaintOverlays::default(), 50, 80);
    h.render().unwrap();
    assert!(h.tbuf().contains_text("k11"));
    assert!(h.tbuf().contains_text("›"));
}

#[test]
fn source_cards_and_table_previews_show_every_line() {
    let html: String = (1..=9).map(|i| format!("<p>line {i}</p>\n")).collect();
    let p = prepared(&format!("<div>\n{html}</div>\n"), 40);
    let rows: Vec<String> = (0..p.height()).map(|row| row_text(&p, row)).collect();
    assert!(
        rows.iter().any(|row| row.contains("line 9")),
        "a source card shows its last line: {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("more source")),
        "{rows:?}"
    );

    let long = "word ".repeat(60);
    let p = prepared(&format!("| a | b |\n| --- | --- |\n| k | {long}|\n"), 30);
    let flow = p
        .blocks
        .iter()
        .find_map(|block| block.flow.clone())
        .expect("a flowed table");
    // Three hundred cells of value at most 30 wide need far more than the
    // four lines a preview once allowed.
    assert!(
        flow.height() > 12,
        "the value wraps in full: {}",
        flow.height()
    );
}

#[test]
fn cards_and_tables_label_only_what_their_formatting_cannot_show() {
    let rows = |p: &PresentationSnapshot| -> Vec<String> {
        (0..p.height()).map(|row| row_text(p, row)).collect()
    };
    let p = prepared("```\nplain\n```\n", 40);
    assert_eq!(
        p.height(),
        1,
        "a plain block is its line alone: {:?}",
        rows(&p)
    );
    let p = prepared("```rust\nfn main() {}\n```\n", 40);
    let shown = rows(&p);
    assert_eq!(shown[0].trim_end(), "rust", "the label names the language");
    assert!(
        !shown
            .iter()
            .any(|row| row.contains("lines") || row.contains("complete"))
    );

    let p = prepared("| a | b |\n| --- | --- |\n| 1 | 2 |\n", 40);
    assert!(
        !rows(&p).iter().any(|row| row.contains("table")),
        "{:?}",
        rows(&p)
    );
    assert_eq!(
        p.blocks[0].table_start, 0,
        "the grid starts at the block's top"
    );

    let p = prepared("<div>\nraw\n</div>\n", 40);
    assert!(
        !rows(&p).iter().any(|row| row.contains("Unrendered")),
        "{:?}",
        rows(&p)
    );
}

#[test]
fn an_alert_rail_and_label_take_its_tone() {
    let p = prepared(
        "> [!caution]\n> risky words that wrap onto another line\n",
        24,
    );
    // Neighbouring cells of one style share a run.
    let first = |row: u32| {
        let run = &p.row(row).unwrap().runs[0];
        (run.text.clone(), run.role)
    };
    assert_eq!(
        first(0),
        ("│ CAUTION".to_owned(), "markdown/alert/caution"),
        "the rail and the label take the tone"
    );
    let (rail, role) = first(1);
    assert!(rail.starts_with('│'), "{rail:?}");
    assert_eq!(
        role, "markdown/alert/caution",
        "a wrapped line keeps the coloured rail"
    );
    let plain = prepared("> just a quote\n", 24);
    assert_eq!(
        plain.row(0).unwrap().runs[0].role,
        "markdown/structure",
        "an ordinary quote keeps a quiet rail"
    );
}

#[test]
fn a_quote_bar_runs_unbroken_through_spacing_and_cards() {
    let p = PresentationSnapshot::prepare(
        document("> one\n>\n> ```\n> code\n> ```\n\n> two\n"),
        GeometryKey {
            width: 30,
            spacing: 1,
            ..Default::default()
        },
        None,
        &Supplied::default(),
    );
    let texts: Vec<_> = (0..p.height()).map(|row| row_text(&p, row)).collect();
    assert_eq!(
        texts,
        ["│ one", "│ ", "│ code", "", "│ two", ""],
        "spacing within a quote keeps the rail; spacing between quotes does not"
    );
    let card = p.row(2).unwrap();
    assert_eq!(card.ground, "markdown/card");
    assert_eq!(card.runs[0].x, 2, "the rail sits at the margin");
    assert_eq!(card.runs[1].x, 4, "the card sits inside the rail");
    assert_eq!(card.ground_x, 4, "the card ground starts after the rail");
}

#[test]
fn a_character_wider_than_the_line_takes_a_line_of_its_own() {
    assert_eq!(wrap_words("中文", 1), ["中", "文"]);
    assert_eq!(wrap_words("a 中b", 2), ["a", "中", "b"]);
}

#[test]
fn a_centered_document_starts_every_block_at_its_centered_margin() {
    let text = "Some prose.\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n```\ncode\n```\n";
    let layout = |centered: bool| {
        PresentationSnapshot::prepare(
            document(text),
            GeometryKey {
                // An inset of two, a body of a hundred, and the gutter.
                width: 103,
                spacing: 0,
                prose_width: 40,
                centered,
                ..Default::default()
            },
            None,
            &Supplied::default(),
        )
    };
    let start = |p: &PresentationSnapshot, block: usize| p.blocks[block].rows[0].runs[0].x;
    let plain = layout(false);
    assert_eq!(start(&plain, 0), 2, "prose starts at the inset");
    assert_eq!(plain.blocks[1].inset, 2, "a table starts at the inset");
    assert_eq!(start(&plain, 2), 2, "code starts at the inset");

    // The margin moves in by half of what the prose leaves of the body.
    let centered = layout(true);
    assert_eq!(start(&centered, 0), 32, "prose starts at the margin");
    assert_eq!(centered.blocks[1].inset, 32, "a table starts at the margin");
    assert_eq!(start(&centered, 2), 32, "code starts at the margin");
    assert_eq!(
        centered.blocks[2].rows[0].ground_x, 30,
        "a card's ground starts at the margin's edge"
    );
}

/// Paints one document window at a destination, as a host that lays out
/// many documents in one canvas does.
struct Placed {
    prepared: PresentationSnapshot,
    visible: Rect,
    destination: Point,
}
impl Widget for Placed {
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn render(&mut self, render: &mut Render, _: &dyn ViewContext) -> Result<()> {
        self.prepared.paint_at(
            render,
            self.visible,
            self.destination,
            &PaintOverlays::default(),
        )?;
        Ok(())
    }
}
fn placed(text: &str, width: u32, visible: Rect, destination: Point, size: Size) -> Harness {
    Harness::builder(Placed {
        prepared: prepared(text, width),
        visible,
        destination,
    })
    .configure(|setup| {
        crate::register_styles(setup);
        Ok(())
    })
    .size(size.w, size.h)
    .build()
    .unwrap()
}

#[test]
fn a_window_paints_at_its_destination_as_it_paints_at_zero() {
    // A paragraph, a code card, a table that flows, and a table that clips.
    let text = "intro words here\n\n```rust\nlet value = 1;\n```\n\n| A | B |\n| - | - |\n| x | y |\n\n\
                | Alpha column | Beta column | Gamma column | Delta column |\n| - | - | - | - |\n\
                | one | two | three | four |\n";
    let width = 30;
    let height = prepared(text, width).height();
    let visible = Rect::new(1, 1, width - 1, height - 1);
    let destination = Point { x: 4, y: 3 };
    let size = Size::new(width + 4, height + 3);
    let mut zero = placed(text, width, visible, Point { x: 0, y: 0 }, size);
    let mut moved = placed(text, width, visible, destination, size);
    zero.render().unwrap();
    moved.render().unwrap();
    for y in 0..visible.h {
        for x in 0..visible.w {
            let at_zero = zero.buf().get(Point { x, y }).unwrap();
            let at_destination = moved
                .buf()
                .get(Point {
                    x: x + destination.x,
                    y: y + destination.y,
                })
                .unwrap();
            assert_eq!(at_zero, at_destination, "cell {x},{y}");
        }
    }
    assert!(moved.tbuf().contains_text("value"));
    assert!(moved.tbuf().contains_text("Alpha"));
    // Nothing paints before the destination.
    let before = moved.buf().get(Point { x: 1, y: 1 }).unwrap();
    assert_eq!(
        before,
        zero.buf()
            .get(Point {
                x: width + 2,
                y: height + 1
            })
            .unwrap()
    );
}

#[test]
fn the_end_caret_follows_the_last_grapheme_of_the_text() {
    let caret = |text: &str, width: u32| prepared(text, width).end_caret();
    assert_eq!(caret("", 40), None, "an empty document has no end");
    // The body starts after the two-column inset.
    assert_eq!(caret("alpha", 40), Some(Point { x: 7, y: 0 }));
    assert_eq!(caret("alpha\n", 40), Some(Point { x: 7, y: 0 }));
    // A wide grapheme takes two cells.
    assert_eq!(caret("界", 40), Some(Point { x: 4, y: 0 }));
    // A wrapped line ends on its last row.
    let wrapped = prepared("alpha beta gamma delta", 13);
    let end = wrapped.end_caret().unwrap();
    assert_eq!(
        end,
        Point {
            x: 7,
            y: wrapped.height() - 1
        }
    );
    // A last row that fills the body leaves no cell for the caret.
    assert_eq!(caret("alpha beta gamma delta", 14), None);
    // A completed table ends no text.
    assert_eq!(caret("| A | B |\n| - | - |\n| x | y |\n", 40), None);
    // A body too narrow for the inset still ends inside the body or not at all.
    if let Some(end) = caret("abc", 3) {
        assert!(end.x < 2);
    }
}

#[test]
fn the_end_caret_of_a_stream_follows_an_open_fence_and_a_receiving_table() {
    let streamed = |text: &str| {
        let mut store = SourceStore::stream();
        store.append(text).unwrap();
        let document = Arc::new(parse_document(
            &store.snapshot(),
            &ParseLimits::default(),
            None,
        ));
        PresentationSnapshot::prepare(
            document,
            GeometryKey {
                width: 40,
                spacing: 0,
                ..Default::default()
            },
            None,
            &Supplied::default(),
        )
    };
    let fence = streamed("```rust\nlet a = 1;\n");
    let end = fence
        .end_caret()
        .expect("an open fence ends after its last line");
    assert!(row_text(&fence, end.y).contains("let a = 1;"));
    let table = streamed("| A | B |\n| - | - |\n| x | y |\n");
    let end = table
        .end_caret()
        .expect("a receiving trailing table is literal source");
    assert!(row_text(&table, end.y).contains('y'));
}
