//! Vi visual mode of `Editor` against Neovim.
//!
//! The test writes generated and curated cases to a file, runs them all in
//! one headless Neovim with `tests/vim/run_cases.lua`, runs the same cases on
//! `Editor`, and compares the text, the mode, the cursor, and the selection.
//!
//! The test needs `nvim` on the path, so the routine run ignores it. Run it
//! with `cargo xtask vim-diff`, or set `VIM_DIFF_CASES` to the number of
//! generated cases and run the ignored test. `VIM_DIFF_SEED` changes the seed
//! of the generator.

use std::{
    env,
    fmt::Write as _,
    fs,
    path::Path,
    process::{Command, Stdio},
};

use canopy::{input::key, testing::harness::Harness};
use canopy_widgets::editor::{
    EditMode, Editor, EditorConfig, TextPosition, TextRange, ViMode, VisualMode,
};
use serde_json::{Value, json};
use unicode_segmentation::UnicodeSegmentation;

/// The seed of the generator when `VIM_DIFF_SEED` is not set.
const DEFAULT_SEED: u64 = 0x5eed_c0de;

/// One case: a text, a start for the cursor, and keys in Vim notation.
#[derive(Debug, Clone)]
struct Case {
    /// The text of the buffer.
    text: String,
    /// The line of the cursor at the start.
    line: usize,
    /// The column of the cursor at the start, in chars.
    column: usize,
    /// The keys, such as "v", "<Esc>", and "<CR>".
    keys: Vec<String>,
}

impl Case {
    /// Build a case from literal parts.
    fn new(text: &str, line: usize, column: usize, keys: &[&str]) -> Self {
        Self {
            text: text.to_string(),
            line,
            column,
            keys: keys.iter().map(|key| (*key).to_string()).collect(),
        }
    }

    /// Return whether the keys hold `key`.
    fn has_key(&self, key: &str) -> bool {
        self.keys.iter().any(|candidate| candidate == key)
    }
}

/// What a run of a case ends with.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Outcome {
    /// The vi mode.
    mode: ViMode,
    /// The text.
    text: String,
    /// The cursor.
    cursor: TextPosition,
    /// The selected range in visual mode.
    selection: Option<TextRange>,
}

/// A difference from Vim on purpose: a test that names the cases where the
/// editor differs, and the reason.
struct Deliberate {
    /// What differs.
    name: &'static str,
    /// Why the editor differs.
    reason: &'static str,
    /// Return whether the cursor differs in `case` for this reason.
    cursor_differs: fn(&Case) -> bool,
}

/// The deliberate differences. The generator avoids the others: tabs, nroff
/// macros at line starts, and search queries whose matches can overlap.
const DELIBERATE: &[Deliberate] = &[
    Deliberate {
        name: "cursor after insert mode",
        reason: "Esc in insert mode leaves the cursor after the typed text; Vim moves it \
                 back one character. This is insert mode, not visual mode.",
        cursor_differs: |case| case.has_key("c"),
    },
    Deliberate {
        name: "cursor after undo",
        reason: "Undo restores the cursor of the undone step; Vim puts it at the start of \
                 the changed text.",
        cursor_differs: |case| case.has_key("u"),
    },
];

/// A small deterministic generator.
struct Rng(u64);

impl Rng {
    /// Return the next value.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Return a value below `bound`.
    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }

    /// Return true with a chance of `percent` in a hundred.
    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    /// Return one of `items`.
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Words of the generated prose.
const WORDS: &[&str] = &[
    "one",
    "two",
    "three",
    "alpha",
    "beta",
    "gamma",
    "x",
    "a",
    "I",
    "foo",
    "bar",
    "baz",
    "word",
    "text",
    "fix_me",
    "x2",
    "42",
    "café",
    "naïve",
    "über",
    "世界",
    "日本語",
    "こんにちは",
    "카페",
    "😀",
    "👩‍💻",
    "e\u{301}x",
    "µs",
];

