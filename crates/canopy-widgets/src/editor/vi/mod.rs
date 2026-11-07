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
    /// Character-wise visual mode: the selection holds the characters at
    /// both ends and every character between them.
    Character,
    /// Line-wise visual mode: the selection holds whole lines.
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

/// An edit that `.` repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RepeatableEdit {
    /// An insert session: its entry, then the keys typed after it.
    Insert {
        /// What started the session.
        entry: Entry,
        /// The keys of the session, after its last relocation.
        keys: Vec<InsertKey>,
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
    /// Delete the character under the cursor.
    DeleteChar,
    /// Delete to the end of the line.
    DeleteToEnd,
    /// Delete a visual selection of the extent, from the cursor.
    VisualDelete(Extent),
}

/// What starts an insert session, which `.` replays before the keys of
/// the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Entry {
    /// `i`: insert before the cursor. A session also starts a new segment
    /// with this entry after the cursor moves.
    Insert,
    /// `a`: insert after the cursor.
    Append,
    /// `I`: insert at the line start.
    InsertLineStart,
    /// `A`: insert at the line end.
    AppendLineEnd,
    /// `o`: open a line below.
    OpenBelow,
    /// `O`: open a line above.
    OpenAbove,
    /// `cc`: change the line.
    ChangeLine,
    /// `C`: change to the line end.
    ChangeToEnd,
    /// `c` in visual mode: change a selection of the extent, from the
    /// cursor.
    VisualChange(Extent),
}

impl Entry {
    /// Whether the entry edits the text itself, so that `.` repeats it
    /// even when the session typed nothing.
    fn edits(self) -> bool {
        !matches!(
            self,
            Self::Insert | Self::Append | Self::InsertLineStart | Self::AppendLineEnd
        )
    }
}

/// The extent of a visual selection, which `.` selects again from the
/// cursor, as Vim does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Extent {
    /// A count of whole lines.
    Lines(usize),
    /// Characters of one line: the display columns from the first
    /// character to the start of the last.
    Span(usize),
    /// Characters across lines: the count of lines, and the display column
    /// of the last character.
    Rows {
        /// The count of lines.
        lines: usize,
        /// The display column of the last character.
        column: usize,
    },
    /// Characters to the end of the last of a count of lines, line break
    /// included, as `v$` selects them.
    ToLineEnd(usize),
}

impl Extent {
    /// Return the kind of visual selection of the extent.
    pub(super) fn mode(self) -> VisualMode {
        match self {
            Self::Lines(_) => VisualMode::Line,
            Self::Span(_) | Self::Rows { .. } | Self::ToLineEnd(_) => VisualMode::Character,
        }
    }
}

/// One key of an insert session that `.` replays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InsertKey {
    /// Typed or pasted text, line breaks included.
    Text(String),
    /// Delete backward.
    Backspace,
    /// Delete forward.
    Delete,
}

/// The segment of an insert session that `.` repeats: its entry and the
/// keys after it. A relocation of the cursor ends a segment, as in Vim.
#[derive(Debug, Clone)]
struct InsertSession {
    /// What started the segment.
    entry: Entry,
    /// The keys of the segment, in order.
    keys: Vec<InsertKey>,
}

impl InsertSession {
    /// Start a segment at `entry`.
    fn new(entry: Entry) -> Self {
        Self {
            entry,
            keys: Vec::new(),
        }
    }

    /// Record one key, joining text to the text before it.
    fn push(&mut self, key: InsertKey) {
        match (self.keys.last_mut(), key) {
            (_, InsertKey::Text(text)) if text.is_empty() => {}
            (Some(InsertKey::Text(last)), InsertKey::Text(text)) => last.push_str(&text),
            (_, key) => self.keys.push(key),
        }
    }

    /// Return what `.` repeats of the segment: nothing for a segment that
    /// neither typed nor edited.
    fn record(self) -> Option<RepeatableEdit> {
        (self.entry.edits() || !self.keys.is_empty()).then_some(RepeatableEdit::Insert {
            entry: self.entry,
            keys: self.keys,
        })
    }
}

/// The vi mode, with the session of insert mode.
#[derive(Debug, Clone)]
enum State {
    /// Normal mode.
    Normal,
    /// Insert mode, and the segment that `.` would repeat.
    Insert(InsertSession),
    /// Visual mode.
    Visual(VisualMode),
}

