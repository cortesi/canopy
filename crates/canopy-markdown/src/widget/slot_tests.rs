//! The block editor in the slot, driven through the reader's keys and
//! commands, as a host with no bindings of its own drives it.

use canopy::{render::TermBuf, runtime::NoticeLevel, testing::harness::Harness};
use canopy_widgets::editor::{EditMode, EditorConfig, TextPosition, ViMode};

use super::{
    cursor_tests::{bar_rows, marked, press, service, settle, view},
    *,
};
use crate::{
    executor::{ExecutorOptions, ManualExecutor, MarkdownExecutor},
    units::parse_units,
};

/// The notice of the tests' lock on block edits.
const LOCKED: &str = "Blocks wait for the words";

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
        .script("markdown-defaults", "markdown.default_bindings()")
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
    // ctrl-x asks, and asking again while the question shows changes
    // nothing; keep editing declines.
    press(&mut harness, "ctrl-x");
    reader(&mut harness, |reader, ctx| reader.discard_block(ctx, false));
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
        .script(r#"canopy.bind("ctrl-x", { description = "Host" }, command.markdown.lock_blocks("Host"))"#)
        .unwrap();
    let locked = |harness: &mut Harness| reader(harness, |reader, _| Ok(reader.blocks_locked()));
    // With no editor open, the host's ctrl-x takes the key.
    press(&mut harness, "ctrl-x");
    assert!(locked(&mut harness));
    reader(&mut harness, |reader, _| {
        reader.lock_blocks(None);
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
    for key in ["e", "a", "d"] {
        press(&mut harness, key);
        assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
        assert_eq!(screen(&harness), before, "{key}");
    }
    reader(&mut harness, |reader, ctx| reader.edit_block(ctx));
    assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
    assert!(reader(&mut harness, |reader, _| Ok(reader.take_edits())).is_empty());
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
    // An old epoch, a reversed range, a split character at either end, a
    // split CRLF pair, a range past the end, and a text past the source
    // limit.
    assert!(refused(&mut harness, epoch + 1, 0..1, "x"));
    #[expect(clippy::reversed_empty_ranges, reason = "the edit's range is reversed")]
    let reversed = 3..1;
    assert!(refused(&mut harness, epoch, reversed, "x"));
    assert!(refused(&mut harness, epoch, 9..10, "x"));
    assert!(refused(&mut harness, epoch, 10..11, "x"));
    assert!(refused(&mut harness, epoch, 4..5, "x"));
    assert!(refused(&mut harness, epoch, 0..99, "x"));
    assert!(refused(&mut harness, epoch, 0..0, "x"));
    // A locked reader refuses too, and nothing changed.
    reader(&mut harness, |reader, _| {
        reader.lock_blocks(Some(LOCKED.into()));
        Ok(())
    });
    assert!(refused(&mut harness, epoch, 0..3, "ONE"));
    let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
    assert_eq!(&*source, text);
    assert!(reader(&mut harness, |reader, _| Ok(reader.take_edits())).is_empty());
    // Unlocked, it edits.
    reader(&mut harness, |reader, ctx| {
        reader.lock_blocks(None);
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
        reader.lock_blocks(Some(LOCKED.into()));
        Ok(())
    });
    press(&mut harness, "e");
    assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
    reader(&mut harness, |reader, _| {
        reader.lock_blocks(None);
        Ok(())
    });
    press(&mut harness, "e");
    harness.key('x').unwrap();
    service(&mut harness);
    reader(&mut harness, |reader, _| {
        reader.lock_blocks(Some(LOCKED.into()));
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
    for (text, expected, key) in [
        ("", "First", "e"),
        ("[a]: https://a.test\n", "[a]: https://a.test\n\nFirst", "e"),
        ("[a]: https://a.test\n", "[a]: https://a.test\n\nFirst", "a"),
    ] {
        let (mut harness, manual) = editing(text, EditMode::Text, 40, 10);
        settle(&mut harness, &manual);
        press(&mut harness, key);
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
fn a_opens_an_empty_editor_after_the_marked_unit_and_its_commit_adds_a_block() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    let next = super::cursor_tests::extents(&mut harness)[2].start;
    let row = next - view(&mut harness).0;
    press(&mut harness, "a");
    // The editor opens empty, and takes text at once.
    let (text, _, mode) = editor_state(&mut harness);
    assert_eq!(text, "");
    assert_eq!(mode, Some(ViMode::Insert));
    // It lies where the next unit started, with the blank rows between the
    // units above and below it.
    let lines = harness.tbuf().lines();
    assert_eq!(harness.buf().get(Point::new(0, row)).unwrap().ch, '▌');
    assert!(lines[row as usize - 1].trim().is_empty(), "{lines:#?}");
    assert!(lines[row as usize + 1].trim().is_empty(), "{lines:#?}");
    assert!(lines[row as usize + 2].contains("item a"), "{lines:#?}");
    type_text(&mut harness, "new words");
    press(&mut harness, "Escape");
    press(&mut harness, "Escape");
    // The new block lies in the quote, after a blank line of it.
    let at = DOCUMENT.find("> beta line").unwrap() + "> beta line".len();
    let edits = reader(&mut harness, |reader, _| Ok(reader.take_edits()));
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].range, at..at);
    assert_eq!(edits[0].text, "\n>\n> new words");
    settle(&mut harness, &manual);
    // The cursor marks the new block, on the row where the editor was.
    let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
    let selected = reader(&mut harness, |reader, _| Ok(reader.selected_block())).unwrap();
    assert_eq!(&source[selected.range], "> new words");
    let (_, rows) = marked(&mut harness).unwrap();
    assert_eq!(rows.start - view(&mut harness).0, row);
}

#[test]
fn d_deletes_the_marked_unit_and_marks_the_next_on_its_row() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    let (_, rows) = marked(&mut harness).unwrap();
    let row = rows.start - view(&mut harness).0;
    press(&mut harness, "d");
    // One edit drops the quote and the blank line after it.
    let start = DOCUMENT.find("> alpha").unwrap();
    let end = DOCUMENT.find("- item a").unwrap();
    let edits = reader(&mut harness, |reader, _| Ok(reader.take_edits()));
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].range, start..end);
    assert_eq!(edits[0].text, "");
    settle(&mut harness, &manual);
    // The next unit takes the deleted unit's row.
    let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
    let selected = reader(&mut harness, |reader, _| Ok(reader.selected_block())).unwrap();
    assert_eq!(&source[selected.range], "- item a");
    let (_, rows) = marked(&mut harness).unwrap();
    assert_eq!(rows.start - view(&mut harness).0, row);
    // The last unit goes with the blank line before it, and the cursor
    // marks the unit before it.
    press(&mut harness, "G");
    press(&mut harness, "d");
    settle(&mut harness, &manual);
    let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
    assert!(source.ends_with("c2\n```\n"), "{source:?}");
    let selected = reader(&mut harness, |reader, _| Ok(reader.selected_block())).unwrap();
    assert_eq!(&source[selected.range], "```\nc1\nc2\n```");
}

