//! The reader's search matches for its query and text, as one machine.

use super::MatchPlace;
use crate::{ViewType, search::SearchResults, source_view::SourceMatches};

/// What the reader's search holds: nothing yet, an error, or the matches of
/// one view and the current one.
#[derive(Debug)]
pub(super) enum Matches {
    /// The matches of the current query and text have not published. A
    /// host that reloads keeps the place that the first matches make
    /// current.
    Pending {
        /// Where the match lies that the next matches make current, without
        /// a reveal.
        kept: Option<MatchPlace>,
    },
    /// The query failed to compile or to scan. The error stays until the
    /// next matches, so a new text keeps it.
    Failed(String),
    /// Bounded Rendered matches for one displayed revision.
    Rendered {
        /// The matches.
        results: SearchResults,
        /// The index of the current match.
        current: Option<usize>,
    },
    /// Exact Source match ranges for one displayed revision.
    Source {
        /// The matches.
        matches: SourceMatches,
        /// The index of the current match.
        current: Option<usize>,
    },
}

impl Default for Matches {
    fn default() -> Self {
        Self::Pending { kept: None }
    }
}

impl Matches {
    /// The index of the current match.
    pub(super) fn current(&self) -> Option<usize> {
        match self {
            Self::Rendered { current, .. } | Self::Source { current, .. } => *current,
            Self::Pending { .. } | Self::Failed(_) => None,
        }
    }

    /// The index of the current Rendered match.
    pub(super) fn rendered_current(&self) -> Option<usize> {
        match self {
            Self::Rendered { current, .. } => *current,
            _ => None,
        }
    }

    /// The Rendered matches.
    pub(super) fn rendered(&self) -> Option<&SearchResults> {
        match self {
            Self::Rendered { results, .. } => Some(results),
            _ => None,
        }
    }

    /// The Source matches.
    pub(super) fn source(&self) -> Option<&SourceMatches> {
        match self {
            Self::Source { matches, .. } => Some(matches),
            _ => None,
        }
    }

    /// The query's error.
    pub(super) fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// How many matches `view` holds.
    pub(super) fn count(&self, view: ViewType) -> usize {
        match (self, view) {
            (Self::Rendered { results, .. }, ViewType::Rendered) => results.matches.len(),
            (Self::Source { matches, .. }, ViewType::Source) => matches.ranges.len(),
            _ => 0,
        }
    }

    /// Whether the matches of `view` have yet to publish.
    pub(super) fn waits(&self, view: ViewType) -> bool {
        match self {
            Self::Pending { .. } => true,
            Self::Failed(_) => false,
            Self::Rendered { .. } => view != ViewType::Rendered,
            Self::Source { .. } => view != ViewType::Source,
        }
    }

    /// A new query: nothing has published, and no place is kept.
    pub(super) fn requery(&mut self) {
        *self = Self::default();
    }

    /// A new text, or with `keep_place` a new view: the matches go, and so
    /// does a kept place unless `keep_place`. An error stays until the next
    /// matches.
    pub(super) fn invalidate(&mut self, keep_place: bool) {
        match self {
            Self::Failed(_) => {}
            Self::Pending { .. } if keep_place => {}
            _ => *self = Self::default(),
        }
    }

    /// Keep `place` for the next matches, while they wait. Return the place
    /// to restore now, among Rendered matches that have published already;
    /// with no `place`, no Rendered match stays current. Source keeps no
    /// place, and an error has no matches.
    pub(super) fn keep(&mut self, place: Option<MatchPlace>) -> Option<MatchPlace> {
        match self {
            Self::Pending { kept } => {
                *kept = place;
                None
            }
            Self::Rendered { current, .. } => {
                if place.is_none() {
                    *current = None;
                }
                place
            }
            Self::Source { .. } | Self::Failed(_) => None,
        }
    }

    /// Make match `index` current.
    pub(super) fn set_current(&mut self, index: Option<usize>) {
        if let Self::Rendered { current, .. } | Self::Source { current, .. } = self {
            *current = index;
        }
    }

    /// The query failed.
    pub(super) fn fail(&mut self, error: String) {
        *self = Self::Failed(error);
    }

    /// Source matches publish. The current match stays current where it
    /// still is.
    pub(super) fn publish_source(&mut self, matches: SourceMatches) {
        let current = match self {
            Self::Source {
                matches: old,
                current: Some(index),
            } => old
                .ranges
                .get(*index)
                .and_then(|range| matches.ranges.iter().position(|next| next == range)),
            _ => None,
        };
        *self = Self::Source { matches, current };
    }

