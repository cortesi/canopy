use proptest::prelude::*;

use super::*;
use crate::{
    document::{ParseLimits, parse_document},
    source_store::SourceStore,
    testing::{self, check_edit, check_units, document, hostile_text, spec_examples},
};

/// The kinds of the units of `text`.
fn kinds(text: &str) -> Vec<EditKind> {
    parse_units(text).iter().map(|unit| unit.kind).collect()
}

/// The text of each unit of `text`.
fn texts(text: &str) -> Vec<&str> {
    parse_units(text)
        .iter()
        .map(|unit| &text[unit.range.clone()])
        .collect()
}

/// The own text of each unit of `text`.
fn owns(text: &str) -> Vec<Option<&str>> {
    parse_units(text)
        .iter()
        .map(|unit| unit.own.clone().map(|own| &text[own]))
        .collect()
}

#[test]
fn units_hold_whole_lines_of_each_block() {
    let text = "# Timing\n\n- Ship the beta\n- Tell them\n\n> First line\n> second line\n\n```rust\nfn a() {}\n```\n\n| a | b |\n| - | - |\n| 1 | 2 |\n";
    assert_eq!(
        texts(text),
        [
            "# Timing",
            "- Ship the beta",
            "- Tell them",
            "> First line\n> second line",
            "```rust\nfn a() {}\n```",
            "| a | b |\n| - | - |\n| 1 | 2 |",
        ]
    );
    assert_eq!(
        kinds(text),
        [
            EditKind::AtxHeading,
            EditKind::ListItem,
            EditKind::ListItem,
            EditKind::Paragraph,
            EditKind::FencedCode,
            EditKind::Table,
        ]
    );
}

#[test]
fn units_end_before_the_terminator_of_their_last_line() {
    assert_eq!(texts("a\r\nb\r\n\r\nc"), ["a\r\nb", "c"]);
    assert_eq!(texts("para  \n"), ["para  "]);
    assert_eq!(texts("> a\n> b"), ["> a\n> b"]);
}

#[test]
fn text_without_blocks_has_no_units() {
    assert!(parse_units("").is_empty());
    assert!(parse_units("\n\n").is_empty());
    assert!(parse_units("[a]: https://a.test\n\n[b]: /b\n").is_empty());
    // A lone block id names the block before it, and has no unit itself.
    assert_eq!(texts("para\n\n^lone\n"), ["para"]);
}

#[test]
fn each_kind_of_block_makes_its_kind_of_unit() {
    let cases: &[(&str, EditKind, Option<&str>)] = &[
        ("para\n", EditKind::Paragraph, Some("para")),
        ("- item\n", EditKind::ListItem, Some("item")),
        ("- [ ] task\n", EditKind::ListItem, Some("task")),
        ("[^1]: note\n", EditKind::Footnote, Some("note")),
        ("## Head ##  \n", EditKind::AtxHeading, Some("Head")),
        ("#\n", EditKind::AtxHeading, Some("")),
        ("Set\next\n===\n", EditKind::SetextHeading, Some("Set\next")),
        ("#foo\n---\n", EditKind::SetextHeading, Some("#foo")),
        (
            "> [!note]- Title  \n",
            EditKind::CalloutTitle,
            Some("Title"),
        ),
        ("> [!TIP]\n", EditKind::CalloutTitle, Some("")),
        (
            "```rust\na\n\nb\n```\n",
            EditKind::FencedCode,
            Some("a\n\nb"),
        ),
        ("```\n```\n", EditKind::FencedCode, Some("")),
        (
            "    a\n      b\n",
            EditKind::IndentedCode,
            Some("a\n      b"),
        ),
        ("| a |\n| - |\n", EditKind::Table, None),
        ("---\ntitle: A\n---\n", EditKind::FrontMatter, None),
        ("---\n- not simple\n---\n", EditKind::FrontMatter, None),
        ("***\n", EditKind::Rule, None),
        ("<div>\nx\n</div>\n", EditKind::Html, None),
        ("![alt](a.png)\n", EditKind::Image, None),
        ("![[note]]\n", EditKind::Embed, None),
    ];
    for (text, kind, own) in cases {
        assert_eq!(kinds(text), [*kind], "{text:?}");
        assert_eq!(owns(text), [*own], "{text:?}");
    }
}