#[test]
fn a_blank_new_block_adds_nothing() {
    for mode in [EditMode::Vi, EditMode::Text] {
        let (mut harness, manual) = editing(DOCUMENT, mode, 40, 20);
        settle(&mut harness, &manual);
        press(&mut harness, "j");
        let before = screen(&harness);
        press(&mut harness, "a");
        type_text(&mut harness, "  ");
        press(&mut harness, "Escape");
        press(&mut harness, "Escape");
        settle(&mut harness, &manual);
        assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
        assert!(reader(&mut harness, |reader, _| Ok(reader.take_edits())).is_empty());
        assert_eq!(screen(&harness), before, "{mode:?}");
    }
}

#[test]
fn the_draft_of_a_new_block_is_its_source() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Text, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    press(&mut harness, "j");
    press(&mut harness, "a");
    type_text(&mut harness, "more");
    let at = DOCUMENT.find("- item a").unwrap() + "- item a".len();
    let edit = reader(&mut harness, |reader, ctx| Ok(reader.block_draft(ctx))).unwrap();
    // After a list item, the new block is the next item of the tight list.
    assert_eq!(edit.range, at..at);
    assert_eq!(edit.text, "\n- more");
}

#[test]
fn a_new_item_without_its_marker_is_a_paragraph_after_its_item() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    press(&mut harness, "j");
    press(&mut harness, "a");
    press(&mut harness, "Backspace");
    press(&mut harness, "Backspace");
    type_text(&mut harness, "para");
    press(&mut harness, "Escape");
    press(&mut harness, "Escape");
    // The paragraph splits the list, with blank lines around it.
    let at = DOCUMENT.find("- item a").unwrap() + "- item a".len();
    let edits = reader(&mut harness, |reader, _| Ok(reader.take_edits()));
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].range, at..at);
    assert_eq!(edits[0].text, "\n\npara\n");
    settle(&mut harness, &manual);
    // The cursor marks the paragraph.
    let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
    let selected = reader(&mut harness, |reader, _| Ok(reader.selected_block())).unwrap();
    assert_eq!(&source[selected.range], "para");
}

