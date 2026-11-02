//! Proptest strategies and invariant checks that tests in this crate, its
//! hosts, and fuzz targets share.
//!
//! The strategies make Markdown documents from a grammar of blocks, hostile
//! text that changes structure when it goes into a block, and the examples of
//! the parser's spec files. [`check_units`] and [`check_edit`] check the
//! invariants of edit units, and compare their ranges with a slow reference.

use std::{env, fs, io, ops::Range, path::Path, sync::Arc, time::Duration};

use canopy::{
    Context, NodeId, ViewContextExt,
    error::Result as CanopyResult,
    runtime::TurnInput,
    testing::{ManualClock, harness::Harness},
};
use proptest::{
    collection::vec,
    prelude::*,
    test_runner::{Config, RngSeed},
};

use crate::{
    BlockEdit, DocumentInput, Markdown, MarkdownOptions, Mode,
    document::{DocumentSnapshot, ParseLimits, parse_document},
    executor::{ExecutorOptions, HeldWork, ManualExecutor, MarkdownExecutor},
    source_store::SourceStore,
    units::{EditKind, EditUnit, edit_units, parse_units, replace_own_text, unit_at},
};

/// The seed of routine property runs, so each run checks the same cases.
pub const ROUTINE_SEED: u64 = 0x6d61_726b_646f_776e;

/// Return a proptest configuration of `cases` cases with the routine seed.
/// The variables `PROPTEST_CASES` and `PROPTEST_RNG_SEED` override them, as
/// a soak run sets them.
pub fn config(cases: u32) -> Config {
    let mut config = Config::default();
    if env::var_os("PROPTEST_CASES").is_none() {
        config.cases = cases;
    }
    if env::var_os("PROPTEST_RNG_SEED").is_none() {
        config.rng_seed = RngSeed::Fixed(ROUTINE_SEED);
    }
    config
}

/// One word of inline text: plain letters, CJK, emoji, a combining mark, a
/// tab, or inline markup.
fn word() -> impl Strategy<Value = String> {
    prop_oneof![
        12 => "[a-z]{1,8}",
        1 => Just("日本語".to_owned()),
        1 => Just("🙂".to_owned()),
        1 => Just("e\u{301}".to_owned()),
        1 => Just("a\tb".to_owned()),
        1 => Just("*em*".to_owned()),
        1 => Just("**strong**".to_owned()),
        1 => Just("`code`".to_owned()),
        1 => Just("[link](https://example.test)".to_owned()),
        1 => Just("[^1]".to_owned()),
        1 => Just("^[an inline note]".to_owned()),
        1 => Just("[[wiki]]".to_owned()),
        1 => Just("#tag".to_owned()),
        1 => Just("==mark==".to_owned()),
        1 => Just("<b>".to_owned()),
    ]
}

/// One line of inline text, sometimes with trailing spaces.
fn text_line() -> impl Strategy<Value = String> {
    (vec(word(), 1..6), prop::bool::weighted(0.15)).prop_map(|(words, trailing)| {
        let mut line = words.join(" ");
        if trailing {
            line.push_str("  ");
        }
        line
    })
}

/// One line of code, which may be blank or hold Markdown syntax.
fn code_line() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => "[a-z(){};= ]{0,12}",
        1 => Just(String::new()),
        1 => Just("# not a heading".to_owned()),
        1 => Just("\tindented".to_owned()),
        1 => Just("``".to_owned()),
    ]
}

