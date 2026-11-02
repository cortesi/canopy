//! Vi visual mode: an inclusive selection, motions, search, text objects,
//! mode switches, and operators, as in Vim.
//!
//! `tests/vim_diff.rs` compares many more cases with Neovim. These examples
//! and properties run in the routine loop.

use std::cell::RefCell;

use canopy::{Canopy, text};
use proptest::{
    prelude::*,
    test_runner::{Config, RngSeed},
};

use super::*;
use crate::{editor::VisualMode, text_buffer::TextBuffer};

/// Return the key of one token: a char, or the name of a key in angle
/// brackets.
fn token_key(token: &str) -> key::Key {
    match token {
        "<Esc>" => key::KeyCode::Esc.into(),
        "<CR>" => key::KeyCode::Enter.into(),
        "<Left>" => key::KeyCode::Left.into(),
        "<Right>" => key::KeyCode::Right.into(),
        "<Up>" => key::KeyCode::Up.into(),
        "<Down>" => key::KeyCode::Down.into(),
        _ => {
            let mut chars = token.chars();
            let key = chars.next().expect("a token").into();
            assert!(chars.next().is_none(), "unknown token {token}");
            key
        }
    }
}

/// Split `keys` into tokens: each char, or a name in angle brackets.
fn tokens(keys: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = keys.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' && chars.peek().is_some_and(char::is_ascii_uppercase) {
            let mut name = String::from("<");
            for next in chars.by_ref() {
                name.push(next);
                if next == '>' {
                    break;
                }
            }
            out.push(name);
        } else {
            out.push(c.to_string());
        }
    }
    out
}

thread_local! {
    /// One application gives the context of every run on a thread, since
    /// building one costs more than the keys.
    static APP: RefCell<Canopy> =
        RefCell::new(CanopyBuilder::new().build().expect("an empty application builds"));
}

/// Run `keys` on a vi editor of `text` with its cursor at `line:column`,
/// with no render, and return the editor.
fn run_with(config: EditorConfig, text: &str, start: (usize, usize), keys: &str) -> Editor {
    APP.with(|app| run_in(&mut app.borrow_mut(), config, text, start, keys))
}

/// Run `keys` as [`run_with`] does, in the context of `app`.
fn run_in(
    app: &mut Canopy,
    config: EditorConfig,
    text: &str,
    start: (usize, usize),
    keys: &str,
) -> Editor {
    let mut editor = Editor::with_config(text, config);
    app.with_root_context(|ctx| {
        editor.set_cursor(ctx, TextPosition::new(start.0, start.1));
        for token in tokens(keys) {
            editor.on_event(&Event::Key(token_key(&token)), ctx)?;
        }
        Ok(())
    })
    .expect("the keys run");
    editor
}

/// Run `keys` on a vi editor of `text` with its cursor at `start`.
fn run(text: &str, start: (usize, usize), keys: &str) -> Editor {
    run_with(
        EditorConfig::new().with_mode(EditMode::Vi),
        text,
        start,
        keys,
    )
}

/// Return the range from `start` to `end`, each a `(line, column)` pair.
fn range(start: (usize, usize), end: (usize, usize)) -> TextRange {
    TextRange::new(
        TextPosition::new(start.0, start.1),
        TextPosition::new(end.0, end.1),
    )
}

/// Return the selected range and the cursor of `editor`.
fn state(editor: &Editor) -> (TextRange, TextPosition) {
    (editor.selection().range(), editor.cursor())
}

/// Assert that `keys` from `start` select `selected`, with the cursor at
/// `cursor`, in visual mode of kind `mode`.
fn assert_selects(
    text: &str,
    start: (usize, usize),
    keys: &str,
    mode: VisualMode,
    selected: ((usize, usize), (usize, usize)),
    cursor: (usize, usize),
) {
    let editor = run(text, start, keys);
    assert_eq!(
        editor.vi_mode(),
        Some(ViMode::Visual(mode)),
        "{keys} on {text:?}"
    );
    assert_eq!(
        state(&editor),
        (
            range(selected.0, selected.1),
            TextPosition::new(cursor.0, cursor.1)
        ),
        "{keys} on {text:?} from {start:?}"
    );
}

#[test]
fn v_alone_selects_the_character_at_the_cursor() {
    let chars = VisualMode::Character;
    assert_selects("abc", (0, 1), "v", chars, ((0, 1), (0, 2)), (0, 1));
    assert_selects("a世b", (0, 1), "v", chars, ((0, 1), (0, 2)), (0, 1));
    assert_selects("e\u{301}x", (0, 0), "v", chars, ((0, 0), (0, 2)), (0, 0));
    // An empty line selects its line break, and the end of the text holds
    // nothing.
    assert_selects("a\n\nb", (1, 0), "v", chars, ((1, 0), (2, 0)), (1, 0));
    assert_selects("a\n", (1, 0), "v", chars, ((1, 0), (1, 0)), (1, 0));
    assert_selects("", (0, 0), "v", chars, ((0, 0), (0, 0)), (0, 0));
    // A cursor on the end of a line starts on its last character.
    assert_selects("abc", (0, 3), "v", chars, ((0, 2), (0, 3)), (0, 2));
}

