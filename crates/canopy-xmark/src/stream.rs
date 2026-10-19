//! Growing documents: the part of a document that more input could still
//! change.
//!
//! A document that arrives in pieces, such as an agent's output, parses
//! again as it grows. Its last block may not be finished: an append can
//! continue a paragraph, close a fence, or turn a paragraph into a table's
//! header. [`TailWatch`] reads the events of a parse and says where that
//! open tail starts.

use alloc::vec::Vec;
use core::ops::Range;

use crate::{CodeBlockKind, Event, Tag, TagEnd};

/// The part of a growing document that more input could still change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenTail {
    /// Where it starts.
    pub start: usize,
    /// How many quotes hold its start.
    pub quote_depth: usize,
}

/// What kind of block a document ends with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Last {
    /// A fenced code block, ending at its source end.
    Fenced(usize),
    /// An indented code block, ending at its source end.
    Indented(usize),
    /// A heading.
    Heading,
    /// A table.
    Table,
    /// An HTML block.
    Html,
    /// Front matter.
    Metadata,
    /// A thematic break.
    Rule,
    /// A paragraph, or the text of a tight list item.
    Text,
}

/// Reads the events of a parse, with their source ranges, to find a growing
/// document's open tail.
#[derive(Clone, Debug, Default)]
pub struct TailWatch {
    /// The block that starts last, and where.
    last: Option<(Last, usize)>,
    /// The source ranges of the quotes seen.
    quotes: Vec<Range<usize>>,
    /// How many quotes are open.
    open_quotes: usize,
    /// How many leaf blocks are open.
    open_leaves: usize,
    /// Whether the inline run outside any leaf block was already counted.
    in_run: bool,
}

/// Where a watch stood after some part of a parse, so a watch of a parse
/// resumed there can carry on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TailMark {
    /// The block that started last, and where.
    last: Option<(Last, usize)>,
}

impl TailWatch {
    /// Start watching a parse.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start watching a parse resumed after a prefix whose watch left
    /// `mark`. Only the prefix's last block carries over: a quote that
    /// closed in the prefix holds nothing after it.
    #[must_use]
    pub fn after(mark: TailMark) -> Self {
        Self {
            last: mark.last,
            ..Self::default()
        }
    }

    /// Where the watch stands, for resuming after what it read so far.
    #[must_use]
    pub fn mark(&self) -> TailMark {
        TailMark { last: self.last }
    }

    /// Read the next event of the parse, with its source range.
    pub fn observe(&mut self, event: &Event<'_>, range: &Range<usize>) {
        match event {
            Event::Start(Tag::BlockQuote(_) | Tag::Callout(_)) => {
                self.quotes.push(range.clone());
                self.open_quotes += 1;
                self.in_run = false;
            }
            Event::End(TagEnd::BlockQuote(_) | TagEnd::Callout(_)) => {
                self.open_quotes = self.open_quotes.saturating_sub(1);
                self.in_run = false;
            }
            Event::Start(tag) => match leaf(tag, range) {
                Some(kind) => {
                    self.block(kind, range.start);
                    self.open_leaves += 1;
                }
                None if is_inline(tag) => self.inline(range.start),
                None => self.in_run = false,
            },
            Event::End(end) => {
                if is_leaf_end(*end) {
                    self.open_leaves = self.open_leaves.saturating_sub(1);
                }
                if !is_inline_end(*end) {
                    self.in_run = false;
                }
            }
            Event::Rule => self.block(Last::Rule, range.start),
            Event::Text(_)
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::InlineHtml(_)
            | Event::Hashtag(_)
            | Event::FootnoteReference(_)
            | Event::TaskListMarker(_) => self.inline(range.start),
            _ => {}
        }
    }

    /// Return the open tail of `text`, the source the watched parse read,
    /// given the spans of its reference definitions.
    ///
    /// A trailing reference definition can gain a title on the next line,
    /// until a blank line or another block. A block is open until what ends
    /// it arrives: a closing fence and its line break, a blank line, the end
    /// of a heading's or a break's line, the closing delimiter of front
    /// matter or of an HTML block. A table stays open, since another row can
    /// follow.
    #[must_use]
    pub fn open_tail(&self, text: &str, definitions: &[Range<usize>]) -> Option<OpenTail> {
        let tail = |start| OpenTail {
            start,
            quote_depth: self
                .quotes
                .iter()
                .filter(|quote| quote.contains(&start))
                .count(),
        };
        let leaf_start = self.last.map(|(_, start)| start);
        if let Some(definition) = definitions
            .iter()
            .max_by_key(|range| range.start)
            .filter(|range| leaf_start.is_none_or(|start| range.start > start))
        {
            let after = &text[definition.end.min(text.len())..];
            if blank_line_after_first(&text[definition.start..]) || !after.trim().is_empty() {
                return None;
            }
            return Some(tail(definition.start));
        }
        let (kind, start) = self.last?;
        let rest = &text[start..];
        let closed = match kind {
            Last::Fenced(end) => fence_closed(text, end),
            Last::Indented(end) => blank_line_after_first(&text[end..]),
            Last::Heading | Last::Rule => rest.ends_with(['\n', '\r']),
            Last::Table => false,
            Last::Html => html_closed(rest),
            Last::Metadata => rest
                .lines()
                .skip(1)
                .any(|line| matches!(line.trim(), "---" | "...")),
            Last::Text => blank_line_after_first(rest),
        };
        (!closed).then(|| tail(start))
    }

    /// A block of `kind` starts at `start`.
    fn block(&mut self, kind: Last, start: usize) {
        self.in_run = false;
        if self.last.is_none_or(|(_, last)| start >= last) {
            self.last = Some((kind, start));
        }
    }

