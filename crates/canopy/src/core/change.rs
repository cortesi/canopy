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