#[test]
fn a_new_item_opens_on_its_marker_and_lands_at_the_level_of_its_item() {
    let text = "Intro\n\n1. one\n   - inner\n2. two\n";
    let (mut harness, manual) = editing(text, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    press(&mut harness, "a");
    // The editor opens on the next marker, with the cursor after it.
    let (draft, cursor, mode) = editor_state(&mut harness);
    assert_eq!(draft, "2. ");
    assert_eq!(cursor, TextPosition::new(0, 3));
    assert_eq!(mode, Some(ViMode::Insert));
    type_text(&mut harness, "new");
    press(&mut harness, "Escape");
    press(&mut harness, "Escape");
    // The item follows the nested list of its item, at its level.
    let at = text.find("   - inner").unwrap() + "   - inner".len();
    let edits = reader(&mut harness, |reader, _| Ok(reader.take_edits()));
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].range, at..at);
    assert_eq!(edits[0].text, "\n2. new");
    settle(&mut harness, &manual);
    let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
    assert_eq!(&*source, "Intro\n\n1. one\n   - inner\n2. new\n2. two\n");
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
fn an_edit_of_a_tall_unit_puts_its_first_row_at_the_top_by_either_path() {
    let code: String = (0..30).map(|n| format!("line {n}\n")).collect();
    let text = format!("# Top\n\n```\n{code}```\n\nend\n");
    let new: String = (0..30).map(|n| format!("new {n}\n")).collect();
    let replacement = format!("```\n{new}```");
    let mut ends = Vec::new();
    for direct in [false, true] {
        let (mut harness, manual) = editing(&text, EditMode::Vi, 30, 10);
        settle(&mut harness, &manual);
        for _ in 0..3 {
            press(&mut harness, "j");
        }
        let (_, rows) = marked(&mut harness).unwrap();
        assert!(
            rows.start < view(&mut harness).0,
            "the unit starts above the view"
        );
        let unit = reader(&mut harness, |reader, _| Ok(reader.selected_block())).unwrap();
        if direct {
            reader(&mut harness, |reader, ctx| {
                let epoch = reader.epoch();
                reader.apply_edit(
                    ctx,
                    epoch,
                    unit.range.start,
                    unit.range.end,
                    replacement.clone(),
                )
            });
        } else {
            press(&mut harness, "e");
            reader(&mut harness, |reader, ctx| {
                let editor = reader.block_editor().unwrap();
                ctx.with_widget_mut(editor, |editor: &mut Editor, _| {
                    editor.set_text(replacement.clone());
                    Ok(())
                })?;
                reader.commit_block(ctx)
            });
        }
        // Both record the unit's first row on the top row, where the slot
        // opens on such a unit.
        let pending = reader(&mut harness, |reader, _| {
            Ok(reader.shared.borrow().pending_mark)
        });
        assert_eq!(pending.and_then(|mark| mark.row), Some(0));
        settle(&mut harness, &manual);
        let (_, rows) = marked(&mut harness).unwrap();
        let (top, _) = view(&mut harness);
        assert_eq!(rows.start, top, "the edited unit starts at the top");
        ends.push((pending.map(|mark| mark.byte), screen(&harness)));
    }
    assert_eq!(ends[0], ends[1]);
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

#[test]
fn closing_the_editor_keeps_the_view_where_it_was() {
    // A view scrolled down stays put through an edit with no change.
    let text: String = (0..20).map(|n| format!("paragraph {n}\n\n")).collect();
    let (mut harness, manual) = editing(&text, EditMode::Vi, 30, 8);
    settle(&mut harness, &manual);
    press(&mut harness, "G");
    let (top, _) = view(&mut harness);
    assert!(top > 0);
    let before = marked(&mut harness);
    press(&mut harness, "e");
    press(&mut harness, "Escape");
    settle(&mut harness, &manual);
    assert_eq!(view(&mut harness).0, top);
    assert_eq!(marked(&mut harness), before);
}

#[test]
fn e_does_nothing_where_the_reader_cannot_edit() {
    let open = |harness: &mut Harness| reader(harness, |reader, _| Ok(reader.block_open()));
    // An open editor stays as it is.
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "e");
    let editor = reader(&mut harness, |reader, _| Ok(reader.block_editor()));
    reader(&mut harness, |reader, ctx| reader.edit_block(ctx));
    assert_eq!(
        reader(&mut harness, |reader, _| Ok(reader.block_editor())),
        editor
    );
    // An inspector covers the reader.
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    reader(&mut harness, |reader, ctx| reader.inspect_source(ctx));
    service(&mut harness);
    assert!(reader(&mut harness, |reader, _| Ok(reader
        .inspector
        .is_some())));
    reader(&mut harness, |reader, ctx| reader.edit_block(ctx));
    assert!(!open(&mut harness));
    // Source shows.
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    reader(&mut harness, |reader, ctx| {
        reader.set_view_type(ctx, ViewType::Source)
    });
    settle(&mut harness, &manual);
    reader(&mut harness, |reader, ctx| reader.edit_block(ctx));
    assert!(!open(&mut harness));
    // A new text has not shown yet.
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    reader(&mut harness, |reader, ctx| {
        reader
            .replace(ctx, DocumentInput::Complete("# New\n".into()))
            .map(|_| ())
    });
    reader(&mut harness, |reader, ctx| reader.edit_block(ctx));
    assert!(!open(&mut harness));
    settle(&mut harness, &manual);
    reader(&mut harness, |reader, ctx| reader.edit_block(ctx));
    assert!(open(&mut harness));
}

