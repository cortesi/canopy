//! The spec suites, generated from the spec files.
#![cfg(feature = "html")]
// The spec suites and their helpers come from pulldown-cmark, as generated
// tests outside a test module.
#![allow(
    missing_docs,
    clippy::absolute_paths,
    clippy::invisible_characters,
    clippy::items_after_statements,
    clippy::tests_outside_test_module
)]

use canopy_xmark::{Options, Parser};

#[rustfmt::skip]
mod suite;

/// Render `input` with the spec suites' options and the named
/// `extensions`, and check it against `output`.
#[inline(never)]
pub fn test_markdown_html(input: &str, output: &str, extensions: &[&str]) {
    let on = |name: &str| extensions.contains(&name);
    let mut s = String::new();

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_MATH);
    opts.insert(Options::ENABLE_TABLES);
    if on("strikethrough") {
        opts.insert(Options::ENABLE_STRIKETHROUGH);
    }
    opts.insert(Options::ENABLE_HIGHLIGHT);
    opts.insert(Options::ENABLE_SUPERSCRIPT);
    if on("wikilinks") {
        opts.insert(Options::ENABLE_WIKILINKS);
    }
    if on("subscript") {
        opts.insert(Options::ENABLE_SUBSCRIPT);
    }
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_GFM);
    if on("old_footnotes") {
        opts.insert(Options::ENABLE_OLD_FOOTNOTES);
    } else {
        opts.insert(Options::ENABLE_FOOTNOTES);
    }
    if on("metadata_blocks") {
        opts.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
        opts.insert(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS);
    }
    if on("smart_punct") {
        opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    }
    opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    if on("deflists") {
        opts.insert(Options::ENABLE_DEFINITION_LIST);
    }
    if on("container_extensions") {
        opts.insert(Options::ENABLE_CONTAINER_EXTENSIONS);
    }
    if on("autolinks") {
        opts.insert(Options::ENABLE_AUTOLINKS);
    }
    if on("cjk_friendly_emphasis") {
        opts.insert(Options::ENABLE_CJK_FRIENDLY_EMPHASIS);
    }

    // The fork's examples read exactly the syntax Canopy and fh read.
    if on("xmark") {
        opts = Options::obsidian();
    }

    let input = canopy_xmark::hide_comments(input, opts);
    let p = Parser::new_ext(&input, opts);
    canopy_xmark::html::push_html(&mut s, p);

    // normalizing the HTML using html5ever may hide actual errors
    // assert_eq!(html_standardize(output), html_standardize(&s));
    assert_eq!(html_standardize(output), html_standardize(&s));
}

fn html_standardize(s: &str) -> String {
    s.replace("<br>", "<br />")
        .replace("<br/>", "<br />")
        .replace("<hr>", "<hr />")
        .replace("<hr/>", "<hr />")
        // permit extra or missing line breaks only between tags
        .replace(">\n<", "><")
}