#[test]
fn a_selection_holds_both_ends_in_either_direction() {
    let chars = VisualMode::Character;
    assert_selects("abcd", (0, 1), "vl", chars, ((0, 1), (0, 3)), (0, 2));
    assert_selects("abcd", (0, 2), "vh", chars, ((0, 1), (0, 3)), (0, 1));
    assert_selects("ab\ncd", (1, 1), "vk", chars, ((0, 1), (1, 2)), (0, 1));
    let editor = run("abcd", (0, 2), "vh");
    let selection = editor.selection();
    assert_eq!(
        selection.head(),
        TextPosition::new(0, 1),
        "the head is at the cursor"
    );
    assert_eq!(selection.anchor(), TextPosition::new(0, 3));
}

#[test]
fn a_line_selection_holds_whole_lines_without_the_last_break() {
    let lines = VisualMode::Line;
    let text = "one\ntwo\nthree";
    assert_selects(text, (0, 1), "V", lines, ((0, 0), (0, 3)), (0, 1));
    assert_selects(text, (0, 1), "Vj", lines, ((0, 0), (1, 3)), (1, 1));
    assert_selects(text, (2, 1), "Vk", lines, ((1, 0), (2, 5)), (1, 1));
    assert_selects("a\n\nb", (1, 0), "V", lines, ((1, 0), (1, 0)), (1, 0));
}

#[test]
fn horizontal_motions_stay_in_the_line_and_reach_its_end() {
    let chars = VisualMode::Character;
    assert_selects("ab\ncd", (1, 0), "vh", chars, ((1, 0), (1, 1)), (1, 0));
    // `l` reaches the line break, and holds it, but goes no further.
    assert_selects("ab\ncd", (0, 1), "vl", chars, ((0, 1), (1, 0)), (0, 2));
    assert_selects("ab\ncd", (0, 1), "vll", chars, ((0, 1), (1, 0)), (0, 2));
    assert_selects("ab\ncd", (1, 1), "vl", chars, ((1, 1), (1, 2)), (1, 2));
    assert_selects(
        "ab\ncd",
        (0, 1),
        "v<Right><Left>",
        chars,
        ((0, 1), (0, 2)),
        (0, 1),
    );
    assert_selects("a世界b", (0, 0), "vll", chars, ((0, 0), (0, 3)), (0, 2));
    assert_selects(
        "a👩\u{200d}💻b",
        (0, 0),
        "vll",
        chars,
        ((0, 0), (0, 5)),
        (0, 4),
    );
}

#[test]
fn vertical_motions_keep_their_column() {
    let chars = VisualMode::Character;
    assert_selects("abcdef\nde", (0, 5), "vj", chars, ((0, 5), (1, 2)), (1, 2));
    assert_selects("abcdef\nde", (0, 5), "vjk", chars, ((0, 5), (0, 6)), (0, 5));
    // At the last line, `j` does nothing.
    assert_selects("abcdef\nde", (1, 1), "vj", chars, ((1, 1), (1, 2)), (1, 1));
    assert_selects(
        "abcdef\nde",
        (1, 1),
        "v<Up>",
        chars,
        ((0, 1), (1, 2)),
        (0, 1),
    );
    // After `$`, vertical moves keep to the ends of lines.
    assert_selects(
        "abc\nde\nfghi",
        (0, 1),
        "v$j",
        chars,
        ((0, 1), (2, 0)),
        (1, 2),
    );
    assert_selects(
        "abc\nde\nfghi",
        (0, 1),
        "v$jj",
        chars,
        ((0, 1), (2, 4)),
        (2, 4),
    );
}

#[test]
fn line_motions_move_the_free_end() {
    let chars = VisualMode::Character;
    let text = "  abc def\nx";
    assert_selects(text, (0, 5), "v0", chars, ((0, 0), (0, 6)), (0, 0));
    assert_selects(text, (0, 5), "v^", chars, ((0, 2), (0, 6)), (0, 2));
    assert_selects(text, (0, 5), "v$", chars, ((0, 5), (1, 0)), (0, 9));
    // `^` on a line of blanks goes to its last blank.
    assert_selects("   \nx", (0, 0), "v^", chars, ((0, 0), (0, 3)), (0, 2));
    assert_selects(
        "ab\n  cd\nef",
        (2, 1),
        "vgg",
        chars,
        ((0, 1), (2, 2)),
        (0, 1),
    );
    assert_selects(
        "ab\n  cd\nef",
        (0, 1),
        "vG",
        chars,
        ((0, 1), (2, 2)),
        (2, 1),
    );
}

#[test]
fn word_motions_split_words_punctuation_and_scripts() {
    let chars = VisualMode::Character;
    let text = "foo.bar baz";
    assert_selects(text, (0, 0), "vw", chars, ((0, 0), (0, 4)), (0, 3));
    assert_selects(text, (0, 0), "vww", chars, ((0, 0), (0, 5)), (0, 4));
    assert_selects(text, (0, 0), "vwww", chars, ((0, 0), (0, 9)), (0, 8));
    assert_selects(text, (0, 0), "ve", chars, ((0, 0), (0, 3)), (0, 2));
    assert_selects(text, (0, 9), "vb", chars, ((0, 8), (0, 10)), (0, 8));
    assert_selects(text, (0, 9), "vbb", chars, ((0, 4), (0, 10)), (0, 4));
    // Kanji and Latin letters are words of different classes.
    assert_selects("a世界b c", (0, 0), "vw", chars, ((0, 0), (0, 2)), (0, 1));
    assert_selects("a世界b c", (0, 0), "vww", chars, ((0, 0), (0, 4)), (0, 3));
    // An empty line is a word, and the end of the text stops `w` and `e`.
    assert_selects("foo\n\nbar", (0, 0), "vw", chars, ((0, 0), (2, 0)), (1, 0));
    assert_selects("foo bar", (0, 4), "vw", chars, ((0, 4), (0, 7)), (0, 7));
    assert_selects("a b", (0, 2), "ve", chars, ((0, 2), (0, 3)), (0, 3));
}