    /// Rendered matches publish. The current match stays current where it
    /// still is. Return the place kept for these matches.
    pub(super) fn publish_rendered(&mut self, results: SearchResults) -> Option<MatchPlace> {
        let (current, kept) = match self {
            Self::Rendered {
                results: old,
                current: Some(index),
            } => (
                old.matches
                    .get(*index)
                    .and_then(|found| results.matches.iter().position(|next| next == found)),
                None,
            ),
            Self::Pending { kept } => (None, *kept),
            _ => (None, None),
        };
        *self = Self::Rendered { results, current };
        kept
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{SearchAddress, SearchMatch};

    fn found(unit_id: u64, start: usize) -> SearchMatch {
        SearchMatch {
            source_range: None,
            address: SearchAddress::Unit { unit_id },
            range: start..start + 1,
            char_range: start..start + 1,
        }
    }

    fn results(matches: Vec<SearchMatch>) -> SearchResults {
        SearchResults {
            epoch: 1,
            revision: 1,
            generation: 1,
            count: matches.len(),
            matches,
            coverage_complete: true,
            storage_limited: false,
            scanned_bytes: 0,
        }
    }

    /// Source matches of one byte at each of `starts`.
    fn source(starts: &[usize]) -> SourceMatches {
        SourceMatches {
            epoch: 1,
            revision: 1,
            ranges: starts.iter().map(|&start| start..start + 1).collect(),
            complete: true,
        }
    }

    const PLACE: MatchPlace = MatchPlace {
        source_byte: 4,
        rank: 1,
    };

    /// The state of `matches`, apart from what its matches hold.
    fn kind(matches: &Matches) -> &'static str {
        match matches {
            Matches::Pending { kept: None } => "pending",
            Matches::Pending { kept: Some(_) } => "kept",
            Matches::Failed(_) => "failed",
            Matches::Rendered { .. } => "rendered",
            Matches::Source { .. } => "source",
        }
    }

    /// One value of each state.
    fn states() -> [Matches; 5] {
        [
            Matches::Pending { kept: None },
            Matches::Pending { kept: Some(PLACE) },
            Matches::Failed("bad".into()),
            Matches::Rendered {
                results: results(vec![found(1, 0), found(1, 4)]),
                current: Some(1),
            },
            Matches::Source {
                matches: source(&[0]),
                current: Some(0),
            },
        ]
    }

    #[test]
    fn each_event_moves_each_state() {
        // (after a query, after a text, after a view, kept now, after a keep)
        let expected = [
            ("pending", "pending", "pending", None, "kept"),
            ("pending", "pending", "kept", None, "kept"),
            ("pending", "failed", "failed", None, "failed"),
            ("pending", "pending", "pending", Some(PLACE), "rendered"),
            ("pending", "pending", "pending", None, "source"),
        ];
        for (index, (query, text, view, now, keep)) in expected.into_iter().enumerate() {
            let state = || states().into_iter().nth(index).unwrap();
            let before = kind(&state());
            let mut next = state();
            next.requery();
            assert_eq!(kind(&next), query, "a query from {before}");
            let mut next = state();
            next.invalidate(false);
            assert_eq!(kind(&next), text, "a text from {before}");
            let mut next = state();
            next.invalidate(true);
            assert_eq!(kind(&next), view, "a view from {before}");
            let mut next = state();
            assert_eq!(next.keep(Some(PLACE)), now, "a keep from {before}");
            assert_eq!(kind(&next), keep, "a keep from {before}");
        }
    }

    #[test]
    fn published_matches_keep_the_current_match_and_take_the_kept_place() {
        let mut matches = Matches::Pending { kept: Some(PLACE) };
        let kept = matches.publish_rendered(results(vec![found(1, 0), found(1, 4)]));
        assert_eq!(kept, Some(PLACE));
        assert_eq!(matches.current(), None);
        matches.set_current(Some(1));
        // A new publication finds the current match among its own.
        let kept = matches.publish_rendered(results(vec![found(1, 4), found(2, 0)]));
        assert_eq!((kept, matches.current()), (None, Some(0)));
        // One that has it no more has none current.
        matches.publish_rendered(results(vec![found(2, 0)]));
        assert_eq!(matches.current(), None);
        // Source matches start with none current, and keep theirs.
        matches.publish_source(source(&[0, 4]));
        assert_eq!(matches.current(), None);
        matches.set_current(Some(1));
        matches.publish_source(source(&[4]));
        assert_eq!(matches.current(), Some(0));
        assert_eq!(matches.count(ViewType::Source), 1);
        assert_eq!(matches.count(ViewType::Rendered), 0);
        matches.fail("bad".into());
        assert_eq!((matches.error(), matches.current()), (Some("bad"), None));
        assert!(!matches.waits(ViewType::Rendered));
    }
}