/// A leaf block, as lines without terminators.
fn leaf() -> BoxedStrategy<Vec<String>> {
    prop_oneof![
        1 => (1..=6usize, text_line(), any::<bool>()).prop_map(|(level, text, closing)| {
            let closing = if closing { " ##" } else { "" };
            vec![format!("{} {text}{closing}", "#".repeat(level))]
        }),
        1 => Just(vec!["#".to_owned()]),
        1 => (vec(text_line(), 1..3), any::<bool>()).prop_map(|(mut lines, level)| {
            lines.push(if level { "===" } else { "---" }.to_owned());
            lines
        }),
        4 => (vec(text_line(), 1..4), prop::bool::weighted(0.1)).prop_map(|(mut lines, id)| {
            if id && let Some(last) = lines.last_mut() {
                last.push_str(" ^blk");
            }
            lines
        }),
        1 => (
            prop_oneof![Just("```"), Just("~~~"), Just("````")],
            prop_oneof![Just(""), Just("rust"), Just("luau")],
            vec(code_line(), 0..4),
            prop::bool::weighted(0.9),
        )
            .prop_map(|(fence, info, code, closed)| {
                let mut lines = vec![format!("{fence}{info}")];
                lines.extend(code);
                if closed {
                    lines.push(fence.to_owned());
                }
                lines
            }),
        1 => vec(code_line(), 1..3).prop_map(|code| {
            code.iter()
                .enumerate()
                .map(|(index, line)| {
                    // Indented code opens on a line of text.
                    if index == 0 && line.trim().is_empty() {
                        "    code".to_owned()
                    } else {
                        format!("    {line}")
                    }
                })
                .collect()
        }),
        1 => (1..4usize, 0..3usize).prop_map(|(columns, rows)| {
            let row = |cell: &str| format!("| {} |", vec![cell; columns].join(" | "));
            let mut lines = vec![row("head"), row("---")];
            lines.extend((0..rows).map(|_| row("cell")));
            lines
        }),
        1 => prop_oneof![Just("---"), Just("***"), Just("___")].prop_map(|rule| vec![rule.to_owned()]),
        1 => text_line().prop_map(|text| vec!["<div>".to_owned(), text, "</div>".to_owned()]),
        1 => text_line().prop_map(|text| vec![format!("[^1]: {text}")]),
        1 => Just(vec!["[ref]: https://example.test".to_owned()]),
        1 => Just(vec!["^lone".to_owned()]),
        1 => Just(vec!["![alt](image.png)".to_owned()]),
        1 => Just(vec!["![[note]]".to_owned()]),
    ]
    .boxed()
}

/// Join the lines of `blocks`, with a blank line between blocks.
fn spaced(blocks: Vec<Vec<String>>) -> Vec<String> {
    let mut lines = Vec::new();
    for (index, block) in blocks.into_iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        lines.extend(block);
    }
    lines
}

/// Return `lines` behind a quote marker.
fn quoted(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|line| {
            if line.is_empty() {
                ">".to_owned()
            } else {
                format!("> {line}")
            }
        })
        .collect()
}

/// A list of `items`, each a run of blocks.
fn list(items: Vec<Vec<Vec<String>>>, ordered: bool, loose: bool, task: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 && loose {
            lines.push(String::new());
        }
        let marker = if ordered {
            format!("{}.", index + 1)
        } else {
            "-".to_owned()
        };
        let indent = " ".repeat(marker.len() + 1);
        for (number, line) in spaced(item).into_iter().enumerate() {
            lines.push(if number == 0 {
                let task = if task { "[ ] " } else { "" };
                format!("{marker} {task}{line}")
            } else if line.is_empty() {
                line
            } else {
                format!("{indent}{line}")
            });
        }
    }
    lines
}

/// A block: a leaf, or a list, quote, or callout of blocks, nested to depth
/// three.
fn block() -> BoxedStrategy<Vec<String>> {
    leaf()
        .prop_recursive(3, 32, 3, |inner| {
            prop_oneof![
                (
                    vec(vec(inner.clone(), 1..3), 1..4),
                    any::<bool>(),
                    any::<bool>(),
                    prop::bool::weighted(0.2),
                )
                    .prop_map(|(items, ordered, loose, task)| list(items, ordered, loose, task)),
                vec(inner.clone(), 1..3).prop_map(|blocks| quoted(spaced(blocks))),
                (
                    prop_oneof![Just("note"), Just("TIP"), Just("warning")],
                    prop::option::of(text_line()),
                    vec(inner, 0..2),
                )
                    .prop_map(|(kind, title, blocks)| {
                        let title = title.map_or(String::new(), |title| format!(" {title}"));
                        let mut lines = vec![format!("[!{kind}]{title}")];
                        lines.extend(spaced(blocks));
                        quoted(lines)
                    }),
            ]
        })
        .boxed()
}