#[test]
fn e_from_scroll_enters_the_block_cursor_and_edits_its_unit() {
    let (mut harness, manual) = editing_with(DOCUMENT, EditMode::Vi, 40, 20, |options| {
        options.resting_mode = Mode::Scroll;
    });
    settle(&mut harness, &manual);
    assert!(marked(&mut harness).is_none());
    press(&mut harness, "e");
    assert_eq!(editor_state(&mut harness).0, "## One");
}

#[test]
fn a_tall_unit_that_starts_at_the_top_opens_with_its_first_line_showing() {
    let code: String = (0..30).map(|n| format!("line {n}\n")).collect();
    let text = format!("# Top\n\n```\n{code}```\n");
    let (mut harness, manual) = editing(&text, EditMode::Vi, 30, 10);
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    let (_, rows) = marked(&mut harness).unwrap();
    assert_eq!(
        rows.start,
        view(&mut harness).0,
        "the unit starts at the top"
    );
    press(&mut harness, "e");
    // The editor opens on the code's first line, below the fence, and the
    // fence still shows.
    assert_eq!(editor_state(&mut harness).1, TextPosition::new(1, 0));
    assert!(harness.tbuf().lines()[0].contains("```"));
}

#[test]
fn the_slot_follows_the_margin_through_a_resize() {
    let (mut harness, manual) = editing_with(DOCUMENT, EditMode::Vi, 40, 20, |options| {
        options.centered = true;
        options.prose_width = 20;
    });
    settle(&mut harness, &manual);
    press(&mut harness, "e");
    // The column of the editor's text on the top row, and the presentation's
    // inset and bar.
    let place = |harness: &mut Harness| {
        let line = &harness.tbuf().lines()[0];
        let x = line[..line.find("## One").unwrap()].chars().count() as u32;
        reader(harness, |reader, _| {
            let state = reader.shared.borrow();
            let presentation = state.presentation.as_ref().unwrap();
            Ok((x, presentation.inset(), presentation.bar_column()))
        })
    };
    let (x, inset, bar) = place(&mut harness);
    assert_eq!((x, bar), (inset, inset - 2));
    assert_eq!(harness.buf().get(Point::new(bar, 0)).unwrap().ch, '▌');
    harness.canopy.set_screen_size(Size::new(60, 20)).unwrap();
    settle(&mut harness, &manual);
    let (x, new_inset, bar) = place(&mut harness);
    assert!(new_inset > inset, "a wider view moves the margin in");
    assert_eq!((x, bar), (new_inset, new_inset - 2));
    assert_eq!(harness.buf().get(Point::new(bar, 0)).unwrap().ch, '▌');
}