/// A vi mode to move to, with the entry of an insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    /// Normal mode.
    Normal,
    /// Insert mode, started by the entry.
    Insert(Entry),
    /// Visual mode.
    Visual(VisualMode),
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
///
/// [`Editor::transition`] is the one writer of the mode.
#[derive(Debug, Clone)]
pub struct ViState {
    /// The mode, with the session of insert mode.
    state: State,
    /// Pending multi-key command state.
    pending: Option<PendingKey>,
    /// Last repeatable edit.
    last_edit: Option<RepeatableEdit>,
}

impl ViState {
    /// Construct a new vi state in normal mode.
    pub fn new() -> Self {
        Self {
            state: State::Normal,
            pending: None,
            last_edit: None,
        }
    }

    /// Return the current vi mode.
    pub fn mode(&self) -> ViMode {
        match self.state {
            State::Normal => ViMode::Normal,
            State::Insert(_) => ViMode::Insert,
            State::Visual(mode) => ViMode::Visual(mode),
        }
    }

    /// Return the pending key state.
    pub fn pending(&self) -> Option<PendingKey> {
        self.pending
    }

    /// Set the pending key state.
    pub fn set_pending(&mut self, pending: Option<PendingKey>) {
        self.pending = pending;
    }

    /// Record `edit`, when there is one, as the last repeatable edit.
    fn record(&mut self, edit: Option<RepeatableEdit>) {
        if let Some(edit) = edit {
            self.last_edit = Some(edit);
        }
    }

    /// Record one key of the insert session, if one is open.
    fn push_key(&mut self, key: InsertKey) {
        if let State::Insert(session) = &mut self.state {
            session.push(key);
        }
    }

    /// End the segment of an open insert session at a relocation of the
    /// cursor: record it, and start a new one at the cursor.
    pub(super) fn end_segment(&mut self) {
        if let State::Insert(session) = &mut self.state {
            let ended = mem::replace(session, InsertSession::new(Entry::Insert));
            self.record(ended.record());
        }
    }

    /// Drop the segment of an open insert session whose text an undo or a
    /// redo replaced, and start a new one at the cursor.
    pub(super) fn restart_segment(&mut self) {
        if let State::Insert(session) = &mut self.state {
            *session = InsertSession::new(Entry::Insert);
        }
    }

    /// Return the last repeatable edit.
    fn last_edit(&self) -> Option<RepeatableEdit> {
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
                self.insert_key(InsertKey::Text(content.clone()));
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
            PlainCommand::BeginInsert => self.enter_insert(Entry::Insert),
            PlainCommand::AppendAfter => self.enter_insert(Entry::Append),
            PlainCommand::InsertLineStart => self.enter_insert(Entry::InsertLineStart),
            PlainCommand::AppendLineEnd => self.enter_insert(Entry::AppendLineEnd),
            PlainCommand::OpenBelow => self.enter_insert(Entry::OpenBelow),
            PlainCommand::OpenAbove => self.enter_insert(Entry::OpenAbove),
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
                self.enter_insert(Entry::ChangeToEnd);
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
                self.enter_insert(Entry::ChangeLine);
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
        let key = match command {
            InsertCommand::EndInsert => {
                self.transition(Target::Normal);
                self.ensure_cursor_visible(ctx);
                return;
            }
            InsertCommand::Move(direction) => {
                let cursor = self.buffer.cursor();
                match direction {
                    MoveCommand::Left | MoveCommand::Right => {
                        let multiline = self.config.multiline;
                        let moved = if direction == MoveCommand::Left {
                            self.buffer.move_left(multiline)
                        } else {
                            self.buffer.move_right(multiline)
                        };
                        if moved {
                            self.update_preferred_column();
                        }
                    }
                    MoveCommand::Up => self.move_vertical(-1),
                    MoveCommand::Down => self.move_vertical(1),
                }
                if self.buffer.cursor() != cursor {
                    // Text typed after a move goes elsewhere: it starts a
                    // new undo step, and `.` repeats only the typing after
                    // it.
                    self.end_entry();
                    self.ensure_cursor_visible(ctx);
                }
                return;
            }
            InsertCommand::Insert(character) => {
                InsertKey::Text(character.encode_utf8(&mut [0; 4]).to_string())
            }
            InsertCommand::Newline => InsertKey::Text("\n".to_string()),
            InsertCommand::Backspace => InsertKey::Backspace,
            InsertCommand::Delete => InsertKey::Delete,
        };
        if self.insert_key(key) {
            self.ensure_cursor_visible(ctx);
        }
    }