/// Punctuation after a word.
const SUFFIXES: &[&str] = &[
    ".", ",", "!", "?", ";", ":", ".)", ".\"", "'s", "-", "...", "?'", ")",
];

/// Tokens of punctuation alone.
const MARKS: &[&str] = &["--", "(", "\"", "→", "©", "。", "*", "&"];

/// Generate a line of prose.
fn prose_line(rng: &mut Rng) -> String {
    let mut line = String::new();
    if rng.chance(15) {
        line.push_str(rng.pick(&["  ", "    ", "      ", " "]));
    }
    let words = 1 + rng.below(9);
    for index in 0..words {
        if index > 0 {
            line.push_str(if rng.chance(15) { "  " } else { " " });
        }
        if rng.chance(8) {
            line.push_str(rng.pick(MARKS));
            continue;
        }
        if rng.chance(8) {
            line.push('(');
        }
        line.push_str(rng.pick(WORDS));
        if rng.chance(30) {
            line.push_str(rng.pick(SUFFIXES));
        }
    }
    if rng.chance(10) {
        line.push(' ');
    }
    // A line that starts with a period could be an nroff macro, which Vim
    // reads as a paragraph start.
    if line.starts_with('.') {
        line.insert(0, 'x');
    }
    line
}

/// Generate a text of prose, blank lines, and lines of blanks.
fn prose(rng: &mut Rng) -> String {
    let lines = 1 + rng.below(6);
    let mut text = Vec::new();
    for _ in 0..lines {
        let line = match rng.below(20) {
            0..=2 => String::new(),
            3 => "  ".to_string(),
            _ => prose_line(rng),
        };
        text.push(line);
    }
    let mut text = text.join("\n");
    if rng.chance(25) {
        text.push('\n');
    }
    text
}

/// Return the char columns of the graphemes of `line`.
fn grapheme_columns(line: &str) -> Vec<usize> {
    let mut columns = Vec::new();
    let mut column = 0;
    for grapheme in line.graphemes(true) {
        columns.push(column);
        column += grapheme.chars().count();
    }
    columns
}

/// Return whether a proper prefix of `query` is also its suffix, so that two
/// matches of it can overlap.
fn has_border(query: &str) -> bool {
    let chars: Vec<char> = query.chars().collect();
    (1..chars.len()).any(|size| chars[..size] == chars[chars.len() - size..])
}

/// Pick a search query: part of an ASCII word of `text`, or a query with
/// no match.
fn query(rng: &mut Rng, text: &str) -> String {
    let words: Vec<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() || rng.chance(10) {
        return "zqx".to_string();
    }
    for _ in 0..8 {
        let word = *rng.pick(&words);
        let start = rng.below(word.len());
        let len = 1 + rng.below((word.len() - start).min(4));
        let candidate = &word[start..start + len];
        if !has_border(candidate) {
            return candidate.to_string();
        }
    }
    "zqx".to_string()
}

/// Motions of visual mode.
const MOTIONS: &[&str] = &[
    "h", "j", "k", "l", "<Left>", "<Right>", "<Up>", "<Down>", "0", "^", "$", "w", "b", "e", "gg",
    "G",
];

/// Text objects of visual mode.
const OBJECTS: &[&str] = &["iw", "aw", "is", "as", "ip", "ap"];

/// Push the keys of `sequence`, one key for each char.
fn push_chars(keys: &mut Vec<String>, sequence: &str) {
    keys.extend(sequence.chars().map(String::from));
}

