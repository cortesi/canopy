use std::{
    mem,
    ops::{Deref, DerefMut},
};

use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

use super::{
    Selection, TextPosition, TextRange, display_width,
    edit::{Edit, Transaction},
    util::{next_grapheme_boundary, prev_grapheme_boundary},
};

/// Information about how an edit changed logical line counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineChange {
    /// First affected line index.
    pub start_line: usize,
    /// Number of lines replaced.
    pub old_line_count: usize,
    /// Number of lines inserted.
    pub new_line_count: usize,
}

/// Un-synced line-change state for the layout cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingChange {
    /// No un-synced edit.
    None,
    /// One incremental change since the last sync.
    One(LineChange),
    /// Two or more un-synced edits; the cache must rebuild.
    Dirty,
}

/// Rope-backed text buffer with selection and undo/redo support.
#[derive(Debug, Clone)]
pub struct TextBuffer {
    /// Rope storage for the buffer contents.
    rope: Rope,
    /// Current selection state.
    selection: Selection,
    /// Monotonic revision for cache invalidation.
    revision: u64,
    /// Latest line change since the last sync.
    pending_change: PendingChange,
    /// Undo history.
    undo: Vec<Transaction>,
    /// Redo history.
    redo: Vec<Transaction>,
    /// Active transaction for grouped edits.
    transaction: Option<Transaction>,
}

