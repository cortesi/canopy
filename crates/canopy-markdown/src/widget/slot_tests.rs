//! The block editor in the slot, driven through the reader's keys and
//! commands, as a host with no bindings of its own drives it.

use canopy::{render::TermBuf, testing::harness::Harness};
use canopy_widgets::editor::{EditMode, EditorConfig, TextPosition, ViMode};

use super::{
    cursor_tests::{bar_rows, marked, press, service, settle, view},
    *,
};
use crate::{
    executor::{ExecutorOptions, ManualExecutor, MarkdownExecutor},
    units::parse_units,
};

/// A document of a heading, a paragraph in a quote, a list, and a code
/// block.
pub(super) const DOCUMENT: &str =
    "## One\n\n> alpha line\n> beta line\n\n- item a\n- item b\n\n```\nc1\nc2\n```\n\nomega\n";

/// A mounted reader over `text` that rests in the block cursor and edits in
/// `mode`, `width` by `height` cells, with the built-in search field and no
/// status row.
pub(super) fn editing(
    text: &str,
    mode: EditMode,
    width: u32,
    height: u32,
) -> (Harness, ManualExecutor) {
    editing_with(text, mode, width, height, |_| {})
}

/// A reader as [`editing`] makes one, whose options `configure` adjusts.
pub(super) fn editing_with(
    text: &str,
    mode: EditMode,
    width: u32,
    height: u32,
    configure: impl FnOnce(&mut MarkdownOptions),
) -> (Harness, ManualExecutor) {
    let (executor, manual) = MarkdownExecutor::manual(ExecutorOptions {
        worker_count: 1,
        ..ExecutorOptions::default()
    });
    let mut options = MarkdownOptions {
        executor,
        resting_mode: Mode::Interact,
        block_editor: Some(EditorConfig::new().with_mode(mode)),
        clipboard: true,
        status_row: false,
        coalesce: Duration::ZERO,
        ..MarkdownOptions::default()
    };
    configure(&mut options);
    let reader = Markdown::new(text.to_owned(), options).unwrap();
    let harness = Harness::builder(reader)
        .register::<Markdown>()
        .size(width, height)
        .build()
        .unwrap();
    (harness, manual)
}

/// Run `f` on the reader.
pub(super) fn reader<R>(
    harness: &mut Harness,
    f: impl FnOnce(&mut Markdown, &mut dyn Context) -> Result<R>,
) -> R {
    harness.with_root_widget_context(f).unwrap()
}

/// The text of the open block editor.
pub(super) fn draft(harness: &mut Harness) -> Option<String> {
    reader(harness, |reader, ctx| {
        Ok(reader.block_draft(ctx).map(|edit| edit.text))
    })
}

/// The text and cursor of the open block editor.
fn editor_state(harness: &mut Harness) -> (String, TextPosition, Option<ViMode>) {
    reader(harness, |reader, ctx| {
        let editor = reader.block_editor().unwrap();
        ctx.with_widget(editor, |editor: &Editor| {
            Ok((editor.text(), editor.cursor(), editor.vi_mode()))
        })
    })
}

/// Type `text` key by key.
pub(super) fn type_text(harness: &mut Harness, text: &str) {
    for c in text.chars() {
        harness.key(c).unwrap();
    }
    service(harness);
}

/// The screen.
fn screen(harness: &Harness) -> TermBuf {
    harness.buf().clone()
}

#[test]
fn e_opens_the_editor_on_the_marked_unit_in_place() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    let (_, rows) = marked(&mut harness).unwrap();
    let (top, _) = view(&mut harness);
    press(&mut harness, "e");
    assert!(reader(&mut harness, |reader, _| Ok(reader.block_open())));
    let (text, cursor, mode) = editor_state(&mut harness);
    assert_eq!(text, "> alpha line\n> beta line");
    // The editor opens at the start of the unit's own text, in normal mode.
    assert_eq!(cursor, TextPosition::new(0, 2));
    assert_eq!(mode, Some(ViMode::Normal));
    // The slot opens on the unit's screen row, so the text does not jump.
    let row = rows.start - top;
    let lines = harness.tbuf().lines();
    assert!(lines[row as usize].contains("> alpha line"), "{lines:#?}");
    // A bar in the margin marks the rows of the slot.
    let bar = harness.buf().get(Point::new(0, row)).unwrap();
    assert_eq!(bar.ch, '▌');
}

