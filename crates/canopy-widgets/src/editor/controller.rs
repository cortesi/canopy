use super::TextBuffer;

/// Editor control state that is derived from user movement and edit sessions.
#[derive(Debug, Clone)]
pub struct EditorController {
    /// Preferred display column for vertical movement.
    preferred_column: usize,
    /// Whether a text-entry transaction is active.
    text_entry_transaction: bool,
}

impl EditorController {
    /// Construct control state from the current buffer cursor.
    pub(crate) fn new(buffer: &TextBuffer, tab_stop: usize) -> Self {
        Self {
            preferred_column: buffer.column_for_position(buffer.cursor(), tab_stop),
            text_entry_transaction: false,
        }
    }

    /// Return the preferred display column for vertical movement.
    pub(crate) fn preferred_column(&self) -> usize {
        self.preferred_column
    }

    /// Refresh the preferred display column from the current cursor.
    pub(crate) fn refresh_preferred_column(&mut self, buffer: &TextBuffer, tab_stop: usize) {
        self.preferred_column = buffer.column_for_position(buffer.cursor(), tab_stop);
    }

    /// Begin a grouped text-entry transaction if needed.
    pub(crate) fn begin_text_entry_transaction(&mut self, buffer: &mut TextBuffer) {
        if !self.text_entry_transaction {
            buffer.begin_transaction();
            self.text_entry_transaction = true;
        }
    }

    /// Commit the active text-entry transaction if present.
    pub(crate) fn commit_text_entry_transaction(&mut self, buffer: &mut TextBuffer) {
        if self.text_entry_transaction {
            buffer.commit_transaction();
            self.text_entry_transaction = false;
        }
    }
}
