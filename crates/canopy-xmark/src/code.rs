//! Fenced code blocks, for tools that read Markdown source line by line.

use alloc::vec::Vec;
use core::ops::Range;

use crate::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};

/// A fenced code block in Markdown source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FencedCode<'a> {
    /// The info string after the opening fence, such as `rust,ignore`.
    pub info: CowStr<'a>,
    /// The source range of its content, from the start of its first line to
    /// the end of its last. In a quote or a list, it includes the container
    /// markers of lines after the first.
    pub body: Range<usize>,
}

/// Return the fenced code blocks of Markdown `text`, as CommonMark reads
/// them with [`Options::obsidian`]: in quotes and lists too, and not when
/// indented four spaces or more. A block with no content has no entry.
#[must_use]
pub fn fenced_code(text: &str) -> Vec<FencedCode<'_>> {
    let mut found = Vec::new();
    let mut open: Option<(CowStr<'_>, Option<Range<usize>>)> = None;
    for (event, range) in Parser::new_ext(text, Options::obsidian()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => open = Some((info, None)),
            Event::Text(_) => {
                if let Some((_, body)) = &mut open {
                    let start = body.as_ref().map_or(range.start, |body| body.start);
                    *body = Some(start..range.end);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((info, Some(body))) = open.take() {
                    found.push(FencedCode { info, body });
                }
            }
            _ => {}
        }
    }
    found
}

/// Return the language a fence's info string names: its first word, which a
/// comma also ends, so `rust,ignore` names `rust`.
#[must_use]
pub fn fence_language(info: &str) -> Option<&str> {
    info.trim_start()
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .next()
        .filter(|word| !word.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_follow_commonmark() {
        let text = "```rust,ignore\nfn a() {}\n```\n\n> ~~~py\n> x = 1\n> ~~~\n\n    ```\n    indented\n    ```\n\n```\n```\n";
        let found = fenced_code(text);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(&*found[0].info, "rust,ignore");
        assert_eq!(&text[found[0].body.clone()], "fn a() {}\n");
        assert_eq!(fence_language(&found[0].info), Some("rust"));
        assert_eq!(&*found[1].info, "py");
        assert_eq!(&text[found[1].body.clone()], "x = 1\n");
        assert_eq!(fence_language("  "), None);
    }
}