/// Make Markdown documents from a grammar of blocks.
///
/// Documents hold headings of both forms, paragraphs, tight, loose, and
/// ordered lists, quotes, callouts, fenced and indented code, tables, rules,
/// front matter, HTML blocks, footnotes, link reference definitions, and
/// block ids, with containers nested to depth three. Text holds CJK, emoji,
/// combining marks, tabs, and trailing spaces. A document ends with or
/// without a newline, and may use CRLF. Empty documents and documents of
/// only definitions occur.
pub fn document() -> impl Strategy<Value = String> {
    let blocks = (
        prop::bool::weighted(0.15),
        vec((block(), prop::bool::weighted(0.2)), 0..7),
    )
        .prop_map(|(front, blocks)| {
            let mut text = String::new();
            if front {
                text.push_str("---\ntitle: A\n---\n\n");
            }
            for (index, (block, tight)) in blocks.into_iter().enumerate() {
                if index > 0 {
                    text.push_str(if tight { "\n" } else { "\n\n" });
                }
                text.push_str(&block.join("\n"));
            }
            text
        });
    let definitions = vec(
        prop_oneof![
            Just("[a]: https://a.test"),
            Just("[b]: /b \"title\""),
            Just("^id"),
        ],
        1..4,
    )
    .prop_map(|lines| lines.join("\n\n"));
    (
        prop_oneof![
            12 => blocks,
            1 => definitions,
            1 => Just(String::new()),
        ],
        prop::bool::weighted(0.8),
        prop::bool::weighted(0.15),
    )
        .prop_map(|(mut text, newline, crlf)| {
            if newline && !text.is_empty() {
                text.push('\n');
            }
            if crlf {
                text = text.replace('\n', "\r\n");
            }
            text
        })
}

/// Make text that changes structure when it goes into a block: an unclosed
/// fence or HTML comment, a setext underline, a blank line, a list or quote
/// marker at a line start, a table row, and empty text, alone or after a
/// line of text.
pub fn hostile_text() -> impl Strategy<Value = String> {
    let hostile = prop_oneof![
        Just("```"),
        Just("~~~\ncode"),
        Just("<!-- open"),
        Just("%% open"),
        Just("==="),
        Just("---"),
        Just("a\n\nb"),
        Just("- item"),
        Just("1. item"),
        Just("> quote"),
        Just("| a | b |\n| - | - |"),
        Just(""),
        Just("\n"),
        Just("# heading"),
        Just("    indented"),
        Just("[ref]: /url"),
        Just("^id"),
        Just("<div>"),
        Just("[!note]"),
    ];
    prop_oneof![
        hostile.clone().prop_map(str::to_owned),
        (text_line(), hostile).prop_map(|(line, hostile)| format!("{line}\n{hostile}")),
        text_line(),
    ]
}

/// The fence of 32 backticks that opens and closes each spec example.
const SPEC_FENCE: &str = "````````````````````````````````";

/// Return the Markdown of every example in the spec files of `canopy-xmark`,
/// the files that `cargo xtask xmark-specs` reads, in a fixed order.
pub fn spec_examples() -> io::Result<Vec<String>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../canopy-xmark");
    let mut files = fs::read_dir(root.join("specs"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .collect::<Vec<_>>();
    files.sort();
    files.extend(
        [
            "CommonMark/spec.txt",
            "CommonMark/smart_punct.txt",
            "GitHub/gfm_table.txt",
            "GitHub/gfm_strikethrough.txt",
            "GitHub/gfm_tasklist.txt",
        ]
        .iter()
        .map(|file| root.join("third_party").join(file)),
    );
    let opening = format!("{SPEC_FENCE} example");
    let mut examples = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file)?;
        let mut rest = text.as_str();
        while let Some(at) = rest.find(&opening) {
            let after = &rest[at + opening.len()..];
            let Some(body) = after.find('\n').map(|end| &after[end + 1..]) else {
                break;
            };
            let Some(split) = body.find("\n.\n") else {
                break;
            };
            examples.push(body[..split + 1].replace('→', "\t"));
            rest = &body[split..];
            match rest.find(SPEC_FENCE) {
                Some(close) => rest = &rest[close + SPEC_FENCE.len()..],
                None => break,
            }
        }
    }
    Ok(examples)
}