#[test]
fn search_moves_the_free_end_to_a_match() {
    let chars = VisualMode::Character;
    let text = "foo bar foo baz foo";
    assert_selects(text, (0, 0), "v/foo<CR>", chars, ((0, 0), (0, 9)), (0, 8));
    assert_selects(
        text,
        (0, 0),
        "v/foo<CR>n",
        chars,
        ((0, 0), (0, 17)),
        (0, 16),
    );
    assert_selects(text, (0, 0), "v/foo<CR>nn", chars, ((0, 0), (0, 1)), (0, 0));
    assert_selects(text, (0, 0), "v?foo<CR>", chars, ((0, 0), (0, 17)), (0, 16));
    assert_selects(text, (0, 9), "v?foo<CR>", chars, ((0, 8), (0, 10)), (0, 8));
    assert_selects(
        text,
        (0, 9),
        "v?foo<CR>N",
        chars,
        ((0, 9), (0, 17)),
        (0, 16),
    );
    // No match, or no search yet, leaves the selection.
    assert_selects(text, (0, 9), "v/zz<CR>l", chars, ((0, 9), (0, 11)), (0, 10));
    assert_selects(text, (0, 9), "vn", chars, ((0, 9), (0, 10)), (0, 9));
    // A search that finds nothing still takes the cursor's column for
    // vertical moves, as in Vim.
    let editor = run("abcdef\nab\nabcdef", (0, 5), "vjnj");
    assert_eq!(editor.cursor(), TextPosition::new(2, 2));
    let editor = run("abcdef\nab\nabcdef", (0, 5), "vj/zz<CR>j");
    assert_eq!(editor.cursor(), TextPosition::new(2, 2));
    // Esc in the prompt keeps visual mode.
    assert_selects(text, (0, 9), "v/f<Esc>l", chars, ((0, 9), (0, 11)), (0, 10));
}

#[test]
fn normal_mode_search_starts_from_the_cursor() {
    let editor = run("foo bar foo baz foo", (0, 9), "/foo<CR>");
    assert_eq!(editor.cursor(), TextPosition::new(0, 16));
    let editor = run("foo bar foo baz foo", (0, 9), "/foo<CR>n");
    assert_eq!(editor.cursor(), TextPosition::new(0, 0));
    let editor = run("foo bar foo baz foo", (0, 9), "/foo<CR>N");
    assert_eq!(editor.cursor(), TextPosition::new(0, 8));
}

#[test]
fn o_moves_the_cursor_to_the_other_end() {
    let chars = VisualMode::Character;
    assert_selects("ab\ncd", (1, 1), "vko", chars, ((0, 1), (1, 2)), (1, 1));
    assert_selects("ab\ncd", (1, 1), "vkoo", chars, ((0, 1), (1, 2)), (0, 1));
    assert_selects("ab\ncd", (1, 1), "vkO", chars, ((0, 1), (1, 2)), (1, 1));
    assert_selects("ab\ncd", (1, 1), "vkoh", chars, ((0, 1), (1, 1)), (1, 0));
}

#[test]
fn word_objects_take_words_or_blanks() {
    let chars = VisualMode::Character;
    let text = "this is a test, of words.  foo";
    assert_selects(text, (0, 1), "viw", chars, ((0, 0), (0, 4)), (0, 3));
    assert_selects(text, (0, 1), "vaw", chars, ((0, 0), (0, 5)), (0, 4));
    // On blanks, `iw` takes the blanks and `aw` the next word too.
    assert_selects(text, (0, 25), "viw", chars, ((0, 25), (0, 27)), (0, 26));
    assert_selects(text, (0, 25), "vaw", chars, ((0, 25), (0, 30)), (0, 29));
    // Punctuation is a word of its own.
    assert_selects(text, (0, 14), "viw", chars, ((0, 14), (0, 15)), (0, 14));
    // With no blanks after it, `aw` takes the blanks before the word.
    assert_selects(text, (0, 28), "vaw", chars, ((0, 25), (0, 30)), (0, 29));
    // Again, the selection grows by the next word or run of blanks.
    assert_selects(text, (0, 1), "viwiw", chars, ((0, 0), (0, 5)), (0, 4));
    assert_selects(text, (0, 1), "vawaw", chars, ((0, 0), (0, 8)), (0, 7));
    // A line selection becomes a character selection.
    assert_selects(text, (0, 6), "Viw", chars, ((0, 5), (0, 7)), (0, 6));
}

#[test]
fn word_motions_cross_line_ends_indents_and_empty_lines() {
    // The outcomes are Neovim's.
    let chars = VisualMode::Character;
    assert_selects("abc\n  def", (0, 1), "v$w", chars, ((0, 1), (1, 3)), (1, 2));
    assert_selects("foo\n  bar", (0, 0), "vw", chars, ((0, 0), (1, 3)), (1, 2));
    assert_selects("foo\n\nbar", (0, 2), "ve", chars, ((0, 2), (2, 3)), (2, 2));
    assert_selects("abc\n  def", (1, 3), "v0b", chars, ((0, 0), (1, 4)), (0, 0));
    // Word objects grow backward from a cursor before the anchor.
    assert_selects(
        "ab cd ef",
        (0, 7),
        "vhawaw",
        chars,
        ((0, 0), (0, 8)),
        (0, 0),
    );
    assert_selects(
        "ab cd ef",
        (0, 7),
        "vhiwiwiw",
        chars,
        ((0, 2), (0, 8)),
        (0, 2),
    );
}