#[test]
fn blocks_that_share_a_line_merge_into_one_unit() {
    // An inline footnote's note becomes a block of its own on the line of
    // the paragraph that holds it.
    let text = "Text with a note.^[An inline note.]\n\nAfter.\n";
    let units = parse_units(text);
    assert_eq!(units[0].kind, EditKind::Merged, "{units:?}");
    assert_eq!(units[0].own, None);
    assert_eq!(units[0].blocks.len(), 2);
    assert!(replace_own_text(text, &units[0], "x").is_none());
    check_units(text).unwrap();
}

#[test]
fn unit_at_takes_the_holder_else_the_next_else_the_one_before() {
    let text = "a\n\nb\n\n";
    let units = parse_units(text);
    assert_eq!(texts(text), ["a", "b"]);
    // In "a", at its end, on the blank line, in "b", and past it.
    for (byte, unit) in [(0, 0), (1, 0), (2, 1), (3, 1), (4, 1), (6, 1), (60, 1)] {
        assert_eq!(unit_at(&units, byte), Some(unit), "{byte}");
    }
    assert_eq!(unit_at(&[], 0), None);
}

/// Apply a replace of the own text of unit `index` of `before`.
fn replaced(before: &str, index: usize, replacement: &str) -> String {
    let units = parse_units(before);
    replace_own_text(before, &units[index], replacement)
        .unwrap_or_else(|| panic!("unit {index} of {before:?} has no own text"))
        .apply(before)
}

#[test]
fn a_replace_of_own_text_keeps_the_structure_of_its_block() {
    // The source before, the unit, the replacement, and the source after.
    let cases: &[(&str, usize, &str, &str)] = &[
        // Paragraphs: new lines take the prefix of continuation lines.
        ("one\n", 0, "two", "two\n"),
        ("one", 0, "two\nthree", "two\nthree"),
        ("a\n", 0, "x\n\n", "x\n"),
        ("> a\n> b\n", 0, "x\ny", "> x\n> y\n"),
        ("> a\nlazy\n", 0, "x\ny\nz", "> x\n> y\n> z\n"),
        ("> > a\n", 0, "x\ny", "> > x\n> > y\n"),
        ("> - a\n", 0, "x\ny", "> - x\n>   y\n"),
        ("a ^id\n", 0, "x", "x ^id\n"),
        ("> a\r\n> b\r\n", 0, "x\ny", "> x\r\n> y\r\n"),
        ("> a\r\n", 0, "x\r\ny", "> x\r\n> y\r\n"),
        (">a\n", 0, "x", ">x\n"),
        ("> a\r> b", 0, "x\ny", "> x\r> y"),
        ("a\nb\n", 0, "", "\n"),
        // List items: new lines line up under the text.
        ("- a\n- b\n", 1, "x\ny", "- a\n- x\n  y\n"),
        ("10. a\n", 0, "x\ny", "10. x\n    y\n"),
        ("- [ ] a\n", 0, "x\ny", "- [ ] x\n      y\n"),
        ("- [x]\n", 0, "x", "- [x] x\n"),
        ("- a\n\n- b\n", 1, "x\ny", "- a\n\n- x\n  y\n"),
        ("- a\n\n  b\n", 1, "x\ny", "- a\n\n  x\n  y\n"),
        ("-\ta\n", 0, "x\ny", "-\tx\n \ty\n"),
        // Footnotes: the label becomes blanks.
        ("[^1]: a\n", 0, "x\ny", "[^1]: x\n      y\n"),
        ("[^1]: > a\n", 0, "x\ny", "[^1]: > x\n      > y\n"),
        ("[^a>b]: a\n", 0, "x\ny", "[^a>b]: x\n        y\n"),
        // ATX headings: line breaks become spaces, and marks stay apart.
        ("# a\n", 0, "x\ny", "# x y\n"),
        ("## a ##\n", 0, "x", "## x ##\n"),
        ("#\n", 0, "x", "# x\n"),
        ("# #\n", 0, "x", "# x #\n"),
        ("> # a\n", 0, "b\n", "> # b\n"),
        ("# a\r\n", 0, "x\r\ny", "# x y\r\n"),
        // Setext headings: the text lines go, and the underline stays.
        ("a\nb\n===\n", 0, "x\ny", "x y\n===\n"),
        ("> a\n> b\n> ---\n", 0, "x", "> x\n> ---\n"),
        // A callout of only its title: the title is its text.
        ("> [!note] Title\n", 0, "New\nline", "> [!note] New line\n"),
        ("> [!tip]\n", 0, "x", "> [!tip] x\n"),
        ("> [!note]- Folded\n", 0, "x", "> [!note]- x\n"),
        // Fenced code: new lines take the prefix of the code lines.
        ("```\na\nb\n```\n", 0, "x\ny", "```\nx\ny\n```\n"),
        ("```\na\n```\n", 0, "x\n", "```\nx\n\n```\n"),
        ("> ```\n> a\n> ```\n", 0, "x\ny", "> ```\n> x\n> y\n> ```\n"),
        ("- ```\n  a\n  ```\n", 0, "x\ny", "- ```\n  x\n  y\n  ```\n"),
        ("```\n```\n", 0, "x", "```\nx\n```\n"),
        ("> ```\n> ```\n", 0, "x\ny", "> ```\n> x\n> y\n> ```\n"),
        ("```", 0, "x", "```\nx"),
        ("```\n```\n", 0, "", "```\n```\n"),
        ("```\n\n```\n", 0, "x", "```\nx\n```\n"),
        ("```\r\na\r\n```\r\n", 0, "x\ny", "```\r\nx\r\ny\r\n```\r\n"),
        // Indented code: new lines take the indent.
        ("    a\n    b\n", 0, "x\ny", "    x\n    y\n"),
        (">     a\n", 0, "x\ny", ">     x\n>     y\n"),
        (
            "- a\n\n      code\n",
            1,
            "x\ny",
            "- a\n\n      x\n      y\n",
        ),
        ("\tcode", 0, "x\ny", "\tx\n\ty"),
    ];
    for (before, index, replacement, after) in cases {
        assert_eq!(
            replaced(before, *index, replacement),
            *after,
            "{before:?} unit {index} with {replacement:?}"
        );
        check_edit(before, *index, replacement, true).unwrap();
    }
}