/// Parse `text` as a complete document with the default limits.
pub fn parse(text: &str) -> DocumentSnapshot {
    let store = SourceStore::static_with_limit(text, usize::MAX).expect("no source limit");
    parse_document(&store.snapshot(), &ParseLimits::default(), None)
}

/// One line of `text`: the bytes of its text, and where the next starts.
struct Line {
    /// The bytes of the line's text, without its terminator.
    text: Range<usize>,
    /// The start of the next line.
    next: usize,
}

/// Split `text` into lines at CRLF, LF, and CR, one character at a time.
fn reference_lines(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let next = match c {
            '\r' if chars.peek().is_some_and(|&(_, c)| c == '\n') => {
                chars.next();
                at + 2
            }
            '\r' | '\n' => at + 1,
            _ => continue,
        };
        lines.push(Line {
            text: start..at,
            next,
        });
        start = next;
    }
    lines.push(Line {
        text: start..text.len(),
        next: text.len(),
    });
    lines
}

/// The reference ranges of the units of `document`: mark the lines of each
/// block, then join the blocks that share a line, directly or through other
/// blocks. Return each unit's range and its blocks, in source order.
fn reference_units(text: &str, document: &DocumentSnapshot) -> Vec<(Range<usize>, Range<usize>)> {
    let lines = reference_lines(text);
    let line_of = |byte: usize| {
        lines
            .iter()
            .position(|line| byte <= line.text.end || byte < line.next)
            .unwrap_or(lines.len() - 1)
    };
    // Each group: its first and last lines, and its first and last blocks.
    let mut groups: Vec<(usize, usize, usize, usize)> = Vec::new();
    for (index, block) in document.blocks.iter().enumerate() {
        let mut last = block.source.end.min(text.len());
        while last > block.source.start && matches!(text.as_bytes()[last - 1], b'\n' | b'\r') {
            last -= 1;
        }
        let mut group = (
            line_of(block.source.start),
            line_of(last.saturating_sub(1).max(block.source.start)),
            index,
            index,
        );
        // Join every group that shares a line with this one.
        while let Some(shared) = groups
            .iter()
            .position(|other| other.0 <= group.1 && group.0 <= other.1)
        {
            let other = groups.remove(shared);
            group = (
                group.0.min(other.0),
                group.1.max(other.1),
                group.2.min(other.2),
                group.3.max(other.3),
            );
        }
        groups.push(group);
    }
    groups.sort_unstable();
    groups
        .into_iter()
        .map(|(first, last, from, to)| {
            (lines[first].text.start..lines[last].text.end, from..to + 1)
        })
        .collect()
}

/// Return whether `at` lies between the CR and the LF of a CRLF pair.
fn splits_crlf(text: &str, at: usize) -> bool {
    at > 0 && text.as_bytes().get(at - 1) == Some(&b'\r') && text.as_bytes().get(at) == Some(&b'\n')
}

/// Return whether `at` is a byte where a line starts.
fn line_start(text: &str, at: usize) -> bool {
    at == 0 || matches!(text.as_bytes()[at - 1], b'\n' | b'\r') && !splits_crlf(text, at)
}

/// Return whether `at` is a byte where the text of a line ends.
fn line_end(text: &str, at: usize) -> bool {
    at == text.len() || matches!(text.as_bytes()[at], b'\n' | b'\r') && !splits_crlf(text, at)
}