#[test]
fn words_and_sentences_cross_into_the_last_lines() {
    // The outcomes are Neovim's.
    let chars = VisualMode::Character;
    assert_selects(
        "foo bar\nbaz",
        (0, 6),
        "vw",
        chars,
        ((0, 6), (1, 1)),
        (1, 0),
    );
    assert_selects("a\nfoo\nbar", (1, 2), "vw", chars, ((1, 2), (2, 1)), (2, 0));
    assert_selects("abc.\n", (1, 0), "vis", chars, ((0, 3), (1, 0)), (0, 3));
    assert_selects("abc.\n", (1, 0), "vas", chars, ((0, 3), (1, 0)), (0, 3));
    assert_selects("abc. d\n\n", (2, 0), "vis", chars, ((1, 0), (2, 0)), (1, 0));
    assert_selects("abc. d\n\n", (1, 0), "vis", chars, ((0, 4), (2, 0)), (0, 4));
    assert_selects(
        "abc. d\n\n",
        (0, 5),
        "visis",
        chars,
        ((0, 5), (0, 6)),
        (0, 5),
    );
    assert_selects("x\n\nabc", (1, 0), "vas", chars, ((1, 0), (2, 3)), (2, 2));
}

#[test]
fn a_form_feed_at_a_line_start_starts_a_paragraph() {
    // The outcomes are Neovim's.
    let lines = VisualMode::Line;
    let chars = VisualMode::Character;
    let text = "abc\n\u{c}def\nghi";
    assert_selects(text, (0, 1), "vip", lines, ((0, 0), (0, 3)), (0, 0));
    assert_selects(text, (2, 1), "vip", lines, ((1, 0), (2, 3)), (2, 0));
    assert_selects(text, (2, 1), "vap", lines, ((1, 0), (2, 3)), (2, 0));
    let text = "one. two\n\u{c}three four";
    assert_selects(text, (1, 3), "vis", chars, ((0, 7), (1, 1)), (0, 7));
    let text = "one two\n\u{c}three four";
    assert_selects(text, (0, 5), "vas", chars, ((0, 0), (0, 7)), (0, 6));
}

#[test]
fn sentence_objects_follow_sentence_ends() {
    let chars = VisualMode::Character;
    let text = "Hello there.  This is a test.  Another one!";
    assert_selects(text, (0, 16), "vis", chars, ((0, 14), (0, 29)), (0, 28));
    assert_selects(text, (0, 16), "vas", chars, ((0, 14), (0, 31)), (0, 30));
    assert_selects(text, (0, 2), "vis", chars, ((0, 0), (0, 12)), (0, 11));
    // On the blanks between sentences, `is` takes the blanks.
    assert_selects(text, (0, 12), "vis", chars, ((0, 12), (0, 14)), (0, 13));
    // Again, the selection grows by the next sentence.
    assert_selects(text, (0, 16), "visis", chars, ((0, 14), (0, 31)), (0, 30));
    // Closing quotes and brackets end a sentence with its period.
    let quoted = "The (end.)  A \"quote.\"  Last";
    assert_selects(quoted, (0, 1), "vis", chars, ((0, 0), (0, 10)), (0, 9));
    assert_selects(quoted, (0, 13), "vis", chars, ((0, 12), (0, 22)), (0, 21));
}

#[test]
fn paragraph_objects_take_lines_between_blank_lines() {
    let lines = VisualMode::Line;
    let text = "first para\nstill first\n\n\nsecond para\n  \nthird one";
    assert_selects(text, (1, 2), "vip", lines, ((0, 0), (1, 11)), (1, 0));
    assert_selects(text, (1, 2), "vap", lines, ((0, 0), (3, 0)), (3, 0));
    // Blank lines, including a line of blanks, form a paragraph of their own.
    assert_selects(text, (2, 0), "vip", lines, ((2, 0), (3, 0)), (3, 0));
    assert_selects(text, (5, 0), "vip", lines, ((5, 0), (5, 2)), (5, 0));
    // With no blank lines after it, `ap` takes the blank lines before.
    assert_selects(text, (6, 2), "vap", lines, ((5, 0), (6, 9)), (6, 0));
    // Again, the selection grows by the next paragraph.
    assert_selects(text, (0, 2), "vipip", lines, ((0, 0), (3, 0)), (3, 0));
    assert_selects(text, (0, 2), "Vipip", lines, ((0, 0), (3, 0)), (3, 0));
}

#[test]
fn v_and_shift_v_switch_and_leave() {
    let text = "one\ntwo";
    assert_selects(
        text,
        (0, 1),
        "vV",
        VisualMode::Line,
        ((0, 0), (0, 3)),
        (0, 1),
    );
    assert_selects(
        text,
        (0, 1),
        "vlVv",
        VisualMode::Character,
        ((0, 1), (0, 3)),
        (0, 2),
    );
    for keys in ["vv", "VV", "vVV", "v<Esc>"] {
        let editor = run(text, (0, 1), keys);
        assert_eq!(editor.vi_mode(), Some(ViMode::Normal), "{keys}");
        assert!(editor.selection().is_empty(), "{keys}");
    }
    // Leaving puts a cursor on the end of a line on its last character.
    let editor = run(text, (0, 1), "v$<Esc>");
    assert_eq!(editor.cursor(), TextPosition::new(0, 2));
}