#[test]
fn a_replace_takes_the_terminator_of_its_unit() {
    // A unit of one line takes the terminator after it, and a unit at the
    // end of the text takes the first terminator of the text.
    assert_eq!(replaced("a\n\nb\r\n", 1, "x\ny"), "a\n\nx\r\ny\r\n");
    assert_eq!(replaced("a\r\n\r\nb", 1, "x\ny"), "a\r\n\r\nx\r\ny");
    assert_eq!(replaced("a", 0, "x\ny"), "x\ny");
}

#[test]
fn own_text_moves_inside_its_lines_and_onto_characters() {
    let text = "aé\nb";
    // Byte 2 lies inside "é", which starts at byte 1.
    assert_eq!(inside(text, 2..9, &(0..3)), 1..3);
    assert_eq!(inside(text, 5..9, &(0..3)), 3..3);
    assert_eq!(inside(text, 0..0, &(1..3)), 1..1);
}

#[test]
fn a_block_of_no_bytes_takes_its_whole_line() {
    let store = SourceStore::static_text("a\n\nb\n").unwrap();
    let mut document = parse_document(&store.snapshot(), &ParseLimits::default(), None);
    document.blocks[1].source = 3..3;
    let units = edit_units(&document);
    assert_eq!(units[1].range, 3..4);
}

#[test]
fn units_without_own_text_have_no_replace() {
    for text in [
        "| a |\n| - |\n",
        "***\n",
        "<div>\nx\n</div>\n",
        "---\ntitle: A\n---\n",
        "![alt](a.png)\n",
    ] {
        let units = parse_units(text);
        assert_eq!(units.len(), 1, "{text:?}");
        assert!(replace_own_text(text, &units[0], "x").is_none(), "{text:?}");
    }
}

#[test]
fn an_edit_can_change_the_kinds_of_the_units_before_it() {
    // Markdown is not local: an underline under a paragraph makes it a
    // heading. Only the bytes before the unit stay as they were.
    let text = "one\n# two\n";
    assert_eq!(kinds(text), [EditKind::Paragraph, EditKind::AtxHeading]);
    let units = parse_units(text);
    let mut after = text.to_owned();
    after.replace_range(units[1].range.clone(), "===\n");
    assert_eq!(after, "one\n===\n\n");
    assert_eq!(kinds(&after), [EditKind::SetextHeading]);
    check_edit(text, 1, "===\n", false).unwrap();
}

#[test]
fn the_spec_corpus_is_whole() {
    let examples = spec_examples().unwrap();
    assert!(examples.len() > 1000, "{} examples", examples.len());
}

#[test]
fn spec_examples_keep_the_unit_invariants_0() {
    check_spec_shard(&spec_examples().unwrap(), 0);
}

#[test]
fn spec_examples_keep_the_unit_invariants_1() {
    check_spec_shard(&spec_examples().unwrap(), 1);
}

