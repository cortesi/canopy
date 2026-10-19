//! GitHub's markup, from `github::push_html`.
#![cfg(feature = "html")]
#![allow(missing_docs, clippy::tests_outside_test_module)]

use canopy_xmark::{Options, Parser, github, hide_comments};

/// Write `text` as GitHub writes it, with fenced code in a marked `pre`.
fn render(text: &str) -> String {
    let hidden = hide_comments(text, Options::obsidian());
    let mut out = String::new();
    github::push_html(
        &mut out,
        Parser::new_ext(&hidden, Options::obsidian()),
        |info, code| Some(format!("<pre lang=\"{info}\">{code}</pre>")),
    );
    out
}

#[test]
fn headings_carry_anchors_to_their_ids() {
    assert_eq!(
        render("# Hello\n"),
        "<h1 id=\"hello\"><a class=\"anchor\" aria-hidden=\"true\" href=\"#hello\">\
         <span class=\"octicon octicon-link\"></span></a>Hello</h1>\n"
    );
}

#[test]
fn callouts_are_titled_boxes() {
    let html = render("> [!tip] Pack <light>\n> Take less.\n");
    assert!(
        html.starts_with("<div class=\"markdown-alert markdown-alert-tip\">"),
        "{html}"
    );
    assert!(
        html.contains("Pack &lt;light&gt;</p>\n<p>Take less.</p>\n</div>"),
        "{html}"
    );
}

#[test]
fn task_lists_footnotes_block_ids_and_code_take_githubs_markup() {
    let html = render(
        "- [x] done\n- [ ] todo\n\nA note.[^n] ^here\n\n[^n]: Said.\n\n```rust\nfn a() {}\n```\n",
    );
    assert!(html.contains("<ul class=\"contains-task-list\">"), "{html}");
    assert!(
        html.contains("class=\"task-list-item-checkbox\" checked=\"\""),
        "{html}"
    );
    assert!(html.contains("<a href=\"#fn-1\" id=\"fnref-1\""), "{html}");
    assert!(html.contains("<li id=\"fn-1\">"), "{html}");
    assert!(html.contains("<a id=\"^here\"></a>"), "{html}");
    assert!(
        html.contains("<pre lang=\"rust\">fn a() {}\n</pre>"),
        "{html}"
    );
}

#[test]
fn inline_footnotes_number_with_the_rest() {
    let html = render("One[^a] and two^[inline].\n\n[^a]: First.\n");
    assert!(html.contains("<a href=\"#fn-2\" id=\"fnref-2\""), "{html}");
    assert!(html.contains("<li id=\"fn-2\">\n<p>inline"), "{html}");
}

#[test]
fn front_matter_shows_as_a_table_of_its_properties() {
    let html = render("---\ntitle: Trip <one>\ntags: [a, b]\n---\n\n# Body\n");
    assert!(
        html.starts_with(
            "<table>\n<thead>\n<tr>\n<th>title</th>\n<th>tags</th>\n</tr>\n</thead>\n\
             <tbody>\n<tr>\n<td>Trip &lt;one&gt;</td>\n<td>a, b</td>\n</tr>\n</tbody>\n</table>\n"
        ),
        "{html}"
    );
    assert!(!render("---\nmap:\n  key: value\n---\n").contains("key"));
}