#[test]
fn a_commit_with_no_change_leaves_the_screen_and_records_nothing() {
    for mode in [EditMode::Vi, EditMode::Text] {
        let (mut harness, manual) = editing(DOCUMENT, mode, 40, 20);
        settle(&mut harness, &manual);
        press(&mut harness, "j");
        press(&mut harness, "j");
        let before = screen(&harness);
        press(&mut harness, "e");
        assert!(reader(&mut harness, |reader, _| Ok(reader.block_open())));
        press(&mut harness, "Escape");
        settle(&mut harness, &manual);
        assert!(
            !reader(&mut harness, |reader, _| Ok(reader.block_open())),
            "{mode:?}"
        );
        assert!(reader(&mut harness, |reader, _| Ok(reader.take_edits())).is_empty());
        assert_eq!(screen(&harness), before, "{mode:?}");
    }
}

#[test]
fn a_commit_records_an_edit_and_keeps_the_unit_on_its_row() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    let (_, rows) = marked(&mut harness).unwrap();
    let row = rows.start - view(&mut harness).0;
    press(&mut harness, "e");
    // Append to the first line, and commit.
    harness.key('A').unwrap();
    type_text(&mut harness, " more");
    press(&mut harness, "Escape");
    press(&mut harness, "Escape");
    // The text changed, and the slot shows it until its layout shows.
    let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
    assert!(
        source.contains("> alpha line more\n> beta line"),
        "{source}"
    );
    let edits = reader(&mut harness, |reader, _| Ok(reader.take_edits()));
    let start = DOCUMENT.find("> alpha").unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(
        edits[0].range,
        start..start + "> alpha line\n> beta line".len()
    );
    assert_eq!(edits[0].text, "> alpha line more\n> beta line");
    assert!(harness.tbuf().lines()[row as usize].contains("alpha line more"));
    settle(&mut harness, &manual);
    // The new block shows rendered, marked, on the same screen row.
    assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
    let (_, rows) = marked(&mut harness).unwrap();
    assert_eq!(rows.start - view(&mut harness).0, row);
    assert_eq!(bar_rows(&mut harness)[0], rows.start);
    let selected = reader(&mut harness, |reader, _| Ok(reader.selected_block())).unwrap();
    assert_eq!(selected.range.start, start);
}

#[test]
fn a_discard_asks_first_and_drops_changes_since_the_checkpoint() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "e");
    harness.key('A').unwrap();
    type_text(&mut harness, " kept");
    press(&mut harness, "Escape");
    reader(&mut harness, |reader, ctx| reader.checkpoint_block(ctx));
    harness.key('A').unwrap();
    type_text(&mut harness, " dropped");
    press(&mut harness, "Escape");
    // ctrl-x asks; keep editing declines.
    press(&mut harness, "ctrl-x");
    assert!(reader(&mut harness, |reader, _| Ok(reader.block_open())));
    press(&mut harness, "k");
    assert!(reader(&mut harness, |reader, _| Ok(reader.block_open())));
    assert_eq!(draft(&mut harness).unwrap(), "## One kept dropped");
    // Discard returns to the checkpoint, which differs from the text before
    // the edit, so it commits it.
    press(&mut harness, "ctrl-x");
    press(&mut harness, "d");
    settle(&mut harness, &manual);
    assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
    let edits = reader(&mut harness, |reader, _| Ok(reader.take_edits()));
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].text, "## One kept");
    assert!(
        reader(&mut harness, |reader, _| Ok(reader.accepted_source())).starts_with("## One kept\n")
    );
}

#[test]
fn a_discard_without_changes_closes_at_once() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "e");
    press(&mut harness, "ctrl-x");
    assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
    assert!(reader(&mut harness, |reader, _| Ok(reader.take_edits())).is_empty());
}