#[test]
fn spec_examples_keep_the_unit_invariants_2() {
    check_spec_shard(&spec_examples().unwrap(), 2);
}

#[test]
fn spec_examples_keep_the_unit_invariants_3() {
    check_spec_shard(&spec_examples().unwrap(), 3);
}

/// The shards that the spec examples split into, so the checks of a shard
/// run in a test of their own.
const SPEC_SHARDS: usize = 4;

/// Check every spec example whose number is `shard` modulo the shards: the
/// invariants of its units, and a hostile edit of each unit.
fn check_spec_shard(examples: &[String], shard: usize) {
    let hostile = ["```", "a\n\nb", "- x", "==="];
    for (number, example) in examples.iter().enumerate().skip(shard).step_by(SPEC_SHARDS) {
        check_units(example).unwrap_or_else(|error| panic!("example {number}: {error}"));
        let count = parse_units(example).len();
        for index in 0..count {
            let replacement = hostile[(number + index) % hostile.len()];
            check_edit(example, index, replacement, false)
                .unwrap_or_else(|error| panic!("example {number}: {error}"));
        }
    }
}

/// The source after a new block of `typed` after unit `index` of `text`.
fn appended(text: &str, index: usize, typed: &str) -> String {
    let document = testing::parse(text);
    let block = NewBlock::after(&document, &edit_units(&document)[index]);
    let mut out = text.to_owned();
    out.insert_str(block.at, &block.text(typed));
    out
}

#[test]
fn a_new_block_lies_in_the_containers_of_the_unit_before_it() {
    let cases: &[(&str, usize, &str, &str)] = &[
        ("a\n\nb\n", 0, "new", "a\n\nnew\n\nb\n"),
        ("a\n\nb\n", 1, "new", "a\n\nb\n\nnew\n"),
        ("a", 0, "new", "a\n\nnew"),
        // A quote keeps its markers, and its blank lines take them too.
        ("> a\n> b\n", 0, "x\n\ny", "> a\n> b\n>\n> x\n>\n> y\n"),
        // A list item takes blanks in place of its marker. A line that
        // follows the unit takes a blank line, so it stays out of the block.
        ("- a\n- b\n", 0, "new", "- a\n\n  new\n\n- b\n"),
        ("> - a\n", 0, "new", "> - a\n>\n>   new\n"),
        // The marks of a heading and the indent of code are no container.
        ("# H\npara\n", 0, "new", "# H\n\nnew\n\npara\n"),
        ("    code\n", 0, "new", "    code\n\nnew\n"),
        (
            "- a\n\n      code\n",
            1,
            "new",
            "- a\n\n      code\n\n  new\n",
        ),
        // CRLF text takes CRLF, and breaks at the end of the text drop.
        (
            "a\r\n\r\nb\r\n",
            0,
            "x\ny\n\n",
            "a\r\n\r\nx\r\ny\r\n\r\nb\r\n",
        ),
        // A blank text adds nothing.
        ("a\n", 0, " \n\t", "a\n"),
    ];
    for (text, index, typed, expected) in cases {
        assert_eq!(&appended(text, *index, typed), expected, "{text:?}");
    }
}

#[test]
fn a_new_block_at_the_end_starts_after_a_blank_line() {
    for (source, lead) in [
        ("", ""),
        ("a", "\n\n"),
        ("a\n", "\n"),
        ("a\n\n", ""),
        ("a\r\n", "\r\n"),
    ] {
        let block = NewBlock::at_end(source);
        assert_eq!(block.at, source.len());
        assert_eq!(block.lead, lead, "{source:?}");
        assert_eq!(block.text("new"), format!("{lead}new"));
    }
}

/// The words of the new blocks of the checks.
const FRESH: &str = "Fresh words";

/// Check a new block of words after unit `index` of `text`, as
/// [`check_new_block_in`] does, with the invariants of the new text's units.
fn check_new_block(text: &str, index: usize) -> Result<(), String> {
    let document = testing::parse(text);
    check_new_block_in(text, &document, &edit_units(&document), index, true)
}