    /// Inline content starts at `start`: outside any leaf block, as in a
    /// tight list item, it reads as a paragraph.
    fn inline(&mut self, start: usize) {
        if self.open_leaves == 0 && !self.in_run {
            self.block(Last::Text, start);
            self.in_run = true;
        }
    }
}

/// The kind of leaf block `tag` opens, over `range`, if it opens one.
fn leaf(tag: &Tag<'_>, range: &Range<usize>) -> Option<Last> {
    Some(match tag {
        Tag::Paragraph => Last::Text,
        Tag::Heading { .. } => Last::Heading,
        Tag::CodeBlock(CodeBlockKind::Fenced(_)) => Last::Fenced(range.end),
        Tag::CodeBlock(CodeBlockKind::Indented) => Last::Indented(range.end),
        Tag::Table(_) => Last::Table,
        Tag::HtmlBlock => Last::Html,
        Tag::MetadataBlock(_) => Last::Metadata,
        _ => return None,
    })
}

/// Whether `end` closes a leaf block that [`leaf`] opens.
fn is_leaf_end(end: TagEnd) -> bool {
    matches!(
        end,
        TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::CodeBlock
            | TagEnd::Table
            | TagEnd::HtmlBlock
            | TagEnd::MetadataBlock(_)
    )
}

/// Whether `tag` opens inline content.
fn is_inline(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Highlight
            | Tag::Superscript
            | Tag::Subscript
            | Tag::Link { .. }
            | Tag::Image { .. }
            | Tag::Embed(_)
    )
}

/// Whether `end` closes inline content.
fn is_inline_end(end: TagEnd) -> bool {
    matches!(
        end,
        TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Highlight
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Link
            | TagEnd::Image
            | TagEnd::Embed
    )
}

/// Whether a blank line, of spaces and tabs at most, follows the first line
/// of `text`, across CR, LF, and CRLF.
fn blank_line_after_first(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut first = true;
    while let Some(len) = bytes[start..]
        .iter()
        .position(|byte| matches!(byte, b'\n' | b'\r'))
    {
        let end = start + len;
        if !first
            && bytes[start..end]
                .iter()
                .all(|byte| matches!(byte, b' ' | b'\t'))
        {
            return true;
        }
        first = false;
        start = end
            + if bytes[end..].starts_with(b"\r\n") {
                2
            } else {
                1
            };
    }
    false
}

/// Whether a fenced code block ending at `end` closed with a fence of its
/// own and that fence's line break. The parser ends an explicitly closed
/// block before the closing fence's line break; a block that a container or
/// the end of input closed ends after one, or at the end.
fn fence_closed(text: &str, end: usize) -> bool {
    text.as_bytes()
        .get(end)
        .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
        && !text[..end].ends_with(['\n', '\r'])
}

/// Whether an HTML block starting `text` closed: by its kind's terminator,
/// or by a blank line.
fn html_closed(text: &str) -> bool {
    let lower = text.trim_start().to_ascii_lowercase();
    let delimiter = if lower.starts_with("<!--") {
        Some("-->")
    } else if lower.starts_with("<?") {
        Some("?>")
    } else if lower.starts_with("<![cdata[") {
        Some("]]>")
    } else if lower.starts_with("<!") {
        Some(">")
    } else if lower.starts_with("<script") {
        Some("</script>")
    } else if lower.starts_with("<pre") {
        Some("</pre>")
    } else if lower.starts_with("<style") {
        Some("</style>")
    } else if lower.starts_with("<textarea") {
        Some("</textarea>")
    } else {
        None
    };
    delimiter.is_some_and(|delimiter| lower.contains(delimiter)) || blank_line_after_first(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, Parser};

    /// The open tail of `text`, as it would arrive so far.
    fn tail(text: &str) -> Option<OpenTail> {
        let mut watch = TailWatch::new();
        let events = Parser::new_ext(text, Options::obsidian()).into_offset_iter();
        let definitions = events.reference_definitions().spans().to_vec();
        for (event, range) in events {
            watch.observe(&event, &range);
        }
        watch.open_tail(text, &definitions)
    }

    #[test]
    fn a_finished_block_closes_and_an_unfinished_one_stays_open() {
        assert_eq!(tail("Para.\n\n"), None);
        assert_eq!(
            tail("Done.\n\nStill"),
            Some(OpenTail {
                start: 7,
                quote_depth: 0
            })
        );
        assert_eq!(tail("```\ncode\n```\n"), None);
        assert!(
            tail("```\ncode\n```").is_some(),
            "the fence's line may still grow"
        );
        assert!(tail("```\ncode\n").is_some());
        assert_eq!(tail("# Heading\n"), None);
        assert!(tail("| a |\n| - |\n| 1 |\n").is_some());
        assert_eq!(tail("<!-- x -->\n"), None);
    }

    #[test]
    fn a_trailing_definition_can_still_gain_a_title() {
        assert!(tail("[a]: /url\n").is_some());
        assert_eq!(tail("[a]: /url\n\n"), None);
        assert_eq!(
            tail("> quote\n> [a]: /url\n").map(|tail| tail.quote_depth),
            Some(1)
        );
    }

    #[test]
    fn whitespace_only_lines_are_blank() {
        for text in ["a\r\nb", "a\n  ", "\nb\n", "a\r"] {
            assert!(!blank_line_after_first(text), "{text:?}");
        }
        for text in ["a\n\n", "a\r\n\r\n", "a\r\r", "a\n \t\n", "a\r\n  \r\nb"] {
            assert!(blank_line_after_first(text), "{text:?}");
        }
    }

    #[test]
    fn a_tight_list_item_reads_as_a_paragraph() {
        let text = "- one\n- two";
        assert_eq!(tail(text).map(|tail| tail.start), Some(8));
    }
}