/// Check that `range` lies in `text` on character boundaries and splits no
/// CRLF pair.
fn check_range(text: &str, range: &Range<usize>, what: &str) -> Result<(), String> {
    if range.start > range.end || range.end > text.len() {
        return Err(format!(
            "{what} {range:?} is not a range in {} bytes",
            text.len()
        ));
    }
    for at in [range.start, range.end] {
        if !text.is_char_boundary(at) {
            return Err(format!(
                "{what} {range:?} is off a character boundary at {at}"
            ));
        }
        if splits_crlf(text, at) {
            return Err(format!("{what} {range:?} splits a CRLF pair at {at}"));
        }
    }
    Ok(())
}

/// Check the invariants of the edit units of `text`, and compare their
/// ranges with a slow reference. Return a description of the first failure.
///
/// Units lie in source order and do not overlap. Each starts at a line
/// start and ends at the end of a line's text, on character boundaries,
/// with no CRLF pair split. Each block lies in exactly one unit. The own
/// text of each unit lies inside it. [`unit_at`] returns the unit that holds
/// a byte, else the next, else the one before. A replace of each own text
/// changes only that text, and parses to valid units.
pub fn check_units(text: &str) -> Result<(), String> {
    let document = parse(text);
    let units = edit_units(&document);
    check_unit_list(text, &document, &units)?;
    for (index, unit) in units.iter().enumerate() {
        if unit.own.is_none() {
            continue;
        }
        for replacement in ["", "one\ntwo"] {
            edit(text, &units, index, replacement, true)?;
        }
    }
    Ok(())
}

/// Check `units`, the edit units of `document`, the parse of `text`, without
/// any edit.
fn check_unit_list(
    text: &str,
    document: &DocumentSnapshot,
    units: &[EditUnit],
) -> Result<(), String> {
    let mut covered = 0;
    for (index, unit) in units.iter().enumerate() {
        let range = &unit.range;
        check_range(text, range, "unit")?;
        if !line_start(text, range.start) {
            return Err(format!("unit {index} {range:?} starts inside a line"));
        }
        if !line_end(text, range.end) {
            return Err(format!("unit {index} {range:?} ends inside a line"));
        }
        if let Some(next) = units.get(index + 1)
            && next.range.start <= range.end
        {
            return Err(format!(
                "units {index} and {} overlap or are out of order",
                index + 1
            ));
        }
        if unit.blocks.start != covered || unit.blocks.is_empty() {
            return Err(format!(
                "unit {index} holds blocks {:?} after {covered}",
                unit.blocks
            ));
        }
        covered = unit.blocks.end;
        if (unit.blocks.len() > 1) != (unit.kind == EditKind::Merged) {
            return Err(format!(
                "unit {index} of {} blocks is {:?}",
                unit.blocks.len(),
                unit.kind
            ));
        }
        for block in &document.blocks[unit.blocks.clone()] {
            let source = &block.source;
            let end = source.start + text[source.clone()].trim_end_matches(['\n', '\r']).len();
            if source.start < range.start || end > range.end {
                return Err(format!(
                    "block {source:?} lies outside unit {index} {range:?}"
                ));
            }
        }
        if let Some(own) = &unit.own {
            check_range(text, own, "own text")?;
            if own.start < range.start || own.end > range.end {
                return Err(format!(
                    "own text {own:?} lies outside unit {index} {range:?}"
                ));
            }
        }
    }
    if covered != document.blocks.len() {
        return Err(format!(
            "units hold {covered} of {} blocks",
            document.blocks.len()
        ));
    }
    let reference = reference_units(text, document);
    let found: Vec<_> = units
        .iter()
        .map(|unit| (unit.range.clone(), unit.blocks.clone()))
        .collect();
    if found != reference {
        return Err(format!(
            "units {found:?} differ from the reference {reference:?}"
        ));
    }
    // Every byte for a short text; a sample for a long one.
    let step = (text.len() / 512).max(1);
    for byte in (0..=text.len()).step_by(step).chain([text.len()]) {
        let expected = units
            .iter()
            .position(|unit| unit.range.start <= byte && byte <= unit.range.end)
            .or_else(|| units.iter().position(|unit| unit.range.start > byte))
            .or_else(|| units.len().checked_sub(1));
        let found = unit_at(units, byte);
        if found != expected {
            return Err(format!("unit_at({byte}) is {found:?}, not {expected:?}"));
        }
    }
    Ok(())
}