/// Generate the keys of a case: a start of visual mode, actions, and an
/// end.
///
/// A text object can change the kind of the selection, so after one the
/// generator no longer knows which switch key would leave visual mode, and
/// switches no more.
fn generate_keys(rng: &mut Rng, text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let start = if rng.chance(70) { "v" } else { "V" };
    let mut mode = Some(start);
    keys.push(start.to_string());
    if rng.chance(30) {
        push_chars(&mut keys, rng.pick(OBJECTS));
        mode = None;
    }
    let actions = rng.below(7);
    for _ in 0..actions {
        match rng.below(20) {
            0..=9 => {
                let motion = *rng.pick(MOTIONS);
                if motion.starts_with('<') {
                    keys.push(motion.to_string());
                } else {
                    push_chars(&mut keys, motion);
                }
            }
            10 => keys.push(rng.pick(&["o", "O"]).to_string()),
            11..=13 => {
                push_chars(&mut keys, rng.pick(OBJECTS));
                mode = None;
            }
            14 => {
                keys.push(rng.pick(&["/", "?"]).to_string());
                push_chars(&mut keys, &query(rng, text));
                keys.push("<CR>".to_string());
            }
            15 => keys.push(rng.pick(&["n", "N"]).to_string()),
            _ => {
                let Some(current) = mode else {
                    continue;
                };
                let switch = if rng.chance(50) { "v" } else { "V" };
                keys.push(switch.to_string());
                if switch == current {
                    // That left visual mode. Start again, or end in normal
                    // mode.
                    if rng.chance(50) {
                        return keys;
                    }
                    let again = if rng.chance(70) { "v" } else { "V" };
                    keys.push(again.to_string());
                    mode = Some(again);
                } else {
                    mode = Some(switch);
                }
            }
        }
    }
    let edit = match rng.below(17) {
        0..=4 => return keys,
        5..=6 => {
            keys.push("<Esc>".to_string());
            return keys;
        }
        7..=8 => {
            keys.push("y".to_string());
            return keys;
        }
        9..=10 => "d",
        11 => "x",
        12..=13 => {
            keys.push("c".to_string());
            push_chars(&mut keys, rng.pick(&["XY", "Z ", "q"]));
            keys.push("<Esc>".to_string());
            ""
        }
        14..=15 => ">",
        _ => "<",
    };
    if !edit.is_empty() {
        keys.push(edit.to_string());
    }
    if rng.chance(30) {
        keys.push("u".to_string());
    }
    keys
}

/// Generate a case.
fn generate(rng: &mut Rng) -> Case {
    let text = prose(rng);
    let lines: Vec<&str> = text.split('\n').collect();
    let line = rng.below(lines.len());
    let columns = grapheme_columns(lines[line]);
    let column = if columns.is_empty() {
        0
    } else {
        *rng.pick(&columns)
    };
    let keys = generate_keys(rng, &text);
    Case {
        text,
        line,
        column,
        keys,
    }
}

/// The texts of the sweep of text objects, from Vim's help.
const SWEEP_TEXTS: [&str; 3] = [
    "Hello there.  This is a test.  Another one!\nThe (end.)  A \"quote.\"  Last",
    "first para\nstill first\n\n\nsecond para\n  \nthird one\nends here",
    "this is a test, of words.  foo-bar baz_qux 世界 x",
];

/// The keys of the sweep: each text object once, twice, from a selection
/// of two characters, and from a line selection, then word motions.
const SWEEP_KEYS: [&str; 32] = [
    "viw", "viwiw", "vhiw", "Viw", "vaw", "vawaw", "vhaw", "Vaw", "vis", "visis", "vhis", "Vis",
    "vas", "vasas", "vhas", "Vas", "vip", "vipip", "vhip", "Vip", "vap", "vapap", "vhap", "Vap",
    "vw", "vb", "ve", "vww", "vbb", "vee", "vwb", "veb",
];

/// Every sequence of [`SWEEP_KEYS`] from every start in [`SWEEP_TEXTS`]. The
/// recorded table of `src/editor/tests/visual.rs` builds the same cases in
/// the same order.
fn object_sweep() -> Vec<Case> {
    let mut cases = Vec::new();
    for text in SWEEP_TEXTS {
        for (line, content) in text.split('\n').enumerate() {
            let mut columns = grapheme_columns(content);
            if columns.is_empty() {
                columns.push(0);
            }
            for column in columns {
                for sequence in SWEEP_KEYS {
                    let mut keys = Vec::new();
                    push_chars(&mut keys, sequence);
                    cases.push(Case {
                        text: text.to_string(),
                        line,
                        column,
                        keys,
                    });
                }
            }
        }
    }
    cases
}

