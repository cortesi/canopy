//! Event-level tests of the fork's Obsidian syntax, for what HTML does not
//! show.
#![allow(missing_docs, clippy::tests_outside_test_module)]

use std::ops::Range;

use canopy_xmark::{Event, Options, Parser, Tag, TagEnd, hide_comments};

/// Parse `text` as Canopy and fh read it, with source ranges.
fn events(text: &str) -> Vec<(Event<'static>, Range<usize>)> {
    let hidden = hide_comments(text, Options::obsidian());
    Parser::new_ext(&hidden, Options::obsidian())
        .into_offset_iter()
        .map(|(event, range)| (event.into_static(), range))
        .collect()
}

/// Parse `text` without ranges.
fn plain(text: &str) -> Vec<Event<'static>> {
    events(text).into_iter().map(|(event, _)| event).collect()
}

#[test]
fn a_trailing_block_id_names_its_block() {
    assert_eq!(
        plain("Para one ^p1\n"),
        [
            Event::Start(Tag::Paragraph),
            Event::Text("Para one".into()),
            Event::BlockId("p1".into()),
            Event::End(TagEnd::Paragraph),
        ]
    );
    let source = "- item a ^li\n- item b\n";
    let events = events(source);
    let (_, range) = events
        .iter()
        .find(|(event, _)| matches!(event, Event::BlockId(_)))
        .expect("a block id");
    assert_eq!(&source[range.clone()], "^li");
    assert!(
        events
            .iter()
            .any(|(event, _)| *event == Event::Text("item a".into()))
    );
}

#[test]
fn a_paragraph_of_only_an_id_names_the_block_before_it() {
    assert_eq!(
        plain("| a |\n| - |\n| 1 |\n\n^tab\n")
            .into_iter()
            .skip_while(|event| *event != Event::End(TagEnd::Table))
            .collect::<Vec<_>>(),
        [Event::End(TagEnd::Table), Event::BlockId("tab".into())]
    );
}

#[test]
fn a_heading_id_leaves_out_its_block_id() {
    let events = plain("## Head ^h\n");
    assert!(
        matches!(&events[0], Event::Start(Tag::Heading { id: Some(id), .. }) if &**id == "head"),
        "{events:?}"
    );
    assert!(events.contains(&Event::BlockId("h".into())), "{events:?}");
}

#[test]
fn ids_in_code_links_and_words_are_text() {
    for text in [
        "x^y and ^bad!\n",
        "see `code ^c`\n",
        "a [link ^l](x)\n",
        "word^w\n",
    ] {
        assert!(
            !plain(text)
                .iter()
                .any(|event| matches!(event, Event::BlockId(_))),
            "{text:?}"
        );
    }
}

