//! Regular expressions that match text as ripgrep does, and the ranges that
//! they highlight.
//!
//! An application that searches files or documents with a typed expression,
//! and then highlights what it found in an [`Editor`](crate::editor::Editor)
//! or a [`DiffView`](crate::DiffView), compiles the expression once with
//! [`smart_matcher`]. The same matcher can drive `grep-searcher` over files,
//! so a result list and the text that shows a result match alike.
//! [`match_ranges`] turns its matches in shown text into the ranges that
//! [`Editor::set_matches`](crate::editor::Editor::set_matches) takes.

use std::{iter, ops::Range};

use grep_matcher::Matcher as _;
pub use grep_regex::{Error, RegexMatcher};

use crate::{
    table::{CellRange, TableData},
    text_buffer::{TextPosition, TextRange},
};

/// Compile a ripgrep expression that matches case smartly.
///
/// The expression uses `grep-regex` syntax. An expression with an uppercase
/// letter matches case, and one without ignores it, as ripgrep's smart case
/// does. Only literal letters count, so an escape such as `\S` leaves a
/// lowercase expression ignoring case.
pub fn smart_matcher(pattern: &str) -> Result<RegexMatcher, Error> {
    grep_regex::RegexMatcherBuilder::new()
        .case_smart(true)
        .build(pattern)
}

/// Return the ranges of the matches of `matcher` in `text`, one line at a
/// time.
///
/// Columns count characters, and no range spans lines, as the editor's own
/// matches do. The ranges arrive in ascending order. Empty matches are
/// skipped: they mark positions rather than text worth highlighting.
pub fn match_ranges(matcher: &RegexMatcher, text: &str) -> Vec<TextRange> {
    line_matches(matcher, text)
        .map(|(index, _, line, found)| {
            TextRange::new(
                TextPosition::new(index, char_column(line, found.start)),
                TextPosition::new(index, char_column(line, found.end)),
            )
        })
        .collect()
}

/// Return the matches of `matcher` in the cells of `data`, in reading order:
/// the header's cells, then each row's, from left to right.
///
/// A value matches one line at a time, as text does, and empty matches are
/// skipped. Each match names its cell and its bytes in the cell's value, as
/// a [`TableReader`](crate::TableReader) takes them.
pub fn cell_matches(matcher: &RegexMatcher, data: &TableData) -> Vec<CellRange> {
    let header = (
        None,
        data.columns().iter().map(|column| &column.label).collect(),
    );
    let rows = data
        .rows()
        .iter()
        .enumerate()
        .map(|(index, row)| (Some(index), row.cells.iter().collect::<Vec<_>>()));
    let mut out = Vec::new();
    for (row, cells) in iter::once(header).chain(rows) {
        for (column, cell) in cells.into_iter().enumerate() {
            out.extend(
                line_matches(matcher, cell.text()).map(|(_, offset, _, found)| CellRange {
                    row,
                    column,
                    bytes: offset + found.start..offset + found.end,
                }),
            );
        }
    }
    out
}

/// Yield the nonempty matches of `matcher` in `text`, one line at a time:
/// each line's index, its byte offset in `text`, the line, and the match's
/// bytes in the line. Empty matches mark positions rather than text worth
/// highlighting, so they are skipped.
fn line_matches<'a>(
    matcher: &'a RegexMatcher,
    text: &'a str,
) -> impl Iterator<Item = (usize, usize, &'a str, Range<usize>)> + 'a {
    let lines = text.split('\n').scan(0, |at, line| {
        let offset = *at;
        *at += line.len() + 1;
        Some((offset, line))
    });
    lines.enumerate().flat_map(move |(index, (offset, line))| {
        let mut start = 0;
        iter::from_fn(move || {
            while start <= line.len() {
                let found = matcher.find_at(line.as_bytes(), start).ok().flatten()?;
                if found.start() == found.end() {
                    // Skip a zero-width match, and stop at the end of the line.
                    let next = next_char_boundary(line, found.start() + 1);
                    if next <= found.start() {
                        return None;
                    }
                    start = next;
                    continue;
                }
                start = found.end();
                return Some((index, offset, line, found.start()..found.end()));
            }
            None
        })
    })
}

/// Return the character column of a byte offset, saturating past the end.
fn char_column(line: &str, byte: usize) -> usize {
    line.get(..byte)
        .map_or_else(|| line.chars().count(), |prefix| prefix.chars().count())
}

/// Return the next character boundary at or after `byte`.
fn next_char_boundary(line: &str, byte: usize) -> usize {
    (byte..=line.len())
        .find(|index| line.is_char_boundary(*index))
        .unwrap_or(line.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Return the ranges of a case-sensitive `pattern` in `text`.
    fn ranges(pattern: &str, text: &str) -> Vec<TextRange> {
        let matcher = RegexMatcher::new(pattern).expect("pattern");
        match_ranges(&matcher, text)
    }

    #[test]
    fn expression_matches_highlight_per_line_in_order() {
        let found = ranges(r"fn\s+\w+", "fn main() {}\nfn  run() {}");
        assert_eq!(
            found,
            [
                TextRange::new(TextPosition::new(0, 0), TextPosition::new(0, 7)),
                TextRange::new(TextPosition::new(1, 0), TextPosition::new(1, 7)),
            ]
        );
    }

    #[test]
    fn case_is_smart() {
        let smart = |pattern| {
            let matcher = smart_matcher(pattern).expect("pattern");
            match_ranges(&matcher, "needle Needle NEEDLE").len()
        };
        assert_eq!(smart("needle"), 3, "a lowercase expression ignores case");
        assert_eq!(smart("Needle"), 1, "an uppercase letter matches case");
        assert_eq!(smart(r"\Sdle"), 3, "an escape is not an uppercase letter");
        assert!(
            smart_matcher("(").is_err(),
            "invalid syntax fails to compile"
        );
    }

    #[test]
    fn matches_never_span_lines_and_empty_matches_are_skipped() {
        assert_eq!(ranges("b\nc", "b\nc"), []);
        assert_eq!(ranges("x*", "ab"), []);
    }

    #[test]
    fn cell_matches_name_their_cells_in_reading_order() {
        use crate::table::TableData;

        let data = TableData::builder("t", 1, ["ab", "x"])
            .row("r0", ["b", "ab\nab"])
            .row("r1", ["", "b"])
            .build()
            .unwrap();
        let found = cell_matches(&smart_matcher("b").unwrap(), &data);
        let cells = found
            .iter()
            .map(|m| (m.row, m.column, m.bytes.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            cells,
            [
                (None, 0, 1..2),
                (Some(0), 0, 0..1),
                (Some(0), 1, 1..2),
                (Some(0), 1, 4..5),
                (Some(1), 1, 0..1),
            ]
        );
    }

    #[test]
    fn columns_count_characters() {
        assert_eq!(
            ranges("é+", "aéé"),
            [TextRange::new(
                TextPosition::new(0, 1),
                TextPosition::new(0, 3)
            )]
        );
    }
}