impl TextBuffer {
    /// Create a new buffer from an initial string.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let rope = Rope::from_str(&text);
        let line_count = rope.len_lines();
        let last_line = line_count.saturating_sub(1);
        let mut last_len = rope.line(last_line).len_chars();
        if last_line + 1 < line_count {
            last_len = last_len.saturating_sub(1);
        }
        let selection = Selection::caret(TextPosition::new(last_line, last_len));
        Self {
            rope,
            selection,
            revision: 0,
            pending_change: PendingChange::None,
            undo: Vec::new(),
            redo: Vec::new(),
            transaction: None,
        }
    }

    /// Return the current buffer revision.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Return the current selection.
    pub fn selection(&self) -> Selection {
        self.selection
    }

    /// Replace the selection, clamping to bounds.
    pub fn set_selection(&mut self, selection: Selection) {
        let anchor = self.clamp_position(selection.anchor());
        let head = self.clamp_position(selection.head());
        self.selection = Selection::new(anchor, head);
    }

    /// Return the cursor position (selection head).
    pub fn cursor(&self) -> TextPosition {
        self.selection.head()
    }

    /// Replace the cursor and collapse the selection.
    pub fn set_cursor(&mut self, pos: TextPosition) {
        let pos = self.clamp_position(pos);
        self.selection = Selection::caret(pos);
    }

    /// Return the full buffer contents as a string.
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Return the total number of logical lines.
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Return the line length in chars, excluding any trailing newline.
    pub fn line_char_len(&self, line: usize) -> usize {
        let line = line.min(self.line_count().saturating_sub(1));
        self.line_char_len_at(line)
    }

    /// Return the line length in chars without clamping the line index.
    fn line_char_len_at(&self, line: usize) -> usize {
        let slice = self.rope.line(line);
        slice.len_chars() - self.line_separator_len(line)
    }

    /// Return the trailing separator length without changing stored bytes.
    fn line_separator_len(&self, line: usize) -> usize {
        if line + 1 == self.line_count() {
            return 0;
        }
        let slice = self.rope.line(line);
        let len = slice.len_chars();
        if len >= 2 && slice.char(len - 2) == '\r' && slice.char(len - 1) == '\n' {
            2
        } else {
            1
        }
    }

    /// Return the text of a logical line without a trailing newline.
    pub fn line_text(&self, line: usize) -> String {
        let line = line.min(self.line_count().saturating_sub(1));
        self.line_text_at(line)
    }

    /// Return the text of a logical line without clamping the line index.
    fn line_text_at(&self, line: usize) -> String {
        let slice = self.rope.line(line);
        let mut text = slice.to_string();
        for _ in 0..self.line_separator_len(line) {
            let _ = text.pop();
        }
        text
    }

    /// Take the pending line change, if any.
    ///
    /// Returns `Some` only when exactly one edit has landed since the last
    /// sync. Multiple edits drain as `None` so the layout cache rebuilds.
    pub fn take_change(&mut self) -> Option<LineChange> {
        match mem::replace(&mut self.pending_change, PendingChange::None) {
            PendingChange::One(change) => Some(change),
            PendingChange::None | PendingChange::Dirty => None,
        }
    }

    /// Begin a grouped transaction.
    pub fn begin_transaction(&mut self) {
        if self.transaction.is_none() {
            self.transaction = Some(Transaction::new(self.selection));
        }
    }

    /// Commit the active transaction, if any.
    pub fn commit_transaction(&mut self) {
        let Some(mut transaction) = self.transaction.take() else {
            return;
        };
        if transaction.is_empty() {
            return;
        }
        transaction.finish(self.selection);
        self.undo.push(transaction);
        self.redo.clear();
    }

    /// Begin a grouped transaction that commits when the guard is dropped.
    pub fn transaction(&mut self) -> TextTransaction<'_> {
        self.begin_transaction();
        TextTransaction { buffer: self }
    }

    /// Undo the most recent transaction.
    pub fn undo(&mut self) -> bool {
        let Some(transaction) = self.undo.pop() else {
            return false;
        };
        for edit in transaction.edits.iter().rev() {
            self.apply_edit(edit, EditDirection::Undo);
        }
        self.selection = transaction.before;
        self.redo.push(transaction);
        true
    }

    /// Redo the most recently undone transaction.
    pub fn redo(&mut self) -> bool {
        let Some(transaction) = self.redo.pop() else {
            return false;
        };
        for edit in &transaction.edits {
            self.apply_edit(edit, EditDirection::Redo);
        }
        self.selection = transaction.after;
        self.undo.push(transaction);
        true
    }

    /// Insert text at the cursor, replacing any selection.
    pub fn insert_text(&mut self, text: &str) {
        self.replace_range(self.selection.range(), text);
    }

    /// Replace a range with the provided text.
    pub fn replace_range(&mut self, range: TextRange, text: &str) {
        let before = self.selection;
        let range = self.normalize_range(range);
        let start_char = self.position_to_char(range.start);
        let end_char = self.position_to_char(range.end);
        let deleted = self.rope.slice(start_char..end_char).to_string();

        self.rope.remove(start_char..end_char);
        self.rope.insert(start_char, text);

        let edit = Edit::new(range, deleted, text.to_string());
        let new_cursor = advance_position(range.start, text);
        self.selection = Selection::caret(new_cursor);
        self.record_edit(edit, before);
        self.bump_revision(range, text);
    }

    /// Delete the selection or the grapheme before the cursor.
    pub fn delete_backward(&mut self, allow_line_wrap: bool) -> bool {
        if !self.selection.is_empty() {
            let range = self.selection.range();
            self.replace_range(range, "");
            return true;
        }

        let cursor = self.selection.head();
        if cursor.column == 0 {
            if !allow_line_wrap || cursor.line == 0 {
                return false;
            }
            let prev_line = cursor.line.saturating_sub(1);
            let prev_len = self.line_char_len(prev_line);
            let start = TextPosition::new(prev_line, prev_len);
            let end = TextPosition::new(cursor.line, 0);
            self.replace_range(TextRange::new(start, end), "");
            return true;
        }

        let line_text = self.line_text(cursor.line);
        let prev = prev_grapheme_boundary(&line_text, cursor.column);
        let start = TextPosition::new(cursor.line, prev);
        let end = cursor;
        self.replace_range(TextRange::new(start, end), "");
        true
    }

    /// Return the range a forward delete at `from` would remove.
    ///
    /// At the end of a line the range joins the next line when
    /// `allow_line_wrap` is set. Returns `None` when there is nothing after
    /// the position to delete.
    pub fn forward_delete_range(
        &self,
        from: TextPosition,
        allow_line_wrap: bool,
    ) -> Option<TextRange> {
        if from.column >= self.line_char_len(from.line) {
            if !allow_line_wrap || from.line + 1 >= self.line_count() {
                return None;
            }
            return Some(TextRange::new(from, TextPosition::new(from.line + 1, 0)));
        }
        let next = next_grapheme_boundary(&self.line_text(from.line), from.column);
        Some(TextRange::new(from, TextPosition::new(from.line, next)))
    }

    /// Delete the selection or the grapheme after the cursor.
    pub fn delete_forward(&mut self, allow_line_wrap: bool) -> bool {
        if !self.selection.is_empty() {
            let range = self.selection.range();
            self.replace_range(range, "");
            return true;
        }

        let Some(range) = self.forward_delete_range(self.selection.head(), allow_line_wrap) else {
            return false;
        };
        self.replace_range(range, "");
        true
    }

    /// Move the cursor left by one grapheme.
    pub fn move_left(&mut self, allow_line_wrap: bool) -> bool {
        let cursor = self.selection.head();
        if cursor.column == 0 {
            if !allow_line_wrap || cursor.line == 0 {
                return false;
            }
            let prev_line = cursor.line - 1;
            let prev_len = self.line_char_len(prev_line);
            self.selection = Selection::caret(TextPosition::new(prev_line, prev_len));
            return true;
        }
        let line_text = self.line_text(cursor.line);
        let prev = prev_grapheme_boundary(&line_text, cursor.column);
        self.selection = Selection::caret(TextPosition::new(cursor.line, prev));
        true
    }

    /// Move the cursor right by one grapheme.
    pub fn move_right(&mut self, allow_line_wrap: bool) -> bool {
        let cursor = self.selection.head();
        let line_len = self.line_char_len(cursor.line);
        if cursor.column >= line_len {
            if !allow_line_wrap || cursor.line + 1 >= self.line_count() {
                return false;
            }
            self.selection = Selection::caret(TextPosition::new(cursor.line + 1, 0));
            return true;
        }
        let line_text = self.line_text(cursor.line);
        let next = next_grapheme_boundary(&line_text, cursor.column);
        self.selection = Selection::caret(TextPosition::new(cursor.line, next));
        true
    }

    /// Move the cursor to the start of the current line.
    pub fn move_line_start(&mut self) {
        let cursor = self.selection.head();
        self.selection = Selection::caret(TextPosition::new(cursor.line, 0));
    }

    /// Move the cursor to the end of the current line.
    pub fn move_line_end(&mut self) {
        let cursor = self.selection.head();
        let line_len = self.line_char_len(cursor.line);
        self.selection = Selection::caret(TextPosition::new(cursor.line, line_len));
    }

    /// Move the cursor to the first non-whitespace character in the line.
    pub fn move_line_first_non_ws(&mut self) {
        let cursor = self.selection.head();
        let column = self
            .line_text(cursor.line)
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .count();
        self.selection = Selection::caret(TextPosition::new(cursor.line, column));
    }

    /// Return the display column for a position.
    pub fn column_for_position(&self, pos: TextPosition, tab_stop: usize) -> usize {
        let line_text = self.line_text(pos.line);
        column_for_char(&line_text, pos.column, tab_stop)
    }

    /// Return the closest position for a display column within a line.
    pub fn position_for_column(&self, line: usize, column: usize, tab_stop: usize) -> TextPosition {
        // `char_for_column` already stops at the end of the line, so no pre-clamp is
        // needed.
        let char_index = char_for_column(&self.line_text(line), column, tab_stop);
        TextPosition::new(line, char_index)
    }

    /// Return the end position for a line, optionally including the newline.
    pub fn line_end_position(&self, line: usize, include_newline: bool) -> TextPosition {
        let line_len = self.line_char_len(line);

        if include_newline && line + 1 < self.line_count() {
            TextPosition::new(line + 1, 0)
        } else {
            TextPosition::new(line, line_len)
        }
    }

    /// Return the text in a range.
    pub fn range_text(&self, range: TextRange) -> String {
        let range = self.normalize_range(range);
        let start_char = self.position_to_char(range.start);
        let end_char = self.position_to_char(range.end);
        self.rope.slice(start_char..end_char).to_string()
    }

    /// Record an edit into the active transaction or history.
    fn record_edit(&mut self, edit: Edit, before: Selection) {
        if let Some(transaction) = self.transaction.as_mut() {
            transaction.edits.push(edit);
        } else {
            let mut transaction = Transaction::new(before);
            transaction.edits.push(edit);
            transaction.finish(self.selection);
            self.undo.push(transaction);
            self.redo.clear();
        }
    }

    /// Normalize and clamp a range within buffer bounds.
    fn normalize_range(&self, range: TextRange) -> TextRange {
        let normalized = range.normalized();
        let start = self.clamp_position(normalized.start);
        let end = self.clamp_position(normalized.end);
        TextRange::new(start, end)
    }

    /// Clamp a position to valid buffer bounds.
    fn clamp_position(&self, pos: TextPosition) -> TextPosition {
        let line = pos.line.min(self.line_count().saturating_sub(1));
        let column = pos.column.min(self.line_char_len(line));
        TextPosition::new(line, column)
    }

    /// Convert a text position to a rope char index.
    fn position_to_char(&self, pos: TextPosition) -> usize {
        let pos = self.clamp_position(pos);
        let line_start = self.rope.line_to_char(pos.line);
        line_start.saturating_add(pos.column)
    }

    /// Update revision tracking and pending line-change metadata.
    fn bump_revision(&mut self, range: TextRange, inserted: &str) {
        self.revision = self.revision.saturating_add(1);
        let range = range.normalized();
        let start_line = range.start.line;
        let end_line = range.end.line;
        let old_line_count = end_line.saturating_sub(start_line).saturating_add(1);
        let new_line_count = inserted.matches('\n').count().saturating_add(1);
        let change = LineChange {
            start_line,
            old_line_count,
            new_line_count,
        };
        self.pending_change = match self.pending_change {
            PendingChange::None => PendingChange::One(change),
            PendingChange::One(_) | PendingChange::Dirty => PendingChange::Dirty,
        };
    }

    /// Apply an edit in the specified direction.
    fn apply_edit(&mut self, edit: &Edit, direction: EditDirection) {
        let (remove_text, insert_text) = match direction {
            EditDirection::Undo => (edit.inserted.as_str(), edit.deleted.as_str()),
            EditDirection::Redo => (edit.deleted.as_str(), edit.inserted.as_str()),
        };

        let start = edit.range.start;
        let end = advance_position(start, remove_text);
        let start_char = self.position_to_char(start);
        let end_char = self.position_to_char(end);
        self.rope.remove(start_char..end_char);
        self.rope.insert(start_char, insert_text);
        self.revision = self.revision.saturating_add(1);
        self.pending_change = PendingChange::Dirty;
    }
}