#[test]
fn tags_start_words_and_respect_escapes_and_code() {
    let tags = |text: &str| {
        plain(text)
            .into_iter()
            .filter_map(|event| match event {
                Event::Hashtag(name) => Some(name.into_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        tags("Tags: #fixture, #project/garden, and #to-do.\n"),
        ["fixture", "project/garden", "to-do"]
    );
    assert!(tags("C#, issue #1, \\#escaped, `#code`, and https://x.org/#top\n").is_empty());
    assert_eq!(tags("#start of a line\n"), ["start"]);
    assert_eq!(tags("#my_tag_ stays whole\n"), ["my_tag_"]);
}

#[test]
fn repeated_heading_ids_count_each_base_on_its_own() {
    let mut source = String::from("# A\n\n# A-1\n\n");
    for _ in 2..=32 {
        source.push_str("# A\n\n");
    }
    source.push_str("# A-1\n\n# A\n");
    let ids: Vec<String> = plain(&source)
        .into_iter()
        .filter_map(|event| match event {
            Event::Start(Tag::Heading { id, .. }) => id.map(|id| id.into_string()),
            _ => None,
        })
        .collect();
    let mut expected = vec!["a".to_owned(), "a-1".to_owned()];
    expected.extend((2..=32).map(|suffix| format!("a-{suffix}")));
    expected.extend(["a-1-1".to_owned(), "a-33".to_owned()]);
    assert_eq!(ids, expected);
}

#[test]
fn a_row_says_how_many_cells_it_dropped() {
    let source = "| A | B |\n| - | - |\n| a | b | c | d |\n| e |\n";
    let events = events(source);
    let (event, range) = events
        .iter()
        .find(|(event, _)| matches!(event, Event::IgnoredCells(_)))
        .expect("dropped cells");
    assert_eq!(*event, Event::IgnoredCells(2));
    assert_eq!(&source[range.clone()], " c | d |");
    assert_eq!(
        events
            .iter()
            .filter(|(event, _)| matches!(event, Event::IgnoredCells(_)))
            .count(),
        1
    );
}

#[test]
fn every_reference_definition_keeps_its_span() {
    let source = "[a]: one\n[a]: two\n\n> [b]: three\n";
    let parser = Parser::new_ext(source, Options::obsidian()).into_offset_iter();
    let spans: Vec<&str> = parser
        .reference_definitions()
        .spans()
        .iter()
        .map(|span| source[span.clone()].trim_end())
        .collect();
    assert_eq!(spans, ["[a]: one", "[a]: two", "[b]: three"]);
    assert_eq!(parser.reference_definitions().iter().count(), 2);
}

#[test]
fn a_parse_past_its_limits_stops_and_says_so() {
    use canopy_xmark::{DefaultParserCallbacks, Limits};

    let parse = |text: &str, limits| {
        let mut events =
            Parser::new_with_limits(text, Options::obsidian(), limits, DefaultParserCallbacks)
                .into_offset_iter();
        let count = events.by_ref().count();
        (count, events.limit_hit())
    };
    let long = "para\n\n".repeat(100_000);
    let (count, hit) = parse(
        &long,
        Limits {
            max_items: 10,
            max_depth: 64,
        },
    );
    assert!(hit && count <= 10, "{count}");
    let (_, hit) = parse(
        "> > > deep\n",
        Limits {
            max_items: 100,
            max_depth: 2,
        },
    );
    assert!(hit);
    let (count, hit) = parse(
        "a *b*\n",
        Limits {
            max_items: 6,
            max_depth: 2,
        },
    );
    assert!(!hit && count == 6, "{count}");
}

#[test]
fn a_resumed_parse_matches_a_whole_one_from_its_start() {
    use canopy_xmark::{DefaultParserCallbacks, Limits};

    let text = "# Intro\n\nSee [the site][site] and a note.[^n]\n\n[site]: https://example.com\n\n[^n]: The note.\n\n# Intro\n\n## Later\n\nAgain [site], again[^n], and # Intro.\n\n# Intro\n";
    let from = text.find("## Later").expect("the resume point");
    let whole: Vec<_> = Parser::new_ext(text, Options::obsidian())
        .into_offset_iter()
        .map(|(event, range)| (event.into_static(), range))
        .collect();
    let mut first = Parser::new_ext(text, Options::obsidian()).into_offset_iter();
    first.by_ref().for_each(drop);
    let resume = first.resume().before(from);
    let resumed: Vec<_> = Parser::new_resumed(
        text,
        Options::obsidian(),
        Limits::NONE,
        from,
        &resume,
        DefaultParserCallbacks,
    )
    .into_offset_iter()
    .map(|(event, range)| (event.into_static(), range))
    .collect();
    let tail: Vec<_> = whole
        .into_iter()
        .filter(|(_, range)| range.start >= from)
        .collect();
    assert_eq!(resumed, tail);
    assert!(
        resumed.iter().any(|(event, _)| matches!(
            event,
            Event::Start(Tag::Heading { id: Some(id), .. }) if &**id == "intro-2"
        )),
        "{resumed:?}"
    );
}

#[test]
fn an_inline_footnote_is_a_reference_and_a_footnote_at_the_end() {
    let events = plain("Text^[an *inline* note] and^[two].\n\nAfter.\n");
    assert_eq!(
        events,
        [
            Event::Start(Tag::Paragraph),
            Event::Text("Text".into()),
            Event::FootnoteReference("inline-1".into()),
            Event::Text(" and".into()),
            Event::FootnoteReference("inline-2".into()),
            Event::Text(".".into()),
            Event::End(TagEnd::Paragraph),
            Event::Start(Tag::Paragraph),
            Event::Text("After.".into()),
            Event::End(TagEnd::Paragraph),
            Event::Start(Tag::FootnoteDefinition("inline-1".into())),
            Event::Start(Tag::Paragraph),
            Event::Text("an ".into()),
            Event::Start(Tag::Emphasis),
            Event::Text("inline".into()),
            Event::End(TagEnd::Emphasis),
            Event::Text(" note".into()),
            Event::End(TagEnd::Paragraph),
            Event::End(TagEnd::FootnoteDefinition),
            Event::Start(Tag::FootnoteDefinition("inline-2".into())),
            Event::Start(Tag::Paragraph),
            Event::Text("two".into()),
            Event::End(TagEnd::Paragraph),
            Event::End(TagEnd::FootnoteDefinition),
        ]
    );
    assert!(
        !plain("x^ [no] and ^[] and a^[open\n")
            .iter()
            .any(|event| matches!(event, Event::FootnoteReference(_)))
    );
}