/// The examples of Vim's help, cases at the edges, and the sweep of text
/// objects.
fn curated() -> Vec<Case> {
    let mut cases = vec![
        // One char, in both directions and on every kind of place.
        Case::new("abc", 0, 0, &["v"]),
        Case::new("abc", 0, 2, &["v"]),
        Case::new("abc\n\nxyz", 1, 0, &["v"]),
        Case::new("abc\n", 1, 0, &["v"]),
        Case::new("", 0, 0, &["v"]),
        Case::new("", 0, 0, &["V"]),
        Case::new("", 0, 0, &["v", "d"]),
        Case::new("", 0, 0, &["V", "d"]),
        Case::new("a世界b", 0, 1, &["v"]),
        Case::new("e\u{301}x", 0, 0, &["v"]),
        Case::new("a👩‍💻b", 0, 1, &["v", "l"]),
        Case::new("abc\ndef", 0, 2, &["v", "l"]),
        Case::new("abc\ndef", 1, 2, &["v", "l", "l"]),
        Case::new("abc\ndef", 0, 0, &["v", "h"]),
        Case::new("abc\ndef", 1, 1, &["v", "k"]),
        Case::new("abc\ndef", 1, 1, &["v", "k", "o"]),
        Case::new("abc\ndef", 1, 1, &["v", "k", "o", "o"]),
        Case::new("abcdef\nde", 0, 5, &["v", "j"]),
        Case::new("abcdef\nde", 0, 5, &["v", "j", "k"]),
        Case::new("abc\ndef", 0, 1, &["v", "$"]),
        Case::new("abc\ndef", 0, 1, &["v", "$", "j"]),
        Case::new("abc\ndef", 1, 1, &["v", "$"]),
        Case::new("abc\ndef", 0, 1, &["v", "$", "d"]),
        Case::new("abc\ndef", 0, 1, &["v", "$", "<Esc>"]),
        Case::new("  abc\ndef", 0, 4, &["v", "^"]),
        Case::new("   \nx", 0, 0, &["v", "^"]),
        Case::new("  abc\ndef", 0, 4, &["v", "0"]),
        Case::new("abc\n  def\nghi", 1, 3, &["v", "g", "g"]),
        Case::new("abc\n  def\nghi", 0, 2, &["v", "G"]),
        Case::new("abc\n  def\nghi", 0, 2, &["v", "$", "G"]),
        // Words, punctuation, and blanks.
        Case::new("foo.bar baz", 0, 0, &["v", "w", "w", "w"]),
        Case::new("foo bar\nbaz", 1, 0, &["v", "w"]),
        Case::new("foo bar\nbaz", 0, 4, &["v", "w", "d"]),
        Case::new("a b", 0, 2, &["v", "e"]),
        Case::new("a b\nc", 0, 2, &["v", "e", "e"]),
        Case::new("abc def\ncd", 0, 5, &["v", "b", "b"]),
        Case::new("foo\n\nbar", 0, 0, &["v", "w"]),
        Case::new("foo\n\nbar", 2, 1, &["v", "b"]),
        Case::new("a世界b c", 0, 0, &["v", "w", "w"]),
        Case::new("a😀b c", 0, 0, &["v", "w"]),
        // Line selections.
        Case::new("one\ntwo\nthree", 0, 1, &["V"]),
        Case::new("one\ntwo\nthree", 0, 1, &["V", "j"]),
        Case::new("one\ntwo\nthree", 2, 1, &["V", "k"]),
        Case::new("one\ntwo\nthree", 1, 1, &["V", "d"]),
        Case::new("one\ntwo\nthree", 2, 1, &["V", "d"]),
        Case::new("one\ntwo\nthree", 0, 1, &["V", "G", "d"]),
        Case::new("  one\n  two\nthree", 0, 3, &["V", "j", "y"]),
        Case::new("  one\n  two\nthree", 1, 3, &["V", "k", "y"]),
        Case::new("  one\n  two\nthree", 0, 3, &["V", "j", "d"]),
        Case::new("  one\n  two\n  three", 1, 3, &["V", "j", "d"]),
        Case::new("one\ntwo", 0, 0, &["V", "c", "X", "<Esc>"]),
        Case::new("one\ntwo\nthree", 0, 0, &["V", "j", "c", "X", "<Esc>"]),
        Case::new("abc def\n  cd", 0, 3, &["V", ">"]),
        Case::new("abc def\n\ncd", 0, 3, &["v", "j", "j", ">"]),
        Case::new("abc def\n      cd", 1, 7, &["v", "<"]),
        Case::new("abc def\n  cd", 0, 3, &["v", "j", "<"]),
        Case::new("one\ntwo", 0, 1, &["V", "v"]),
        Case::new("one\ntwo", 0, 1, &["v", "l", "V", "v"]),
        Case::new("one\ntwo", 0, 1, &["v", "v"]),
        Case::new("one\ntwo", 0, 1, &["V", "V"]),
        // Operators of character selections.
        Case::new("abc def", 0, 1, &["v", "l", "y"]),
        Case::new("abc def", 0, 5, &["v", "b", "y"]),
        Case::new("abc def", 0, 1, &["v", "l", "d"]),
        Case::new("abc def", 0, 6, &["v", "d"]),
        Case::new("abc\n\nx", 1, 0, &["v", "d"]),
        Case::new("a\n", 1, 0, &["v", "d"]),
        Case::new("abc def", 0, 1, &["v", "e", "c", "X", "<Esc>"]),
        Case::new("abc def", 0, 1, &["v", "e", "d", "u"]),
        // Search.
        Case::new(
            "foo bar foo baz foo",
            0,
            0,
            &["v", "/", "f", "o", "o", "<CR>"],
        ),
        Case::new(
            "foo bar foo baz foo",
            0,
            0,
            &["v", "/", "f", "o", "o", "<CR>", "n", "n"],
        ),
        Case::new(
            "foo bar foo baz foo",
            0,
            9,
            &["v", "?", "f", "o", "o", "<CR>"],
        ),
        Case::new(
            "foo bar foo baz foo",
            0,
            9,
            &["v", "?", "f", "o", "o", "<CR>", "N"],
        ),
        Case::new("foo bar", 0, 0, &["v", "/", "f", "o", "o", "<CR>"]),
        Case::new("foo bar", 0, 2, &["v", "/", "z", "<CR>", "l"]),
        Case::new("foo bar", 0, 2, &["v", "n"]),
        // A prefix that a key cancels.
        Case::new("abc def", 0, 1, &["v", "i", "<Esc>", "l"]),
        Case::new("abc def", 0, 1, &["v", "g", "<Esc>", "l"]),
    ];
    cases.extend(object_sweep());
    cases
}

