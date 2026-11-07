//! Fuzzy ranking of text against a typed query.
//!
//! Scoring is `nucleo-matcher`'s, which applies fzf's algorithm: a match that
//! starts a word, and runs of consecutive characters, rank highest. The query
//! follows fzf's syntax: spaces separate terms that must all match, and `^`,
//! `$`, `'`, and `!` anchor, quote, or negate a term. [`Case`] says how case
//! matches, and [`Haystack`] whether the text is a path.
//!
//! A [`Pattern`] is a compiled query, and a [`Scorer`] holds the scratch space
//! that scoring reuses. A list that jumps to what the operator types ranks its
//! labels with [`rank`]. [`paint_matched`] draws matched text with the
//! graphemes that the match used picked out.

use canopy::{
    error::Result,
    geom::{Line, Point, Rect},
    render::Render,
    text,
};
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization},
};
use unicode_segmentation::UnicodeSegmentation;

/// How a query matches case.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Case {
    /// Case never matters.
    #[default]
    Ignore,
    /// A term with an uppercase letter matches case, and other terms ignore
    /// it.
    Smart,
}

/// What a haystack holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Haystack {
    /// Plain text.
    #[default]
    Text,
    /// Paths, which take bonuses for matches that start a path segment.
    Paths,
}

/// A compiled query.
#[derive(Clone, Debug)]
pub struct Pattern {
    /// The parsed fzf pattern.
    inner: nucleo_matcher::pattern::Pattern,
}

impl Pattern {
    /// Compile `query`, matching case as `case` says.
    pub fn new(query: &str, case: Case) -> Self {
        let case = match case {
            Case::Ignore => CaseMatching::Ignore,
            Case::Smart => CaseMatching::Smart,
        };
        Self {
            inner: nucleo_matcher::pattern::Pattern::parse(query, case, Normalization::Smart),
        }
    }

    /// Return whether the query has no terms, so every text matches it.
    pub fn is_empty(&self) -> bool {
        self.inner.atoms.is_empty()
    }
}

/// The matcher and the scratch space that scoring reuses.
pub struct Scorer {
    /// Matcher with its configured bonuses.
    matcher: Matcher,
    /// Characters of a non-ASCII haystack.
    buffer: Vec<char>,
}

impl Scorer {
    /// Construct a scorer for haystacks of `kind`.
    pub fn new(kind: Haystack) -> Self {
        let config = match kind {
            Haystack::Text => Config::DEFAULT,
            Haystack::Paths => Config::DEFAULT.match_paths(),
        };
        Self {
            matcher: Matcher::new(config),
            buffer: Vec::new(),
        }
    }

    /// Score `text` against `pattern`, or return `None` when it does not
    /// match. An empty pattern matches every text. The score's scale means
    /// nothing on its own: a higher score is a better match.
    pub fn score(&mut self, pattern: &Pattern, text: &str) -> Option<u32> {
        let haystack = Utf32Str::new(text, &mut self.buffer);
        pattern.inner.score(haystack, &mut self.matcher)
    }

    /// Return the sorted, distinct grapheme positions in `text` that a match
    /// of `pattern` used, or none when it does not match.
    ///
    /// Positions cost more than a score, so a caller asks for them only for
    /// the rows it shows.
    pub fn positions(&mut self, pattern: &Pattern, text: &str) -> Vec<u32> {
        let haystack = Utf32Str::new(text, &mut self.buffer);
        let mut found = Vec::new();
        if pattern
            .inner
            .indices(haystack, &mut self.matcher, &mut found)
            .is_none()
        {
            return Vec::new();
        }
        found.sort_unstable();
        found.dedup();
        found
    }
}

/// Return the indexes of the labels that match `query`, best first.
///
/// Case never matters. An empty query returns every index in order. Equally
/// scoring labels sort shortest first, then keep their original order.
pub fn rank<'a>(query: &str, labels: impl IntoIterator<Item = &'a str>) -> Vec<usize> {
    let pattern = Pattern::new(query, Case::Ignore);
    let labels = labels.into_iter();
    if pattern.is_empty() {
        return labels.enumerate().map(|(index, _)| index).collect();
    }
    let mut scorer = Scorer::new(Haystack::Text);
    let mut scored: Vec<(u32, usize, usize)> = labels
        .enumerate()
        .filter_map(|(index, label)| {
            scorer
                .score(&pattern, label)
                .map(|score| (score, label.chars().count(), index))
        })
        .collect();
    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });
    scored.into_iter().map(|(_, _, index)| index).collect()
}

