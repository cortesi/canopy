use canopy::{
    Context, EventOutcome,
    event::{Event, key},
};
use unicode_segmentation::UnicodeSegmentation;

use super::{
    Selection, TextPosition, TextRange,
    search::SearchDirection,
    widget::{Editor, is_word_char},
};

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
    pub fn end_insert(&mut self) -> Option<RepeatableEdit> {
        self.mode = ViMode::Normal;
        self.pending = None;
        let insert_text = self.insert_text.clone();
        self.insert_text.clear();
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

    /// Handle events in vi mode.
    pub(super) fn handle_vi_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        if self.prompt.is_some() {
            return self.handle_prompt_event(event, ctx);
        }

        match self.vi.mode() {
            ViMode::Insert => return self.handle_vi_insert(event, ctx),
            ViMode::Visual(mode) => return self.handle_vi_visual(event, ctx, mode),
            ViMode::Normal => {}
        }

        if let Some(pending) = self.vi.pending() {
            let outcome = self.handle_pending_vi(pending, event, ctx);
            if outcome != EventOutcome::Ignore {
                return outcome;
            }
        }

        match event {
            Event::Key(key::Key {
                key: key::KeyCode::Char('i'),
                ..
            }) => {
                self.begin_text_entry_transaction();
                self.vi.begin_insert();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('a'),
                ..
            }) => {
                let _ = self.buffer.move_right(self.config.multiline);
                self.update_preferred_column();
                self.begin_text_entry_transaction();
                self.vi.begin_insert();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('I'),
                ..
            }) => {
                self.buffer.move_line_start();
                self.update_preferred_column();
                self.begin_text_entry_transaction();
                self.vi.begin_insert();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('A'),
                ..
            }) => {
                self.buffer.move_line_end();
                self.update_preferred_column();
                self.begin_text_entry_transaction();
                self.vi.begin_insert();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('o'),
                ..
            }) => {
                self.begin_text_entry_transaction();
                if self.config.multiline {
                    let cursor = self.buffer.cursor();
                    let end = self.buffer.line_end_position(cursor.line, true);
                    self.buffer.set_cursor(end);
                    self.handle_insert_text("\n");
                }
                self.vi.begin_insert();
                self.vi.set_last_edit(RepeatableEdit::OpenBelow);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('O'),
                ..
            }) => {
                self.begin_text_entry_transaction();
                if self.config.multiline {
                    let cursor = self.buffer.cursor();
                    let start = self.buffer.line_start_position(cursor.line);
                    self.buffer.set_cursor(start);
                    self.handle_insert_text("\n");
                    let _ = self.buffer.move_left(true);
                }
                self.vi.begin_insert();
                self.vi.set_last_edit(RepeatableEdit::OpenAbove);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('v'),
                ..
            }) => {
                if let ViMode::Visual(_) = self.vi.mode() {
                    self.exit_visual();
                } else {
                    self.enter_visual(VisualMode::Character);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('V'),
                ..
            }) => {
                self.enter_visual(VisualMode::Line);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Esc,
                ..
            }) => {
                self.vi.set_pending(None);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('/'),
                ..
            }) => {
                self.start_search_prompt(SearchDirection::Forward);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('?'),
                ..
            }) => {
                self.start_search_prompt(SearchDirection::Backward);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('n'),
                ..
            }) => {
                if let Some(pos) = self.search.move_next(&self.buffer, false) {
                    self.buffer.set_cursor(pos);
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('N'),
                ..
            }) => {
                if let Some(pos) = self.search.move_next(&self.buffer, true) {
                    self.buffer.set_cursor(pos);
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('R'),
                ..
            }) => {
                self.start_replace_prompt();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('u'),
                ..
            }) => {
                self.buffer.undo();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('r'),
                mods,
            }) if mods.ctrl => {
                self.buffer.redo();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('.'),
                ..
            }) => {
                self.repeat_last_edit();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('h'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Left,
                ..
            }) => {
                let moved = self.buffer.move_left(true);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('l'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Right,
                ..
            }) => {
                let moved = self.buffer.move_right(true);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('j'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Down,
                ..
            }) => {
                self.move_vertical(1);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('k'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Up,
                ..
            }) => {
                self.move_vertical(-1);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('0'),
                ..
            }) => {
                self.buffer.move_line_start();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('$'),
                ..
            }) => {
                self.buffer.move_line_end();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('^'),
                ..
            }) => {
                self.buffer.move_line_first_non_ws();
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('w'),
                ..
            }) => {
                self.move_word_forward();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('b'),
                ..
            }) => {
                self.move_word_backward();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('e'),
                ..
            }) => {
                self.move_word_end();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('g'),
                ..
            }) => {
                self.vi.set_pending(Some(PendingKey::G));
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('d'),
                ..
            }) => {
                self.vi.set_pending(Some(PendingKey::Delete));
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('c'),
                ..
            }) => {
                self.vi.set_pending(Some(PendingKey::Change));
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('y'),
                ..
            }) => {
                self.vi.set_pending(Some(PendingKey::Yank));
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('Y'),
                ..
            }) => {
                self.yank_line();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('p'),
                ..
            }) => {
                let text = self.yank.clone();
                let linewise = self.yank_linewise;
                self.put_yank(false);
                if !text.is_empty() {
                    self.vi.set_last_edit(RepeatableEdit::Put {
                        text,
                        linewise,
                        before: false,
                    });
                }
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('P'),
                ..
            }) => {
                let text = self.yank.clone();
                let linewise = self.yank_linewise;
                self.put_yank(true);
                if !text.is_empty() {
                    self.vi.set_last_edit(RepeatableEdit::Put {
                        text,
                        linewise,
                        before: true,
                    });
                }
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('x'),
                ..
            }) => {
                if self.delete_char_forward() {
                    self.vi.set_last_edit(RepeatableEdit::DeleteChar);
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('D'),
                ..
            }) => {
                self.delete_to_line_end();
                self.vi.set_last_edit(RepeatableEdit::DeleteToEnd);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('C'),
                ..
            }) => {
                self.begin_text_entry_transaction();
                self.delete_to_line_end();
                self.vi.set_last_edit(RepeatableEdit::ChangeToEnd);
                self.vi.begin_insert();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('G'),
                ..
            }) => {
                let last_line = self.buffer.line_count().saturating_sub(1);
                let pos = TextPosition::new(last_line, 0);
                self.buffer.set_cursor(pos);
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
        }
    }

    /// Handle a pending multi-key vi command.
    pub(super) fn handle_pending_vi(
        &mut self,
        pending: PendingKey,
        event: &Event,
        ctx: &mut dyn Context,
    ) -> EventOutcome {
        match (pending, event) {
            (
                PendingKey::G,
                Event::Key(key::Key {
                    key: key::KeyCode::Char('g'),
                    ..
                }),
            ) => {
                self.buffer.set_cursor(TextPosition::new(0, 0));
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
                self.vi.set_pending(None);
                EventOutcome::Handle
            }
            (
                PendingKey::G,
                Event::Key(key::Key {
                    key: key::KeyCode::Char('j'),
                    ..
                }),
            ) => {
                self.move_display_line(1, ctx);
                self.ensure_cursor_visible(ctx);
                self.vi.set_pending(None);
                EventOutcome::Handle
            }
            (
                PendingKey::G,
                Event::Key(key::Key {
                    key: key::KeyCode::Char('k'),
                    ..
                }),
            ) => {
                self.move_display_line(-1, ctx);
                self.ensure_cursor_visible(ctx);
                self.vi.set_pending(None);
                EventOutcome::Handle
            }
            (
                PendingKey::Delete,
                Event::Key(key::Key {
                    key: key::KeyCode::Char('d'),
                    ..
                }),
            ) => {
                self.delete_line();
                self.vi.set_last_edit(RepeatableEdit::DeleteLine);
                self.vi.set_pending(None);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            (
                PendingKey::Change,
                Event::Key(key::Key {
                    key: key::KeyCode::Char('c'),
                    ..
                }),
            ) => {
                self.begin_text_entry_transaction();
                self.delete_line();
                self.vi.set_last_edit(RepeatableEdit::ChangeLine);
                self.vi.begin_insert();
                self.vi.set_pending(None);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            (
                PendingKey::Yank,
                Event::Key(key::Key {
                    key: key::KeyCode::Char('y'),
                    ..
                }),
            ) => {
                self.yank_line();
                self.vi.set_pending(None);
                EventOutcome::Handle
            }
            _ => {
                self.vi.set_pending(None);
                EventOutcome::Ignore
            }
        }
    }

    /// Handle insert-mode vi events.
    pub(super) fn handle_vi_insert(
        &mut self,
        event: &Event,
        ctx: &mut dyn Context,
    ) -> EventOutcome {
        match event {
            Event::Key(key::Key {
                key: key::KeyCode::Esc,
                ..
            }) => {
                self.commit_text_entry_transaction();
                let _ = self.vi.end_insert();
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char(c),
                mods,
            }) if !mods.ctrl && !mods.alt => {
                self.handle_insert_text(&c.to_string());
                self.vi.push_inserted(&c.to_string());
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Backspace,
                ..
            }) => {
                if self.handle_delete_backward() {
                    self.vi.pop_inserted_grapheme();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Delete,
                ..
            }) => {
                if self.handle_delete_forward() {
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Enter,
                ..
            }) => {
                if self.config.multiline {
                    self.handle_insert_text("\n");
                    self.vi.push_inserted("\n");
                    self.ensure_cursor_visible(ctx);
                    EventOutcome::Handle
                } else {
                    EventOutcome::Ignore
                }
            }
            Event::Key(key::Key {
                key: key::KeyCode::Left,
                ..
            }) => {
                let moved = self.buffer.move_left(self.config.multiline);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Right,
                ..
            }) => {
                let moved = self.buffer.move_right(self.config.multiline);
                if moved {
                    self.update_preferred_column();
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Up,
                ..
            }) => {
                self.move_vertical(-1);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Down,
                ..
            }) => {
                self.move_vertical(1);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Paste(content) => {
                let inserted = self.handle_paste(content);
                self.vi.push_inserted(&inserted);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
        }
    }

    /// Handle visual-mode vi events.
    pub(super) fn handle_vi_visual(
        &mut self,
        event: &Event,
        ctx: &mut dyn Context,
        mode: VisualMode,
    ) -> EventOutcome {
        match event {
            Event::Key(key::Key {
                key: key::KeyCode::Esc,
                ..
            }) => {
                self.exit_visual();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('d'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Char('x'),
                ..
            }) => {
                if self.config.read_only {
                    self.exit_visual();
                    return EventOutcome::Handle;
                }
                let linewise = matches!(mode, VisualMode::Line);
                let mut range = self.buffer.selection().range();
                if linewise {
                    range = self.linewise_range(range);
                }
                self.set_yank(range, linewise);
                self.buffer.replace_range(range, "");
                self.update_preferred_column();
                self.exit_visual();
                self.vi.set_last_edit(RepeatableEdit::DeleteChar);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('y'),
                ..
            }) => {
                let linewise = matches!(mode, VisualMode::Line);
                let mut range = self.buffer.selection().range();
                if linewise {
                    range = self.linewise_range(range);
                }
                self.set_yank(range, linewise);
                self.exit_visual();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('c'),
                ..
            }) => {
                if self.config.read_only {
                    self.exit_visual();
                    return EventOutcome::Handle;
                }
                let linewise = matches!(mode, VisualMode::Line);
                let mut range = self.buffer.selection().range();
                if linewise {
                    range = self.linewise_range(range);
                }
                self.set_yank(range, linewise);
                self.begin_text_entry_transaction();
                self.buffer.replace_range(range, "");
                self.vi.begin_insert();
                self.exit_visual();
                self.vi.set_last_edit(RepeatableEdit::ChangeLine);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('>'),
                ..
            }) => {
                self.indent_selection(true, mode);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('<'),
                ..
            }) => {
                self.indent_selection(false, mode);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('h'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Left,
                ..
            }) => {
                let anchor = self.buffer.selection().anchor();
                let moved = self.buffer.move_left(true);
                if moved {
                    self.update_visual_selection(anchor, mode);
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('l'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Right,
                ..
            }) => {
                let anchor = self.buffer.selection().anchor();
                let moved = self.buffer.move_right(true);
                if moved {
                    self.update_visual_selection(anchor, mode);
                    self.ensure_cursor_visible(ctx);
                }
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('j'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Down,
                ..
            }) => {
                let anchor = self.buffer.selection().anchor();
                self.move_vertical(1);
                self.update_visual_selection(anchor, mode);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Char('k'),
                ..
            })
            | Event::Key(key::Key {
                key: key::KeyCode::Up,
                ..
            }) => {
                let anchor = self.buffer.selection().anchor();
                self.move_vertical(-1);
                self.update_visual_selection(anchor, mode);
                self.ensure_cursor_visible(ctx);
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
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
        self.buffer.replace_range(delete_range, "");
        if cursor.line + 1 == line_count && cursor.line > 0 {
            let prev_line = cursor.line.saturating_sub(1);
            self.buffer
                .set_cursor(self.buffer.line_start_position(prev_line));
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
        self.buffer.replace_range(range, "");
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
        let yank = self.yank.clone();
        let content = self.normalize_insert_text(&yank);
        let multiline = self.config.multiline;
        let linewise = self.yank_linewise;
        {
            let mut transaction = self.buffer.transaction();
            if linewise {
                let cursor = transaction.cursor();
                let target = if before {
                    transaction.line_start_position(cursor.line)
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

    /// Move to the start of the next word on the current line.
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

    /// Move to the start of the previous word on the current line.
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

    /// Move to the end of the current word on the current line.
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

    /// Repeat the last recorded vi edit.
    pub(super) fn repeat_last_edit(&mut self) {
        if self.config.read_only {
            return;
        }
        let Some(edit) = self.vi.last_edit() else {
            return;
        };
        match edit {
            RepeatableEdit::Insert { text } => {
                self.handle_insert_text(&text);
            }
            RepeatableEdit::Put {
                text,
                linewise,
                before,
            } => {
                self.yank = text;
                self.yank_linewise = linewise;
                self.put_yank(before);
            }
            RepeatableEdit::DeleteLine => {
                self.delete_line();
            }
            RepeatableEdit::ChangeLine => {
                self.delete_line();
                self.vi.begin_insert();
                self.begin_text_entry_transaction();
            }
            RepeatableEdit::DeleteChar => {
                let _ = self.handle_delete_forward();
            }
            RepeatableEdit::DeleteToEnd => {
                self.delete_to_line_end();
            }
            RepeatableEdit::ChangeToEnd => {
                self.delete_to_line_end();
                self.vi.begin_insert();
                self.begin_text_entry_transaction();
            }
            RepeatableEdit::OpenBelow => {
                if self.config.multiline {
                    let cursor = self.buffer.cursor();
                    let end = self.buffer.line_end_position(cursor.line, true);
                    self.buffer.set_cursor(end);
                    self.handle_insert_text("\n");
                }
                self.vi.begin_insert();
                self.begin_text_entry_transaction();
            }
            RepeatableEdit::OpenAbove => {
                if self.config.multiline {
                    let cursor = self.buffer.cursor();
                    let start = self.buffer.line_start_position(cursor.line);
                    self.buffer.set_cursor(start);
                    self.handle_insert_text("\n");
                    let _ = self.buffer.move_left(true);
                }
                self.vi.begin_insert();
                self.begin_text_entry_transaction();
            }
        }
    }
}