#[test]
fn the_scrollbar_hides_while_the_editor_is_open() {
    let text: String = (0..30).map(|n| format!("para {n}\n\n")).collect();
    let (mut harness, manual) = editing(&text, EditMode::Vi, 30, 10);
    settle(&mut harness, &manual);
    for _ in 0..12 {
        press(&mut harness, "j");
    }
    let last = |harness: &Harness| -> String {
        (0..10)
            .map(|y| harness.buf().get(Point::new(29, y)).unwrap().ch)
            .collect()
    };
    assert_ne!(last(&harness).trim(), "", "the scrollbar shows");
    press(&mut harness, "e");
    assert_eq!(last(&harness).trim(), "", "the scrollbar hides");
}

#[test]
fn the_rows_around_the_editor_keep_their_images() {
    let text = "![Local](pics/a.png)\n\nAfter.\n";
    let (mut harness, manual) = editing_with(text, EditMode::Vi, 40, 20, |options| {
        options.resources = true;
    });
    settle(&mut harness, &manual);
    let requests = reader(
        &mut harness,
        |reader, _| Ok(reader.take_resource_requests()),
    );
    reader(&mut harness, |reader, ctx| {
        reader.supply_resource(
            ctx,
            requests[0].id,
            Resource::Image(super::tests::two_tone()),
        )
    });
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    let (_, rows) = marked(&mut harness).unwrap();
    // The cells of the rows above the unit, with their styles.
    let image = |harness: &Harness| -> Vec<String> {
        let screen = harness.buf();
        (0..rows.start)
            .flat_map(|y| (0..40).map(move |x| (x, y)))
            .map(|(x, y)| format!("{:?}", screen.get(Point::new(x, y))))
            .collect()
    };
    let before = image(&harness);
    // The image paints its colours as the ground of blank cells.
    let blank = format!("{:?}", harness.buf().get(Point::new(39, 19)));
    assert!(before.iter().any(|cell| *cell != blank), "the image shows");
    press(&mut harness, "e");
    assert!(reader(&mut harness, |reader, _| Ok(reader.block_open())));
    assert_eq!(image(&harness), before);
}