#[test]
fn the_readers_keys_never_reach_the_editor() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "e");
    // An edit, then undo and redo in the editor: ctrl-r is the editor's redo,
    // not the reader's view toggle.
    harness.key('x').unwrap();
    service(&mut harness);
    harness.key('u').unwrap();
    service(&mut harness);
    press(&mut harness, "ctrl-r");
    assert_eq!(editor_state(&mut harness).0, "## ne");
    assert_eq!(
        reader(&mut harness, |reader, _| Ok(reader.view_type())),
        ViewType::Rendered
    );
    // The reader's ctrl-s, ctrl-o, and n do nothing, and / searches the
    // editor.
    for key in ["ctrl-s", "ctrl-o", "n"] {
        press(&mut harness, key);
    }
    assert!(reader(&mut harness, |reader, _| Ok(reader
        .inspector
        .is_none())));
    press(&mut harness, "/");
    assert!(reader(&mut harness, |reader, ctx| {
        let search = reader.nodes.as_ref().unwrap().search;
        Ok(ctx.view_of(search).is_none_or(|view| view.content.h == 0))
    }));
    assert!(reader(&mut harness, |reader, _| Ok(reader.block_open())));
}

#[test]
fn ctrl_x_is_the_editors_discard_and_a_hosts_key_otherwise() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    harness
        .script(r#"canopy.bind("ctrl-x", { description = "Host" }, command.markdown.lock_blocks(true))"#)
        .unwrap();
    let locked = |harness: &mut Harness| reader(harness, |reader, _| Ok(reader.blocks_locked()));
    // With no editor open, the host's ctrl-x takes the key.
    press(&mut harness, "ctrl-x");
    assert!(locked(&mut harness));
    reader(&mut harness, |reader, _| {
        reader.lock_blocks(false);
        Ok(())
    });
    // In the editor, ctrl-x discards before the editor reads it, even in
    // visual mode.
    press(&mut harness, "e");
    harness.key('x').unwrap();
    service(&mut harness);
    press(&mut harness, "v");
    press(&mut harness, "ctrl-x");
    assert!(!locked(&mut harness));
    assert_eq!(draft(&mut harness).unwrap(), "## ne");
    press(&mut harness, "d");
    settle(&mut harness, &manual);
    assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
    assert!(reader(&mut harness, |reader, _| Ok(reader.take_edits())).is_empty());
}

#[test]
fn e_does_nothing_in_a_reader_without_a_block_editor() {
    let (mut harness, manual) = editing_with(DOCUMENT, EditMode::Vi, 40, 20, |options| {
        options.block_editor = None;
    });
    settle(&mut harness, &manual);
    let before = screen(&harness);
    press(&mut harness, "e");
    assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
    assert_eq!(screen(&harness), before);
    reader(&mut harness, |reader, ctx| reader.edit_block(ctx));
    assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
}

#[test]
fn esc_in_text_mode_commits() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Text, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "e");
    type_text(&mut harness, "Big ");
    press(&mut harness, "Escape");
    settle(&mut harness, &manual);
    let edits = reader(&mut harness, |reader, _| Ok(reader.take_edits()));
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].text, "## Big One");
}

#[test]
fn apply_edit_refuses_what_it_cannot_make() {
    let text = "one\r\ntwo é\r\n";
    let (mut harness, manual) = editing_with(text, EditMode::Vi, 40, 20, |options| {
        options.source_limit = text.len();
    });
    settle(&mut harness, &manual);
    let epoch = reader(&mut harness, |reader, _| Ok(reader.epoch()));
    let refused = |harness: &mut Harness, epoch: u64, range: Range<usize>, text: &str| {
        reader(harness, |reader, ctx| {
            Ok(reader
                .apply_edit(ctx, epoch, range.start, range.end, text.to_owned())
                .is_err())
        })
    };
    // An old epoch, a split character, a split CRLF pair, a range past the
    // end, and a text past the source limit.
    assert!(refused(&mut harness, epoch + 1, 0..1, "x"));
    assert!(refused(&mut harness, epoch, 9..10, "x"));
    assert!(refused(&mut harness, epoch, 4..5, "x"));
    assert!(refused(&mut harness, epoch, 0..99, "x"));
    assert!(refused(&mut harness, epoch, 0..0, "x"));
    // A locked reader refuses too, and nothing changed.
    reader(&mut harness, |reader, _| {
        reader.lock_blocks(true);
        Ok(())
    });
    assert!(refused(&mut harness, epoch, 0..3, "ONE"));
    let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
    assert_eq!(&*source, text);
    assert!(reader(&mut harness, |reader, _| Ok(reader.take_edits())).is_empty());
    // Unlocked, it edits.
    reader(&mut harness, |reader, ctx| {
        reader.lock_blocks(false);
        reader.apply_edit(ctx, epoch, 0, 3, "ONE".into())
    });
    let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
    assert_eq!(&*source, "ONE\r\ntwo é\r\n");
    settle(&mut harness, &manual);
    assert_eq!(
        reader(&mut harness, |reader, _| Ok(reader.take_edits())).len(),
        1
    );
}

