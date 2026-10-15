//! Bounded, line-local matching shared by source and readable projections.
use std::{error::Error as StdError, fmt, iter, ops::Range};

use grep_matcher::Matcher as _;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};

use crate::{
    document::{BlockKind, DocumentSnapshot, UnitKind},
    source_store::SourceSnapshot,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// How a query interprets its text.
pub enum SearchMode {
    #[default]
    /// Match the query as literal text.
    Literal,
    /// Use the same ripgrep regular-expression syntax as fh.
    Regex,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Case sensitivity shared by Source and Rendered search.
pub enum SearchCase {
    #[default]
    /// Match case exactly.
    Sensitive,
    /// Ignore letter case.
    Insensitive,
    /// Match case only when the expression contains literal uppercase letters.
    Smart,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// A retained query with explicit matching semantics.
pub struct SearchQuery {
    /// Readable text, excluding generated terminal decoration.
    pub text: String,
    /// Literal or regular-expression matching.
    pub mode: SearchMode,
    /// Explicit case-sensitivity policy.
    pub case: SearchCase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Bounds on query compilation, scanned bytes, and result storage.
pub struct SearchLimits {
    /// Maximum UTF-8 query size before compilation.
    pub max_query_bytes: usize,
    /// Maximum stored match records; covered matches are still counted.
    pub max_matches: usize,
    /// Maximum source or readable bytes examined in this run.
    pub max_scan_bytes: usize,
}
impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            max_query_bytes: 4096,
            max_matches: 10_000,
            max_scan_bytes: 16 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// A query validation, compilation, or matching diagnostic.
pub struct SearchError(
    /// Human-readable diagnostic retained with the query.
    pub String,
);
impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl StdError for SearchError {}

/// A compiled line-local matcher reusable across source revisions.
pub struct CompiledQuery {
    /// The compiled expression, absent for an empty query.
    matcher: Option<RegexMatcher>,
}
impl CompiledQuery {
    /// Return the next match whose endpoints preserve UTF-8 boundaries.
    pub(crate) fn find_at(
        &self,
        text: &str,
        start: usize,
    ) -> Result<Option<Range<usize>>, SearchError> {
        let Some(matcher) = &self.matcher else {
            return Ok(None);
        };
        let mut at = start;
        while at <= text.len() {
            while at < text.len() && !text.is_char_boundary(at) {
                at += 1;
            }
            let Some(found) = matcher
                .find_at(text.as_bytes(), at)
                .map_err(|error| SearchError(error.to_string()))?
            else {
                return Ok(None);
            };
            if text.is_char_boundary(found.start()) && text.is_char_boundary(found.end()) {
                return Ok(Some(found.start()..found.end()));
            }
            at = found.start().saturating_add(1);
        }
        Ok(None)
    }

    /// Compile one bounded query; reject multiline text and invalid regex.
    pub fn new(query: &SearchQuery, limits: &SearchLimits) -> Result<Self, SearchError> {
        if query.text.len() > limits.max_query_bytes {
            return Err(SearchError(format!(
                "Query exceeds {} bytes",
                limits.max_query_bytes
            )));
        }
        if query.text.contains(['\n', '\r']) {
            return Err(SearchError("Multiline queries are not supported".into()));
        }
        if query.text.is_empty() {
            return Ok(Self { matcher: None });
        }
        let pattern = if query.mode == SearchMode::Literal {
            escape_regex(&query.text)
        } else {
            query.text.clone()
        };
        let mut builder = RegexMatcherBuilder::new();
        builder
            .case_insensitive(query.case == SearchCase::Insensitive)
            .case_smart(query.case == SearchCase::Smart);
        let matcher = builder
            .build(&pattern)
            .map_err(|error| SearchError(error.to_string()))?;
        Ok(Self {
            matcher: Some(matcher),
        })
    }
}
/// Escape literal query characters using the shared regex syntax.
fn escape_regex(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if matches!(
            c,
            '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// The readable location containing one search match.
pub enum SearchAddress {
    /// A raw source line.
    SourceLine {
        /// Zero-based line index within the addressed payload.
        line: usize,
    },
    /// A prose reading unit.
    Unit {
        /// Identity of the associated reading unit.
        unit_id: u64,
    },
    /// A line within an atomic code card.
    CodeLine {
        /// Identity of the associated reading unit.
        unit_id: u64,
        /// Zero-based line index within the addressed payload.
        line: usize,
    },
    /// A line within a source card.
    SourceCardLine {
        /// Identity of the associated reading unit.
        unit_id: u64,
        /// Zero-based line index within the addressed payload.
        line: usize,
    },
    /// One table cell, independently searchable.
    TableCell {
        /// Identity of the associated reading unit.
        unit_id: u64,
        /// Body row identity, or None for the header.
        row_key: Option<u64>,
        /// The source-derived column identity.
        column_key: u64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// A nonempty match with both UTF-8 byte and character offsets.
pub struct SearchMatch {
    /// Exact accepted-source bytes when the projection preserves raw source.
    pub source_range: Option<Range<usize>>,
    /// The unit, line, or cell containing the match.
    pub address: SearchAddress,
    /// UTF-8 byte range in the addressed readable text.
    pub range: Range<usize>,
    /// Zero-based character columns within the matched line.
    pub char_range: Range<usize>,
}
#[derive(Clone, Debug)]
/// Revision-addressed results with explicit coverage and storage bounds.
pub struct SearchResults {
    /// Source identity; replacement allocates a new epoch.
    pub epoch: u64,
    /// Monotonic accepted revision within this source epoch.
    pub revision: u64,
    /// The query generation that produced these results.
    pub generation: u64,
    /// Stored matches in reading order.
    pub matches: Vec<SearchMatch>,
    /// Total matches in the covered window, including hits beyond storage
    /// capacity.
    pub count: usize,
    /// Whether all searchable bytes were examined.
    pub coverage_complete: bool,
    /// Whether matching records exceeded the storage bound.
    pub storage_limited: bool,
    /// Bytes consumed from the configured search window.
    pub scanned_bytes: usize,
}
/// One bounded scan over a compiled query and displayed revision.
struct Run<'a> {
    /// Compiled query shared by this scan.
    query: &'a CompiledQuery,
    /// Explicit work and result-storage bounds.
    limits: &'a SearchLimits,
    /// Accumulated covered-window matches and counters.
    result: SearchResults,
}
impl<'a> Run<'a> {
    /// Start one bounded result window for the displayed source revision.
    fn new(
        source: &SourceSnapshot,
        generation: u64,
        query: &'a CompiledQuery,
        limits: &'a SearchLimits,
    ) -> Self {
        Self {
            query,
            limits,
            result: SearchResults {
                epoch: source.epoch,
                revision: source.revision,
                generation,
                matches: Vec::new(),
                count: 0,
                coverage_complete: true,
                storage_limited: false,
                scanned_bytes: 0,
            },
        }
    }
    /// Match one bounded logical line and retain exact source mapping when
    /// known.
    fn line(
        &mut self,
        line: &str,
        address: &SearchAddress,
        offset: usize,
        source_start: Option<usize>,
    ) -> bool {
        if self.query.matcher.is_none() {
            return true;
        }
        let remaining = self
            .limits
            .max_scan_bytes
            .saturating_sub(self.result.scanned_bytes);
        let mut end = line.len().min(remaining);
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        let covered = &line[..end];
        self.result.scanned_bytes += end;
        let mut at = 0;
        while at <= covered.len() {
            let found = match self.query.find_at(covered, at) {
                Ok(Some(found)) => found,
                Ok(None) => break,
                Err(_) => {
                    self.result.coverage_complete = false;
                    return false;
                }
            };
            if found.start == found.end {
                if found.end == covered.len() {
                    break;
                }
                at = found.end
                    + covered[found.end..]
                        .chars()
                        .next()
                        .map_or(1, char::len_utf8);
                continue;
            }
            self.result.count += 1;
            if self.result.matches.len() < self.limits.max_matches {
                self.result.matches.push(SearchMatch {
                    address: address.clone(),
                    source_range: source_start.map(|start| start + found.start..start + found.end),
                    range: offset + found.start..offset + found.end,
                    char_range: covered[..found.start].chars().count()
                        ..covered[..found.end].chars().count(),
                });
            } else {
                self.result.storage_limited = true;
            }
            at = found.end;
        }
        if end < line.len() {
            self.result.coverage_complete = false;
            return false;
        }
        true
    }
    /// Scan line-local matches while charging separators to the byte budget.
    fn lines(
        &mut self,
        text: &str,
        mut address: impl FnMut(usize) -> SearchAddress,
        unit_offsets: bool,
        source_base: Option<usize>,
    ) -> bool {
        let mut offset = 0;
        for (index, raw) in text.split('\n').enumerate() {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            let separator = usize::from(index > 0) + usize::from(raw.ends_with('\r'));
            if separator
                > self
                    .limits
                    .max_scan_bytes
                    .saturating_sub(self.result.scanned_bytes)
            {
                self.result.coverage_complete = false;
                return false;
            }
            self.result.scanned_bytes += separator;
            if !self.line(
                line,
                &address(index),
                if unit_offsets { offset } else { 0 },
                source_base.map(|base| base + offset),
            ) {
                return false;
            }
            offset += raw.len() + 1;
        }
        true
    }
}
/// Search exact source lines with bounded scanning and storage.
pub fn search_source(
    source: &SourceSnapshot,
    query: &CompiledQuery,
    generation: u64,
    limits: &SearchLimits,
) -> SearchResults {
    let mut run = Run::new(source, generation, query, limits);
    run.lines(
        &source.text(),
        |line| SearchAddress::SourceLine { line },
        false,
        Some(0),
    );
    run.result
}
/// Search readable units, code lines, and table cells independently.
pub fn search_document(
    document: &DocumentSnapshot,
    query: &CompiledQuery,
    generation: u64,
    limits: &SearchLimits,
) -> SearchResults {
    let mut run = Run::new(&document.source, generation, query, limits);
    'blocks: for block in &document.blocks {
        let Some(unit) = document.unit(block.unit_id) else {
            continue;
        };
        let id = unit.id;
        match &block.kind {
            BlockKind::Table(table) => {
                for (row_key, cells) in iter::once((None, &table.headers))
                    .chain(table.rows.iter().map(|r| (Some(r.key), &r.cells)))
                {
                    for cell in cells {
                        if !run.lines(
                            &cell.text,
                            |_| SearchAddress::TableCell {
                                unit_id: id,
                                row_key,
                                column_key: cell.key,
                            },
                            false,
                            None,
                        ) {
                            break 'blocks;
                        }
                    }
                }
            }
            BlockKind::Code(_) => {
                if !run.lines(
                    &unit.text,
                    |line| SearchAddress::CodeLine { unit_id: id, line },
                    false,
                    None,
                ) {
                    break;
                }
            }
            _ if unit.kind == UnitKind::Source => {
                if !run.lines(
                    &unit.text,
                    |line| SearchAddress::SourceCardLine { unit_id: id, line },
                    false,
                    Some(block.source.start),
                ) {
                    break;
                }
            }
            _ => {
                if !run.lines(
                    &unit.text,
                    |_| SearchAddress::Unit { unit_id: id },
                    true,
                    None,
                ) {
                    break;
                }
            }
        }
    }
    run.result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document::parse_document, source_store::SourceStore};
    fn results(pattern: &str, mode: SearchMode, case: SearchCase, text: &str) -> SearchResults {
        let limits = SearchLimits::default();
        let query = CompiledQuery::new(
            &SearchQuery {
                text: pattern.into(),
                mode,
                case,
            },
            &limits,
        )
        .unwrap();
        search_source(
            &SourceStore::static_text(text).unwrap().snapshot(),
            &query,
            7,
            &limits,
        )
    }
    #[test]
    fn literal_regex_and_smart_case_share_line_matching() {
        assert_eq!(
            results("a.b", SearchMode::Literal, SearchCase::Sensitive, "a.b axb").count,
            1
        );
        assert_eq!(
            results("a.b", SearchMode::Regex, SearchCase::Sensitive, "a.b axb").count,
            2
        );
        assert_eq!(
            results(
                "needle",
                SearchMode::Regex,
                SearchCase::Smart,
                "needle Needle NEEDLE"
            )
            .count,
            3
        );
        assert_eq!(
            results(
                "Needle",
                SearchMode::Regex,
                SearchCase::Smart,
                "needle Needle NEEDLE"
            )
            .count,
            1
        );
        assert_eq!(
            results("x*", SearchMode::Regex, SearchCase::Sensitive, "é終").count,
            0
        );
        assert_eq!(
            results("é+", SearchMode::Regex, SearchCase::Sensitive, "aéé").matches[0].char_range,
            1..3
        );
    }
    #[test]
    fn invalid_multiline_and_limits_are_explicit() {
        let limits = SearchLimits {
            max_query_bytes: 4,
            max_matches: 1,
            max_scan_bytes: 5,
        };
        assert!(
            CompiledQuery::new(
                &SearchQuery {
                    text: "(".into(),
                    mode: SearchMode::Regex,
                    case: SearchCase::Smart
                },
                &limits
            )
            .is_err()
        );
        assert!(
            CompiledQuery::new(
                &SearchQuery {
                    text: "a\nb".into(),
                    ..Default::default()
                },
                &limits
            )
            .is_err()
        );
        let query = CompiledQuery::new(
            &SearchQuery {
                text: "a".into(),
                ..Default::default()
            },
            &limits,
        )
        .unwrap();
        let source = SourceStore::static_text("aaaaaa").unwrap().snapshot();
        let result = search_source(&source, &query, 1, &limits);
        assert_eq!(result.count, 5);
        assert_eq!(result.matches.len(), 1);
        assert!(!result.coverage_complete);
        assert!(result.storage_limited);
    }
    #[test]
    fn rendered_search_is_cell_and_line_addressed_and_source_finds_destinations() {
        let source = SourceStore::static_text(
            "[label](https://secret.invalid)\n\nA | B\n- | -\nx | y\n\n```\nx\nx\n```\n",
        )
        .unwrap()
        .snapshot();
        let doc = parse_document(&source, &Default::default(), None);
        let limits = SearchLimits::default();
        let query = CompiledQuery::new(
            &SearchQuery {
                text: "x".into(),
                ..Default::default()
            },
            &limits,
        )
        .unwrap();
        let result = search_document(&doc, &query, 9, &limits);
        assert_eq!(result.count, 3);
        assert!(matches!(
            result.matches[0].address,
            SearchAddress::TableCell {
                row_key: Some(_),
                ..
            }
        ));
        assert!(matches!(
            result.matches[1].address,
            SearchAddress::CodeLine { line: 0, .. }
        ));
        assert!(matches!(
            result.matches[2].address,
            SearchAddress::CodeLine { line: 1, .. }
        ));
        let query = CompiledQuery::new(
            &SearchQuery {
                text: "secret".into(),
                ..Default::default()
            },
            &limits,
        )
        .unwrap();
        assert_eq!(search_document(&doc, &query, 9, &limits).count, 0);
        assert_eq!(search_source(&source, &query, 9, &limits).count, 1);
        let limits = SearchLimits {
            max_scan_bytes: 2,
            ..Default::default()
        };
        let source = SourceStore::static_text("\n\n\n\n").unwrap().snapshot();
        let result = search_source(&source, &query, 9, &limits);
        assert!(!result.coverage_complete);
        assert_eq!(result.scanned_bytes, 2);
    }
    #[test]
    fn source_card_matches_carry_exact_original_byte_ranges() {
        let source = SourceStore::static_text("prefix\n\n<div>\r\nneedle\r\n")
            .unwrap()
            .snapshot();
        let document = parse_document(&source, &Default::default(), None);
        let limits = SearchLimits::default();
        let query = CompiledQuery::new(
            &SearchQuery {
                text: "needle".into(),
                ..Default::default()
            },
            &limits,
        )
        .unwrap();
        let result = search_document(&document, &query, 1, &limits);
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].source_range, Some(15..21));
        assert!(matches!(
            result.matches[0].address,
            SearchAddress::SourceCardLine { line: 1, .. }
        ));
        assert_eq!(
            search_source(&source, &query, 1, &limits).matches[0].source_range,
            Some(15..21)
        );
    }
}
