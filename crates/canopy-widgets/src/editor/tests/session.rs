//! Insert sessions and text entry: their undo groups, and what `.` repeats.
//!
//! The repeat table holds the text that Neovim gives for the same keys. A
//! case moves the cursor with a motion before `.` where the editor's cursor
//! after Esc differs from Vim's, which steps back one character.

use proptest::{
    prelude::*,
    test_runner::{Config, RngSeed},
};

use super::{
    super::search::Source,
    keys::{Step, drive, in_root, run_step, step, tokens},
    *,
};

/// Return a vi editor of `text` with its cursor at `line:column`.
fn vi_at(text: &str, line: usize, column: usize) -> Editor {
    let mut editor = Editor::new(text, EditorConfig::new().with_mode(EditMode::Vi));
    editor.buffer.set_cursor(TextPosition::new(line, column));
    editor.update_preferred_column();
    editor
}

/// Return a vi editor of `text` with its cursor at the start.
fn vi(text: &str) -> Editor {
    vi_at(text, 0, 0)
}

#[test]
fn an_undo_with_nothing_to_undo_keeps_the_prompt() {
    // Regression: an undo that undid nothing still closed the prompt.
    let mut editor = vi("abc");
    drive(&mut editor, "/b<Undo><Redo>");
    assert!(editor.prompt.is_some());
    assert_eq!(editor.text(), "abc");
}

#[test]
fn an_insert_move_ends_the_undo_step() {
    // Regression: the move ended the step that `.` repeats, but one undo
    // took all the typing of the insert, unlike Vim.
    let mut editor = vi("");
    drive(&mut editor, "Afoo<Left>bar<Esc>u");
    assert_eq!(editor.text(), "foo");
}

#[test]
fn a_config_that_keeps_the_mode_keeps_the_prompt() {
    // Regression: a host that set the configuration for an unrelated change
    // closed the prompt.
    let mut editor = vi("abc");
    drive(&mut editor, "/b");
    editor.set_config(editor.config().clone().with_tab_stop(8));
    assert!(editor.prompt.is_some(), "the same mode keeps the prompt");
    editor.set_config(editor.config().clone().with_mode(EditMode::Text));
    assert!(editor.prompt.is_none(), "a change of mode closes it");
}

#[test]
fn enter_on_an_empty_search_prompt_clears_the_search() {
    // Regression: the empty prompt became a query of no text.
    let mut editor = vi("abc");
    drive(&mut editor, "/b<CR>");
    assert!(matches!(editor.search.source(), Source::Query { .. }));
    drive(&mut editor, "/<CR>");
    assert_eq!(editor.search.source(), &Source::None);
}

#[test]
fn an_undo_in_insert_mode_after_backspaces_undoes_them() {
    // Regression: the undo popped the step of `abc` while the open group
    // held the backspaces, and replayed it on an empty rope, which panicked.
    let mut editor = vi("");
    drive(&mut editor, "iabc<Esc>A<BS><BS><BS><Undo>");
    assert_eq!(editor.text(), "abc");
    assert_eq!(editor.vi_mode(), Some(ViMode::Insert));
}

#[test]
fn an_undo_in_insert_mode_undoes_the_open_group() {
    // Regression: the undo removed the older `a` and kept the open `b`.
    let mut editor = vi("");
    drive(&mut editor, "ia<Esc>Ab<Undo>");
    assert_eq!(editor.text(), "a");
    assert_eq!(editor.vi_mode(), Some(ViMode::Insert));
}

#[test]
fn an_undo_of_the_first_open_group_restores_the_text() {
    // Regression: with no older step, the undo did nothing.
    let mut editor = vi("x");
    drive(&mut editor, "iab<Undo>");
    assert_eq!(editor.text(), "x");
    assert_eq!(editor.vi_mode(), Some(ViMode::Insert));
}

#[test]
fn typing_after_an_undo_in_insert_mode_opens_a_new_group() {
    let mut editor = vi("");
    drive(&mut editor, "iab<Undo>cd");
    assert_eq!(editor.text(), "cd");
    drive(&mut editor, "<Undo>");
    assert_eq!(editor.text(), "");
    drive(&mut editor, "<Redo>");
    assert_eq!(editor.text(), "cd");
    drive(&mut editor, "e<Esc>u");
    assert_eq!(
        editor.text(),
        "cd",
        "the typing after the redo is its own step"
    );
    drive(&mut editor, "u");
    assert_eq!(editor.text(), "");
}

