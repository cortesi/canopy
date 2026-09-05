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

/// Work invalidated by a mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalidation {
    /// Recompute layout, paint, cursor and observations.
    Layout,
    /// Repaint and publish the visible state.
    Paint,
    /// Republish semantic observations.
    Semantics,
}

/// Pending work accumulated independently of adapter scheduling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChangeSet {
    /// Geometry must be recomputed.
    pub layout: bool,
    /// Visible cells must be rebuilt.
    pub paint: bool,
    /// Cursor state must be refreshed.
    pub cursor: bool,
    /// Observations must be published.
    pub observation: bool,
}

impl ChangeSet {
    /// Record work, including the dependencies of the requested invalidation.
    pub fn invalidate(&mut self, invalidation: Invalidation) {
        match invalidation {
            Invalidation::Layout => {
                self.layout = true;
                self.paint = true;
                self.cursor = true;
            }
            Invalidation::Paint => {
                self.paint = true;
                self.cursor = true;
            }
            Invalidation::Semantics => {}
        }
        self.observation = true;
    }

    /// Whether any work remains to be published.
    pub fn is_pending(self) -> bool {
        self.layout || self.paint || self.cursor || self.observation
    }
}