/// Check one edit of `text`: put `replacement` in place of unit `index`, or
/// of its own text when `own` is set. Return a description of the first
/// failure, or `Ok` when the unit does not exist or has no own text.
///
/// The bytes before the unit do not change. A replace of own text changes
/// only bytes of the unit, and none after it. The units of the new text keep
/// their invariants. A unit replaced with its own bytes gives the same text
/// and the same units.
pub fn check_edit(text: &str, index: usize, replacement: &str, own: bool) -> Result<(), String> {
    edit(text, &parse_units(text), index, replacement, own)
}

/// Check one edit of `text`, whose units are `units`, as [`check_edit`] does.
fn edit(
    text: &str,
    units: &[EditUnit],
    index: usize,
    replacement: &str,
    own: bool,
) -> Result<(), String> {
    let Some(unit) = units.get(index) else {
        return Ok(());
    };
    let (range, new) = if own {
        let Some(splice) = replace_own_text(text, unit, replacement) else {
            return Ok(());
        };
        if Some(&splice.range) != unit.own.as_ref() {
            return Err(format!(
                "a splice of {:?} replaces {:?}",
                unit.own, splice.range
            ));
        }
        (splice.range.clone(), splice.apply(text))
    } else {
        let range = unit.range.clone();
        let mut new = text.to_owned();
        new.replace_range(range.clone(), replacement);
        (range, new)
    };
    if new[..unit.range.start] != text[..unit.range.start] {
        return Err(format!("an edit of unit {index} changed bytes before it"));
    }
    if own && new[new.len() - (text.len() - range.end)..] != text[range.end..] {
        return Err(format!(
            "a replace of the own text of unit {index} changed bytes after it"
        ));
    }
    let document = parse(&new);
    check_unit_list(&new, &document, &edit_units(&document))
        .map_err(|error| format!("after an edit of unit {index} to {new:?}: {error}"))?;
    let mut same = text.to_owned();
    same.replace_range(unit.range.clone(), &text[unit.range.clone()]);
    if same != text || parse_units(&same).as_slice() != units {
        return Err(format!(
            "unit {index} replaced with its own bytes changed the units"
        ));
    }
    Ok(())
}

/// Check one edit of `text` through a reader, as `Markdown::apply_edit` makes
/// it: `replacement` in place of bytes `from..to`. Return a description of
/// the first failure. [`EditCheck::check`] says what it checks.
pub fn check_apply_edit(
    text: &str,
    from: usize,
    to: usize,
    replacement: &str,
) -> Result<(), String> {
    EditCheck::new()?.check(text, from, to, replacement)
}

/// A reader in a harness that checks direct edits, one text after another.
/// Each check puts its text in the reader as a new document, so the checks
/// of one reader give the results of fresh ones, and a fuzz target pays for
/// the application once.
pub struct EditCheck {
    /// The harness, whose root is the reader.
    harness: Harness,
    /// The reader's preparation work.
    work: Schedule,
    /// The reader's node.
    reader: NodeId,
}

impl EditCheck {
    /// Make a reader that rests in the block cursor, with a manual executor
    /// and a manual clock.
    pub fn new() -> Result<Self, String> {
        let (executor, manual) = MarkdownExecutor::manual(ExecutorOptions {
            worker_count: 1,
            ..ExecutorOptions::default()
        });
        let options = MarkdownOptions {
            executor,
            resting_mode: Mode::Interact,
            status_row: false,
            coalesce: Duration::ZERO,
            ..MarkdownOptions::default()
        };
        let reader = Markdown::new(String::new(), options).map_err(|error| error.to_string())?;
        let clock = Arc::new(ManualClock::new());
        let harness = Harness::builder(reader)
            .register::<Markdown>()
            .script("markdown-defaults", "markdown.default_bindings()")
            .clock(clock.clone())
            .size(40, 12)
            .build()
            .map_err(|error| error.to_string())?;
        let reader = harness
            .find_nodes("**/markdown")
            .map_err(|error| error.to_string())?[0];
        Ok(Self {
            harness,
            work: Schedule::new(manual, clock),
            reader,
        })
    }

