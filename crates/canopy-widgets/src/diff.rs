//! Line diffs of two full texts and the rows a diff view shows.
//!
//! A [`Diff`](crate::diff::Diff) holds both versions and the changed line
//! ranges between them. [`Diff::rows`](crate::diff::Diff::rows) derives the
//! display sequence for a [`Scope`](crate::diff::Scope): every line in
//! whole-file scope, or each change block with context and gap rows in context
//! scope. Unified and side-by-side renderers consume the same rows, and every
//! row names the line numbers it shows, so a renderer can ask a syntax
//! highlighter for the correct side and line.

use std::ops::Range;

use imara_diff::{Algorithm, InternedInput};

pub use crate::diff_view::{DiffModel, Mode};

/// How much unchanged text the rows show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Every line of both versions.
    WholeFile,
    /// Changes with this many unchanged lines around them.
    Context(usize),
}

impl Scope {
    /// Return the context lines around changes, or nothing in whole-file scope.
    fn context(self) -> Option<usize> {
        match self {
            Self::WholeFile => None,
            Self::Context(lines) => Some(lines),
        }
    }
}

/// One row of a diff.
///
/// Rows are ordered the way a unified diff reads: removed lines precede added
/// lines inside a change, and a side-by-side renderer pairs consecutive
/// removed and added runs at render time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffRow {
    /// A line both versions hold at the same place.
    Unchanged {
        /// Zero-based line in the old version.
        old: usize,
        /// Zero-based line in the new version.
        new: usize,
    },
    /// A line only the old version holds.
    Removed {
        /// Zero-based line in the old version.
        old: usize,
    },
    /// A line only the new version holds.
    Added {
        /// Zero-based line in the new version.
        new: usize,
    },
    /// A run of unchanged lines the context scope hides.
    Gap {
        /// Hidden old lines.
        old: Range<usize>,
        /// Hidden new lines.
        new: Range<usize>,
    },
    /// The heading that opens one change block in context scope.
    Header {
        /// Old lines the block shows, changes and context together.
        old: Range<usize>,
        /// New lines the block shows, changes and context together.
        new: Range<usize>,
    },
}

/// One changed region: old lines replaced by new lines.
#[derive(Clone, Debug)]
struct Hunk {
    /// Replaced old lines.
    old: Range<usize>,
    /// Replacing new lines.
    new: Range<usize>,
}

/// One change block in context scope: the lines it shows and the hunks it
/// holds.
#[derive(Clone, Debug)]
struct Block {
    /// Old lines the block shows, changes and context together.
    old: Range<usize>,
    /// New lines the block shows, changes and context together.
    new: Range<usize>,
    /// Indices of the hunks the block holds.
    hunks: Range<usize>,
}

/// A line diff of two full texts.
pub struct Diff {
    /// Old version text.
    old: String,
    /// New version text.
    new: String,
    /// Byte ranges of old lines, without their terminators.
    old_spans: Vec<Range<usize>>,
    /// Byte ranges of new lines, without their terminators.
    new_spans: Vec<Range<usize>>,
    /// Changed line ranges, in order.
    hunks: Vec<Hunk>,
    /// Lines the new version adds.
    insertions: usize,
    /// Lines the old version holds and the new version does not.
    deletions: usize,
}

impl Diff {
    /// Compute the line diff of `old` and `new`.
    ///
    /// The caller bounds the texts; a 1 MiB side is the intended ceiling for
    /// interactive use. An empty side or two equal texts skip the diff
    /// algorithm, which makes a whole-file creation or deletion immediate.
    #[must_use]
    pub fn new(old: impl Into<String>, new: impl Into<String>) -> Self {
        let old = old.into();
        let new = new.into();
        let old_spans = line_spans(&old);
        let new_spans = line_spans(&new);
        let hunks = if old == new {
            Vec::new()
        } else if old.is_empty() {
            vec![Hunk {
                old: 0..0,
                new: 0..new_spans.len(),
            }]
        } else if new.is_empty() {
            vec![Hunk {
                old: 0..old_spans.len(),
                new: 0..0,
            }]
        } else {
            let input = InternedInput::new(old.as_str(), new.as_str());
            let diff = imara_diff::Diff::compute(Algorithm::Histogram, &input);
            diff.hunks()
                .map(|hunk| Hunk {
                    old: hunk.before.start as usize..hunk.before.end as usize,
                    new: hunk.after.start as usize..hunk.after.end as usize,
                })
                .collect()
        };
        let insertions = hunks.iter().map(|hunk| hunk.new.len()).sum();
        let deletions = hunks.iter().map(|hunk| hunk.old.len()).sum();
        Self {
            old_spans,
            new_spans,
            old,
            new,
            hunks,
            insertions,
            deletions,
        }
    }