    /// Run one key of insert mode, and record it for `.`. The first edit
    /// after an undo or a relocation opens the undo group again. Returns
    /// whether the key changed the text.
    pub(super) fn insert_key(&mut self, key: InsertKey) -> bool {
        self.buffer.begin_transaction();
        let (changed, key) = match key {
            InsertKey::Text(text) => {
                let inserted = self.handle_insert_text(&text);
                (!inserted.is_empty(), InsertKey::Text(inserted))
            }
            InsertKey::Backspace => (self.handle_delete_backward(), key),
            InsertKey::Delete => (self.handle_delete_forward(), key),
        };
        // As in Vim, `.` repeats no backspace that deleted nothing, but
        // repeats a forward delete that deleted nothing.
        if changed || key == InsertKey::Delete {
            self.vi.push_key(key);
        }
        changed
    }

    /// Move the vi mode to `to`: the one writer of the mode. Leaving insert
    /// mode commits the undo group of the session and records the session
    /// for `.`. Entering it opens an undo group, which the edits of the
    /// entry join. Every transition ends a pointer drag.
    pub(super) fn transition(&mut self, to: Target) {
        self.mouse.end_drag();
        let state = match to {
            Target::Normal => State::Normal,
            Target::Visual(mode) => State::Visual(mode),
            Target::Insert(entry) => State::Insert(InsertSession::new(entry)),
        };
        self.vi.pending = None;
        if let State::Insert(ended) = mem::replace(&mut self.vi.state, state) {
            self.buffer.commit_transaction();
            self.vi.record(ended.record());
        }
        if let Target::Insert(_) = to {
            self.buffer.begin_transaction();
        }
    }

    /// Enter insert mode at `entry`, and run the edits of the entry in the
    /// session's undo group. A visual change deletes its selection itself.
    pub(super) fn enter_insert(&mut self, entry: Entry) {
        self.transition(Target::Insert(entry));
        match entry {
            Entry::Insert | Entry::VisualChange(_) => return,
            Entry::Append => {
                let _ = self.buffer.move_right(self.config.multiline);
            }
            Entry::InsertLineStart => self.buffer.move_line_start(),
            Entry::AppendLineEnd => self.buffer.move_line_end(),
            Entry::OpenBelow => {
                if self.config.multiline {
                    let cursor = self.buffer.cursor();
                    let end = self.buffer.line_end_position(cursor.line, false);
                    self.buffer.set_cursor(end);
                    self.handle_insert_text("\n");
                }
            }
            Entry::OpenAbove => {
                if self.config.multiline {
                    let cursor = self.buffer.cursor();
                    let start = TextPosition::new(cursor.line, 0);
                    self.buffer.set_cursor(start);
                    self.handle_insert_text("\n");
                    let _ = self.buffer.move_left(true);
                }
            }
            Entry::ChangeLine => self.clear_line(),
            Entry::ChangeToEnd => self.delete_to_line_end(),
        }
        self.update_preferred_column();
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

    /// Yank the current line, and delete its text but not the line.
    fn clear_line(&mut self) {
        if self.config.interaction.read_only() {
            return;
        }
        let line = self.buffer.cursor().line;
        let start = TextPosition::new(line, 0);
        self.set_yank(
            TextRange::new(start, self.buffer.line_end_position(line, true)),
            true,
        );
        let end = self.buffer.line_end_position(line, false);
        self.replace_range(TextRange::new(start, end), "");
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
            RepeatableEdit::Insert { entry, keys } => {
                match *entry {
                    Entry::VisualChange(extent) => {
                        self.select_extent(extent);
                        self.visual_change(extent.mode());
                    }
                    entry => self.enter_insert(entry),
                }
                // An entry that typed nothing stays in insert mode, as its
                // key did.
                if !keys.is_empty() {
                    for key in keys {
                        self.insert_key(key.clone());
                    }
                    self.transition(Target::Normal);
                }
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
            RepeatableEdit::DeleteChar => self.delete_char_forward(),
            RepeatableEdit::DeleteToEnd => {
                self.delete_to_line_end();
                true
            }
            RepeatableEdit::VisualDelete(extent) => {
                self.select_extent(*extent);
                self.visual_delete(extent.mode());
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
            self.vi.last_edit = Some(edit);
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
        // The replay records itself as it runs, with the extent clamped to
        // this place; the next `.` repeats the original edit.
        self.vi.last_edit = Some(edit);
    }
}