#[test]
fn a_commit_keeps_the_images() {
    let text = "![Local](pics/a.png)\n\nAfter.\n";
    let (mut harness, manual) = editing_with(text, EditMode::Vi, 40, 20, |options| {
        options.resources = true;
    });
    settle(&mut harness, &manual);
    let requests = reader(
        &mut harness,
        |reader, _| Ok(reader.take_resource_requests()),
    );
    reader(&mut harness, |reader, ctx| {
        reader.supply_resource(
            ctx,
            requests[0].id,
            Resource::Image(super::tests::two_tone()),
        )
    });
    settle(&mut harness, &manual);
    press(&mut harness, "j");
    let (_, rows) = marked(&mut harness).unwrap();
    // The cells of the rows above the unit, with their styles.
    let image = |harness: &Harness| -> Vec<String> {
        let screen = harness.buf();
        (0..rows.start)
            .flat_map(|y| (0..40).map(move |x| (x, y)))
            .map(|(x, y)| format!("{:?}", screen.get(Point::new(x, y))))
            .collect()
    };
    let before = image(&harness);
    press(&mut harness, "e");
    press(&mut harness, "A");
    type_text(&mut harness, " More.");
    press(&mut harness, "Escape");
    press(&mut harness, "Escape");
    settle(&mut harness, &manual);
    assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
    // The new text keeps the image that the host supplied, and asks for
    // nothing again.
    assert_eq!(image(&harness), before);
    let again = reader(
        &mut harness,
        |reader, _| Ok(reader.take_resource_requests()),
    );
    assert!(again.is_empty());
}

#[test]
fn a_commit_keeps_the_unit_where_the_editor_moved_to_fit() {
    let paras = |range: Range<u32>| -> String { range.map(|n| format!("para {n}\n\n")).collect() };
    let text = format!("{}```\ncode\n```\n\n{}", paras(0..7), paras(7..20));
    let (mut harness, manual) = editing(&text, EditMode::Vi, 30, 20);
    settle(&mut harness, &manual);
    for _ in 0..7 {
        press(&mut harness, "j");
    }
    let (_, rows) = marked(&mut harness).unwrap();
    let opened = rows.start - view(&mut harness).0;
    press(&mut harness, "e");
    for n in 0..10 {
        press(&mut harness, "o");
        type_text(&mut harness, &format!("more {n}"));
        press(&mut harness, "Escape");
    }
    let (above, slot, _) =
        reader(&mut harness, |reader, ctx| Ok(slot::parts(reader, ctx))).unwrap();
    assert!(above < opened, "the editor moved up to fit its {slot} rows");
    press(&mut harness, "Escape");
    settle(&mut harness, &manual);
    let (_, rows) = marked(&mut harness).unwrap();
    assert_eq!(rows.start - view(&mut harness).0, above);
}

#[test]
fn the_readers_commands_wait_while_the_editor_is_open() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "e");
    let editor = reader(&mut harness, |reader, _| Ok(reader.block_editor())).unwrap();
    reader(&mut harness, |reader, ctx| {
        reader.return_position = Some(reader.position(ctx)?);
        reader.select_text(ctx)?;
        reader.follow(ctx)?;
        reader.return_from_jump(ctx)?;
        reader.set_mode(ctx, Mode::Scroll)
    });
    service(&mut harness);
    assert!(reader(&mut harness, |reader, ctx| {
        let state = reader.shared.borrow();
        Ok(reader.block_open()
            && ctx.focused_node() == Some(editor.into())
            && reader.return_position.is_some()
            && state.overlays.selection.is_none()
            && state.follow == Follow::Paused
            && state.mode == Mode::Interact)
    }));
}

