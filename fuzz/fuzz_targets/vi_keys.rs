//! Arbitrary text and keys into a vi `Editor`.
//!
//! After each key, the cursor and the selection must lie in the text, on
//! char boundaries, and in visual mode on grapheme boundaries. The editor
//! must predict how it takes each key, and a read-only editor must keep its
//! text. At the end, undo back to the start must restore the text.
//!
//! An input is a flags byte, the text, a zero byte, and the keys, so seeds
//! are easy to write by hand. The text and the keys decode as lossy UTF-8.
//! Each char of the keys is one key, and control chars name the keys that
//! are not chars: see [`key_for`]. The keys leave out `R`, whose prompt can
//! replace every match in one key, so that each key adds at most one undo
//! step.

#![no_main]

use std::cell::RefCell;

use canopy::{
    Canopy, CanopyBuilder, Widget,
    input::{Event, key},
};
use canopy_widgets::editor::{EditMode, Editor, EditorConfig, Interaction, TextPosition, ViMode};
use libfuzzer_sys::fuzz_target;
use unicode_segmentation::UnicodeSegmentation;

/// The most keys of one input.
const MAX_KEYS: usize = 256;

/// The longest text of one input, in chars.
const MAX_TEXT: usize = 512;

/// The text, in bytes, past which a run takes no more keys. Yanks and puts
/// can grow a text without bound, and each key costs time in its length.
const MAX_GROWN_TEXT: usize = 1 << 16;

thread_local! {
    /// One application gives the context for every run.
    static APP: RefCell<Canopy> = RefCell::new(
        CanopyBuilder::new().build().expect("an empty application builds"),
    );
}

/// Return the key that `c` names: a control char names a key that is not a
/// char, and any other char names itself.
fn key_for(c: char) -> key::Key {
    match c {
        '\u{1b}' => key::KeyCode::Esc.into(),
        '\n' | '\r' => key::KeyCode::Enter.into(),
        '\u{8}' => key::KeyCode::Backspace.into(),
        '\u{7f}' => key::KeyCode::Delete.into(),
        '\u{1}' => key::KeyCode::Left.into(),
        '\u{2}' => key::KeyCode::Right.into(),
        '\u{3}' => key::KeyCode::Up.into(),
        '\u{4}' => key::KeyCode::Down.into(),
        '\u{5}' => key::KeyCode::Home.into(),
        '\u{6}' => key::KeyCode::End.into(),
        '\t' => key::KeyCode::Tab.into(),
        '\u{12}' => key::Ctrl + 'r',
        c => c.into(),
    }
}

/// Return the lines of `text` without their breaks: the editor breaks
/// lines at LF and CRLF alone.
fn lines(text: &str) -> Vec<String> {
    let parts: Vec<&str> = text.split('\n').collect();
    let last = parts.len() - 1;
    parts
        .iter()
        .enumerate()
        .map(|(index, line)| match line.strip_suffix('\r') {
            Some(before) if index < last => before.to_string(),
            _ => (*line).to_string(),
        })
        .collect()
}

/// Check that `position` lies in `lines`, and return the text of its line.
fn line_of<'a>(lines: &'a [String], position: TextPosition, what: &str) -> &'a str {
    let line = lines
        .get(position.line)
        .unwrap_or_else(|| panic!("{what} {position:?} is past the last line"));
    assert!(
        position.column <= line.chars().count(),
        "{what} {position:?} is past the end of its line {line:?}"
    );
    line
}

/// Return whether `column` starts a grapheme of `line`, or ends the line.
fn on_grapheme(line: &str, column: usize) -> bool {
    let mut start = 0;
    for grapheme in line.graphemes(true) {
        if start == column {
            return true;
        }
        start += grapheme.chars().count();
    }
    start == column
}

/// Check the cursor and the selection of `editor`.
fn check(editor: &Editor) {
    let lines = lines(&editor.text());
    line_of(&lines, editor.cursor(), "the cursor");
    let range = editor.selection().range();
    let start = line_of(&lines, range.start, "the selection start");
    let end = line_of(&lines, range.end, "the selection end");
    assert!(range.start <= range.end, "the range {range:?} is ordered");
    if let Some(ViMode::Visual(_)) = editor.vi_mode() {
        assert!(
            on_grapheme(start, range.start.column),
            "the selection starts inside a grapheme: {range:?} in {start:?}"
        );
        assert!(
            on_grapheme(end, range.end.column),
            "the selection ends inside a grapheme: {range:?} in {end:?}"
        );
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&flags, rest)) = data.split_first() else {
        return;
    };
    let split = rest
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(rest.len());
    let text: String = String::from_utf8_lossy(&rest[..split])
        .chars()
        .take(MAX_TEXT)
        .collect();
    let keys: Vec<key::Key> = String::from_utf8_lossy(rest.get(split + 1..).unwrap_or_default())
        .chars()
        .filter(|&c| c != 'R')
        .take(MAX_KEYS)
        .map(key_for)
        .collect();
    let read_only = flags & 2 != 0;
    let config = EditorConfig::new()
        .with_mode(EditMode::Vi)
        .with_multiline(flags & 1 == 0)
        .with_history_arrows(flags & 4 != 0)
        .with_interaction(if read_only {
            Interaction::View
        } else {
            Interaction::Edit
        });
    let mut editor = Editor::with_config(text.clone(), config);
    APP.with(|app| {
        app.borrow_mut()
            .with_root_context(|ctx| {
                check(&editor);
                let mut taken = 0;
                for &key in &keys {
                    if editor.text().len() > MAX_GROWN_TEXT {
                        break;
                    }
                    taken += 1;
                    let predicted = editor.key_outcome(key, ctx);
                    let outcome = editor.on_event(&Event::Key(key), ctx)?;
                    assert_eq!(
                        predicted, outcome,
                        "the editor predicts how it takes {key:?}"
                    );
                    check(&editor);
                    if read_only {
                        assert_eq!(editor.text(), text, "a read-only editor keeps its text");
                    }
                }
                editor.set_vi_mode(ViMode::Normal);
                check(&editor);
                // Each key adds at most one undo step.
                for _ in 0..=taken {
                    editor.undo(ctx);
                }
                assert_eq!(editor.text(), text, "undo restores the text");
                check(&editor);
                Ok(())
            })
            .expect("the editor runs");
    });
});
