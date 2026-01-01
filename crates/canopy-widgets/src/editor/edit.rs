use super::{Selection, TextRange};

/// A single text edit applied to the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Range replaced by the edit (before applying it).
    pub range: TextRange,
    /// Text removed from the range.
    pub deleted: String,
    /// Text inserted in place of the range.
    pub inserted: String,
}

impl Edit {
    /// Construct a new edit.
    pub fn new(range: TextRange, deleted: String, inserted: String) -> Self {
        Self {
            range,
            deleted,
            inserted,
        }
    }
}

/// A group of edits that form a single undo/redo step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    /// Edits in the order they were applied.
    pub edits: Vec<Edit>,
    /// Selection state before applying edits.
    pub before: Selection,
    /// Selection state after applying edits.
    pub after: Selection,
}

impl Transaction {
    /// Construct a new transaction with a starting selection.
    pub fn new(before: Selection) -> Self {
        Self {
            edits: Vec::new(),
            before,
            after: before,
        }
    }

    /// Return true when there are no edits recorded.
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Record the selection state after edits are applied.
    pub fn finish(&mut self, after: Selection) {
        self.after = after;
    }
}
