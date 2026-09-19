use std::mem;

use canopy::{
    Context, EventOutcome,
    event::{Event, key},
};
use unicode_segmentation::UnicodeSegmentation;

use super::{
    search::{PromptCommand, SearchDirection},
    widget::{Editor, is_word_char},
};
use crate::text_buffer::{Selection, TextPosition, TextRange};

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

/// A caret movement shared by insert and visual modes.
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

/// One visual-mode vi action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VisualCommand {
    /// Leave visual mode.
    Exit,
    /// Delete the selection.
    Delete,
    /// Yank the selection.
    Yank,
    /// Change the selection.
    Change,
    /// Indent the selection right or left.
    Indent(bool),
    /// Move the selection edge.
    Move(MoveCommand),
}

/// One pending multi-key vi command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingCommand {
    /// Jump to the first line.
    GotoTop,
    /// Move by display lines.
    MoveDisplay(isize),
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
    /// Move the caret.
    Move(MoveCommand),
    /// Move to the start of the line.
    LineStart,
    /// Move to the end of the line.
    LineEnd,
    /// Move to the first non-blank character.
    FirstNonBlank,
    /// Move forward one word.
    WordForward,
    /// Move back one word.
    WordBackward,
    /// Move to the word end.
    WordEnd,
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
    /// Jump to the last line.
    GotoLastLine,
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
    /// Enter visual mode and initialize selection.
    pub(super) fn enter_visual(&mut self, mode: VisualMode) {
        let cursor = self.buffer.cursor();
        self.vi.set_mode(ViMode::Visual(mode));
        let selection = match mode {
            VisualMode::Line => {
                let start = TextPosition::new(cursor.line, 0);
                let end = self.buffer.line_end_position(cursor.line, false);
                Selection::new(start, end)
            }
            VisualMode::Character => Selection::new(cursor, cursor),
        };
        self.buffer.set_selection(selection);
    }

    /// Exit visual mode and collapse selection.
    pub(super) fn exit_visual(&mut self) {
        let cursor = self.buffer.cursor();
        self.vi.set_mode(ViMode::Normal);
        self.buffer.set_selection(Selection::caret(cursor));
    }

    /// Classify one key in vi mode, accounting for prompt state and pending
    /// prefixes, without running its effect.
    pub(super) fn vi_command(&self, key: key::Key) -> Option<ViCommand> {
        if self.prompt.is_some() {
            return self.prompt_command(key).map(ViCommand::Prompt);
        }
        match self.vi.mode() {
            ViMode::Insert => self.insert_command(key).map(ViCommand::Insert),
            ViMode::Visual(mode) => {
                Self::visual_command(key).map(|command| ViCommand::Visual(mode, command))
            }
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
            key::KeyCode::Enter if self.config.multiline => InsertCommand::Newline,
            key::KeyCode::Left => InsertCommand::Move(MoveCommand::Left),
            key::KeyCode::Right => InsertCommand::Move(MoveCommand::Right),
            key::KeyCode::Up => InsertCommand::Move(MoveCommand::Up),
            key::KeyCode::Down => InsertCommand::Move(MoveCommand::Down),
            _ => return None,
        })
    }

    /// Classify one visual-mode key.
    fn visual_command(key: key::Key) -> Option<VisualCommand> {
        Some(match key.key {
            key::KeyCode::Esc => VisualCommand::Exit,
            key::KeyCode::Char('d' | 'x') => VisualCommand::Delete,
            key::KeyCode::Char('y') => VisualCommand::Yank,
            key::KeyCode::Char('c') => VisualCommand::Change,
            key::KeyCode::Char('>') => VisualCommand::Indent(true),
            key::KeyCode::Char('<') => VisualCommand::Indent(false),
            key::KeyCode::Char('h') | key::KeyCode::Left => VisualCommand::Move(MoveCommand::Left),
            key::KeyCode::Char('l') | key::KeyCode::Right => {
                VisualCommand::Move(MoveCommand::Right)
            }
            key::KeyCode::Char('j') | key::KeyCode::Down => VisualCommand::Move(MoveCommand::Down),
            key::KeyCode::Char('k') | key::KeyCode::Up => VisualCommand::Move(MoveCommand::Up),
            _ => return None,
        })
    }

    /// Classify one normal-mode key, resolving a pending prefix first.
    ///
    /// Pending matches ignore modifiers, exactly as the handler does, so a
    /// chord that would otherwise fall through completes the prefix.
    fn normal_command(&self, key: key::Key) -> Option<NormalCommand> {
        if let Some(pending) = self.vi.pending()
            && let Some(command) = Self::pending_command(pending, key)
        {
            return Some(NormalCommand::Pending(command));
        }
        Self::plain_command(key).map(NormalCommand::Plain)
    }

    /// Classify one pending-prefix continuation.
    fn pending_command(pending: PendingKey, key: key::Key) -> Option<PendingCommand> {
        let character = match key.key {
            key::KeyCode::Char(character) => character,
            _ => return None,
        };
        Some(match (pending, character) {
            (PendingKey::G, 'g') => PendingCommand::GotoTop,
            (PendingKey::G, 'j') => PendingCommand::MoveDisplay(1),
            (PendingKey::G, 'k') => PendingCommand::MoveDisplay(-1),
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
            key::KeyCode::Left => PlainCommand::Move(MoveCommand::Left),
            key::KeyCode::Right => PlainCommand::Move(MoveCommand::Right),
            key::KeyCode::Up => PlainCommand::Move(MoveCommand::Up),
            key::KeyCode::Down => PlainCommand::Move(MoveCommand::Down),
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
            key::KeyCode::Char('h') => PlainCommand::Move(MoveCommand::Left),
            key::KeyCode::Char('l') => PlainCommand::Move(MoveCommand::Right),
            key::KeyCode::Char('j') => PlainCommand::Move(MoveCommand::Down),
            key::KeyCode::Char('k') => PlainCommand::Move(MoveCommand::Up),
            key::KeyCode::Char('0') => PlainCommand::LineStart,
            key::KeyCode::Char('$') => PlainCommand::LineEnd,
            key::KeyCode::Char('^') => PlainCommand::FirstNonBlank,
            key::KeyCode::Char('w') => PlainCommand::WordForward,
            key::KeyCode::Char('b') => PlainCommand::WordBackward,
            key::KeyCode::Char('e') => PlainCommand::WordEnd,
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
            key::KeyCode::Char('G') => PlainCommand::GotoLastLine,
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
        if self.prompt.is_none() && self.vi.mode() == ViMode::Normal && self.vi.pending().is_some()
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
            PlainCommand::NextMatch => {
                if let Some(pos) = self.search.move_next(&self.buffer, false) {
                    self.buffer.set_cursor(pos);
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
            }
            PlainCommand::PrevMatch => {
                if let Some(pos) = self.search.move_next(&self.buffer, true) {
                    self.buffer.set_cursor(pos);
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
            }
            PlainCommand::ReplaceMode => {
                self.start_replace_prompt();
            }
            PlainCommand::Undo => {
                if !self.config.read_only {
                    self.undo_edit();
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
            }
            PlainCommand::Redo => {
                if !self.config.read_only {
                    self.redo_edit();
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
            }
            PlainCommand::Repeat => {
                self.repeat_last_edit();
                self.ensure_cursor_visible(ctx);
            }
            PlainCommand::Move(MoveCommand::Left) => {
                let moved = self.buffer.move_left(true);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
            }
            PlainCommand::Move(MoveCommand::Right) => {
                let moved = self.buffer.move_right(true);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
            }
            PlainCommand::Move(MoveCommand::Down) => {
                self.move_vertical(1);
                self.ensure_cursor_visible(ctx);
            }
            PlainCommand::Move(MoveCommand::Up) => {
                self.move_vertical(-1);
                self.ensure_cursor_visible(ctx);
            }
            PlainCommand::LineStart => {
                self.buffer.move_line_start();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
            }
            PlainCommand::LineEnd => {
                self.buffer.move_line_end();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
            }
            PlainCommand::FirstNonBlank => {
                self.buffer.move_line_first_non_ws();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
            }
            PlainCommand::WordForward => {
                self.move_word_forward();
                self.ensure_cursor_visible(ctx);
            }
            PlainCommand::WordBackward => {
                self.move_word_backward();
                self.ensure_cursor_visible(ctx);
            }
            PlainCommand::WordEnd => {
                self.move_word_end();
                self.ensure_cursor_visible(ctx);
            }
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
            PlainCommand::GotoLastLine => {
                let last_line = self.buffer.line_count().saturating_sub(1);
                let pos = TextPosition::new(last_line, 0);
                self.buffer.set_cursor(pos);
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
            }
        }
    }

    /// Run one pending-prefix command.
    fn execute_pending(&mut self, command: PendingCommand, ctx: &mut dyn Context) {
        match command {
            PendingCommand::GotoTop => {
                self.buffer.set_cursor(TextPosition::new(0, 0));
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
            }
            PendingCommand::MoveDisplay(direction) => {
                self.move_display_line(direction, ctx);
                self.ensure_cursor_visible(ctx);
            }
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

    /// Run one visual-mode command.
    fn execute_visual(&mut self, mode: VisualMode, command: VisualCommand, ctx: &mut dyn Context) {
        match command {
            VisualCommand::Exit => self.exit_visual(),
            VisualCommand::Delete => {
                if self.config.read_only {
                    self.exit_visual();
                    return;
                }
                let linewise = matches!(mode, VisualMode::Line);
                let mut range = self.buffer.selection().range();
                if linewise {
                    range = self.linewise_range(range);
                }
                self.set_yank(range, linewise);
                self.replace_range(range, "");
                self.update_preferred_column();
                self.exit_visual();
                self.vi.set_last_edit(RepeatableEdit::DeleteChar);
            }
            VisualCommand::Yank => {
                let linewise = matches!(mode, VisualMode::Line);
                let mut range = self.buffer.selection().range();
                if linewise {
                    range = self.linewise_range(range);
                }
                self.set_yank(range, linewise);
                self.exit_visual();
            }
            VisualCommand::Change => {
                if self.config.read_only {
                    self.exit_visual();
                    return;
                }
                let linewise = matches!(mode, VisualMode::Line);
                let mut range = self.buffer.selection().range();
                if linewise {
                    range = self.linewise_range(range);
                }
                self.set_yank(range, linewise);
                self.begin_text_entry_transaction();
                self.replace_range(range, "");
                self.exit_visual();
                self.vi.begin_insert();
                self.vi.set_last_edit(RepeatableEdit::ChangeLine);
            }
            VisualCommand::Indent(right) => {
                self.indent_selection(right, mode);
            }
            VisualCommand::Move(MoveCommand::Left) => {
                let anchor = self.buffer.selection().anchor();
                let moved = self.buffer.move_left(true);
                if moved {
                    self.update_visual_selection(anchor, mode);
                    self.ensure_cursor_visible(ctx);
                }
            }
            VisualCommand::Move(MoveCommand::Right) => {
                let anchor = self.buffer.selection().anchor();
                let moved = self.buffer.move_right(true);
                if moved {
                    self.update_visual_selection(anchor, mode);
                    self.ensure_cursor_visible(ctx);
                }
            }
            VisualCommand::Move(MoveCommand::Down) => {
                let anchor = self.buffer.selection().anchor();
                self.move_vertical(1);
                self.update_visual_selection(anchor, mode);
                self.ensure_cursor_visible(ctx);
            }
            VisualCommand::Move(MoveCommand::Up) => {
                let anchor = self.buffer.selection().anchor();
                self.move_vertical(-1);
                self.update_visual_selection(anchor, mode);
                self.ensure_cursor_visible(ctx);
            }
        }
    }

    /// Extend the current selection in visual mode.
    pub(super) fn extend_selection(&mut self, mode: VisualMode) {
        let mut selection = self.buffer.selection();
        selection.set_head(self.buffer.cursor());
        if let VisualMode::Line = mode {
            let range = selection.range();
            let start = TextPosition::new(range.start.line, 0);
            let end = self.buffer.line_end_position(range.end.line, false);
            self.buffer.set_selection(Selection::new(start, end));
        } else {
            self.buffer.set_selection(selection);
        }
    }

    /// Update a visual selection while preserving the anchor.
    pub(super) fn update_visual_selection(&mut self, anchor: TextPosition, mode: VisualMode) {
        let head = self.buffer.cursor();
        self.buffer.set_selection(Selection::new(anchor, head));
        self.extend_selection(mode);
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
        if self.config.read_only {
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
        if self.config.read_only {
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
        if self.config.read_only || self.yank.is_empty() {
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

    /// Indent or outdent the selected lines.
    pub(super) fn indent_selection(&mut self, indent: bool, mode: VisualMode) {
        if self.config.read_only {
            return;
        }
        if !self.config.multiline {
            return;
        }
        let range = self.buffer.selection().range();
        let start_line = range.start.line;
        let end_line = range.end.line;
        let tab = " ".repeat(self.config.tab_stop.max(1));
        let tab_stop = self.config.tab_stop;
        {
            let mut transaction = self.buffer.transaction();
            for line in start_line..=end_line {
                let line_start = TextPosition::new(line, 0);
                if indent {
                    transaction.replace_range(TextRange::new(line_start, line_start), &tab);
                } else {
                    let line_text = transaction.line_text(line);
                    let remove = line_text
                        .chars()
                        .take(tab_stop)
                        .take_while(|c| *c == ' ')
                        .count();
                    if remove > 0 {
                        let end = TextPosition::new(line, remove);
                        transaction.replace_range(TextRange::new(line_start, end), "");
                    }
                }
            }
        }
        if let VisualMode::Line = mode {
            self.extend_selection(mode);
        }
    }

    /// Move to the start of the next word crossing line boundaries.
    pub(super) fn move_word_forward(&mut self) {
        let mut line = self.buffer.cursor().line;
        let mut column = self.buffer.cursor().column;
        let line_count = self.buffer.line_count().max(1);
        let mut crossed_line = false;

        loop {
            let line_text = self.buffer.line_text(line);
            let chars: Vec<char> = line_text.chars().collect();
            let len = chars.len();
            if column >= len {
                if line + 1 >= line_count {
                    self.buffer.set_cursor(TextPosition::new(line, len));
                    self.update_preferred_column();
                    return;
                }
                line = line.saturating_add(1);
                column = 0;
                crossed_line = true;
                continue;
            }

            if !crossed_line && is_word_char(chars[column]) {
                while column < len && is_word_char(chars[column]) {
                    column = column.saturating_add(1);
                }
            }
            while column < len && !is_word_char(chars[column]) {
                column = column.saturating_add(1);
            }

            if column < len {
                self.buffer.set_cursor(TextPosition::new(line, column));
                self.update_preferred_column();
                return;
            }

            if line + 1 >= line_count {
                self.buffer.set_cursor(TextPosition::new(line, len));
                self.update_preferred_column();
                return;
            }
            line = line.saturating_add(1);
            column = 0;
            crossed_line = true;
        }
    }

    /// Move to the start of the previous word crossing line boundaries.
    pub(super) fn move_word_backward(&mut self) {
        let mut line = self.buffer.cursor().line;
        let mut column = self.buffer.cursor().column;

        loop {
            let line_text = self.buffer.line_text(line);
            let chars: Vec<char> = line_text.chars().collect();
            let len = chars.len();
            let mut idx = column.min(len);
            if idx == 0 {
                if line == 0 {
                    self.buffer.set_cursor(TextPosition::new(0, 0));
                    self.update_preferred_column();
                    return;
                }
                line = line.saturating_sub(1);
                column = self.buffer.line_char_len(line);
                continue;
            }

            idx = idx.saturating_sub(1);
            while idx > 0 && !is_word_char(chars[idx]) {
                idx = idx.saturating_sub(1);
            }

            if !is_word_char(chars[idx]) {
                if line == 0 {
                    self.buffer.set_cursor(TextPosition::new(0, 0));
                    self.update_preferred_column();
                    return;
                }
                line = line.saturating_sub(1);
                column = self.buffer.line_char_len(line);
                continue;
            }

            while idx > 0 && is_word_char(chars[idx.saturating_sub(1)]) {
                idx = idx.saturating_sub(1);
            }

            self.buffer.set_cursor(TextPosition::new(line, idx));
            self.update_preferred_column();
            return;
        }
    }

    /// Move to the end of the current word crossing line boundaries.
    pub(super) fn move_word_end(&mut self) {
        let mut line = self.buffer.cursor().line;
        let mut column = self.buffer.cursor().column;
        let line_count = self.buffer.line_count().max(1);

        loop {
            let line_text = self.buffer.line_text(line);
            let chars: Vec<char> = line_text.chars().collect();
            let len = chars.len();
            if column >= len {
                if line + 1 >= line_count {
                    self.buffer.set_cursor(TextPosition::new(line, len));
                    self.update_preferred_column();
                    return;
                }
                line = line.saturating_add(1);
                column = 0;
                continue;
            }

            let mut idx = column;
            while idx < len && !is_word_char(chars[idx]) {
                idx = idx.saturating_add(1);
            }
            if idx >= len {
                if line + 1 >= line_count {
                    self.buffer.set_cursor(TextPosition::new(line, len));
                    self.update_preferred_column();
                    return;
                }
                line = line.saturating_add(1);
                column = 0;
                continue;
            }
            while idx + 1 < len && is_word_char(chars[idx + 1]) {
                idx = idx.saturating_add(1);
            }
            self.buffer.set_cursor(TextPosition::new(line, idx));
            self.update_preferred_column();
            return;
        }
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
        if self.config.read_only {
            return;
        }
        let Some(edit) = self.vi.last_edit() else {
            return;
        };
        self.apply_edit(&edit);
    }
}