#[test]
fn a_locked_reader_opens_commits_and_discards_nothing() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    reader(&mut harness, |reader, _| {
        reader.lock_blocks(true);
        Ok(())
    });
    press(&mut harness, "e");
    assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
    reader(&mut harness, |reader, _| {
        reader.lock_blocks(false);
        Ok(())
    });
    press(&mut harness, "e");
    harness.key('x').unwrap();
    service(&mut harness);
    reader(&mut harness, |reader, _| {
        reader.lock_blocks(true);
        Ok(())
    });
    press(&mut harness, "Escape");
    press(&mut harness, "ctrl-x");
    reader(&mut harness, |reader, ctx| reader.commit_block(ctx));
    reader(&mut harness, |reader, ctx| reader.discard_block(ctx, true));
    assert!(reader(&mut harness, |reader, _| Ok(reader.block_open())));
    assert!(reader(&mut harness, |reader, _| Ok(reader.take_edits())).is_empty());
}

#[test]
fn e_in_a_document_without_units_opens_an_empty_editor_at_the_end() {
    for (text, expected) in [
        ("", "First"),
        ("[a]: https://a.test\n", "[a]: https://a.test\n\nFirst"),
    ] {
        let (mut harness, manual) = editing(text, EditMode::Text, 40, 10);
        settle(&mut harness, &manual);
        press(&mut harness, "e");
        assert_eq!(draft(&mut harness).as_deref(), Some(""), "{text:?}");
        type_text(&mut harness, "First");
        press(&mut harness, "Escape");
        settle(&mut harness, &manual);
        let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
        assert_eq!(&*source, expected);
        let unit = reader(&mut harness, |reader, _| Ok(reader.selected_block())).unwrap();
        assert_eq!(&source[unit.range], "First");
    }
}

#[test]
fn a_direct_edit_keeps_the_rendered_rows_until_its_layout_shows() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    press(&mut harness, "j");
    let (_, rows) = marked(&mut harness).unwrap();
    let row = rows.start - view(&mut harness).0;
    let unit = reader(&mut harness, |reader, _| Ok(reader.selected_block())).unwrap();
    let before = harness.tbuf().lines()[row as usize].clone();
    reader(&mut harness, |reader, ctx| {
        let epoch = reader.epoch();
        reader.apply_edit(
            ctx,
            epoch,
            unit.range.start,
            unit.range.end,
            "- item A, edited".into(),
        )
    });
    // Held: the old rows still show, and j and k wait for the new layout.
    service(&mut harness);
    assert_eq!(harness.tbuf().lines()[row as usize], before);
    press(&mut harness, "j");
    assert_eq!(
        marked(&mut harness).map(|(_, rows)| rows.start - view(&mut harness).0),
        Some(row)
    );
    settle(&mut harness, &manual);
    assert!(harness.tbuf().lines()[row as usize].contains("item A, edited"));
    let (_, rows) = marked(&mut harness).unwrap();
    assert_eq!(rows.start - view(&mut harness).0, row);
}

#[test]
fn a_resize_keeps_the_draft_and_the_editor_cursor() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    press(&mut harness, "e");
    harness.key('A').unwrap();
    type_text(&mut harness, " draft");
    let (text, cursor, _) = editor_state(&mut harness);
    harness.canopy.set_screen_size(Size::new(24, 14)).unwrap();
    settle(&mut harness, &manual);
    assert!(reader(&mut harness, |reader, _| Ok(reader.block_open())));
    assert_eq!(editor_state(&mut harness).0, text);
    assert_eq!(editor_state(&mut harness).1, cursor);
    let (above, slot, below) =
        reader(&mut harness, |reader, ctx| Ok(slot::parts(reader, ctx))).unwrap();
    let (_, height) = view(&mut harness);
    assert_eq!(above + slot + below, height);
}

