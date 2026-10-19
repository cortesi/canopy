//! The examples in the crate's documentation, which doctests do not run.
#![allow(missing_docs, clippy::tests_outside_test_module)]

use canopy_xmark::{
    BrokenLink, CowStr, Event, Options, Parser, ParserCallbacks, Tag, TextMergeStream,
};

#[cfg(feature = "html")]
#[test]
fn the_crate_example_renders_strikethrough() {
    use canopy_xmark::html;

    let markdown_input = "Hello world, this is a ~~complicated~~ *very simple* example.";
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(markdown_input, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    assert_eq!(
        html_output,
        "<p>Hello world, this is a <del>complicated</del> <em>very simple</em> example.</p>\n"
    );
}

#[test]
fn text_merge_stream_joins_adjacent_text() {
    let markdown_input = "Hello world, this is a ~~complicated~~ *very simple* example.";
    let texts: Vec<String> = TextMergeStream::new(Parser::new(markdown_input))
        .filter_map(|event| match event {
            Event::Text(text) => Some(text.into_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        [
            "Hello world, this is a ~~complicated~~ ",
            "very simple",
            " example."
        ]
    );
}

/// Answers every broken link with one target.
struct CustomCallbacks;

impl<'input> ParserCallbacks<'input> for CustomCallbacks {
    fn handle_broken_link(
        &mut self,
        link: BrokenLink<'input>,
    ) -> Option<(CowStr<'input>, CowStr<'input>)> {
        Some(("https://target".into(), link.reference))
    }
}

#[test]
fn a_broken_link_callback_supplies_a_target() {
    let mut parser = Parser::new_with_callbacks("[broken]", Options::empty(), CustomCallbacks);
    assert!(matches!(
        parser.nth(1),
        Some(Event::Start(Tag::Link { .. }))
    ));
}

#[cfg(feature = "html")]
mod html {
    use std::io::Cursor;

    use canopy_xmark::{Parser, escape, html};

    const SOURCE: &str = "\nhello\n=====\n\n* alpha\n* beta\n";
    const HTML: &str = "<h1>hello</h1>\n<ul>\n<li>alpha</li>\n<li>beta</li>\n</ul>\n";

    #[test]
    fn push_html_writes_a_string() {
        let mut out = String::new();
        html::push_html(&mut out, Parser::new(SOURCE));
        assert_eq!(out, HTML);
    }

    #[test]
    fn write_html_io_writes_bytes() {
        let mut bytes = Vec::new();
        html::write_html_io(Cursor::new(&mut bytes), Parser::new(SOURCE)).unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), HTML);
    }

    #[test]
    fn write_html_fmt_writes_a_formatter() {
        let mut out = String::new();
        html::write_html_fmt(&mut out, Parser::new(SOURCE)).unwrap();
        assert_eq!(out, HTML);
    }

    #[test]
    fn escape_html_quotes_attributes() {
        let mut value = String::new();
        escape::escape_html(&mut value, "two \"words\"").unwrap();
        assert_eq!(value, "two &quot;words&quot;");
    }
}