/// Convert a key in Vim notation to a key of the editor.
fn editor_key(key: &str) -> key::Key {
    match key {
        "<Esc>" => key::KeyCode::Esc.into(),
        "<CR>" => key::KeyCode::Enter.into(),
        "<Left>" => key::KeyCode::Left.into(),
        "<Right>" => key::KeyCode::Right.into(),
        "<Up>" => key::KeyCode::Up.into(),
        "<Down>" => key::KeyCode::Down.into(),
        _ => {
            let mut chars = key.chars();
            let character = chars.next().expect("a key");
            assert!(chars.next().is_none(), "unknown key {key}");
            character.into()
        }
    }
}

/// Run `case` on the editor.
fn run_editor(case: &Case) -> Outcome {
    let config = EditorConfig::new().with_mode(EditMode::Vi).with_tab_stop(4);
    let editor = Editor::with_config(case.text.clone(), config);
    let mut harness = Harness::builder(editor)
        .register::<Editor>()
        .size(80, 24)
        .build()
        .expect("the harness builds");
    harness
        .with_root_widget_context(|editor: &mut Editor, ctx| {
            ctx.set_focus(ctx.node_id())?;
            editor.set_cursor(ctx, TextPosition::new(case.line, case.column));
            Ok(())
        })
        .expect("the editor takes focus");
    for key in &case.keys {
        harness.key(editor_key(key)).expect("the key runs");
    }
    harness.with_root_widget(|editor: &mut Editor| {
        let mode = editor.vi_mode().expect("a vi editor");
        Outcome {
            mode,
            text: editor.text(),
            cursor: editor.cursor(),
            selection: matches!(mode, ViMode::Visual(_)).then(|| editor.selection().range()),
        }
    })
}