#[test]
fn a_pending_prefix_waits_for_its_key() {
    let chars = VisualMode::Character;
    assert_selects(
        "abc def",
        (0, 1),
        "vi<Esc>l",
        chars,
        ((0, 1), (0, 3)),
        (0, 2),
    );
    assert_selects(
        "abc def",
        (0, 1),
        "vg<Esc>l",
        chars,
        ((0, 1), (0, 3)),
        (0, 2),
    );
    // A key that completes nothing drops the prefix, and edits nothing.
    let editor = run("abc def", (0, 1), "vixl");
    assert_eq!(editor.text(), "abc def");
    assert_eq!(
        state(&editor),
        (range((0, 1), (0, 3)), TextPosition::new(0, 2))
    );
}

#[test]
fn delete_removes_the_selection_and_yanks_it() {
    let editor = run("abc def", (0, 1), "vld");
    assert_eq!(editor.text(), "a def");
    assert_eq!(editor.yank, "bc");
    assert_eq!(editor.cursor(), TextPosition::new(0, 1));
    assert_eq!(editor.vi_mode(), Some(ViMode::Normal));
    // At the end of the line, the cursor moves onto the last character.
    let editor = run("abc def", (0, 5), "vlx");
    assert_eq!(editor.text(), "abc d");
    assert_eq!(editor.cursor(), TextPosition::new(0, 4));
    // A selected line break joins the lines.
    let editor = run("abc\ndef", (0, 1), "v$d");
    assert_eq!(editor.text(), "adef");
    let editor = run("abc\n\nx", (1, 0), "vd");
    assert_eq!(editor.text(), "abc\nx");
}

#[test]
fn line_delete_removes_whole_lines_and_keeps_the_column() {
    let editor = run("  one\n  two\nthree", (0, 3), "Vjd");
    assert_eq!(editor.text(), "three");
    assert_eq!(editor.yank, "  one\n  two\n");
    assert!(editor.yank_linewise);
    assert_eq!(editor.cursor(), TextPosition::new(0, 3));
    // The last lines go with the line break before them.
    let editor = run("  one\n  two\n  three", (1, 3), "Vjd");
    assert_eq!(editor.text(), "  one");
    assert_eq!(editor.cursor(), TextPosition::new(0, 3));
    let editor = run("one\ntwo", (0, 1), "VGd");
    assert_eq!(editor.text(), "");
}

#[test]
fn yank_leaves_the_cursor_at_the_start() {
    let editor = run("abc def", (0, 5), "vby");
    assert_eq!(editor.yank, "de");
    assert_eq!(editor.cursor(), TextPosition::new(0, 4));
    assert_eq!(editor.vi_mode(), Some(ViMode::Normal));
    let editor = run("abc\ndef", (0, 2), "vly");
    assert_eq!(editor.yank, "c\n");
    // A line yank moves the cursor to the start of the first line, unless
    // the cursor is above the anchor.
    let editor = run("  one\n  two\nthree", (0, 3), "Vjy");
    assert_eq!(editor.yank, "  one\n  two\n");
    assert_eq!(editor.cursor(), TextPosition::new(0, 0));
    let editor = run("  one\n  two\nthree", (1, 3), "Vky");
    assert_eq!(editor.cursor(), TextPosition::new(0, 3));
}

#[test]
fn change_replaces_the_selection_in_one_undo_step() {
    let editor = run("abc def", (0, 1), "vecX<Esc>");
    assert_eq!(editor.text(), "aX def");
    assert_eq!(editor.yank, "bc");
    let editor = run("one\ntwo\nthree", (0, 1), "VjcX<Esc>");
    assert_eq!(editor.text(), "X\nthree");
    let editor = run("one\ntwo\nthree", (0, 1), "VjcX<Esc>u");
    assert_eq!(editor.text(), "one\ntwo\nthree");
}

#[test]
fn indent_shifts_lines_and_skips_empty_ones() {
    let editor = run("abc def\n\n  cd", (0, 3), "vjj>");
    assert_eq!(editor.text(), "    abc def\n\n      cd");
    assert_eq!(editor.vi_mode(), Some(ViMode::Normal));
    assert_eq!(editor.cursor(), TextPosition::new(0, 3));
    let editor = run("abc def\n      cd", (1, 7), "v<");
    assert_eq!(editor.text(), "abc def\n  cd");
    assert_eq!(editor.cursor(), TextPosition::new(1, 3));
    let editor = run("abc def\n  cd", (0, 3), "vj<");
    assert_eq!(editor.text(), "abc def\ncd");
    assert_eq!(editor.cursor(), TextPosition::new(0, 3));
}

#[test]
fn a_read_only_editor_selects_and_yanks_but_never_edits() {
    let config = EditorConfig::new()
        .with_mode(EditMode::Vi)
        .with_interaction(Interaction::View);
    for keys in ["viwd", "vjx", "Vc", "V>", "V<"] {
        let editor = run_with(config.clone(), "one two\nthree", (0, 1), keys);
        assert_eq!(editor.text(), "one two\nthree", "{keys}");
        assert!(
            matches!(editor.vi_mode(), Some(ViMode::Visual(_))),
            "{keys} reaches the host's bindings"
        );
    }
    let editor = run_with(config, "one two\nthree", (0, 1), "viwy");
    assert_eq!(editor.yank, "one");
}

