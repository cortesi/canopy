//! Obsidian comments: `%%` hides what it wraps, within a line or across
//! blocks, outside code and math.
//!
//! A comment can open in one paragraph and close blocks later, so it cannot
//! be an inline construct. [`hide_comments`] blanks comments before parsing
//! instead, keeping every byte offset, and a parser with
//! [`Options::ENABLE_COMMENTS`] drops what is left of them.

use alloc::{borrow::Cow, string::String, vec::Vec};
use core::ops::Range;

use crate::{Event, Options, Parser, Tag};

/// The byte that stands for a hidden byte on a line that keeps other text.
/// The parser removes it from text.
pub const HIDDEN: u8 = 0x01;

/// The marker that opens and closes a comment.
const MARKER: &str = "%%";

/// Return `text` with its Obsidian comments hidden, for a parser with
/// [`Options::ENABLE_COMMENTS`] to read.
///
/// A comment runs from one `%%` to the next, outside code and math; a `%%`
/// with no partner shows as written. A line that a comment hides entirely
/// turns blank, so it ends the paragraph it was in. Hidden bytes on a line
/// that keeps other text turn into a byte the parser removes. Every byte
/// keeps its offset, so source ranges into the result are ranges into
/// `text`. Text with no comment comes back borrowed.
#[must_use]
pub fn hide_comments(text: &str, options: Options) -> Cow<'_, str> {
    if !text.contains(MARKER) {
        return Cow::Borrowed(text);
    }
    let comments = comments(text, options);
    if comments.is_empty() {
        return Cow::Borrowed(text);
    }
    let mut bytes = text.as_bytes().to_vec();
    for comment in comments {
        hide(&mut bytes, text, comment);
    }
    // Every byte of a hidden character is replaced, so the result is UTF-8.
    Cow::Owned(String::from_utf8(bytes).expect("hiding replaces whole characters"))
}

/// The comments in `text`, each from its opening `%%` through its closing
/// one.
fn comments(text: &str, options: Options) -> Vec<Range<usize>> {
    let shown = literal_regions(text, options - Options::ENABLE_COMMENTS);
    let mut markers = text
        .match_indices(MARKER)
        .map(|(at, _)| at)
        .filter(|at| !shown.iter().any(|region| region.contains(at)));
    let mut comments = Vec::new();
    while let (Some(open), Some(close)) = (markers.next(), markers.next()) {
        comments.push(open..close + MARKER.len());
    }
    comments
}

/// The regions where a `%%` is text: code spans, code blocks, and math.
fn literal_regions(text: &str, options: Options) -> Vec<Range<usize>> {
    Parser::new_ext(text, options)
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Start(Tag::CodeBlock(_))
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_) => Some(range),
            _ => None,
        })
        .collect()
}

/// Hide `comment` in `bytes`, line by line.
fn hide(bytes: &mut [u8], text: &str, comment: Range<usize>) {
    let mut line_start = text[..comment.start]
        .rfind(['\n', '\r'])
        .map_or(0, |at| at + 1);
    while line_start < comment.end {
        let line_end = text[line_start..]
            .find(['\n', '\r'])
            .map_or(text.len(), |at| line_start + at);
        let hidden = comment.start.max(line_start)..comment.end.min(line_end);
        let kept = |range: Range<usize>| text[range].trim().is_empty();
        let blank = kept(line_start..hidden.start) && kept(hidden.end..line_end);
        let fill = if blank { b' ' } else { HIDDEN };
        bytes[hidden].fill(fill);
        // Past the line's end, CRLF counting as one break.
        line_start = line_end + 1;
        if text.as_bytes().get(line_end) == Some(&b'\r')
            && text.as_bytes().get(line_end + 1) == Some(&b'\n')
        {
            line_start += 1;
        }
    }
}

/// Remove hidden bytes from `text`, leaving one space where a comment sat
/// between words, and none where one opened or ended the text.
pub fn strip_hidden(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(char::from(HIDDEN)) {
        out.push_str(&rest[..at]);
        rest = rest[at..].trim_start_matches(char::from(HIDDEN));
        if rest.is_empty() {
            // A comment that ends the text takes the space before it.
            out.truncate(out.trim_end_matches(' ').len());
        } else if (out.is_empty() || out.ends_with(' ')) && rest.starts_with(' ') {
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

/// Whether `bytes` hold only hidden bytes and spaces.
pub fn only_hidden(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|&byte| byte == HIDDEN || byte == b' ' || byte == b'\t')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hide comments as the Obsidian preset reads them.
    fn hidden(text: &str) -> String {
        hide_comments(text, Options::obsidian()).into_owned()
    }

    #[test]
    fn text_without_comments_is_borrowed() {
        assert!(matches!(
            hide_comments("no comment here", Options::obsidian()),
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            hide_comments("one %% alone", Options::obsidian()),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn hidden_text_keeps_its_length_and_line_breaks() {
        let text = "a %%x%% b\n\n%%\nhidden\n%%\n\nend %%é\n%% after\n";
        let out = hidden(text);
        assert_eq!(out.len(), text.len());
        assert_eq!(
            out.lines().collect::<Vec<_>>(),
            [
                "a \u{1}\u{1}\u{1}\u{1}\u{1} b",
                "",
                "  ",
                "      ",
                "  ",
                "",
                "end \u{1}\u{1}\u{1}\u{1}",
                "\u{1}\u{1} after",
            ]
        );
    }

    #[test]
    fn code_and_math_show_their_markers() {
        let text = "`%%` and $%%$ stay\n\n```\n%%\n```\n";
        assert_eq!(hidden(text), text);
    }

    #[test]
    fn stripping_leaves_one_space_between_words() {
        assert_eq!(strip_hidden("a \u{1}\u{1} b"), "a b");
        assert_eq!(strip_hidden("\u{1}\u{1} b"), "b");
        assert_eq!(strip_hidden("a\u{1}b"), "ab");
        assert_eq!(strip_hidden("a \u{1}"), "a");
    }
}
