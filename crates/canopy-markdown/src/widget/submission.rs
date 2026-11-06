//! When the reader submits its replaceable preparation, and what it last
//! submitted.

use std::time::Instant;

use crate::{ViewType, presentation::GeometryKey};

/// The identity of a replaceable preparation request: the source's epoch and
/// revision, the view, the geometry, and the owner's and the query's
/// generations.
pub(super) type SubmitKey = (u64, u64, ViewType, GeometryKey, u64, u64);

/// Whether the reader owes its lane a submission, and the request it last
/// admitted, which a submission of the same request dedupes against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Submission {
    /// Nothing is owed.
    Clean {
        /// The request the lane last admitted.
        last: Option<SubmitKey>,
    },
    /// A submission is owed, at once or after the coalescing deadline.
    Due {
        /// When a coalesced submission goes, or `None` for at once.
        deadline: Option<Instant>,
        /// The request the lane last admitted.
        last: Option<SubmitKey>,
    },
}

impl Default for Submission {
    fn default() -> Self {
        Self::Due {
            deadline: None,
            last: None,
        }
    }
}

impl Submission {
    /// The request the lane last admitted.
    fn last(&self) -> Option<SubmitKey> {
        match *self {
            Self::Clean { last } | Self::Due { last, .. } => last,
        }
    }

    /// Whether a submission is owed.
    pub(super) fn is_due(&self) -> bool {
        matches!(self, Self::Due { .. })
    }

    /// When an owed coalesced submission goes.
    pub(super) fn deadline(&self) -> Option<Instant> {
        match *self {
            Self::Due { deadline, .. } => deadline,
            Self::Clean { .. } => None,
        }
    }

    /// Owe a submission: at once if `immediate`, or else by `coalesced`,
    /// unless a deadline is already set.
    pub(super) fn schedule(&mut self, immediate: bool, coalesced: Instant) {
        let deadline = match *self {
            _ if immediate => None,
            Self::Due {
                deadline: Some(deadline),
                ..
            } => Some(deadline),
            _ => Some(coalesced),
        };
        *self = Self::Due {
            deadline,
            last: self.last(),
        };
    }

    /// Owe a submission, by any deadline already set or else at once, and
    /// forget the last request, so the next one goes even if it is the
    /// same.
    pub(super) fn resubmit(&mut self) {
        *self = Self::Due {
            deadline: self.deadline(),
            last: None,
        };
    }

    /// Forget the last request, so the next submission goes even if it is
    /// the same, without owing one.
    pub(super) fn invalidate(&mut self) {
        match self {
            Self::Clean { last } | Self::Due { last, .. } => *last = None,
        }
    }

    /// Owe nothing more, as when a preparation failed, or a cached view
    /// shows. The last request stays.
    pub(super) fn settle(&mut self) {
        *self = Self::Clean { last: self.last() };
    }

    /// The submission of `key` is owed. Return whether it is the request the
    /// lane last admitted, in which case nothing is owed any more.
    pub(super) fn dedupe(&mut self, key: SubmitKey) -> bool {
        let same = self.last() == Some(key);
        if same {
            self.settle();
        }
        same
    }

    /// The lane admitted `key`.
    pub(super) fn sent(&mut self, key: SubmitKey) {
        *self = Self::Clean { last: Some(key) };
    }

    /// The lane refused the submission: it goes again at `retry`.
    pub(super) fn refused(&mut self, retry: Instant) {
        *self = Self::Due {
            deadline: Some(retry),
            last: self.last(),
        };
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn key(revision: u64) -> SubmitKey {
        (
            1,
            revision,
            ViewType::Rendered,
            GeometryKey::default(),
            1,
            0,
        )
    }

    #[test]
    fn each_event_moves_each_state() {
        let now = Instant::now();
        let soon = now + Duration::from_millis(5);
        let later = now + Duration::from_millis(9);
        let clean = Submission::Clean { last: Some(key(1)) };
        let at_once = Submission::Due {
            deadline: None,
            last: Some(key(1)),
        };
        let coalesced = Submission::Due {
            deadline: Some(soon),
            last: Some(key(1)),
        };
        let due = |deadline| Submission::Due {
            deadline,
            last: Some(key(1)),
        };
        // (state, immediate schedule, coalesced schedule, settle)
        for (state, immediate, coalescing, settled) in [
            (clean, due(None), due(Some(later)), clean),
            (at_once, due(None), due(Some(later)), clean),
            (coalesced, due(None), due(Some(soon)), clean),
        ] {
            let mut next = state;
            next.schedule(true, later);
            assert_eq!(next, immediate, "an immediate schedule from {state:?}");
            let mut next = state;
            next.schedule(false, later);
            assert_eq!(next, coalescing, "a coalesced schedule from {state:?}");
            let mut next = state;
            next.settle();
            assert_eq!(next, settled, "a settle from {state:?}");
            let mut next = state;
            next.invalidate();
            assert_eq!(
                next.is_due(),
                state.is_due(),
                "an invalidation of {state:?}"
            );
            assert_eq!(next.last(), None, "an invalidation of {state:?}");
            let mut next = state;
            next.sent(key(2));
            assert_eq!(next, Submission::Clean { last: Some(key(2)) });
            let mut next = state;
            next.refused(later);
            assert_eq!(next, due(Some(later)), "a refusal from {state:?}");
        }
    }

    #[test]
    fn a_deduped_submission_clears_its_deadline() {
        let now = Instant::now();
        let mut submission = Submission::default();
        submission.sent(key(1));
        submission.schedule(false, now + Duration::from_millis(5));
        assert!(submission.dedupe(key(1)));
        assert_eq!(submission, Submission::Clean { last: Some(key(1)) });
        assert_eq!(submission.deadline(), None, "no stale deadline stays");
        // A later coalesced schedule waits its own full delay.
        let later = now + Duration::from_millis(9);
        submission.schedule(false, later);
        assert_eq!(submission.deadline(), Some(later));
        assert!(!submission.dedupe(key(2)));
        assert!(submission.is_due());
    }

    #[test]
    fn a_refused_submission_retries_the_same_request() {
        let now = Instant::now();
        let retry = now + Duration::from_millis(1);
        let mut submission = Submission::default();
        submission.refused(retry);
        assert_eq!(submission.deadline(), Some(retry));
        // The lane admitted nothing, so the same request is not a duplicate.
        assert!(!submission.dedupe(key(1)));
        submission.sent(key(1));
        assert!(!submission.is_due());
    }
}
