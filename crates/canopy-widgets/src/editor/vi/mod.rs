/// Character classes of word motions.
mod class;
/// Vim's word, sentence, and paragraph motions and text objects.
mod motion;
/// Visual mode.
mod visual;

use std::mem;

use canopy::{
    Context, EventOutcome,
    input::{Event, key},
};
use unicode_segmentation::UnicodeSegmentation;
pub(super) use visual::VisualCommand;

use super::{
    search::{PromptCommand, SearchDirection},
    widget::Editor,
};
use crate::text_buffer::{TextPosition, TextRange};

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
    /// Waiting for the kind of a visual-mode text object: inner without
    /// blanks, or with them when the flag is set.
    Object(bool),
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

/// One insert-mode vi action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InsertCommand {
    /// Leave insert mode.
    EndInsert,
    /// Insert one character.
    Insert(char),
    /// Delete backward.
    Backspace,
    /// Delete forward.
    Delete,
    /// Insert a newline.
    Newline,
    /// Move the caret.
    Move(MoveCommand),
}

/// A caret movement of insert mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MoveCommand {
    /// Move left.
    Left,
    /// Move right.
    Right,
    /// Move up.
    Up,
    /// Move down.
    Down,
}

/// A cursor motion of normal and visual modes, as in Vim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Motion {
    /// `h`: one grapheme left. Normal mode crosses to the line before.
    Left,
    /// `l`: one grapheme right. Normal mode crosses to the next line, and
    /// visual mode reaches the end of the line.
    Right,
    /// `k`: one line up, at the column that vertical moves keep.
    Up,
    /// `j`: one line down, at the column that vertical moves keep.
    Down,
    /// `0`: the start of the line.
    LineStart,
    /// `^`: the first char of the line that is not a blank.
    FirstNonBlank,
    /// `$`: the end of the line. Vertical moves then keep to line ends.
    LineEnd,
    /// `w`: the start of the next word.
    WordForward,
    /// `b`: the start of the word before.
    WordBackward,
    /// `e`: the end of the word.
    WordEnd,
    /// `gg`: the first line, at the column that vertical moves keep.
    Top,
    /// `G`: the last line, at the column that vertical moves keep.
    Bottom,
    /// `gj` and `gk`: display rows down or up.
    Row(isize),
}

/// One pending multi-key vi command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingCommand {
    /// Move the cursor.
    Motion(Motion),
    /// Delete the current line.
    DeleteLine,
    /// Change the current line.
    ChangeLine,
    /// Yank the current line.
    YankLine,
}

/// One normal-mode vi action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NormalCommand {
    /// Complete a pending prefix.
    Pending(PendingCommand),
    /// Run a single-key command after clearing any unmatched prefix.
    Plain(PlainCommand),
}

/// One single-key normal-mode vi action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PlainCommand {
    /// Enter insert before the cursor.
    BeginInsert,
    /// Enter insert after the cursor.
    AppendAfter,
    /// Enter insert at the line start.
    InsertLineStart,
    /// Enter insert at the line end.
    AppendLineEnd,
    /// Open a line below and enter insert.
    OpenBelow,
    /// Open a line above and enter insert.
    OpenAbove,
    /// Enter visual mode.
    BeginVisual(VisualMode),
    /// Clear any pending prefix.
    ClearPending,
    /// Start a search prompt.
    Search(SearchDirection),
    /// Move to the next search match.
    NextMatch,
    /// Move to the previous search match.
    PrevMatch,
    /// Start the replace prompt.
    ReplaceMode,
    /// Undo the last edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
    /// Repeat the last edit.
    Repeat,
    /// Move the cursor.
    Motion(Motion),
    /// Set a pending multi-key prefix.
    SetPending(PendingKey),
    /// Yank the current line.
    YankLine,
    /// Put the yank buffer, before the cursor when true.
    Put(bool),
    /// Delete the character under the cursor.
    DeleteChar,
    /// Delete to the line end.
    DeleteToEnd,
    /// Change to the line end.
    ChangeToEnd,
}

impl PlainCommand {
    /// Whether this command changes the buffer or enters an editing mode.
    fn edits(&self) -> bool {
        matches!(
            self,
            Self::BeginInsert
                | Self::AppendAfter
                | Self::InsertLineStart
                | Self::AppendLineEnd
                | Self::OpenBelow
                | Self::OpenAbove
                | Self::ReplaceMode
                | Self::Undo
                | Self::Redo
                | Self::Repeat
                | Self::SetPending(PendingKey::Delete | PendingKey::Change)
                | Self::Put(_)
                | Self::DeleteChar
                | Self::DeleteToEnd
                | Self::ChangeToEnd
        )
    }
}