#[test]
fn an_inspector_keeps_the_block_cursor_out() {
    let (mut harness, manual) = editing_with(DOCUMENT, EditMode::Vi, 40, 20, |options| {
        options.resting_mode = Mode::Scroll;
    });
    settle(&mut harness, &manual);
    reader(&mut harness, |reader, ctx| reader.inspect_source(ctx));
    service(&mut harness);
    reader(&mut harness, |reader, ctx| {
        reader.set_mode(ctx, Mode::Interact)
    });
    assert_eq!(
        reader(&mut harness, |reader, _| Ok(reader.shared.borrow().mode)),
        Mode::Scroll
    );
}

#[test]
fn focus_block_returns_focus_to_the_editor() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "e");
    let focused = reader(&mut harness, |reader, ctx| {
        let editor = reader.block_editor().unwrap();
        ctx.set_focus(reader.nodes.as_ref().unwrap().rendered)?;
        reader.focus_block(ctx)?;
        Ok(ctx.focused_node() == Some(editor.into()))
    });
    assert!(focused);
}

#[test]
fn keys_that_need_the_new_layout_work_as_soon_as_it_shows() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "e");
    harness.key('x').unwrap();
    press(&mut harness, "Escape");
    let epoch = reader(&mut harness, |reader, _| Ok(reader.epoch()));
    let shown = |harness: &mut Harness| {
        reader(harness, |reader, _| {
            Ok(reader
                .shared
                .borrow()
                .presentation
                .as_ref()
                .map(|p| p.document.source.epoch))
        })
    };
    // Run the work a job at a time, and stop as soon as the new text shows.
    while shown(&mut harness) != Some(epoch) {
        service(&mut harness);
        assert!(manual.run_next(), "work remains until the new text shows");
        service(&mut harness);
    }
    assert!(!reader(&mut harness, |reader, _| Ok(reader
        .shared
        .borrow()
        .awaiting)));
    let (index, _) = marked(&mut harness).unwrap();
    press(&mut harness, "j");
    assert_eq!(marked(&mut harness).unwrap().0, index + 1);
}

#[test]
fn a_view_of_three_columns_has_no_margin_beside_the_editor() {
    // The reader keeps a column for its scrollbar, so its text is three
    // columns wide in four.
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 4, 10);
    settle(&mut harness, &manual);
    press(&mut harness, "e");
    assert!(reader(&mut harness, |reader, _| Ok(reader.block_open())));
    assert_eq!(harness.buf().get(Point::new(0, 0)).unwrap().ch, '#');
}

#[test]
fn return_from_jump_goes_back_once() {
    let text: String = (0..30).map(|n| format!("para {n}\n\n")).collect();
    let (mut harness, manual) = editing(&text, EditMode::Vi, 30, 10);
    settle(&mut harness, &manual);
    let start = reader(&mut harness, |reader, ctx| reader.position(ctx));
    press(&mut harness, "G");
    assert!(view(&mut harness).0 > 0);
    reader(&mut harness, |reader, ctx| {
        reader.return_position = Some(start);
        reader.return_from_jump(ctx)
    });
    service(&mut harness);
    assert_eq!(view(&mut harness).0, 0);
    assert!(reader(&mut harness, |reader, _| Ok(reader
        .return_position
        .is_none())));
}

#[test]
fn a_document_that_is_still_arriving_cannot_be_edited() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    reader(&mut harness, |reader, ctx| {
        reader
            .replace(ctx, DocumentInput::Receiving("# Live\n\nmore".into()))
            .map(|_| ())
    });
    settle(&mut harness, &manual);
    let refused = reader(&mut harness, |reader, ctx| {
        let epoch = reader.epoch();
        Ok(reader.edit_block(ctx).is_err()
            && reader.apply_edit(ctx, epoch, 0, 1, "x".into()).is_err())
    });
    assert!(refused);
    assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
    assert_eq!(
        &*reader(&mut harness, |reader, _| Ok(reader.accepted_source())),
        "# Live\n\nmore"
    );
}

