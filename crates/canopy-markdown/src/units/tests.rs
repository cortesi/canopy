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
    let store = SourceStore::complete("a\n\nb\n").unwrap();
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

/// The seed of a new item after unit `index` of `text`, and the source with
/// the item of `typed` in place.
fn item_appended(text: &str, index: usize, typed: &str) -> (String, String) {
    let document = testing::parse(text);
    let block = NewBlock::item_after(&document, &edit_units(&document)[index])
        .unwrap_or_else(|| panic!("unit {index} of {text:?} is a list item"));
    let mut out = text.to_owned();
    out.insert_str(block.at, &block.text(typed));
    (block.seed, out)
}

#[test]
fn a_new_item_follows_its_item_at_the_same_level() {
    let cases: &[(&str, usize, &str, &str, &str)] = &[
        // A tight list takes the item on the next line.
        ("- a\n- b\n", 0, "- ", "- new", "- a\n- new\n- b\n"),
        ("- a\n- b\n", 1, "- ", "- new", "- a\n- b\n- new\n"),
        ("* a\n", 0, "* ", "* new", "* a\n* new\n"),
        // An ordered list takes the next number and the same delimiter.
        ("1. a\n2. b\n", 0, "2. ", "2. new", "1. a\n2. new\n2. b\n"),
        ("9) a\n", 0, "10) ", "10) new", "9) a\n10) new\n"),
        // A loose list sets the item apart with blank lines.
        ("- a\n\n- b\n", 0, "- ", "- new", "- a\n\n- new\n\n- b\n"),
        // The item follows the nested blocks of its item.
        (
            "- a\n  - x\n- b\n",
            0,
            "- ",
            "- new",
            "- a\n  - x\n- new\n- b\n",
        ),
        // A nested item stays nested, and a quoted one quoted.
        (
            "- a\n  - x\n- b\n",
            1,
            "- ",
            "- new",
            "- a\n  - x\n  - new\n- b\n",
        ),
        (
            "> - a\n> - b\n",
            0,
            "- ",
            "- new",
            "> - a\n> - new\n> - b\n",
        ),
        // A task takes an open box.
        (
            "- [x] done\n",
            0,
            "- [ ] ",
            "- [ ] next",
            "- [x] done\n- [ ] next\n",
        ),
        // Later lines take the indent of the item's text.
        (
            "- a\n- b\n",
            0,
            "- ",
            "- new\nmore",
            "- a\n- new\n  more\n- b\n",
        ),
        // The seed alone adds nothing.
        ("- a\n- b\n", 0, "- ", "- ", "- a\n- b\n"),
        ("- a\n- b\n", 0, "- ", "-   \n", "- a\n- b\n"),
    ];
    for (text, index, seed, typed, expected) in cases {
        let (got_seed, got) = item_appended(text, *index, typed);
        assert_eq!(&got_seed, seed, "{text:?} {index}");
        assert_eq!(&got, expected, "{text:?} {index} {typed:?}");
    }
}

#[test]
fn a_new_item_with_no_marker_is_a_paragraph_in_the_containers_of_its_item() {
    let cases: &[(&str, usize, &str, &str)] = &[
        // A paragraph after a top-level item splits the list.
        ("- a\n- b\n", 0, "para", "- a\n\npara\n\n- b\n"),
        ("- a\n- b\n", 1, "para", "- a\n- b\n\npara\n"),
        ("1. a\n2. b\n", 0, "one\ntwo", "1. a\n\none\ntwo\n\n2. b\n"),
        ("- a\n\n- b\n", 0, "para", "- a\n\npara\n\n- b\n"),
        // After a nested item, the paragraph lies in the item that holds it.
        (
            "- a\n  - x\n- b\n",
            1,
            "para",
            "- a\n  - x\n\n  para\n\n- b\n",
        ),
        // A quoted list keeps its quote.
        ("> - a\n", 0, "para", "> - a\n>\n> para\n"),
        // Text that only looks like a marker is a paragraph too.
        ("- a\n", 0, "*bold* text", "- a\n\n*bold* text\n"),
        ("- a\n", 0, "1.5 more", "- a\n\n1.5 more\n"),
        // An indented marker is still an item, and nothing adds nothing.
        ("- a\n", 0, "  - x", "- a\n  - x\n"),
        ("- a\n", 0, "", "- a\n"),
    ];
    for (text, index, typed, expected) in cases {
        let (_, got) = item_appended(text, *index, typed);
        assert_eq!(&got, expected, "{text:?} {index} {typed:?}");
    }
    // The paragraph parses as one, apart from the items around it.
    let (item, paragraph) = (EditKind::ListItem, EditKind::Paragraph);
    assert_eq!(kinds("- a\n\npara\n\n- b\n"), [item, paragraph, item]);
    assert_eq!(
        kinds("- a\n  - x\n\n  para\n\n- b\n"),
        [item, item, paragraph, item]
    );
}