#[test]
fn a_redo_straight_after_an_undo_in_insert_mode_restores_the_group() {
    let mut editor = vi("");
    drive(&mut editor, "iab<Undo><Redo>");
    assert_eq!(editor.text(), "ab");
    assert_eq!(editor.vi_mode(), Some(ViMode::Insert));
    drive(&mut editor, "c<Esc>");
    assert_eq!(editor.text(), "abc");
    drive(&mut editor, "u");
    assert_eq!(editor.text(), "ab");
    drive(&mut editor, "u");
    assert_eq!(editor.text(), "");
}

#[test]
fn a_repeat_after_an_undo_in_insert_mode_repeats_the_typing_after_it() {
    // Regression: the undo removed the older `foo` instead of the open `bar`.
    let mut editor = vi("");
    drive(&mut editor, "ifoo<Esc>abar<Undo>baz<Esc>");
    assert_eq!(editor.text(), "foobaz");
    drive(&mut editor, ".");
    assert_eq!(editor.text(), "foobazbaz");
    drive(&mut editor, "u.");
    assert_eq!(editor.text(), "foobazbaz", "a repeat after an undo");
}

#[test]
fn a_set_text_ends_the_undo_group_of_an_insert() {
    // Regression: the new buffer had no open group, so each later key was
    // its own step.
    let mut editor = vi("");
    drive(&mut editor, "i");
    editor.set_text("new");
    drive(&mut editor, "ab<Esc>");
    assert_eq!(editor.text(), "abnew");
    drive(&mut editor, "u");
    assert_eq!(editor.text(), "new");
}

#[test]
fn a_switch_from_text_to_vi_mode_ends_the_typing() {
    // Regression: the typing of text mode stayed open in normal mode, and
    // took the vi edits after it.
    let mut editor = Editor::new("", EditorConfig::new());
    drive(&mut editor, "ab");
    editor.set_config(EditorConfig::new().with_mode(EditMode::Vi));
    drive(&mut editor, "hx");
    assert_eq!(editor.text(), "a");
    drive(&mut editor, "u");
    assert_eq!(editor.text(), "ab");
}

/// Type `ab`, run `relocate`, type `cd`, and undo once: a relocation of the
/// cursor ends the typing, so only `cd` goes.
fn assert_relocation_ends_typing(relocate: impl FnOnce(&mut Harness)) {
    let mut harness = build_harness("", EditorConfig::new(), 20, 3);
    harness.type_text("ab").unwrap();
    relocate(&mut harness);
    harness.type_text("cd").unwrap();
    assert_eq!(editor_text(&mut harness), "cdab");
    edit_in_one_turn(&mut harness, |editor, ctx| editor.undo(ctx));
    assert_eq!(editor_text(&mut harness), "ab");
}

#[test]
fn a_click_ends_the_typing_of_text_mode() {
    // Regression: the typing, the click, and more typing undid as one step.
    assert_relocation_ends_typing(|harness| {
        harness
            .mouse(mouse_event(mouse::Action::Down, 0, 0))
            .unwrap();
        harness.mouse(mouse_event(mouse::Action::Up, 0, 0)).unwrap();
    });
}

#[test]
fn set_cursor_ends_the_typing_of_text_mode() {
    // Regression: as for a click.
    assert_relocation_ends_typing(|harness| {
        edit_in_one_turn(harness, |editor, ctx| {
            editor.set_cursor(ctx, TextPosition::new(0, 0));
        });
    });
}

#[test]
fn set_selection_ends_the_typing_of_text_mode() {
    // Regression: as for a click.
    assert_relocation_ends_typing(|harness| {
        with_editor(harness, |editor| {
            editor.set_selection(Selection::caret(TextPosition::new(0, 0)));
        });
    });
}