/// Check a new block of words after unit `index` of `text`, whose document
/// and units these are. The text around it does not change. It makes one
/// paragraph unit right after the unit, and every other unit keeps its kind
/// and its own text. A fence or raw HTML block that runs to the end of its
/// container takes the new block in instead, so that unit grows. With
/// `invariants`, the units of the new text keep their invariants too.
fn check_new_block_in(
    text: &str,
    document: &DocumentSnapshot,
    units: &[EditUnit],
    index: usize,
    invariants: bool,
) -> Result<(), String> {
    let Some(unit) = units.get(index) else {
        return Ok(());
    };
    let block = NewBlock::after(document, unit);
    let mut new = text.to_owned();
    new.insert_str(block.at, &block.text(FRESH));
    let fail = |what: &str| {
        Err(format!(
            "a new block after unit {index} of {text:?} {what}: {new:?}"
        ))
    };
    if block.at != unit.range.end {
        return fail("lies elsewhere");
    }
    if invariants {
        check_units(&new).map_err(|error| format!("{new:?}: {error}"))?;
    }
    let grown = parse_units(&new);
    let open = matches!(
        unit.kind,
        EditKind::FencedCode | EditKind::Html | EditKind::Source
    );
    if open
        && grown.iter().any(|after| {
            after.range.start == unit.range.start && new[after.range.clone()].contains(FRESH)
        })
    {
        return Ok(());
    }
    // A list item of a lone block id loses its own text when its list turns
    // loose, as the id then names the block before it.
    let shape = |text: &str, unit: &EditUnit| {
        let own = unit.own.clone().map(|own| text[own].to_owned());
        let id = own.as_deref().is_some_and(|own| {
            own.strip_prefix('^').is_some_and(|id| {
                !id.is_empty() && id.chars().all(|c| c.is_alphanumeric() || c == '-')
            })
        });
        (unit.kind, own.filter(|_| !id))
    };
    let before: Vec<_> = units.iter().map(|unit| shape(text, unit)).collect();
    let mut after: Vec<_> = grown.iter().map(|unit| shape(&new, unit)).collect();
    if after.len() != before.len() + 1 {
        return fail("changes the count of units");
    }
    // A paragraph in a footnote is a unit of the footnote's kind.
    let fresh = after.remove(index + 1);
    if !matches!(fresh.0, EditKind::Paragraph | EditKind::Footnote)
        || fresh.1.as_deref() != Some(FRESH)
    {
        return fail(&format!("makes {fresh:?}"));
    }
    if after != before {
        let changed: Vec<_> = before.iter().zip(&after).filter(|(a, b)| a != b).collect();
        return fail(&format!("changes another unit, {changed:?}"));
    }
    Ok(())
}

/// Spec examples where a new block after a unit joins other blocks, by their
/// corner cases: a comment across blocks, a footnote definition inside
/// another, and the continuation lines of a link definition.
const JOINING_EXAMPLES: [usize; 5] = [42, 116, 117, 416, 419];

/// Check a new block after each unit of every spec example whose number is
/// `shard` modulo the shards. The unit invariants of each new text have
/// checks of their own, in the hostile edits.
fn check_new_block_shard(shard: usize) {
    let mut failures = Vec::new();
    for (number, example) in spec_examples()
        .unwrap()
        .iter()
        .enumerate()
        .skip(shard)
        .step_by(SPEC_SHARDS)
    {
        if JOINING_EXAMPLES.contains(&number) {
            continue;
        }
        let document = testing::parse(example);
        let units = edit_units(&document);
        for index in 0..units.len() {
            if let Err(error) = check_new_block_in(example, &document, &units, index, false) {
                failures.push(format!("example {number}: {error}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_new_block_after_each_unit_of_the_spec_examples_stands_alone_0() {
    check_new_block_shard(0);
}

#[test]
fn a_new_block_after_each_unit_of_the_spec_examples_stands_alone_1() {
    check_new_block_shard(1);
}

#[test]
fn a_new_block_after_each_unit_of_the_spec_examples_stands_alone_2() {
    check_new_block_shard(2);
}

#[test]
fn a_new_block_after_each_unit_of_the_spec_examples_stands_alone_3() {
    check_new_block_shard(3);
}

proptest! {
    #![proptest_config(testing::config(128))]

    #[test]
    fn generated_documents_keep_the_unit_invariants(text in document()) {
        check_units(&text).map_err(TestCaseError::fail)?;
    }

    #[test]
    fn hostile_edits_keep_the_unit_invariants(
        text in document(),
        index in 0..8usize,
        replacement in hostile_text(),
        own in any::<bool>(),
    ) {
        check_edit(&text, index, &replacement, own).map_err(TestCaseError::fail)?;
    }

    #[test]
    fn a_new_block_after_any_unit_stands_alone(text in document(), index in 0..8usize) {
        check_new_block(&text, index).map_err(TestCaseError::fail)?;
    }
}