#[test]
fn opening_the_editor_closes_the_search_field() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    let open = |harness: &mut Harness| {
        reader(harness, |reader, ctx| {
            let search = reader.nodes.as_ref().unwrap().search;
            Ok(ctx.view_of(search).is_some_and(|view| view.outer.h > 0))
        })
    };
    reader(&mut harness, |reader, ctx| reader.start_search(ctx));
    service(&mut harness);
    assert!(open(&mut harness));
    reader(&mut harness, |reader, ctx| reader.edit_block(ctx));
    service(&mut harness);
    assert!(!open(&mut harness));
    assert!(reader(&mut harness, |reader, _| Ok(reader.block_open())));
    // The reader's search does nothing until the editor closes.
    reader(&mut harness, |reader, ctx| reader.start_search(ctx));
    service(&mut harness);
    assert!(!open(&mut harness));
}

#[test]
fn a_tall_unit_opens_on_the_line_of_its_first_row_in_view() {
    // Code maps rows to source exactly; a long paragraph of inline markup
    // does not, so it opens at the same fraction of the unit.
    let code: String = (0..30).map(|n| format!("line {n}\n")).collect();
    let prose: String = (0..30).map(|n| format!("*word {n}*\n")).collect();
    for (text, expected) in [
        (format!("# Top\n\n```\n{code}```\n"), "line 14"),
        (format!("# Top\n\n{prose}"), "*word "),
    ] {
        let (mut harness, manual) = editing(&text, EditMode::Vi, 30, 10);
        settle(&mut harness, &manual);
        press(&mut harness, "j");
        press(&mut harness, "j");
        press(&mut harness, "j");
        let (_, rows) = marked(&mut harness).unwrap();
        let (top, _) = view(&mut harness);
        assert!(rows.start < top, "the unit continues above the view");
        press(&mut harness, "e");
        let (editor_text, cursor, _) = editor_state(&mut harness);
        let line = editor_text.lines().nth(cursor.line).unwrap();
        assert!(line.contains(expected), "{line:?} in {text:?}");
        assert!(cursor.line > 0);
        // The slot is at the top of the view, and its first row shows the
        // line the editor opens on.
        let lines = harness.tbuf().lines();
        assert!(lines[0].contains(line), "{line:?} on {lines:#?}");
    }
}

/// Commit `replacement` in place of the unit that `j` reaches `steps` times,
/// and return the text after.
fn commit_edit(text: &str, steps: usize, replacement: &str) -> String {
    let (mut harness, manual) = editing(text, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    for _ in 0..steps {
        press(&mut harness, "j");
    }
    press(&mut harness, "e");
    reader(&mut harness, |reader, ctx| {
        let editor = reader.block_editor().unwrap();
        ctx.with_widget_mut(editor, |editor: &mut Editor, _| {
            editor.set_text(replacement);
            Ok(())
        })
    });
    press(&mut harness, "Escape");
    settle(&mut harness, &manual);
    let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source())).to_string();
    // The cursor marks the unit at the edit's first byte, or none.
    let units = parse_units(&source);
    let selected = reader(&mut harness, |reader, _| Ok(reader.selected_block()));
    assert_eq!(selected.is_some(), !units.is_empty(), "{source:?}");
    source
}

#[test]
fn edits_that_change_structure_land_as_text() {
    let text = "one\n\ntwo\n\nthree\n";
    // Empty a unit, merge two, split one, open a fence that takes the rest,
    // and edit at the start and the end of the text.
    assert_eq!(commit_edit(text, 1, ""), "one\n\n\n\nthree\n");
    assert_eq!(
        commit_edit(text, 0, "one\ncontinues"),
        "one\ncontinues\n\ntwo\n\nthree\n"
    );
    assert_eq!(commit_edit(text, 1, "t\n\nwo"), "one\n\nt\n\nwo\n\nthree\n");
    assert_eq!(commit_edit(text, 1, "```"), "one\n\n```\n\nthree\n");
    assert_eq!(commit_edit(text, 0, "ONE"), "ONE\n\ntwo\n\nthree\n");
    assert_eq!(commit_edit(text, 2, "THREE"), "one\n\ntwo\n\nTHREE\n");
    // Deleting the only unit leaves no unit to mark.
    assert_eq!(commit_edit("solo", 0, ""), "");
}