/// Scoped text edit transaction.
pub struct TextTransaction<'a> {
    /// Buffer being edited.
    buffer: &'a mut TextBuffer,
}

impl Deref for TextTransaction<'_> {
    type Target = TextBuffer;

    fn deref(&self) -> &Self::Target {
        self.buffer
    }
}

impl DerefMut for TextTransaction<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buffer
    }
}

impl Drop for TextTransaction<'_> {
    fn drop(&mut self) {
        self.buffer.commit_transaction();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Direction to apply an edit for undo/redo.
enum EditDirection {
    /// Apply edits in the undo direction.
    Undo,
    /// Apply edits in the redo direction.
    Redo,
}

/// Advance a position by the text contents.
fn advance_position(start: TextPosition, text: &str) -> TextPosition {
    let mut line = start.line;
    let mut column = start.column;
    for (idx, part) in text.split('\n').enumerate() {
        if idx == 0 {
            column = column.saturating_add(part.chars().count());
        } else {
            line = line.saturating_add(1);
            column = part.chars().count();
        }
    }
    TextPosition::new(line, column)
}

/// Convert a char index to a display column.
fn column_for_char(line: &str, column: usize, tab_stop: usize) -> usize {
    let mut col = 0usize;
    let mut consumed = 0usize;
    for grapheme in line.graphemes(true) {
        let grapheme_chars = grapheme.chars().count();
        if consumed >= column {
            break;
        }
        let width = display_width(grapheme, col, tab_stop);
        col = col.saturating_add(width);
        consumed = consumed.saturating_add(grapheme_chars);
    }
    col
}

/// Convert a display column to a char index.
fn char_for_column(line: &str, column: usize, tab_stop: usize) -> usize {
    let mut col = 0usize;
    let mut chars = 0usize;
    for grapheme in line.graphemes(true) {
        let width = display_width(grapheme, col, tab_stop);
        if col + width > column {
            break;
        }
        col = col.saturating_add(width);
        chars = chars.saturating_add(grapheme.chars().count());
    }
    chars
}

#[cfg(test)]
mod tests {
    use proptest::{char, prelude::*};

    use super::*;

    #[test]
    fn insert_and_delete_roundtrip() {
        let mut buf = TextBuffer::new("hello");
        buf.insert_text("!");
        assert_eq!(buf.text(), "hello!");
        buf.delete_backward(true);
        assert_eq!(buf.text(), "hello");
    }

    #[test]
    fn grapheme_navigation_handles_emoji() {
        let mut buf = TextBuffer::new("a👩‍💻b");
        buf.set_cursor(TextPosition::new(0, 1));
        buf.move_right(true);
        assert_eq!(buf.cursor(), TextPosition::new(0, 4));
        buf.move_left(true);
        assert_eq!(buf.cursor(), TextPosition::new(0, 1));
    }

    #[test]
    fn column_mapping_respects_tabs() {
        let buf = TextBuffer::new("a\tb");
        let col = buf.column_for_position(TextPosition::new(0, 2), 4);
        assert_eq!(col, 4);
        let pos = buf.position_for_column(0, 4, 4);
        assert_eq!(pos.column, 2);
    }

    #[test]
    fn replace_range_updates_selection() {
        let mut buf = TextBuffer::new("hello");
        let start = TextPosition::new(0, 1);
        let end = TextPosition::new(0, 4);
        buf.replace_range(TextRange::new(start, end), "X");
        assert_eq!(buf.text(), "hXo");
        assert_eq!(buf.cursor(), TextPosition::new(0, 2));
    }

    #[test]
    fn delete_backward_merges_lines() {
        let mut buf = TextBuffer::new("a\nb");
        buf.set_cursor(TextPosition::new(1, 0));
        let deleted = buf.delete_backward(true);
        assert!(deleted);
        assert_eq!(buf.text(), "ab");
        assert_eq!(buf.cursor(), TextPosition::new(0, 1));
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut buf = TextBuffer::new("abc");
        buf.insert_text("d");
        assert_eq!(buf.text(), "abcd");
        assert!(buf.undo());
        assert_eq!(buf.text(), "abc");
        assert!(buf.redo());
        assert_eq!(buf.text(), "abcd");
    }

    #[test]
    fn transaction_guard_groups_edits_until_drop() {
        let mut buf = TextBuffer::new("a");
        {
            let mut transaction = buf.transaction();
            transaction.insert_text("b");
            transaction.insert_text("c");
        }

        assert_eq!(buf.text(), "abc");
        assert!(buf.undo());
        assert_eq!(buf.text(), "a");
    }

    #[test]
    fn standalone_history_restores_both_selections() {
        for (text, selection, inserted, expected, after) in [
            (
                "abc",
                Selection::caret(TextPosition::new(0, 3)),
                "d",
                "abcd",
                TextPosition::new(0, 4),
            ),
            (
                "abc",
                Selection::new(TextPosition::new(0, 1), TextPosition::new(0, 3)),
                "X",
                "aX",
                TextPosition::new(0, 2),
            ),
            (
                "abc",
                Selection::new(TextPosition::new(0, 1), TextPosition::new(0, 3)),
                "",
                "a",
                TextPosition::new(0, 1),
            ),
            (
                "a\nb\nc",
                Selection::new(TextPosition::new(0, 1), TextPosition::new(2, 1)),
                "",
                "a",
                TextPosition::new(0, 1),
            ),
        ] {
            let mut buffer = TextBuffer::new(text);
            buffer.set_selection(selection);
            buffer.insert_text(inserted);
            assert_eq!(buffer.text(), expected);
            assert_eq!(buffer.selection(), Selection::caret(after));
            assert!(buffer.undo());
            assert_eq!(buffer.text(), text);
            assert_eq!(buffer.selection(), selection);
            assert!(buffer.redo());
            assert_eq!(buffer.text(), expected);
            assert_eq!(buffer.selection(), Selection::caret(after));
        }
    }

    #[test]
    fn crlf_lines_exclude_both_separator_characters() {
        let mut buffer = TextBuffer::new("a\r\nb");
        assert_eq!(buffer.line_text(0), "a");
        assert_eq!(buffer.line_char_len(0), 1);
        buffer.set_cursor(TextPosition::new(0, 1));
        assert!(buffer.move_right(true));
        assert_eq!(buffer.cursor(), TextPosition::new(1, 0));
        assert!(buffer.delete_backward(true));
        assert_eq!(buffer.text(), "ab");
        assert_eq!(buffer.cursor(), TextPosition::new(0, 1));
        assert!(buffer.undo());
        assert_eq!(buffer.text(), "a\r\nb");
        assert_eq!(buffer.cursor(), TextPosition::new(1, 0));
        assert!(buffer.redo());
        assert_eq!(buffer.text(), "ab");
        assert_eq!(buffer.cursor(), TextPosition::new(0, 1));
        assert_eq!(TextBuffer::new("\r\n").line_text(0), "");
    }

    proptest! {
        #[test]
        fn replace_range_matches_string(
            text in text_strategy(),
            insert in text_strategy(),
            start in 0usize..=100,
            end in 0usize..=100,
        ) {
            let char_len = text.chars().count();
            let start = start.min(char_len);
            let end = end.min(char_len);
            let range_start = start.min(end);
            let range_end = start.max(end);

            let original = text.clone();
            let mut buf = TextBuffer::new(text.clone());
            let start_pos = position_for_char(&text, range_start);
            let end_pos = position_for_char(&text, range_end);
            buf.replace_range(TextRange::new(start_pos, end_pos), &insert);

            let mut expected = text;
            let start_byte = byte_index_for_char(&expected, range_start);
            let end_byte = byte_index_for_char(&expected, range_end);
            expected.replace_range(start_byte..end_byte, &insert);

            prop_assert_eq!(buf.text(), expected);
            prop_assert!(buf.undo());
            prop_assert_eq!(buf.text(), original);
        }
    }

    fn text_strategy() -> impl Strategy<Value = String> {
        let chars = prop_oneof![
            Just('\n'),
            Just('é'),
            Just('界'),
            Just(' '),
            char::range('a', 'z'),
            char::range('A', 'Z'),
        ];
        prop::collection::vec(chars, 0..40).prop_map(|items| items.into_iter().collect::<String>())
    }

    fn position_for_char(text: &str, char_index: usize) -> TextPosition {
        let mut line = 0usize;
        let mut column = 0usize;
        let mut count = 0usize;
        for ch in text.chars() {
            if count >= char_index {
                break;
            }
            if ch == '\n' {
                line = line.saturating_add(1);
                column = 0;
            } else {
                column = column.saturating_add(1);
            }
            count = count.saturating_add(1);
        }
        TextPosition::new(line, column)
    }

    fn byte_index_for_char(text: &str, char_index: usize) -> usize {
        if char_index == 0 {
            return 0;
        }
        if char_index >= text.chars().count() {
            return text.len();
        }
        text.char_indices()
            .nth(char_index)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len())
    }
}