/// The repeat table: a text, the cursor at the start, the keys, and the
/// text that Neovim gives.
const REPEATS: &[(&str, (usize, usize), &str, &str)] = &[
    ("one two", (0, 0), "iX<Esc>w.", "Xone Xtwo"),
    ("one two", (0, 0), "aX<Esc>w.", "oXne tXwo"),
    ("one\ntwo", (0, 2), "IX<Esc>j.", "Xone\nXtwo"),
    ("one\nthree", (0, 0), "AX<Esc>j.", "oneX\nthreeX"),
    ("one\ntwo", (0, 0), "oX<Esc>j.", "one\nX\ntwo\nX"),
    ("one\ntwo", (1, 0), "OX<Esc>k.", "X\none\nX\ntwo"),
    ("one\ntwo", (0, 0), "ccX<Esc>j.", "X\nX"),
    (
        "one two\nthree four",
        (0, 4),
        "CX<Esc>j0w.",
        "one X\nthree X",
    ),
    ("a\nb\nc\nd\ne", (0, 0), "Vjd.", "e"),
    ("a\nb\nc\nd\ne\nf\ng\nh", (0, 0), "VjjdG.gg.", "g"),
    ("abcdef", (0, 0), "vld.", "ef"),
    ("abcdef\nab\nxyz", (0, 1), "vlldj.", "aef\naxyz"),
    ("abc\nxx\nabcdefgh", (0, 1), "v$dj.", "axx\na"),
    ("ab\ncd\nef\nghi\njklmn", (0, 0), "vj$dj0.", "ef\n"),
    ("abcdef ghijkl", (0, 0), "vlcX<Esc>w.", "Xcdef Xijkl"),
    ("one\ntwo\nthree", (0, 0), "VcX<Esc>j.", "X\nX\nthree"),
    ("abc\ndef\nghi\njkl\nmno", (0, 1), "vjdj.", "af\ngl\nmno"),
    (
        "abc\ndef\nghi\njkl\nmno",
        (0, 1),
        "vjcX<Esc>j0.",
        "aXf\nXl\nmno",
    ),
    ("abcdef", (0, 0), "ifoo<BS>x<Esc>0.", "foxfoxabcdef"),
    ("abcdef", (0, 0), "iX<Del>Y<Esc>0.", "XYYbcdef"),
    ("abc", (0, 0), "i<BS>X<Esc>0ll.", "XaXbc"),
    ("abc", (0, 2), "a<Del>X<Esc>0.", "aXcX"),
    ("ab", (0, 0), "iab<Left>X<Esc>0.", "XaXbab"),
    ("one\ntwo", (0, 0), "oX<Left><Esc>j.", "one\nX\ntwo\nX"),
    ("one two", (0, 0), "i<Esc>xw.", "ne wo"),
];

#[test]
fn a_repeat_replays_the_edit_that_started_the_insert() {
    // Regression for most rows: `.` replayed only the typed text at the
    // cursor, and a visual operator repeated as one char or a whole line.
    for &(text, (line, column), keys, expected) in REPEATS {
        let mut editor = vi_at(text, line, column);
        let (before, last) = keys.split_at(keys.len() - 1);
        drive(&mut editor, before);
        let prior = editor.text();
        drive(&mut editor, last);
        assert_eq!(editor.text(), expected, "{keys} on {text:?}");
        assert_eq!(editor.vi_mode(), Some(ViMode::Normal), "{keys} on {text:?}");
        drive(&mut editor, "u");
        assert_eq!(editor.text(), prior, "one undo reverts {keys} on {text:?}");
    }
}

#[test]
fn a_repeat_of_an_open_that_typed_nothing_opens_another_line_in_insert_mode() {
    let mut editor = vi("one\ntwo");
    drive(&mut editor, "o<Esc>.");
    assert_eq!(editor.text(), "one\n\n\ntwo");
    assert_eq!(editor.vi_mode(), Some(ViMode::Insert));
    assert_eq!(editor.cursor(), TextPosition::new(2, 0));
}

#[test]
fn a_repeat_records_the_typing_of_set_vi_mode() {
    let mut editor = vi("ab");
    editor.set_vi_mode(ViMode::Insert);
    drive(&mut editor, "X");
    editor.set_vi_mode(ViMode::Normal);
    drive(&mut editor, "l.");
    assert_eq!(editor.text(), "XaXb");
}