    /// Return the old version text.
    #[must_use]
    pub fn old_text(&self) -> &str {
        &self.old
    }

    /// Return the new version text.
    #[must_use]
    pub fn new_text(&self) -> &str {
        &self.new
    }

    /// Return the number of lines in the old version.
    #[must_use]
    pub fn old_len(&self) -> usize {
        self.old_spans.len()
    }

    /// Return the number of lines in the new version.
    #[must_use]
    pub fn new_len(&self) -> usize {
        self.new_spans.len()
    }

    /// Return one old line without its terminator.
    #[must_use]
    pub fn old_line(&self, line: usize) -> &str {
        &self.old[self.old_spans[line].clone()]
    }

    /// Return one new line without its terminator.
    #[must_use]
    pub fn new_line(&self, line: usize) -> &str {
        &self.new[self.new_spans[line].clone()]
    }

    /// Return the number of lines the new version adds.
    #[must_use]
    pub fn insertions(&self) -> usize {
        self.insertions
    }

    /// Return the number of lines the old version holds and the new does not.
    #[must_use]
    pub fn deletions(&self) -> usize {
        self.deletions
    }

    /// Return whether the versions differ.
    #[must_use]
    pub fn changed(&self) -> bool {
        !self.hunks.is_empty()
    }

    /// Return the display rows for `scope`.
    ///
    /// Each call rebuilds the sequence; a view caches the result until its
    /// scope or texts change.
    #[must_use]
    pub fn rows(&self, scope: Scope) -> Vec<DiffRow> {
        match scope.context() {
            None => self.whole_file_rows(),
            Some(context) => self.context_rows(context),
        }
    }

    /// Return every line of both versions.
    fn whole_file_rows(&self) -> Vec<DiffRow> {
        let mut rows = Vec::new();
        let mut old = 0;
        let mut new = 0;
        for hunk in &self.hunks {
            push_unchanged(&mut rows, old..hunk.old.start, new..hunk.new.start);
            push_change(&mut rows, hunk);
            old = hunk.old.end;
            new = hunk.new.end;
        }
        push_unchanged(&mut rows, old..self.old_len(), new..self.new_len());
        rows
    }

    /// Return each change block with `context` lines around it.
    fn context_rows(&self, context: usize) -> Vec<DiffRow> {
        let mut rows = Vec::new();
        let mut old = 0;
        let mut new = 0;
        for block in self.blocks(context) {
            if old < block.old.start {
                rows.push(DiffRow::Gap {
                    old: old..block.old.start,
                    new: new..block.new.start,
                });
            }
            old = block.old.start;
            new = block.new.start;
            rows.push(DiffRow::Header {
                old: block.old.clone(),
                new: block.new.clone(),
            });
            for hunk in &self.hunks[block.hunks.clone()] {
                push_unchanged(&mut rows, old..hunk.old.start, new..hunk.new.start);
                push_change(&mut rows, hunk);
                old = hunk.old.end;
                new = hunk.new.end;
            }
            push_unchanged(&mut rows, old..block.old.end, new..block.new.end);
            old = block.old.end;
            new = block.new.end;
        }
        if old < self.old_len() {
            rows.push(DiffRow::Gap {
                old: old..self.old_len(),
                new: new..self.new_len(),
            });
        }
        rows
    }

    /// Return the change blocks for a context scope, with nearby hunks merged.
    fn blocks(&self, context: usize) -> Vec<Block> {
        let mut blocks: Vec<Block> = Vec::new();
        for (index, hunk) in self.hunks.iter().enumerate() {
            let old = hunk.old.start.saturating_sub(context)
                ..(hunk.old.end + context).min(self.old_len());
            let new = hunk.new.start.saturating_sub(context)
                ..(hunk.new.end + context).min(self.new_len());
            match blocks.last_mut() {
                Some(last) if old.start <= last.old.end => {
                    last.old.end = last.old.end.max(old.end);
                    last.new.end = last.new.end.max(new.end);
                    last.hunks.end = index + 1;
                }
                _ => blocks.push(Block {
                    old,
                    new,
                    hunks: index..index + 1,
                }),
            }
        }
        blocks
    }
}

