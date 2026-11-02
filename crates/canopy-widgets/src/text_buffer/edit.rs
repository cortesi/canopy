use super::Selection;

/// A single text edit applied to the buffer.
///
/// The edit starts at a char offset, not at a line and column: an edit can
/// change which chars break lines, as a line break after a carriage return
/// does, and an offset still names the same place when undo replays it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Char offset where the edit starts.
    pub start: usize,
    /// Text removed at the start.
    pub deleted: String,
    /// Text inserted in place of the removed text.
    pub inserted: String,
}

impl Edit {
    /// Construct a new edit.
    pub fn new(start: usize, deleted: String, inserted: String) -> Self {
        Self {
            start,
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