/// One key or host call of the undo property.
fn history_step() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("i"),
        Just("a"),
        Just("A"),
        Just("o"),
        Just("O"),
        Just("x"),
        Just("X"),
        Just("dd"),
        Just("cc"),
        Just("C"),
        Just("D"),
        Just("yy"),
        Just("p"),
        Just("v"),
        Just("V"),
        Just("d"),
        Just("c"),
        Just("l"),
        Just("j"),
        Just("."),
        Just("u"),
        Just("<C-r>"),
        Just("<Esc>"),
        Just("<BS>"),
        Just("<Del>"),
        Just("<CR>"),
        Just("<Left>"),
        Just("<Up>"),
        Just("<Undo>"),
        Just("<Redo>"),
    ]
}

/// The history that the editor's undo must follow: the texts before each
/// step, the texts that redo restores, and the text before the open group.
#[derive(Debug, Default)]
struct History {
    /// The text before each step, oldest first.
    undo: Vec<String>,
    /// The text after each undone step, latest last.
    redo: Vec<String>,
    /// The text before the first change of an open insert group.
    open: Option<String>,
}

impl History {
    /// Close the open group, as a step.
    fn close(&mut self) {
        if let Some(text) = self.open.take() {
            self.undo.push(text);
            self.redo.clear();
        }
    }

    /// Follow one run of `step` on `editor`, and check an undo or a redo
    /// against the model.
    fn follow(&mut self, editor: &mut Editor, step: Step, ctx: &mut dyn Context) -> Result<()> {
        let (text, revision, cursor) = (editor.text(), editor.buffer.revision(), editor.cursor());
        let inserting = editor.vi_mode() == Some(ViMode::Insert);
        let undoes = match step {
            Step::Undo => Some(true),
            Step::Redo => Some(false),
            Step::Key(key) if !inserting && editor.visual_mode().is_none() => match key.key {
                key::KeyCode::Char('u') if !key.mods.ctrl => Some(true),
                key::KeyCode::Char('r') if key.mods.ctrl => Some(false),
                _ => None,
            },
            Step::Key(_) => None,
        };
        run_step(editor, step, ctx)?;
        let after = editor.text();
        if let Some(undo) = undoes {
            self.close();
            let (from, to) = if undo {
                (&mut self.undo, &mut self.redo)
            } else {
                (&mut self.redo, &mut self.undo)
            };
            let expected = from.pop();
            if expected.is_some() {
                to.push(text.clone());
            }
            assert_eq!(after, expected.unwrap_or(text), "{step:?}");
            return Ok(());
        }
        let changed = editor.buffer.revision() != revision;
        let now_inserting = editor.vi_mode() == Some(ViMode::Insert);
        // A move in insert mode closes the open group, as in Vim.
        let arrow = matches!(step, Step::Key(key) if matches!(
            key.key,
            key::KeyCode::Left | key::KeyCode::Right | key::KeyCode::Up | key::KeyCode::Down
        ));
        if inserting && arrow && editor.cursor() != cursor {
            self.close();
        }
        if inserting || now_inserting {
            if changed && self.open.is_none() {
                self.open = Some(text);
            }
            if !now_inserting {
                self.close();
            }
        } else if changed {
            self.undo.push(text);
            self.redo.clear();
        }
        Ok(())
    }
}

/// The routine configuration of the property: a fixed seed.
fn config() -> Config {
    Config {
        cases: 128,
        rng_seed: RngSeed::Fixed(0x0005_eed0_0add),
        ..Config::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn undo_and_redo_follow_a_model_of_the_history(
        text in prop_oneof![Just(""), Just("ab"), Just("one\ntwo\nthree")],
        steps in prop::collection::vec(history_step(), 0..32),
    ) {
        let mut editor = vi(text);
        let mut history = History::default();
        in_root(|ctx| {
                    for token in tokens(&steps.concat()) {
                        history.follow(&mut editor, step(&token), ctx)?;
                    }
                    // Undo back to the start restores the text.
                    history.close();
                    for _ in 0..=history.undo.len() {
                        editor.undo(ctx);
                    }
                    assert_eq!(editor.text(), text, "{steps:?}");
                    Ok(())
        });
    }
}