/// Push one unchanged run of equal length.
fn push_unchanged(rows: &mut Vec<DiffRow>, old: Range<usize>, new: Range<usize>) {
    debug_assert_eq!(old.len(), new.len(), "unchanged runs stay aligned");
    for (index, old_line) in old.enumerate() {
        rows.push(DiffRow::Unchanged {
            old: old_line,
            new: new.start + index,
        });
    }
}

/// Push one changed region: old lines, then new lines.
fn push_change(rows: &mut Vec<DiffRow>, hunk: &Hunk) {
    for old in hunk.old.clone() {
        rows.push(DiffRow::Removed { old });
    }
    for new in hunk.new.clone() {
        rows.push(DiffRow::Added { new });
    }
}

/// Return the byte range of each line, without its terminator.
///
/// The line count matches a line-token diff: an empty text holds no lines, and
/// a final newline ends the last line rather than opening another.
fn line_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte != b'\n' {
            continue;
        }
        let mut end = index;
        if end > start && text.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
        spans.push(start..end);
        start = index + 1;
    }
    if start < text.len() {
        let mut end = text.len();
        if end > start && text.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
        spans.push(start..end);
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::{Diff, DiffRow, Scope};

    #[test]
    fn identical_texts_have_no_changes() {
        let diff = Diff::new("a\nb\n", "a\nb\n");
        assert!(!diff.changed());
        assert_eq!(diff.insertions(), 0);
        assert_eq!(diff.deletions(), 0);
        assert_eq!(
            diff.rows(Scope::WholeFile),
            vec![
                DiffRow::Unchanged { old: 0, new: 0 },
                DiffRow::Unchanged { old: 1, new: 1 },
            ]
        );
        assert_eq!(
            diff.rows(Scope::Context(3)),
            vec![DiffRow::Gap {
                old: 0..2,
                new: 0..2
            }],
            "a context view of identical texts hides them all"
        );
    }

    #[test]
    fn an_added_line_reads_as_an_insertion() {
        let diff = Diff::new("a\nc\n", "a\nb\nc\n");
        assert_eq!(diff.insertions(), 1);
        assert_eq!(diff.deletions(), 0);
        assert_eq!(diff.old_len(), 2);
        assert_eq!(diff.new_len(), 3);
        assert_eq!(
            diff.rows(Scope::WholeFile),
            vec![
                DiffRow::Unchanged { old: 0, new: 0 },
                DiffRow::Added { new: 1 },
                DiffRow::Unchanged { old: 1, new: 2 },
            ]
        );
    }

    #[test]
    fn a_removed_line_reads_as_a_deletion() {
        let diff = Diff::new("a\nb\nc\n", "a\nc\n");
        assert_eq!(diff.insertions(), 0);
        assert_eq!(diff.deletions(), 1);
        assert_eq!(
            diff.rows(Scope::WholeFile),
            vec![
                DiffRow::Unchanged { old: 0, new: 0 },
                DiffRow::Removed { old: 1 },
                DiffRow::Unchanged { old: 2, new: 1 },
            ]
        );
    }

    #[test]
    fn a_replaced_line_removes_then_adds() {
        let diff = Diff::new("a\nb\n", "a\nc\n");
        assert_eq!(diff.insertions(), 1);
        assert_eq!(diff.deletions(), 1);
        assert_eq!(
            diff.rows(Scope::WholeFile),
            vec![
                DiffRow::Unchanged { old: 0, new: 0 },
                DiffRow::Removed { old: 1 },
                DiffRow::Added { new: 1 },
            ]
        );
    }

    #[test]
    fn an_empty_side_is_all_additions_or_deletions() {
        let added = Diff::new("", "a\nb");
        assert_eq!(added.insertions(), 2);
        assert_eq!(added.deletions(), 0);
        assert_eq!(added.old_len(), 0);
        assert_eq!(
            added.rows(Scope::WholeFile),
            vec![DiffRow::Added { new: 0 }, DiffRow::Added { new: 1 }]
        );

        let removed = Diff::new("a\nb", "");
        assert_eq!(removed.insertions(), 0);
        assert_eq!(removed.deletions(), 2);
        assert_eq!(
            removed.rows(Scope::WholeFile),
            vec![DiffRow::Removed { old: 0 }, DiffRow::Removed { old: 1 }]
        );
    }

    #[test]
    fn an_empty_text_holds_no_lines() {
        let diff = Diff::new("", "");
        assert!(!diff.changed());
        assert_eq!(diff.old_len(), 0);
        assert!(diff.rows(Scope::WholeFile).is_empty());
    }

    #[test]
    fn a_final_newline_differs_from_no_final_newline() {
        let diff = Diff::new("a", "a\n");
        assert!(diff.changed());
        assert_eq!(diff.old_len(), 1);
        assert_eq!(diff.new_len(), 1);
        assert_eq!(
            diff.rows(Scope::WholeFile),
            vec![DiffRow::Removed { old: 0 }, DiffRow::Added { new: 0 },],
            "the terminator is part of the line token"
        );
    }

    #[test]
    fn line_texts_drop_terminators_and_carriage_returns() {
        let diff = Diff::new("a\r\nb", "a\r\nb");
        assert_eq!(diff.old_line(0), "a");
        assert_eq!(diff.old_line(1), "b");
        assert_eq!(diff.new_line(0), "a");
        assert_eq!(diff.new_text(), "a\r\nb");
    }

    #[test]
    fn blank_lines_count_as_lines() {
        let diff = Diff::new("\n\n", "x\n\n");
        assert_eq!(diff.old_len(), 2);
        assert_eq!(diff.new_len(), 2);
        assert_eq!(diff.insertions(), 1);
        assert_eq!(diff.deletions(), 1, "the first blank line is replaced");
    }

    #[test]
    fn context_scope_hides_distant_lines() {
        let lines: Vec<String> = (0..20).map(|line| format!("line {line}\n")).collect();
        let old = lines.concat();
        let mut changed = lines;
        changed[10] = "changed\n".to_string();
        let new = changed.concat();
        let diff = Diff::new(old, new);
        assert_eq!(diff.insertions(), 1);
        assert_eq!(diff.deletions(), 1);
        let rows = diff.rows(Scope::Context(3));
        assert_eq!(
            rows,
            vec![
                DiffRow::Gap {
                    old: 0..7,
                    new: 0..7
                },
                DiffRow::Header {
                    old: 7..14,
                    new: 7..14
                },
                DiffRow::Unchanged { old: 7, new: 7 },
                DiffRow::Unchanged { old: 8, new: 8 },
                DiffRow::Unchanged { old: 9, new: 9 },
                DiffRow::Removed { old: 10 },
                DiffRow::Added { new: 10 },
                DiffRow::Unchanged { old: 11, new: 11 },
                DiffRow::Unchanged { old: 12, new: 12 },
                DiffRow::Unchanged { old: 13, new: 13 },
                DiffRow::Gap {
                    old: 14..20,
                    new: 14..20
                },
            ]
        );
    }

    #[test]
    fn context_scope_keeps_nearby_changes_in_one_block() {
        let lines: Vec<String> = (0..20).map(|line| format!("line {line}\n")).collect();
        let old = lines.concat();
        let mut changed = lines;
        changed[8] = "first\n".to_string();
        changed[11] = "second\n".to_string();
        let new = changed.concat();
        let diff = Diff::new(old, new);
        let rows = diff.rows(Scope::Context(3));
        assert_eq!(
            rows.iter()
                .filter(|row| matches!(row, DiffRow::Header { .. }))
                .count(),
            1,
            "blocks merge when their context overlaps"
        );
        assert_eq!(
            rows.iter()
                .filter(|row| matches!(row, DiffRow::Gap { .. }))
                .count(),
            2,
            "only the file head and tail are hidden"
        );
    }

    #[test]
    fn context_scope_without_changes_shows_gaps_only() {
        let lines: Vec<String> = (0..20).map(|line| format!("line {line}\n")).collect();
        let text = lines.concat();
        let diff = Diff::new(text.clone(), text);
        let rows = diff.rows(Scope::Context(3));
        assert_eq!(
            rows,
            vec![DiffRow::Gap {
                old: 0..20,
                new: 0..20
            }]
        );
    }

    #[test]
    fn a_short_file_shows_every_line_in_context_scope() {
        let diff = Diff::new("a\nb\n", "a\nc\n");
        assert_eq!(
            diff.rows(Scope::Context(3)),
            vec![
                DiffRow::Header {
                    old: 0..2,
                    new: 0..2
                },
                DiffRow::Unchanged { old: 0, new: 0 },
                DiffRow::Removed { old: 1 },
                DiffRow::Added { new: 1 },
            ],
            "a short block shows its header and every line"
        );
    }
}