/// One vi-mode action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ViCommand {
    /// A prompt action.
    Prompt(PromptCommand),
    /// An insert-mode action.
    Insert(InsertCommand),
    /// A visual-mode action.
    Visual(VisualMode, VisualCommand),
    /// A normal-mode action.
    Normal(NormalCommand),
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
    pub fn begin_insert(&mut self) {
        self.mode = ViMode::Insert;
        self.insert_text.clear();
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
    pub fn end_insert(&mut self) {
        self.mode = ViMode::Normal;
        self.pending = None;
        let text = mem::take(&mut self.insert_text);
        if !text.is_empty() {
            self.last_edit = Some(RepeatableEdit::Insert { text });
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

impl Editor {
    /// Classify one key in vi mode, accounting for prompt state and pending
    /// prefixes, without running its effect.
    pub(super) fn vi_command(&self, key: key::Key) -> Option<ViCommand> {
        if self.prompt.is_some() {
            return self.prompt_command(key).map(ViCommand::Prompt);
        }
        // A read-only buffer declines edit commands, so their keys reach the
        // host's bindings, as they do in text mode.
        let read_only = self.config.interaction.read_only();
        match self.vi.mode() {
            ViMode::Insert => self.insert_command(key).map(ViCommand::Insert),
            ViMode::Visual(mode) => self
                .visual_command(mode, key)
                .filter(|command| !(read_only && command.edits()))
                .map(|command| ViCommand::Visual(mode, command)),
            ViMode::Normal => self.normal_command(key).map(ViCommand::Normal),
        }
    }

    /// Classify one insert-mode key.
    fn insert_command(&self, key: key::Key) -> Option<InsertCommand> {
        Some(match key.key {
            key::KeyCode::Esc => InsertCommand::EndInsert,
            key::KeyCode::Char(character) if !key.mods.ctrl && !key.mods.alt => {
                InsertCommand::Insert(character)
            }
            key::KeyCode::Backspace => InsertCommand::Backspace,
            key::KeyCode::Delete => InsertCommand::Delete,
            key::KeyCode::Enter if self.enter_adds_line(key) => InsertCommand::Newline,
            key::KeyCode::Left => InsertCommand::Move(MoveCommand::Left),
            key::KeyCode::Right => InsertCommand::Move(MoveCommand::Right),
            key::KeyCode::Up => InsertCommand::Move(MoveCommand::Up),
            key::KeyCode::Down => InsertCommand::Move(MoveCommand::Down),
            _ => return None,
        })
    }

    /// Classify one normal-mode key, resolving a pending prefix first.
    ///
    /// Pending matches ignore modifiers, exactly as the handler does, so a
    /// chord that would otherwise fall through completes the prefix. Esc
    /// clears a pending prefix. With nothing pending it has no work, so it
    /// reaches the host's bindings, which can use it to cancel or close.
    /// With history arrows, Up and Down reach the host's bindings too, and j
    /// and k move the cursor.
    fn normal_command(&self, key: key::Key) -> Option<NormalCommand> {
        match self.vi.pending() {
            Some(pending) => {
                if let Some(command) = Self::pending_command(pending, key) {
                    return Some(NormalCommand::Pending(command));
                }
            }
            None if key.key == key::KeyCode::Esc => return None,
            None => {}
        }
        if self.config.history_arrows && matches!(key.key, key::KeyCode::Up | key::KeyCode::Down) {
            return None;
        }
        Self::plain_command(key)
            .filter(|command| !(self.config.interaction.read_only() && command.edits()))
            .map(NormalCommand::Plain)
    }

    /// Classify one pending-prefix continuation.
    fn pending_command(pending: PendingKey, key: key::Key) -> Option<PendingCommand> {
        let character = match key.key {
            key::KeyCode::Char(character) => character,
            _ => return None,
        };
        Some(match (pending, character) {
            (PendingKey::G, 'g') => PendingCommand::Motion(Motion::Top),
            (PendingKey::G, 'j') => PendingCommand::Motion(Motion::Row(1)),
            (PendingKey::G, 'k') => PendingCommand::Motion(Motion::Row(-1)),
            (PendingKey::Delete, 'd') => PendingCommand::DeleteLine,
            (PendingKey::Change, 'c') => PendingCommand::ChangeLine,
            (PendingKey::Yank, 'y') => PendingCommand::YankLine,
            _ => return None,
        })
    }

    /// Classify one single-key normal-mode command.
    fn plain_command(key: key::Key) -> Option<PlainCommand> {
        if (key.mods.ctrl || key.mods.alt)
            && !(key.mods.ctrl && matches!(key.key, key::KeyCode::Char('r')))
        {
            return None;
        }
        Some(match key.key {
            key::KeyCode::Esc => PlainCommand::ClearPending,
            key::KeyCode::Left => PlainCommand::Motion(Motion::Left),
            key::KeyCode::Right => PlainCommand::Motion(Motion::Right),
            key::KeyCode::Up => PlainCommand::Motion(Motion::Up),
            key::KeyCode::Down => PlainCommand::Motion(Motion::Down),
            key::KeyCode::Char('i') => PlainCommand::BeginInsert,
            key::KeyCode::Char('a') => PlainCommand::AppendAfter,
            key::KeyCode::Char('I') => PlainCommand::InsertLineStart,
            key::KeyCode::Char('A') => PlainCommand::AppendLineEnd,
            key::KeyCode::Char('o') => PlainCommand::OpenBelow,
            key::KeyCode::Char('O') => PlainCommand::OpenAbove,
            key::KeyCode::Char('v') => PlainCommand::BeginVisual(VisualMode::Character),
            key::KeyCode::Char('V') => PlainCommand::BeginVisual(VisualMode::Line),
            key::KeyCode::Char('/') => PlainCommand::Search(SearchDirection::Forward),
            key::KeyCode::Char('?') => PlainCommand::Search(SearchDirection::Backward),
            key::KeyCode::Char('n') => PlainCommand::NextMatch,
            key::KeyCode::Char('N') => PlainCommand::PrevMatch,
            key::KeyCode::Char('R') => PlainCommand::ReplaceMode,
            key::KeyCode::Char('u') => PlainCommand::Undo,
            key::KeyCode::Char('r') if key.mods.ctrl => PlainCommand::Redo,
            key::KeyCode::Char('.') => PlainCommand::Repeat,
            key::KeyCode::Char('h') => PlainCommand::Motion(Motion::Left),
            key::KeyCode::Char('l') => PlainCommand::Motion(Motion::Right),
            key::KeyCode::Char('j') => PlainCommand::Motion(Motion::Down),
            key::KeyCode::Char('k') => PlainCommand::Motion(Motion::Up),
            key::KeyCode::Char('0') => PlainCommand::Motion(Motion::LineStart),
            key::KeyCode::Char('$') => PlainCommand::Motion(Motion::LineEnd),
            key::KeyCode::Char('^') => PlainCommand::Motion(Motion::FirstNonBlank),
            key::KeyCode::Char('w') => PlainCommand::Motion(Motion::WordForward),
            key::KeyCode::Char('b') => PlainCommand::Motion(Motion::WordBackward),
            key::KeyCode::Char('e') => PlainCommand::Motion(Motion::WordEnd),
            key::KeyCode::Char('g') => PlainCommand::SetPending(PendingKey::G),
            key::KeyCode::Char('d') => PlainCommand::SetPending(PendingKey::Delete),
            key::KeyCode::Char('c') => PlainCommand::SetPending(PendingKey::Change),
            key::KeyCode::Char('y') => PlainCommand::SetPending(PendingKey::Yank),
            key::KeyCode::Char('Y') => PlainCommand::YankLine,
            key::KeyCode::Char('p') => PlainCommand::Put(false),
            key::KeyCode::Char('P') => PlainCommand::Put(true),
            key::KeyCode::Char('x') => PlainCommand::DeleteChar,
            key::KeyCode::Char('D') => PlainCommand::DeleteToEnd,
            key::KeyCode::Char('C') => PlainCommand::ChangeToEnd,
            key::KeyCode::Char('G') => PlainCommand::Motion(Motion::Bottom),
            _ => return None,
        })
    }

    /// Handle events in vi mode.
    pub(super) fn handle_vi_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        let Event::Key(key) = event else {
            if self.prompt.is_none()
                && self.vi.mode() == ViMode::Insert
                && let Event::Paste(content) = event
            {
                let inserted = self.handle_insert_text(content);
                self.vi.push_inserted(&inserted);
                self.ensure_cursor_visible(ctx);
                return EventOutcome::Handle;
            }
            self.reset_unmatched_pending();
            return EventOutcome::Ignore;
        };
        match self.vi_command(*key) {
            Some(command) => {
                self.execute_vi_command(command, ctx);
                EventOutcome::Handle
            }
            None => {
                self.reset_unmatched_pending();
                EventOutcome::Ignore
            }
        }
    }

    /// Clear a pending vi prefix when its follow-up key is not part of it.
    fn reset_unmatched_pending(&mut self) {
        if self.prompt.is_none() && self.vi.mode() != ViMode::Insert && self.vi.pending().is_some()
        {
            self.vi.set_pending(None);
        }
    }

    /// Run one classified vi command.
    fn execute_vi_command(&mut self, command: ViCommand, ctx: &mut dyn Context) {
        match command {
            ViCommand::Prompt(command) => self.execute_prompt(command, ctx),
            ViCommand::Insert(command) => self.execute_insert(command, ctx),
            ViCommand::Visual(mode, command) => self.execute_visual(mode, command, ctx),
            ViCommand::Normal(command) => self.execute_normal(command, ctx),
        }
    }

    /// Run one normal-mode command.
    fn execute_normal(&mut self, command: NormalCommand, ctx: &mut dyn Context) {
        let plain = match command {
            NormalCommand::Pending(command) => {
                self.execute_pending(command, ctx);
                return;
            }
            NormalCommand::Plain(plain) => plain,
        };
        self.vi.set_pending(None);
        match plain {
            PlainCommand::BeginInsert => {
                self.begin_text_entry_transaction();
                self.vi.begin_insert();
            }
            PlainCommand::AppendAfter => {
                let _ = self.buffer.move_right(self.config.multiline);
                self.update_preferred_column();
                self.begin_text_entry_transaction();
                self.vi.begin_insert();
            }
            PlainCommand::InsertLineStart => {
                self.buffer.move_line_start();
                self.update_preferred_column();
                self.begin_text_entry_transaction();
                self.vi.begin_insert();
            }
            PlainCommand::AppendLineEnd => {
                self.buffer.move_line_end();
                self.update_preferred_column();
                self.begin_text_entry_transaction();
                self.vi.begin_insert();
            }
            PlainCommand::OpenBelow => {
                self.record_and_apply(RepeatableEdit::OpenBelow);
            }
            PlainCommand::OpenAbove => {
                self.record_and_apply(RepeatableEdit::OpenAbove);
            }
            PlainCommand::BeginVisual(mode) => {
                self.enter_visual(mode);
            }
            PlainCommand::ClearPending => {
                self.vi.set_pending(None);
            }
            PlainCommand::Search(direction) => {
                self.start_search_prompt(direction);
            }
            PlainCommand::NextMatch => self.jump_to_match(ctx, false),
            PlainCommand::PrevMatch => self.jump_to_match(ctx, true),
            PlainCommand::ReplaceMode => {
                self.start_replace_prompt();
            }
            PlainCommand::Undo => {
                if !self.config.interaction.read_only() {
                    self.undo_edit();
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
            }
            PlainCommand::Redo => {
                if !self.config.interaction.read_only() {
                    self.redo_edit();
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
            }
            PlainCommand::Repeat => {
                self.repeat_last_edit();
                self.ensure_cursor_visible(ctx);
            }
            PlainCommand::Motion(motion) => self.apply_motion(motion, ctx),
            PlainCommand::SetPending(pending) => {
                self.vi.set_pending(Some(pending));
            }
            PlainCommand::YankLine => {
                self.yank_line();
            }
            PlainCommand::Put(before) => {
                let text = self.yank.clone();
                let linewise = self.yank_linewise;
                self.record_and_apply(RepeatableEdit::Put {
                    text,
                    linewise,
                    before,
                });
                self.ensure_cursor_visible(ctx);
            }
            PlainCommand::DeleteChar => {
                if self.record_and_apply(RepeatableEdit::DeleteChar) {
                    self.ensure_cursor_visible(ctx);
                }
            }
            PlainCommand::DeleteToEnd => {
                self.record_and_apply(RepeatableEdit::DeleteToEnd);
                self.ensure_cursor_visible(ctx);
            }
            PlainCommand::ChangeToEnd => {
                self.record_and_apply(RepeatableEdit::ChangeToEnd);
                self.ensure_cursor_visible(ctx);
            }
        }
    }

    /// Run one pending-prefix command.
    fn execute_pending(&mut self, command: PendingCommand, ctx: &mut dyn Context) {
        match command {
            PendingCommand::Motion(motion) => self.apply_motion(motion, ctx),
            PendingCommand::DeleteLine => {
                self.record_and_apply(RepeatableEdit::DeleteLine);
                self.ensure_cursor_visible(ctx);
            }
            PendingCommand::ChangeLine => {
                self.record_and_apply(RepeatableEdit::ChangeLine);
                self.ensure_cursor_visible(ctx);
            }
            PendingCommand::YankLine => {
                self.yank_line();
            }
        }
        self.vi.set_pending(None);
    }

    /// Run one insert-mode command.
    fn execute_insert(&mut self, command: InsertCommand, ctx: &mut dyn Context) {
        match command {
            InsertCommand::EndInsert => {
                self.commit_text_entry_transaction();
                self.vi.end_insert();
                self.ensure_cursor_visible(ctx);
            }
            InsertCommand::Insert(character) => {
                let text = character.encode_utf8(&mut [0; 4]).to_string();
                self.handle_insert_text(&text);
                self.vi.push_inserted(&text);
                self.ensure_cursor_visible(ctx);
            }
            InsertCommand::Backspace => {
                if self.handle_delete_backward() {
                    self.vi.pop_inserted_grapheme();
                    self.ensure_cursor_visible(ctx);
                }
            }
            InsertCommand::Delete => {
                if self.handle_delete_forward() {
                    self.ensure_cursor_visible(ctx);
                }
            }
            InsertCommand::Newline => {
                self.handle_insert_text("\n");
                self.vi.push_inserted("\n");
                self.ensure_cursor_visible(ctx);
            }
            InsertCommand::Move(MoveCommand::Left) => {
                let moved = self.buffer.move_left(self.config.multiline);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
            }
            InsertCommand::Move(MoveCommand::Right) => {
                let moved = self.buffer.move_right(self.config.multiline);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
            }
            InsertCommand::Move(MoveCommand::Up) => {
                self.move_vertical(-1);
                self.ensure_cursor_visible(ctx);
            }
            InsertCommand::Move(MoveCommand::Down) => {
                self.move_vertical(1);
                self.ensure_cursor_visible(ctx);
            }
        }
    }

    /// Expand a range to full line boundaries, including trailing newline.
    pub(super) fn linewise_range(&self, range: TextRange) -> TextRange {
        let range = range.normalized();
        let start = TextPosition::new(range.start.line, 0);
        let end = self.buffer.line_end_position(range.end.line, true);
        TextRange::new(start, end)
    }

    /// Delete the current line and update yank register.
    pub(super) fn delete_line(&mut self) {
        if self.config.interaction.read_only() {
            return;
        }
        let cursor = self.buffer.cursor();
        let line_count = self.buffer.line_count().max(1);
        let start = TextPosition::new(cursor.line, 0);
        let end = self.buffer.line_end_position(cursor.line, true);
        let yank_range = TextRange::new(start, end);
        let delete_range = if cursor.line + 1 == line_count && cursor.line > 0 {
            let prev_line = cursor.line.saturating_sub(1);
            let prev_len = self.buffer.line_char_len(prev_line);
            let start = TextPosition::new(prev_line, prev_len);
            let end = TextPosition::new(cursor.line, self.buffer.line_char_len(cursor.line));
            TextRange::new(start, end)
        } else {
            yank_range
        };
        self.set_yank(yank_range, true);
        self.replace_range(delete_range, "");
        if cursor.line + 1 == line_count && cursor.line > 0 {
            let prev_line = cursor.line.saturating_sub(1);
            self.buffer.set_cursor(TextPosition::new(prev_line, 0));
        }
        self.update_preferred_column();
    }

    /// Delete from the cursor to the line end and update yank register.
    pub(super) fn delete_to_line_end(&mut self) {
        if self.config.interaction.read_only() {
            return;
        }
        let cursor = self.buffer.cursor();
        let end = self.buffer.line_end_position(cursor.line, false);
        let range = TextRange::new(cursor, end);
        if range.is_empty() {
            return;
        }
        self.set_yank(range, false);
        self.replace_range(range, "");
        self.update_preferred_column();
    }

    /// Update the yank register with a range.
    pub(super) fn set_yank(&mut self, range: TextRange, linewise: bool) {
        self.yank = self.buffer.range_text(range);
        self.yank_linewise = linewise;
    }

    /// Yank the current line into the register.
    pub(super) fn yank_line(&mut self) {
        let cursor = self.buffer.cursor();
        let start = TextPosition::new(cursor.line, 0);
        let end = self.buffer.line_end_position(cursor.line, true);
        let range = TextRange::new(start, end);
        self.set_yank(range, true);
    }

    /// Put the yank register contents before or after the cursor.
    pub(super) fn put_yank(&mut self, before: bool) {
        if self.config.interaction.read_only() || self.yank.is_empty() {
            return;
        }
        let mut content = self.normalize_insert_text(&self.yank);
        let multiline = self.config.multiline;
        let linewise = self.yank_linewise;
        {
            let mut transaction = self.buffer.transaction();
            if linewise {
                let cursor = transaction.cursor();
                if multiline {
                    if !before && cursor.line + 1 == transaction.line_count() {
                        if transaction.line_char_len(cursor.line) > 0 {
                            content.insert(0, '\n');
                        }
                    } else if !content.ends_with('\n') {
                        content.push('\n');
                    }
                }
                let target = if before {
                    TextPosition::new(cursor.line, 0)
                } else {
                    transaction.line_end_position(cursor.line, true)
                };
                transaction.set_cursor(target);
            } else if !before {
                let _ = transaction.move_right(multiline);
            }
            transaction.insert_text(&content);
        }
        self.update_preferred_column();
    }

    /// Apply a repeatable edit using normal-mode key semantics.
    ///
    /// Returns whether the edit changed the buffer and should therefore be
    /// recorded as the last edit.
    fn apply_edit(&mut self, edit: &RepeatableEdit) -> bool {
        match edit {
            RepeatableEdit::Insert { text } => {
                self.handle_insert_text(text);
                true
            }
            RepeatableEdit::Put {
                text,
                linewise,
                before,
            } => {
                let changed = !text.is_empty();
                self.yank = text.clone();
                self.yank_linewise = *linewise;
                self.put_yank(*before);
                changed
            }
            RepeatableEdit::DeleteLine => {
                self.delete_line();
                true
            }
            RepeatableEdit::ChangeLine => {
                self.begin_text_entry_transaction();
                self.delete_line();
                self.vi.begin_insert();
                true
            }
            RepeatableEdit::DeleteChar => self.delete_char_forward(),
            RepeatableEdit::DeleteToEnd => {
                self.delete_to_line_end();
                true
            }
            RepeatableEdit::ChangeToEnd => {
                self.begin_text_entry_transaction();
                self.delete_to_line_end();
                self.vi.begin_insert();
                true
            }
            RepeatableEdit::OpenBelow => {
                self.begin_text_entry_transaction();
                if self.config.multiline {
                    let cursor = self.buffer.cursor();
                    let end = self.buffer.line_end_position(cursor.line, false);
                    self.buffer.set_cursor(end);
                    self.handle_insert_text("\n");
                }
                self.vi.begin_insert();
                true
            }
            RepeatableEdit::OpenAbove => {
                self.begin_text_entry_transaction();
                if self.config.multiline {
                    let cursor = self.buffer.cursor();
                    let start = TextPosition::new(cursor.line, 0);
                    self.buffer.set_cursor(start);
                    self.handle_insert_text("\n");
                    let _ = self.buffer.move_left(true);
                }
                self.vi.begin_insert();
                true
            }
        }
    }

    /// Apply an edit and record it as the last repeatable edit.
    ///
    /// Returns whether the edit changed the buffer.
    fn record_and_apply(&mut self, edit: RepeatableEdit) -> bool {
        let applied = self.apply_edit(&edit);
        if applied {
            self.vi.set_last_edit(edit);
        }
        applied
    }

    /// Repeat the last recorded vi edit.
    pub(super) fn repeat_last_edit(&mut self) {
        if self.config.interaction.read_only() {
            return;
        }
        let Some(edit) = self.vi.last_edit() else {
            return;
        };
        self.apply_edit(&edit);
    }
}