    /// Run the reader's work until it shows its text.
    fn settle(&mut self, what: &str) -> Result<(), String> {
        if self
            .work
            .pump(&mut self.harness, &[self.reader])
            .map_err(|error| error.to_string())?
        {
            Ok(())
        } else {
            Err(format!("the reader did not show {what}"))
        }
    }

    /// Run `f` on the reader.
    fn reader<R>(
        &mut self,
        f: impl FnOnce(&mut Markdown, &mut dyn Context) -> CanopyResult<R>,
    ) -> Result<R, String> {
        self.harness
            .with_root_widget_context(f)
            .map_err(|error| error.to_string())
    }

    /// Check one edit of `text`: `replacement` in place of bytes `from..to`.
    /// Return a description of the first failure.
    ///
    /// The reader refuses an edit of another epoch, and a range that is
    /// reversed, past the end, off character boundaries, or across a CRLF
    /// pair, and keeps its text. It takes any other edit: its text becomes
    /// the splice, and it records the edit for the host. Once the new text
    /// shows, the block cursor marks the unit that holds `from`, or none in a
    /// text without units. The units of the new text keep their invariants.
    pub fn check(
        &mut self,
        text: &str,
        from: usize,
        to: usize,
        replacement: &str,
    ) -> Result<(), String> {
        self.reader(|reader, ctx| {
            reader.take_edits();
            reader
                .replace(ctx, DocumentInput::Complete(text.to_owned()))
                .map(|_| ())
        })?;
        self.settle("the text")?;
        let edit = |check: &mut Self, epoch: u64| {
            check.reader(|reader, ctx| {
                Ok(reader.apply_edit(ctx, epoch, from, to, replacement.to_owned()))
            })
        };
        let epoch = self.reader(|reader, _| Ok(reader.epoch()))?;
        if edit(self, epoch + 1)?.is_ok() {
            return Err("the reader took an edit of another epoch".into());
        }
        let splits = |at: usize| {
            at > 0
                && text.as_bytes().get(at - 1) == Some(&b'\r')
                && text.as_bytes().get(at) == Some(&b'\n')
        };
        let valid = from <= to
            && text.is_char_boundary(from)
            && text.is_char_boundary(to)
            && !splits(from)
            && !splits(to);
        let taken = edit(self, epoch)?;
        let (source, edits) =
            self.reader(|reader, _| Ok((reader.accepted_source(), reader.take_edits())))?;
        if !valid {
            if taken.is_ok() {
                return Err(format!("the reader took the range {from}..{to}"));
            }
            if *source != *text || !edits.is_empty() {
                return Err(format!("a refused edit of {from}..{to} changed the text"));
            }
            return Ok(());
        }
        taken.map_err(|error| format!("the reader refused {from}..{to}: {error}"))?;
        let mut expected = text.to_owned();
        expected.replace_range(from..to, replacement);
        if *source != *expected {
            return Err(format!("the text is {source:?}, not {expected:?}"));
        }
        let recorded = BlockEdit {
            epoch,
            range: from..to,
            text: replacement.to_owned(),
        };
        if edits != [recorded] {
            return Err(format!("the reader recorded {edits:?}"));
        }
        self.settle("the edited text")?;
        let units = parse_units(&expected);
        let wanted = unit_at(&units, from).map(|index| units[index].range.clone());
        let marked = self
            .reader(|reader, _| Ok(reader.selected_block()))?
            .map(|unit| unit.range);
        if marked != wanted {
            return Err(format!(
                "the cursor marks {marked:?}, not the unit {wanted:?} that holds byte {from}"
            ));
        }
        check_units(&expected).map_err(|error| format!("after the edit to {expected:?}: {error}"))
    }
}