/// Draw `text` in `style` from `at`, within `width` cells, with the graphemes
/// that a match used in the foreground and attributes of `match_style`.
///
/// `first` is the position of the first grapheme of `text` in the haystack
/// the match ran over, so a caller that cuts or wraps the haystack draws each
/// run with its own offset. `positions` is sorted, as [`Scorer::positions`]
/// returns it. The matched graphemes keep the ground of `style`, so a row
/// that a selection paints stays one band.
#[allow(clippy::too_many_arguments)]
pub fn paint_matched(
    render: &mut Render,
    at: Point,
    width: u32,
    text: &str,
    first: u32,
    positions: &[u32],
    style: &str,
    match_style: &str,
) -> Result<()> {
    render.text(style, Line::new(at.x, at.y, width), text)?;
    if positions.is_empty() || width == 0 {
        return Ok(());
    }
    let mut matched = render.resolve_style(style);
    let highlight = render.resolve_style(match_style);
    matched.fg = highlight.fg;
    matched.attrs = highlight.attrs;
    let row = Rect::new(at.x, at.y, width, 1);
    let limit = at.x.saturating_add(width);
    let mut x = at.x;
    for (offset, grapheme) in text.graphemes(true).enumerate() {
        let grapheme_width = u32::try_from(text::grapheme_width(grapheme)).unwrap_or(0);
        if x.saturating_add(grapheme_width) > limit {
            break;
        }
        let position = first.saturating_add(u32::try_from(offset).unwrap_or(u32::MAX));
        if positions.binary_search(&position).is_ok() {
            let point = Point { x, y: at.y };
            render.put_grapheme(matched.resolve_at(row, point), point, grapheme)?;
        }
        x = x.saturating_add(grapheme_width);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Return the labels matching `query`, best first.
    fn ranked<'a>(labels: &[&'a str], query: &str) -> Vec<&'a str> {
        rank(query, labels.iter().copied())
            .into_iter()
            .map(|index| labels[index])
            .collect()
    }

    /// Score `label` against `query`, ignoring case.
    fn score(query: &str, label: &str) -> Option<u32> {
        Scorer::new(Haystack::Text).score(&Pattern::new(query, Case::Ignore), label)
    }

    #[test]
    fn a_query_matches_ordered_characters_and_ignores_case() {
        assert!(score("nts", "notes.txt").is_some(), "n-t-s in order");
        assert!(score("NTS", "notes.txt").is_some(), "case is ignored");
        assert!(score("子", "子目录").is_some(), "unicode matches");
        assert!(score("stn", "notes.txt").is_none(), "order matters");
        assert!(score("nz", "notes.txt").is_none(), "missing characters");
        assert!(score("", "anything").is_some(), "an empty query matches");
    }

    #[test]
    fn smart_case_matches_case_only_for_an_uppercase_term() {
        let mut scorer = Scorer::new(Haystack::Text);
        let smart = |query| Pattern::new(query, Case::Smart);
        assert!(scorer.score(&smart("go"), "Go to").is_some());
        assert!(scorer.score(&smart("Go"), "Go to").is_some());
        assert!(scorer.score(&smart("GO"), "Go to").is_none());
        assert!(
            scorer
                .score(&Pattern::new("GO", Case::Ignore), "Go to")
                .is_some()
        );
    }

    #[test]
    fn spaces_separate_terms_that_must_all_match() {
        assert!(score("go home", "Go to › home").is_some());
        assert!(score("go away", "Go to › home").is_none());
        assert!(Pattern::new("  ", Case::Ignore).is_empty());
    }

    #[test]
    fn an_empty_query_keeps_every_label_in_order() {
        assert_eq!(ranked(&["b.txt", "a.txt"], ""), ["b.txt", "a.txt"]);
    }

    #[test]
    fn boundaries_and_runs_outrank_scattered_matches() {
        assert_eq!(
            ranked(&["xayb", "ab.txt", "axxb"], "ab"),
            ["ab.txt", "axxb", "xayb"],
            "a run at the start of a label wins"
        );
        assert_eq!(
            ranked(&["rabbit.rs", "random.stuff"], "rs"),
            ["rabbit.rs", "random.stuff"],
            "a run after a separator beats a scattered match"
        );
        assert_eq!(
            ranked(&["@core/agent", "@app/files", "@core/fs"], "fs"),
            ["@core/fs", "@app/files"],
            "a word after a slash starts a match"
        );
    }

    #[test]
    fn labels_that_do_not_match_are_dropped() {
        assert_eq!(
            ranked(&["notes.txt", "alpha.md"], "no"),
            ["notes.txt"],
            "only subsequence matches survive"
        );
    }

    #[test]
    fn length_breaks_equal_scores() {
        assert_eq!(ranked(&["axy", "ax"], "a"), ["ax", "axy"]);
        assert_eq!(ranked(&["axy", "axz"], "a"), ["axy", "axz"]);
    }

    #[test]
    fn positions_name_the_matched_graphemes() {
        let mut scorer = Scorer::new(Haystack::Text);
        let pattern = Pattern::new("gh", Case::Ignore);
        assert_eq!(scorer.positions(&pattern, "Go to › home"), [0, 8]);
        assert!(scorer.positions(&pattern, "root").is_empty());
        assert_eq!(
            scorer.positions(&Pattern::new("目", Case::Ignore), "子目录"),
            [1],
            "positions count graphemes, not bytes"
        );
    }
}