#[test]
fn each_change_the_lock_refuses_shows_its_notice() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    // The number of times the lock's notice has shown.
    let shown = |harness: &Harness| -> u32 {
        harness
            .canopy
            .notice_log()
            .entries()
            .iter()
            .filter(|notice| notice.message == LOCKED && notice.level == NoticeLevel::Info)
            .map(|notice| notice.repeats)
            .sum()
    };
    let lock = |harness: &mut Harness, notice: Option<&str>| {
        reader(harness, |reader, _| {
            reader.lock_blocks(notice.map(str::to_owned));
            Ok(())
        });
    };
    lock(&mut harness, Some(LOCKED));
    press(&mut harness, "d");
    assert_eq!(shown(&harness), 1, "d");
    assert!(reader(&mut harness, |reader, _| Ok(reader.take_edits())).is_empty());
    lock(&mut harness, None);
    lock(&mut harness, Some(LOCKED));
    press(&mut harness, "e");
    assert_eq!(shown(&harness), 2, "e");
    harness.script("markdown.edit_block()").unwrap();
    service(&mut harness);
    assert_eq!(shown(&harness), 3, "edit_block from a script");
    lock(&mut harness, None);
    press(&mut harness, "e");
    harness.key('x').unwrap();
    service(&mut harness);
    lock(&mut harness, Some(LOCKED));
    press(&mut harness, "Escape");
    assert_eq!(shown(&harness), 4, "Esc");
    press(&mut harness, "ctrl-x");
    assert_eq!(shown(&harness), 5, "ctrl-x");
    harness.script("markdown.commit_block()").unwrap();
    harness.script("markdown.discard_block(true)").unwrap();
    assert_eq!(
        shown(&harness),
        7,
        "commit_block and discard_block from a script"
    );
    // apply_edit returns its error to the host, with no notice.
    let refused = reader(&mut harness, |reader, ctx| {
        let epoch = reader.epoch();
        Ok(reader.apply_edit(ctx, epoch, 0, 1, "x".into()).is_err())
    });
    assert!(refused);
    assert_eq!(shown(&harness), 7, "apply_edit");
    assert!(reader(&mut harness, |reader, _| Ok(reader.block_open())));
    // Unlocked, the edit goes through and no notice shows.
    lock(&mut harness, None);
    press(&mut harness, "Escape");
    settle(&mut harness, &manual);
    assert_eq!(shown(&harness), 7);
    assert_eq!(
        reader(&mut harness, |reader, _| Ok(reader.take_edits())).len(),
        1
    );
}

#[test]
fn keys_after_a_commit_reach_the_reader_until_its_layout_shows() {
    let (mut harness, manual) = editing(DOCUMENT, EditMode::Vi, 40, 20);
    settle(&mut harness, &manual);
    press(&mut harness, "e");
    harness.key('A').unwrap();
    type_text(&mut harness, " new");
    press(&mut harness, "Escape");
    // At the start of the line, x in the editor would delete a char.
    press(&mut harness, "0");
    let editor = reader(&mut harness, |reader, _| Ok(reader.block_editor())).unwrap();
    press(&mut harness, "Escape");
    // The commit waits for its layout, and the keys before it shows go to
    // the reader: x changes nothing, and j waits.
    assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
    press(&mut harness, "x");
    press(&mut harness, "j");
    let (text, focused) = reader(&mut harness, |reader, ctx| {
        let text = ctx.with_widget(editor, |editor: &Editor| Ok(editor.text()))?;
        Ok((
            text,
            ctx.focused_node() == Some(reader.nodes.as_ref().unwrap().rendered),
        ))
    });
    assert_eq!(text, "## One new");
    assert!(focused, "the rendered view holds the focus");
    settle(&mut harness, &manual);
    let edits = reader(&mut harness, |reader, _| Ok(reader.take_edits()));
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].text, "## One new");
    assert!(
        reader(&mut harness, |reader, _| Ok(reader.accepted_source())).starts_with("## One new\n")
    );
    assert_eq!(
        marked(&mut harness).unwrap().0,
        0,
        "j waited for the layout"
    );
}
