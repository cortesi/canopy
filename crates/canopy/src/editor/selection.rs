use crate::editor::position::{TextPosition, TextRange};

/// A text selection expressed as an anchor and head position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Anchor position for the selection.
    anchor: TextPosition,
    /// Head position for the selection.
    head: TextPosition,
}

impl Selection {
    /// Construct a collapsed selection at a position.
    pub fn caret(position: TextPosition) -> Self {
        Self {
            anchor: position,
            head: position,
        }
    }

    /// Construct a selection from anchor and head positions.
    pub fn new(anchor: TextPosition, head: TextPosition) -> Self {
        Self { anchor, head }
    }

    /// Return the anchor position.
    pub fn anchor(self) -> TextPosition {
        self.anchor
    }

    /// Return the head position.
    pub fn head(self) -> TextPosition {
        self.head
    }

    /// Update the head position.
    pub fn set_head(&mut self, head: TextPosition) {
        self.head = head;
    }

    /// Update the anchor position.
    pub fn set_anchor(&mut self, anchor: TextPosition) {
        self.anchor = anchor;
    }

    /// Collapse the selection to a caret at the head position.
    pub fn collapse_to_head(&mut self) {
        self.anchor = self.head;
    }

    /// Return the selection range as a normalized text range.
    pub fn range(self) -> TextRange {
        TextRange::new(self.anchor, self.head).normalized()
    }

    /// Return true if the selection is empty.
    pub fn is_empty(self) -> bool {
        self.anchor == self.head
    }
}