/// Return the position after `text` from `start`.
fn advance(start: TextPosition, text: &str) -> TextPosition {
    let mut position = start;
    for c in text.chars() {
        if c == '\n' {
            position = TextPosition::new(position.line + 1, 0);
        } else {
            position.column += 1;
        }
    }
    position
}

/// Read a position from a Neovim result.
fn position(value: &Value) -> TextPosition {
    let part = |index: usize| {
        usize::try_from(value[index].as_u64().expect("a number")).expect("a column fits")
    };
    TextPosition::new(part(0), part(1))
}

/// Convert a Neovim result to an outcome.
fn vim_outcome(result: &Value) -> Result<Outcome, String> {
    if let Some(error) = result.get("error") {
        return Err(format!("Neovim failed: {error}"));
    }
    let mode = match result["mode"].as_str() {
        Some("n") => ViMode::Normal,
        Some("i") => ViMode::Insert,
        Some("v") => ViMode::Visual(VisualMode::Character),
        Some("V") => ViMode::Visual(VisualMode::Line),
        other => return Err(format!("unexpected mode {other:?}")),
    };
    let selection = result.get("start").map(|start| {
        let start = position(start);
        let yank = result["yank"].as_str().expect("a yank");
        let yank = match mode {
            // A line yank ends with the break of its last line, which the
            // editor's range leaves out.
            ViMode::Visual(VisualMode::Line) => yank.strip_suffix('\n').unwrap_or(yank),
            _ => yank,
        };
        TextRange::new(start, advance(start, yank))
    });
    Ok(Outcome {
        mode,
        text: result["text"].as_str().expect("a text").to_string(),
        cursor: position(&result["cursor"]),
        selection,
    })
}

/// Compare the editor with Neovim on `case`, and describe any difference.
fn compare(case: &Case, vim: &Outcome, editor: &Outcome) -> Option<String> {
    let mut problems = Vec::new();
    if vim.text != editor.text {
        problems.push(format!(
            "text: vim {:?}, editor {:?}",
            vim.text, editor.text
        ));
    }
    if vim.mode != editor.mode {
        problems.push(format!(
            "mode: vim {:?}, editor {:?}",
            vim.mode, editor.mode
        ));
    }
    let cursor_differs = DELIBERATE
        .iter()
        .any(|deliberate| (deliberate.cursor_differs)(case));
    if !cursor_differs && vim.cursor != editor.cursor {
        problems.push(format!(
            "cursor: vim {:?}, editor {:?}",
            vim.cursor, editor.cursor
        ));
    }
    if vim.mode == editor.mode && vim.selection != editor.selection {
        problems.push(format!(
            "selection: vim {:?}, editor {:?}",
            vim.selection, editor.selection
        ));
    }
    (!problems.is_empty()).then(|| problems.join("; "))
}

/// Run `cases` in one Neovim, and return its results.
fn run_vim(cases: &[Case]) -> Vec<Value> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let input = dir.path().join("cases.json");
    let output = dir.path().join("results.json");
    let json: Vec<Value> = cases
        .iter()
        .map(|case| {
            json!({
                "text": case.text,
                "line": case.line,
                "column": case.column,
                "keys": case.keys,
            })
        })
        .collect();
    fs::write(&input, serde_json::to_string(&json).expect("cases encode")).expect("cases write");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vim/run_cases.lua");
    let status = Command::new("nvim")
        .args(["--headless", "--clean", "-l"])
        .arg(&script)
        .arg(&input)
        .arg(&output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("nvim runs");
    assert!(status.success(), "nvim failed: {status}");
    let results = fs::read_to_string(&output).expect("nvim writes results");
    let results: Vec<Value> = serde_json::from_str(&results).expect("results decode");
    assert_eq!(results.len(), cases.len(), "one result for each case");
    results
}