#[test]
fn chords_reach_the_host_in_visual_mode() {
    let mut harness = build_harness("ab", EditorConfig::new().with_mode(EditMode::Vi), 10, 2);
    bind_to_host(&mut harness, "ctrl-x");
    harness.key('v').unwrap();
    harness
        .key(key::Key::parse_spec("ctrl-x").unwrap())
        .unwrap();
    assert_eq!(editor_text(&mut harness), "ab");
    assert_eq!(host_binding_hits(&mut harness), 1);
}

#[test]
fn set_selection_round_trips_in_visual_mode() {
    for (text, start, keys) in [
        ("abc def", (0, 1), "vl"),
        ("abc def", (0, 4), "vh"),
        ("abc\ndef", (0, 1), "v$"),
        ("abc\ndef", (1, 1), "v$"),
        ("abc\n\ndef", (1, 0), "vj"),
        ("one\ntwo\nthree", (2, 1), "Vk"),
        ("", (0, 0), "v"),
    ] {
        let mut editor = run(text, start, keys);
        let before = (editor.selection(), editor.cursor());
        editor.set_selection(editor.selection());
        assert_eq!(
            (editor.selection(), editor.cursor()),
            before,
            "{keys} on {text:?}"
        );
    }
    // A range sets the ends of a character selection.
    let mut editor = run("abc def", (0, 0), "v");
    editor.set_selection(Selection::new(
        TextPosition::new(0, 5),
        TextPosition::new(0, 2),
    ));
    assert_eq!(
        state(&editor),
        (range((0, 2), (0, 5)), TextPosition::new(0, 2))
    );
    editor.set_selection(Selection::caret(TextPosition::new(0, 6)));
    assert_eq!(
        state(&editor),
        (range((0, 6), (0, 7)), TextPosition::new(0, 6))
    );
}

#[test]
fn the_host_can_switch_between_kinds_of_selection() {
    let mut editor = run("one\ntwo", (0, 1), "vl");
    editor.set_vi_mode(ViMode::Visual(VisualMode::Line));
    assert_eq!(
        state(&editor),
        (range((0, 0), (0, 3)), TextPosition::new(0, 2))
    );
    editor.set_vi_mode(ViMode::Visual(VisualMode::Character));
    assert_eq!(
        state(&editor),
        (range((0, 1), (0, 3)), TextPosition::new(0, 2))
    );
    editor.set_vi_mode(ViMode::Normal);
    assert!(editor.selection().is_empty());
}

#[test]
fn a_click_ends_visual_mode() {
    let mut harness = build_harness(
        "abc def",
        EditorConfig::new().with_mode(EditMode::Vi),
        10,
        2,
    );
    harness.keys(['v', 'l']).unwrap();
    harness
        .mouse(mouse_event(mouse::Action::Down, 5, 0))
        .unwrap();
    harness.mouse(mouse_event(mouse::Action::Up, 5, 0)).unwrap();
    with_editor(&mut harness, |editor| {
        assert_eq!(editor.vi_mode(), Some(ViMode::Normal));
        assert!(editor.selection().is_empty());
        assert_eq!(editor.cursor(), TextPosition::new(0, 5));
    });
}

#[test]
fn a_selected_line_break_paints_a_cell_after_the_text() {
    let mut harness = build_harness(
        "ab\n\ncd",
        EditorConfig::new().with_mode(EditMode::Vi),
        6,
        3,
    );
    let ground = |path: &str| {
        harness
            .canopy
            .style()
            .resolve(path)
            .bg
            .solid_color()
            .expect("a solid ground")
    };
    let selected = ground("editor/selection");
    let plain = ground("editor/text");
    assert_ne!(selected, plain, "the selection has its own ground");
    let ground = |harness: &Harness, x: u32, y: u32| {
        harness.buf().get(Point { x, y }).expect("a cell").style.bg
    };
    // `v$jj` holds the break of the first line, and the empty line. The
    // cursor ends on the break of the last line.
    harness.keys(['v', '$', 'j', 'j']).unwrap();
    harness.render().unwrap();
    assert_eq!(ground(&harness, 1, 0), selected);
    assert_eq!(
        ground(&harness, 2, 0),
        selected,
        "the break of the first line"
    );
    assert_eq!(ground(&harness, 0, 1), selected, "the empty line");
    assert_eq!(ground(&harness, 1, 1), plain);
    assert_eq!(ground(&harness, 0, 2), selected);
    harness.key(key::KeyCode::Esc).unwrap();
    harness.render().unwrap();
    assert_eq!(ground(&harness, 2, 0), plain);
    assert_eq!(ground(&harness, 0, 1), plain);
}

#[test]
fn a_typed_form_feed_stays_inside_its_line() {
    // Found by the `vi_keys` fuzz target: the rope broke lines at a form
    // feed, and the cursor left the text.
    let editor = run("a", (0, 0), "otwo\u{c}{hree\u{0}<Esc>");
    assert_eq!(editor.text(), "a\ntwo\u{c}{hree\u{0}");
    assert_eq!(editor.cursor(), TextPosition::new(1, 10));
    let editor = run("a", (0, 0), "otwo\u{c}{hree\u{0}<Esc>u");
    assert_eq!(editor.text(), "a");
}

