use unicode_segmentation::UnicodeSegmentation;

use super::TextPosition;

/// Vi mode state for the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViMode {
    /// Normal mode.
    Normal,
    /// Insert mode.
    Insert,
    /// Visual mode.
    Visual(VisualMode),
}

/// Visual selection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualMode {
    /// Character-wise visual mode.
    Character,
    /// Line-wise visual mode.
    Line,
}

/// Pending multi-key command state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingKey {
    /// Waiting for a second `d` or motion.
    Delete,
    /// Waiting for a second `c` or motion.
    Change,
    /// Waiting for a second `y`.
    Yank,
    /// Waiting for a `g` sequence.
    G,
}

/// Repeatable edit actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepeatableEdit {
    /// Repeat the last insert.
    Insert {
        /// Inserted text.
        text: String,
    },
    /// Put the yank buffer contents.
    Put {
        /// Yanked text.
        text: String,
        /// Whether the text is linewise.
        linewise: bool,
        /// Whether to insert before the cursor.
        before: bool,
    },
    /// Delete the current line.
    DeleteLine,
    /// Change the current line.
    ChangeLine,
    /// Delete the character under the cursor.
    DeleteChar,
    /// Delete to the end of the line.
    DeleteToEnd,
    /// Change to the end of the line.
    ChangeToEnd,
    /// Open a line below and enter insert.
    OpenBelow,
    /// Open a line above and enter insert.
    OpenAbove,
}

/// Vi state tracking for command parsing and inserts.
#[derive(Debug, Clone)]
pub struct ViState {
    /// Current vi mode.
    mode: ViMode,
    /// Pending multi-key command state.
    pending: Option<PendingKey>,
    /// Inserted text during the current insert session.
    insert_text: String,
    /// Cursor position at the start of the insert session.
    insert_start: Option<TextPosition>,
    /// Last repeatable edit.
    last_edit: Option<RepeatableEdit>,
}

impl ViState {
    /// Construct a new vi state in normal mode.
    pub fn new() -> Self {
        Self {
            mode: ViMode::Normal,
            pending: None,
            insert_text: String::new(),
            insert_start: None,
            last_edit: None,
        }
    }

    /// Return the current vi mode.
    pub fn mode(&self) -> ViMode {
        self.mode
    }

    /// Set the vi mode.
    pub fn set_mode(&mut self, mode: ViMode) {
        self.mode = mode;
        self.pending = None;
    }

    /// Return the pending key state.
    pub fn pending(&self) -> Option<PendingKey> {
        self.pending
    }

    /// Set the pending key state.
    pub fn set_pending(&mut self, pending: Option<PendingKey>) {
        self.pending = pending;
    }

    /// Begin an insert session.
    pub fn begin_insert(&mut self, start: TextPosition) {
        self.mode = ViMode::Insert;
        self.insert_text.clear();
        self.insert_start = Some(start);
        self.pending = None;
    }

    /// Record inserted text during insert mode.
    pub fn push_inserted(&mut self, text: &str) {
        self.insert_text.push_str(text);
    }

    /// Remove the last inserted grapheme during insert mode.
    pub fn pop_inserted_grapheme(&mut self) {
        if self.insert_text.is_empty() {
            return;
        }
        let new_len = self
            .insert_text
            .grapheme_indices(true)
            .next_back()
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        self.insert_text.truncate(new_len);
    }

    /// Finish the insert session and return a repeatable edit.
    pub fn end_insert(&mut self) -> Option<RepeatableEdit> {
        self.mode = ViMode::Normal;
        self.pending = None;
        let insert_text = self.insert_text.clone();
        self.insert_text.clear();
        self.insert_start = None;
        if insert_text.is_empty() {
            None
        } else {
            let edit = RepeatableEdit::Insert { text: insert_text };
            self.last_edit = Some(edit.clone());
            Some(edit)
        }
    }

    /// Set the last repeatable edit.
    pub fn set_last_edit(&mut self, edit: RepeatableEdit) {
        self.last_edit = Some(edit);
    }

    /// Return the last repeatable edit.
    pub fn last_edit(&self) -> Option<RepeatableEdit> {
        self.last_edit.clone()
    }
}

impl Default for ViState {
    fn default() -> Self {
        Self::new()
    }
}