/// One step of the preparation work of the readers in a harness, which share
/// one manual executor and run under a manual clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleStep {
    /// Run one turn of the event loop. The readers that woke or whose polls
    /// are due poll: each submits its work, and takes and acknowledges a
    /// finished result.
    Turn,
    /// Start the next queued preparation, and hold it.
    Start,
    /// Finish the held preparation at this index, modulo their number.
    Finish(usize),
    /// Reject the next submission of preparation work. The reader retries
    /// once the clock passes its retry deadline.
    Reject,
    /// Move the clock by this many milliseconds.
    Clock(u64),
}

/// Make the steps of a legal schedule: any order of turns, starts, finishes
/// of held work, rejected submissions, and clock moves.
pub fn schedule(steps: Range<usize>) -> impl Strategy<Value = Vec<ScheduleStep>> {
    vec(
        prop_oneof![
            4 => Just(ScheduleStep::Turn),
            3 => Just(ScheduleStep::Start),
            3 => (0..4usize).prop_map(ScheduleStep::Finish),
            1 => Just(ScheduleStep::Reject),
            1 => (1..80u64).prop_map(ScheduleStep::Clock),
        ],
        steps,
    )
}

/// The preparation work of the readers in a harness, under test control: no
/// step waits on the wall clock.
pub struct Schedule {
    /// The executor the readers share.
    manual: ManualExecutor,
    /// The harness's clock.
    clock: Arc<ManualClock>,
    /// Preparations started and not finished, in the order they started.
    held: Vec<HeldWork>,
}

/// The rounds a pump runs at most before it reports that the readers did not
/// settle.
const PUMP_ROUNDS: usize = 64;

impl Schedule {
    /// Control the work of `manual`'s readers, in a harness that runs under
    /// `clock`.
    pub fn new(manual: ManualExecutor, clock: Arc<ManualClock>) -> Self {
        Self {
            manual,
            clock,
            held: Vec::new(),
        }
    }

    /// Return the number of held preparations.
    pub fn held(&self) -> usize {
        self.held.len()
    }

    /// Run one step.
    pub fn step(&mut self, harness: &mut Harness, step: ScheduleStep) -> CanopyResult<()> {
        match step {
            ScheduleStep::Turn => {
                harness.canopy.turn(TurnInput::Wake)?;
            }
            ScheduleStep::Start => {
                if let Some(work) = self.manual.start_next() {
                    self.held.push(work);
                }
            }
            ScheduleStep::Finish(index) => {
                if !self.held.is_empty() {
                    let work = self.held.remove(index % self.held.len());
                    work.finish();
                }
            }
            ScheduleStep::Reject => self.manual.reject_next_submission(),
            ScheduleStep::Clock(millis) => self.clock.advance(Duration::from_millis(millis))?,
        }
        Ok(())
    }

    /// Run turns, finish held and queued work in order, and move the clock
    /// past retry deadlines, until every reader of `readers` is ready, at most
    /// a bounded number of rounds. Then render the harness. Return whether
    /// the readers settled.
    pub fn pump(&mut self, harness: &mut Harness, readers: &[NodeId]) -> CanopyResult<bool> {
        for _ in 0..PUMP_ROUNDS {
            for work in self.held.drain(..) {
                work.finish();
            }
            harness.canopy.turn(TurnInput::Wake)?;
            while let Some(work) = self.manual.start_next() {
                work.finish();
            }
            harness.canopy.turn(TurnInput::Wake)?;
            let ready = readers.iter().try_fold(true, |ready, node| {
                harness.canopy.with_root_context(|ctx| {
                    ctx.with_widget(*node, |reader: &Markdown| Ok(ready && reader.ready()))
                })
            })?;
            if ready {
                harness.canopy.turn(TurnInput::Wake)?;
                harness.render()?;
                return Ok(true);
            }
            self.clock.advance(Duration::from_millis(100))?;
        }
        harness.render()?;
        Ok(false)
    }
}
