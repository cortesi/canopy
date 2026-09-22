/// Outcome of an accepted state mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeOutcome {
    /// The requested state was already active.
    Unchanged,
    /// The request changed state.
    Changed,
}

impl ChangeOutcome {
    /// Return whether the request changed state.
    pub fn changed(self) -> bool {
        matches!(self, Self::Changed)
    }
}

/// Frame work invalidated by a mutation.
///
/// Levels are ordered: each level includes the work of every level below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Invalidation {
    /// Republish the frame snapshot.
    Semantics,
    /// Repaint the frame, then republish the snapshot.
    Paint,
    /// Recompute layout, then repaint and republish the snapshot.
    Layout,
}

/// The highest invalidation level recorded since the last frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChangeSet(Option<Invalidation>);

impl ChangeSet {
    /// Record work; a lower level never replaces a higher one.
    pub fn invalidate(&mut self, invalidation: Invalidation) {
        self.0 = self.0.max(Some(invalidation));
    }

    /// Whether geometry must be recomputed before the next hit test or frame.
    pub fn layout_pending(self) -> bool {
        self.0 == Some(Invalidation::Layout)
    }

    /// Whether any frame work remains to be published.
    pub fn is_pending(self) -> bool {
        self.0.is_some()
    }

    /// Return the recorded level.
    #[cfg(test)]
    pub fn level(self) -> Option<Invalidation> {
        self.0
    }
}