#[test]
fn a_block_that_is_no_list_item_has_no_new_item() {
    let document = testing::parse("para\n\n# H\n");
    for unit in edit_units(&document) {
        assert_eq!(NewBlock::item_after(&document, &unit), None, "{unit:?}");
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

/// The source after a deletion of unit `index` of `text`.
fn deleted(text: &str, index: usize) -> String {
    let document = testing::parse(text);
    delete_unit(&document, &edit_units(&document), index)
        .unwrap()
        .apply(text)
}

#[test]
fn a_deletion_keeps_the_blocks_around_the_unit_apart() {
    let cases: &[(&str, usize, &str)] = &[
        // Of blank lines on both sides, those with fewer quote markers stay.
        ("a\n\nb\n\nc\n", 1, "a\n\nc\n"),
        ("> a\n>\n> b\n>\n> c\n", 1, "> a\n>\n> c\n"),
        ("> a\n>\n> b\n\nc\n", 1, "> a\n\nc\n"),
        ("> > a\n> >\n> > b\n>\n> c\n", 1, "> > a\n>\n> c\n"),
        // A blank line on one side alone stays.
        ("a\n\n---\nb\n", 1, "a\n\nb\n"),
        ("para\n# H\n\nnext\n", 1, "para\n\nnext\n"),
        ("- a\n- b\n- c\n", 1, "- a\n- c\n"),
        ("a\n1. b\n2. c\n", 1, "a\n\n2. c\n"),
        // The first and last units go with the blank lines beside them.
        ("a\n\nb\n", 0, "b\n"),
        ("a\n\nb\n", 1, "a\n"),
        ("a\n", 0, ""),
        // With no blank line on either side, a blank line takes its place.
        ("para\n# H\n    code\n", 1, "para\n\n    code\n"),
        ("> a\n> # H\n> b\n", 1, "> a\n>\n> b\n"),
        // Only the unit's own lines go, so a nested list stays.
        ("- a\n  - b\n", 0, "  - b\n"),
        ("a\r\n\r\nb\r\n\r\nc\r\n", 1, "a\r\n\r\nc\r\n"),
    ];
    for (text, index, expected) in cases {
        assert_eq!(&deleted(text, *index), expected, "{text:?} {index}");
    }
}

/// Check a deletion of unit `index` of `text`, whose document and units
/// these are: every other unit keeps its kind and its own text. The title
/// line of a callout whose only block goes becomes a unit of its own, so
/// the check leaves callout titles out.
///
/// Only the unit's own lines go, so blocks nested under it lose their
/// parent, and the check skips a unit whose next line of text has a deeper
/// indent. The spec's corner cases change the blocks around a deletion
/// in other ways too, such as a block that set two code blocks apart, so
/// the spec examples have no check of deletions. Fences and raw HTML blocks
/// have none either, as one may run to the end of its container.
fn check_delete_in(
    text: &str,
    document: &DocumentSnapshot,
    units: &[EditUnit],
    index: usize,
) -> Result<(), String> {
    let Some(unit) = units.get(index) else {
        return Ok(());
    };
    let indent = |at: usize| {
        let line = &text[at..];
        line.len() - line.trim_start_matches(['>', ' ', '\t']).len()
    };
    let next = next_text_start(text, unit.range.end);
    if next < text.len() && indent(next) > indent(unit.range.start) {
        return Ok(());
    }
    // A fence or raw HTML block may run to the end of its container, and
    // the text around it then reads otherwise without it.
    if matches!(
        unit.kind,
        EditKind::FencedCode | EditKind::Html | EditKind::Source
    ) {
        return Ok(());
    }
    let new = delete_unit(document, units, index)
        .ok_or("no splice")?
        .apply(text);
    // A paragraph that becomes the first block of its list item becomes a
    // unit of the item's kind.
    let titles = |unit: &&EditUnit| unit.kind != EditKind::CalloutTitle;
    let paragraph = |(kind, own): (EditKind, Option<String>)| {
        let kind = if kind == EditKind::ListItem {
            EditKind::Paragraph
        } else {
            kind
        };
        (kind, own)
    };
    let before: Vec<_> = units
        .iter()
        .enumerate()
        .filter(|(other, unit)| *other != index && titles(unit))
        .map(|(_, unit)| paragraph(shape(text, unit)))
        .collect();
    let after: Vec<_> = parse_units(&new)
        .iter()
        .filter(titles)
        .map(|unit| paragraph(shape(&new, unit)))
        .collect();
    if after != before {
        return Err(format!(
            "a deletion of unit {index} of {text:?} changes the other units, {before:?} to {after:?}: {new:?}"
        ));
    }
    Ok(())
}

/// The kind and the own text of `unit` of `text`. A list item of a lone
/// block id loses its own text when its list turns loose, as the id then
/// names the block before it, so such an id counts as no own text.
fn shape(text: &str, unit: &EditUnit) -> (EditKind, Option<String>) {
    let own = unit.own.clone().map(|own| text[own].to_owned());
    let id = own.as_deref().is_some_and(|own| {
        own.strip_prefix('^')
            .is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_alphanumeric() || c == '-'))
    });
    (unit.kind, own.filter(|_| !id))
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
    fn a_deletion_of_any_unit_keeps_the_others(text in document(), index in 0..8usize) {
        let document = testing::parse(&text);
        check_delete_in(&text, &document, &edit_units(&document), index).map_err(TestCaseError::fail)?;
    }

    #[test]
    fn a_new_block_after_any_unit_stands_alone(text in document(), index in 0..8usize) {
        check_new_block(&text, index).map_err(TestCaseError::fail)?;
    }
}