/// Write Neovim's outcome of each case of the sweep to
/// `tests/vim/sweep.txt`, one line each: the mode, the cursor, and the
/// selected range, as numbers.
fn record_sweep(sweep: &[Case], results: &[Value]) {
    let mut table = String::from(
        "# Neovim's outcome of each case of the sweep of text objects and word\n\
         # motions in tests/vim_diff.rs, in order: the mode, the cursor line and\n\
         # column, and the start and end of the selected range. Rewrite with\n\
         # VIM_DIFF_RECORD=1 cargo xtask vim-diff --cases 0\n",
    );
    for (case, result) in sweep.iter().zip(results) {
        let outcome = vim_outcome(result).expect("Neovim runs each case of the sweep");
        let mode = match outcome.mode {
            ViMode::Visual(VisualMode::Character) => 'v',
            ViMode::Visual(VisualMode::Line) => 'V',
            other => panic!("the sweep ends in visual mode, not {other:?}: {case:?}"),
        };
        let range = outcome.selection.expect("a visual selection");
        writeln!(
            table,
            "{mode} {} {} {} {} {} {}",
            outcome.cursor.line,
            outcome.cursor.column,
            range.start.line,
            range.start.column,
            range.end.line,
            range.end.column
        )
        .expect("a table");
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vim/sweep.txt");
    fs::write(path, table).expect("the table writes");
}

/// Return whether `nvim` runs.
fn nvim_available() -> bool {
    Command::new("nvim")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs nvim: run with `cargo xtask vim-diff`"]
    fn visual_mode_matches_neovim() {
        let Some(count) = env::var("VIM_DIFF_CASES").ok() else {
            eprintln!("VIM_DIFF_CASES is not set: skipping the Neovim comparison");
            return;
        };
        let count: usize = count.parse().expect("VIM_DIFF_CASES is a number");
        assert!(
            nvim_available(),
            "VIM_DIFF_CASES is set, but nvim does not run"
        );
        let seed = env::var("VIM_DIFF_SEED").map_or(DEFAULT_SEED, |seed| {
            seed.parse().expect("VIM_DIFF_SEED is a number")
        });

        let mut cases = curated();
        let curated_count = cases.len();
        let sweep = object_sweep();
        let mut rng = Rng(seed | 1);
        cases.extend((0..count).map(|_| generate(&mut rng)));
        let results = run_vim(&cases);
        if env::var_os("VIM_DIFF_RECORD").is_some() {
            record_sweep(&sweep, &results[curated_count - sweep.len()..curated_count]);
        }

        let mut failures = Vec::new();
        for (case, result) in cases.iter().zip(&results) {
            let problem = match vim_outcome(result) {
                Ok(vim) => compare(case, &vim, &run_editor(case)),
                Err(error) => Some(error),
            };
            if let Some(problem) = problem {
                failures.push((case, problem));
            }
        }

        let mut report = String::new();
        for (case, problem) in failures.iter().take(40) {
            writeln!(
                report,
                "text {:?} at {}:{} keys {}\n  {problem}",
                case.text,
                case.line,
                case.column,
                case.keys.concat()
            )
            .expect("a report");
        }
        eprintln!(
            "{} cases ({curated_count} curated, {count} generated, seed {seed}): {} differ",
            cases.len(),
            failures.len()
        );
        for deliberate in DELIBERATE {
            eprintln!("deliberate: {}: {}", deliberate.name, deliberate.reason);
        }
        assert!(failures.is_empty(), "differences from Neovim:\n{report}");
    }
}