#[test]
fn undo_of_a_line_after_a_carriage_return_keeps_it() {
    // Found by the `vi_keys` fuzz target: `o` after a final carriage return
    // makes a CRLF pair, and undo removed the carriage return.
    let editor = run("HelHl\r", (0, 0), "oneee<CR>er<Esc>u");
    assert_eq!(editor.text(), "HelHl\r");
}

#[test]
fn a_delete_that_joins_a_carriage_return_to_a_break_keeps_the_cursor_in_the_line() {
    // Found by the `vi_keys` fuzz target: the cursor landed between the
    // two chars of the new CRLF pair, past the end of the line.
    let editor = run("ab\rx\ny", (0, 3), "x");
    assert_eq!(editor.text(), "ab\r\ny");
    assert_eq!(editor.cursor(), TextPosition::new(0, 2));
}

#[test]
fn visual_mode_starts_at_the_start_of_a_grapheme() {
    // Found by the `vi_keys` fuzz target: a char typed before a combining
    // mark leaves the cursor inside the grapheme they form, and `v` started
    // the selection there.
    let editor = run("\u{73b}%c", (0, 0), "iX<Esc>v");
    assert_eq!(editor.text(), "X\u{73b}%c");
    assert_eq!(
        state(&editor),
        (range((0, 0), (0, 2)), TextPosition::new(0, 0))
    );
}

#[test]
fn a_publication_keeps_the_visual_ends_on_graphemes() {
    let mut editor = run("ae", (0, 0), "v$");
    let mut prepared = Editor::prepare_view("ae\u{301}x", editor.config().clone(), 20);
    editor.publish_view(prepared, true);
    assert_eq!(
        state(&editor),
        (range((0, 0), (0, 3)), TextPosition::new(0, 1))
    );
    prepared = Editor::prepare_view("ae\u{301}x", editor.config().clone(), 20);
    editor.publish_view(prepared, true);
    assert_eq!(editor.cursor(), TextPosition::new(0, 1));
}

/// The texts of the sweep of text objects in `tests/vim_diff.rs`.
const SWEEP_TEXTS: [&str; 3] = [
    "Hello there.  This is a test.  Another one!\nThe (end.)  A \"quote.\"  Last",
    "first para\nstill first\n\n\nsecond para\n  \nthird one\nends here",
    "this is a test, of words.  foo-bar baz_qux 世界 x",
];

/// The keys of the sweep in `tests/vim_diff.rs`.
const SWEEP_KEYS: [&str; 32] = [
    "viw", "viwiw", "vhiw", "Viw", "vaw", "vawaw", "vhaw", "Vaw", "vis", "visis", "vhis", "Vis",
    "vas", "vasas", "vhas", "Vas", "vip", "vipip", "vhip", "Vip", "vap", "vapap", "vhap", "Vap",
    "vw", "vb", "ve", "vww", "vbb", "vee", "vwb", "veb",
];

/// Parse one row of the recorded table: the mode, the cursor, and the
/// selected range.
fn recorded(row: &str) -> (Option<ViMode>, TextPosition, TextRange) {
    let mut fields = row.split(' ');
    let mode = match fields.next() {
        Some("V") => VisualMode::Line,
        _ => VisualMode::Character,
    };
    let numbers: Vec<usize> = fields
        .map(|field| field.parse().expect("a number"))
        .collect();
    (
        Some(ViMode::Visual(mode)),
        TextPosition::new(numbers[0], numbers[1]),
        range((numbers[2], numbers[3]), (numbers[4], numbers[5])),
    )
}

#[test]
fn the_sweep_matches_the_recorded_outcomes_of_neovim() {
    // Neovim's outcomes of the sweep of `tests/vim_diff.rs`, which rewrites
    // the table, in the order of the loops below.
    let table = include_str!("../../../tests/vim/sweep.txt");
    let mut rows = table.lines().filter(|row| !row.starts_with('#'));
    let config = EditorConfig::new().with_mode(EditMode::Vi);
    let mut failures = Vec::new();
    for text in SWEEP_TEXTS {
        for (line, content) in text.split('\n').enumerate() {
            for column in grapheme_starts(content) {
                for keys in SWEEP_KEYS {
                    let row = rows.next().expect("a recorded outcome for each case");
                    let expected = recorded(row);
                    let editor = run_with(config.clone(), text, (line, column), keys);
                    let actual = (
                        editor.vi_mode(),
                        editor.cursor(),
                        editor.selection().range(),
                    );
                    if actual != expected {
                        failures.push(format!(
                            "{keys} at {line}:{column} in {text:?}: {actual:?}, Neovim {expected:?}"
                        ));
                    }
                }
            }
        }
    }
    assert!(rows.next().is_none(), "one recorded outcome for each case");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Text for the properties: words, punctuation, blanks, line breaks, and
/// multibyte graphemes.
fn text_strategy() -> impl Strategy<Value = String> {
    let pieces = prop_oneof![
        Just("a".to_string()),
        Just("word".to_string()),
        Just(" ".to_string()),
        Just("  ".to_string()),
        Just(".".to_string()),
        Just("! ".to_string()),
        Just("\n".to_string()),
        Just("\n\n".to_string()),
        Just("世界".to_string()),
        Just("e\u{301}".to_string()),
        Just("👩\u{200d}💻".to_string()),
        Just("_x2".to_string()),
    ];
    prop::collection::vec(pieces, 0..24).prop_map(|pieces| pieces.concat())
}

/// Keys of visual mode, without edits.
fn motion_strategy() -> impl Strategy<Value = Vec<&'static str>> {
    let keys = prop_oneof![
        Just("h"),
        Just("j"),
        Just("k"),
        Just("l"),
        Just("w"),
        Just("b"),
        Just("e"),
        Just("0"),
        Just("^"),
        Just("$"),
        Just("gg"),
        Just("G"),
        Just("o"),
        Just("iw"),
        Just("aw"),
        Just("is"),
        Just("as"),
        Just("ip"),
        Just("ap"),
        Just("/a<CR>"),
        Just("n"),
        Just("N"),
    ];
    prop::collection::vec(keys, 0..8)
}

/// Return the char column of each grapheme of `line`, or 0 for an empty
/// line.
fn grapheme_starts(line: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut column = 0;
    for grapheme in text::graphemes(line) {
        starts.push(column);
        column += grapheme.chars().count();
    }
    if starts.is_empty() {
        starts.push(0);
    }
    starts
}

/// Return a start in `text` where a Vim cursor in normal mode can rest:
/// the start of a grapheme, or column 0, picked by `seed`.
fn start_in(text: &str, seed: (usize, usize)) -> (usize, usize) {
    let lines: Vec<&str> = text.split('\n').collect();
    let line = seed.0 % lines.len();
    let starts = grapheme_starts(lines[line]);
    (line, starts[seed.1 % starts.len()])
}

/// Return whether `column` is a grapheme boundary of `line`.
fn on_boundary(line: &str, column: usize) -> bool {
    let starts = grapheme_starts(line);
    starts.contains(&column) || column == line.chars().count()
}

/// The routine configuration of the properties: a fixed seed, so each run
/// gives the same result.
fn config() -> Config {
    Config {
        cases: 96,
        rng_seed: RngSeed::Fixed(0x0005_eed0_f00d),
        ..Config::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn the_selection_stays_on_boundaries_in_the_text(
        text in text_strategy(),
        seed in (0usize..64, 0usize..64),
        keys in motion_strategy(),
    ) {
        let start = start_in(&text, seed);
        let keys = format!("v{}", keys.concat());
        let editor = run(&text, start, &keys);
        let lines: Vec<&str> = text.split('\n').collect();
        let selected = editor.selection().range();
        for position in [selected.start, selected.end, editor.cursor()] {
            let line = lines.get(position.line).copied();
            prop_assert!(line.is_some(), "{position:?} is in the text");
            let line = line.unwrap_or_default();
            prop_assert!(position.column <= line.chars().count());
            prop_assert!(on_boundary(line, position.column), "{position:?} in {line:?}");
        }
        prop_assert!(selected.start <= selected.end);
    }

    #[test]
    fn v_alone_selects_one_character(
        text in text_strategy(),
        seed in (0usize..64, 0usize..64),
    ) {
        let start = start_in(&text, seed);
        let editor = run(&text, start, "v");
        let selected = editor.selection().range();
        let held = editor.text().chars().count();
        let at_end = start.0 + 1 == text.split('\n').count()
            && start.1 == text.split('\n').next_back().unwrap_or_default().chars().count();
        prop_assert_eq!(selected.start, TextPosition::new(start.0, start.1));
        prop_assert_eq!(selected.is_empty(), at_end || held == 0);
        if !selected.is_empty() {
            let mut buffer = TextBuffer::new(text);
            buffer.set_cursor(selected.start);
            let one = buffer.range_text(selected);
            prop_assert!(one == "\n" || grapheme_starts(&one) == vec![0]);
        }
    }

    #[test]
    fn o_twice_returns_to_the_start(
        text in text_strategy(),
        seed in (0usize..64, 0usize..64),
        keys in motion_strategy(),
    ) {
        let start = start_in(&text, seed);
        let keys = format!("v{}", keys.concat());
        let editor = run(&text, start, &keys);
        let swapped = run(&text, start, &format!("{keys}oo"));
        prop_assert_eq!(state(&swapped), state(&editor));
        prop_assert_eq!(swapped.vi_mode(), editor.vi_mode());
    }

    #[test]
    fn v_then_escape_changes_nothing(
        text in text_strategy(),
        seed in (0usize..64, 0usize..64),
    ) {
        let start = start_in(&text, seed);
        let editor = run(&text, start, "v<Esc>");
        prop_assert_eq!(editor.text(), text);
        prop_assert_eq!(editor.vi_mode(), Some(ViMode::Normal));
        prop_assert!(editor.selection().is_empty());
        prop_assert_eq!(editor.cursor(), TextPosition::new(start.0, start.1));
    }

    #[test]
    fn undo_after_a_visual_edit_restores_the_text(
        text in text_strategy(),
        seed in (0usize..64, 0usize..64),
        keys in motion_strategy(),
        edit in prop_oneof![Just("d"), Just("x"), Just("cXY<Esc>"), Just(">"), Just("<")],
        line in any::<bool>(),
    ) {
        let start = start_in(&text, seed);
        let keys = format!("{}{}{edit}u", if line { "V" } else { "v" }, keys.concat());
        let editor = run(&text, start, &keys);
        prop_assert_eq!(editor.text(), text);
    }
}
