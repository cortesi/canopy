//! Vi visual mode, as in Vim with `selection=inclusive`.
//!
//! The buffer's selection holds the two ends of a visual selection: the
//! anchor, where the selection started, and the cursor. Each end names a
//! character, and the selection holds both. An end can sit on the end of a
//! line, where the selection then holds the line break. A line selection
//! holds every line from the line of one end to the line of the other.

use canopy::{Context, input::key};

use super::{
    Motion, PendingKey, RepeatableEdit, ViMode, VisualMode,
    motion::{self, Vim, VisualArea},
};
use crate::{
    editor::{search::SearchDirection, widget::Editor},
    text_buffer::{Selection, TextPosition, TextRange},
};

/// A text object of visual mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::editor) enum TextObject {
    /// `iw` and `aw`: a word, or a run of blanks.
    Word,
    /// `is` and `as`: a sentence.
    Sentence,
    /// `ip` and `ap`: a paragraph.
    Paragraph,
}

/// One visual-mode vi action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::editor) enum VisualCommand {
    /// Leave visual mode.
    Exit,
    /// Switch to another kind of visual selection.
    Switch(VisualMode),
    /// Drop a pending prefix and stay in visual mode.
    ClearPending,
    /// Wait for the second key of a command.
    Pending(PendingKey),
    /// Move the cursor, the free end of the selection.
    Motion(Motion),
    /// Move the cursor to the other end of the selection.
    SwapEnds,
    /// Open a search prompt.
    Search(SearchDirection),
    /// Move to the next search match, or the one before when reversed.
    NextMatch(bool),
    /// Select a text object, with its blanks when the flag is set.
    Object(TextObject, bool),
    /// Delete the selection.
    Delete,
    /// Yank the selection.
    Yank,
    /// Change the selection.
    Change,
    /// Indent the selected lines right or left.
    Indent(bool),
}

impl VisualCommand {
    /// Whether this command changes the buffer.
    pub(super) fn edits(self) -> bool {
        matches!(self, Self::Delete | Self::Change | Self::Indent(_))
    }
}

/// The policy for the column that vertical moves keep after a motion.
enum Column {
    /// Take the column of the new cursor.
    Update,
    /// Keep the column.
    Keep,
    /// Keep to the ends of lines.
    End,
}

impl Editor {
    /// Return the kind of the visual selection, or `None` outside visual
    /// mode.
    pub(in crate::editor) fn visual_mode(&self) -> Option<VisualMode> {
        match self.vi_mode() {
            Some(ViMode::Visual(mode)) => Some(mode),
            _ => None,
        }
    }

    /// Return the end of a range that holds the character at `position`:
    /// the next grapheme, or the start of the next line from the end of a
    /// line. The end of the text holds nothing.
    fn after_char(&self, position: TextPosition) -> TextPosition {
        let len = self.buffer.line_char_len(position.line);
        if position.column < len {
            TextPosition::new(position.line, motion::next_column(&self.buffer, position))
        } else if position.line + 1 < self.buffer.line_count() {
            TextPosition::new(position.line + 1, 0)
        } else {
            TextPosition::new(position.line, len)
        }
    }

    /// Return the ends of the visual selection in order.
    fn visual_ends(&self) -> (TextPosition, TextPosition) {
        let selection = self.buffer.selection();
        let (anchor, head) = (selection.anchor(), selection.head());
        (anchor.min(head), anchor.max(head))
    }

    /// Return the half-open range of a visual selection of kind `mode`. A
    /// line selection ends at the end of its last line, before the line
    /// break.
    pub(in crate::editor) fn visual_range(&self, mode: VisualMode) -> TextRange {
        let (first, last) = self.visual_ends();
        match mode {
            VisualMode::Character => TextRange::new(first, self.after_char(last)),
            VisualMode::Line => TextRange::new(
                TextPosition::new(first.line, 0),
                self.buffer.line_end_position(last.line, false),
            ),
        }
    }

    /// Return the span of positions whose line breaks a visual selection of
    /// kind `mode` holds: the end of line `l` is selected when it lies in
    /// the span, both ends included.
    pub(in crate::editor) fn visual_break_span(
        &self,
        mode: VisualMode,
    ) -> (TextPosition, TextPosition) {
        let (first, last) = self.visual_ends();
        match mode {
            VisualMode::Character => (first, last),
            VisualMode::Line => (
                TextPosition::new(first.line, 0),
                TextPosition::new(last.line, usize::MAX),
            ),
        }
    }

    /// Return the visual selection of kind `mode` as a half-open selection
    /// whose head lies at the cursor's end.
    pub(in crate::editor) fn visual_selection(&self, mode: VisualMode) -> Selection {
        let selection = self.buffer.selection();
        let range = self.visual_range(mode);
        if selection.anchor() <= selection.head() {
            Selection::new(range.start, range.end)
        } else {
            Selection::new(range.end, range.start)
        }
    }

    /// Select `selection`, a half-open selection, in visual mode of kind
    /// `mode`: the inverse of [`Self::visual_selection`].
    pub(in crate::editor) fn set_visual_selection(
        &mut self,
        mode: VisualMode,
        selection: Selection,
    ) {
        if self.visual_selection(mode) == selection {
            return;
        }
        let (anchor, head) = (selection.anchor(), selection.head());
        let ends = match mode {
            VisualMode::Character if anchor < head => (anchor, self.last_char_before(head)),
            VisualMode::Character if head < anchor => (self.last_char_before(anchor), head),
            VisualMode::Character => (anchor, anchor),
            VisualMode::Line => {
                let current = self.buffer.selection();
                (
                    TextPosition::new(anchor.line, current.anchor().column),
                    TextPosition::new(head.line, current.head().column),
                )
            }
        };
        // The buffer clamps the ends to the text, and each end then moves to
        // the start of its grapheme, where motions put them.
        self.buffer.set_selection(Selection::new(ends.0, ends.1));
        self.snap_visual_ends();
    }

    /// Return the position of the last character that a half-open range
    /// ending at `end` holds: the grapheme before it, or the end of the line
    /// before for an end at the start of a line.
    fn last_char_before(&self, end: TextPosition) -> TextPosition {
        if end.column > 0 {
            TextPosition::new(end.line, motion::prev_column(&self.buffer, end))
        } else if end.line > 0 {
            self.buffer.line_end_position(end.line - 1, false)
        } else {
            end
        }
    }

    /// Enter visual mode of kind `mode`, with both ends at the cursor. A
    /// cursor on the end of a line moves to its last character first, as a
    /// normal-mode cursor in Vim never rests there, and a cursor inside a
    /// grapheme moves to its start.
    pub(in crate::editor) fn enter_visual(&mut self, mode: VisualMode) {
        let cursor = motion::clamp_to_char(&self.buffer, self.buffer.cursor());
        let cursor = motion::grapheme_start(&self.buffer, cursor);
        self.vi.set_mode(ViMode::Visual(mode));
        self.buffer.set_cursor(cursor);
    }

    /// Move each end of a visual selection inside a grapheme to the start
    /// of the grapheme, as when new text joins the char at an end.
    pub(in crate::editor) fn snap_visual_ends(&mut self) {
        if self.visual_mode().is_none() {
            return;
        }
        let selection = self.buffer.selection();
        self.buffer.set_selection(Selection::new(
            motion::grapheme_start(&self.buffer, selection.anchor()),
            motion::grapheme_start(&self.buffer, selection.head()),
        ));
    }

    /// Leave visual mode, and collapse the selection at the cursor. A cursor
    /// on the end of a line moves to its last character.
    pub(in crate::editor) fn exit_visual(&mut self) {
        self.leave_visual_at(self.buffer.cursor());
    }

    /// Leave visual mode with the cursor at `position`, or at the last
    /// character of its line when `position` is on the end of a line.
    fn leave_visual_at(&mut self, position: TextPosition) {
        let position = motion::clamp_to_char(&self.buffer, position);
        self.vi.set_mode(ViMode::Normal);
        self.buffer.set_cursor(position);
    }

    /// Move the cursor to `position`. In visual mode the anchor stays, so
    /// the cursor moves the free end of the selection.
    pub(in crate::editor) fn place_cursor(&mut self, position: TextPosition) {
        if self.visual_mode().is_some() {
            let anchor = self.buffer.selection().anchor();
            self.buffer.set_selection(Selection::new(anchor, position));
        } else {
            self.buffer.set_cursor(position);
        }
    }

    /// Move the cursor to `position` as a jump, take its column for
    /// vertical moves, and scroll it into view.
    pub(in crate::editor) fn jump_to(&mut self, ctx: &mut dyn Context, position: TextPosition) {
        self.place_cursor(position);
        self.update_preferred_column();
        self.ensure_cursor_visible(ctx);
    }

    /// Move the cursor to the next search match from it, or the one before
    /// with `reverse`. As in Vim, the cursor's column becomes the column
    /// that vertical moves keep even when no match exists.
    pub(in crate::editor) fn jump_to_match(&mut self, ctx: &mut dyn Context, reverse: bool) {
        match self.search_from_cursor(reverse) {
            Some(position) => self.jump_to(ctx, position),
            None => self.update_preferred_column(),
        }
    }

    /// Return the target of a horizontal or vertical step from `cursor`.
    /// Visual mode keeps `h` and `l` in the line, and lets `l` reach the end
    /// of the line. Normal mode steps across lines.
    fn step_target(
        &self,
        motion: Motion,
        cursor: TextPosition,
        visual: bool,
    ) -> Option<TextPosition> {
        let len = self.buffer.line_char_len(cursor.line);
        let lines = self.buffer.line_count().max(1);
        match motion {
            Motion::Left if cursor.column > 0 => Some(TextPosition::new(
                cursor.line,
                motion::prev_column(&self.buffer, cursor),
            )),
            Motion::Left if !visual && cursor.line > 0 => {
                Some(self.buffer.line_end_position(cursor.line - 1, false))
            }
            Motion::Right if cursor.column < len => Some(TextPosition::new(
                cursor.line,
                motion::next_column(&self.buffer, cursor),
            )),
            Motion::Right if !visual && cursor.line + 1 < lines => {
                Some(TextPosition::new(cursor.line + 1, 0))
            }
            Motion::Up if cursor.line > 0 => Some(self.column_in(cursor.line - 1)),
            Motion::Down if cursor.line + 1 < lines => Some(self.column_in(cursor.line + 1)),
            _ => None,
        }
    }

    /// Return the position in `line` at the column that vertical moves keep.
    fn column_in(&self, line: usize) -> TextPosition {
        self.buffer
            .position_for_column(line, self.preferred_column, self.config.tab_stop)
    }

    /// Return the target of `motion` from the cursor, and the policy for the
    /// column that vertical moves keep.
    fn motion_target(&self, motion: Motion, visual: bool) -> Option<(TextPosition, Column)> {
        let cursor = self.buffer.cursor();
        let line = cursor.line;
        Some(match motion {
            Motion::Left | Motion::Right => {
                (self.step_target(motion, cursor, visual)?, Column::Update)
            }
            Motion::Up | Motion::Down => (self.step_target(motion, cursor, visual)?, Column::Keep),
            Motion::LineStart => (TextPosition::new(line, 0), Column::Update),
            Motion::FirstNonBlank => (
                TextPosition::new(line, motion::first_non_blank(&self.buffer, line)),
                Column::Update,
            ),
            Motion::LineEnd => (self.buffer.line_end_position(line, false), Column::End),
            Motion::WordForward | Motion::WordBackward | Motion::WordEnd => {
                let mut vim = Vim::new(&self.buffer, cursor);
                match motion {
                    Motion::WordForward => vim.fwd_word(false),
                    Motion::WordBackward => vim.bck_word(false),
                    _ => vim.end_word(false, false),
                };
                (vim.cursor(), Column::Update)
            }
            Motion::Top => (self.column_in(0), Column::Keep),
            Motion::Bottom => (
                self.column_in(self.buffer.line_count().saturating_sub(1)),
                Column::Keep,
            ),
            Motion::Row(_) => return None,
        })
    }

    /// Move the cursor by `motion`. In visual mode the anchor stays.
    pub(in crate::editor) fn apply_motion(&mut self, motion: Motion, ctx: &mut dyn Context) {
        let visual = self.visual_mode().is_some();
        if let Motion::Row(delta) = motion {
            let anchor = self.buffer.selection().anchor();
            self.move_row(delta, ctx);
            if visual {
                let head = self.buffer.cursor();
                self.buffer.set_selection(Selection::new(anchor, head));
            }
            self.ensure_cursor_visible(ctx);
            return;
        }
        let Some((target, column)) = self.motion_target(motion, visual) else {
            return;
        };
        self.place_cursor(target);
        match column {
            Column::Update => self.update_preferred_column(),
            Column::Keep => self.row_goal = None,
            Column::End => {
                self.preferred_column = usize::MAX;
                self.row_goal = None;
            }
        }
        self.ensure_cursor_visible(ctx);
    }

    /// Classify one visual-mode key of a selection of kind `mode`.
    ///
    /// After a prefix, Esc and every plain character complete or cancel it.
    /// Chords with Ctrl or Alt reach the host's bindings.
    pub(in crate::editor) fn visual_command(
        &self,
        mode: VisualMode,
        key: key::Key,
    ) -> Option<VisualCommand> {
        let chord = key.mods.ctrl || key.mods.alt;
        if let Some(pending) = self.vi.pending() {
            if key.key == key::KeyCode::Esc {
                return Some(VisualCommand::ClearPending);
            }
            let key::KeyCode::Char(character) = key.key else {
                return None;
            };
            if chord {
                return None;
            }
            return Some(Self::visual_pending_command(pending, character));
        }
        if chord {
            return None;
        }
        Some(match key.key {
            key::KeyCode::Esc => VisualCommand::Exit,
            key::KeyCode::Left => VisualCommand::Motion(Motion::Left),
            key::KeyCode::Right => VisualCommand::Motion(Motion::Right),
            key::KeyCode::Up => VisualCommand::Motion(Motion::Up),
            key::KeyCode::Down => VisualCommand::Motion(Motion::Down),
            key::KeyCode::Char(character) => Self::visual_char_command(mode, character)?,
            _ => return None,
        })
    }

    /// Classify the key after a visual-mode prefix.
    fn visual_pending_command(pending: PendingKey, character: char) -> VisualCommand {
        match (pending, character) {
            (PendingKey::G, 'g') => VisualCommand::Motion(Motion::Top),
            (PendingKey::G, 'j') => VisualCommand::Motion(Motion::Row(1)),
            (PendingKey::G, 'k') => VisualCommand::Motion(Motion::Row(-1)),
            (PendingKey::Object(include), 'w') => VisualCommand::Object(TextObject::Word, include),
            (PendingKey::Object(include), 's') => {
                VisualCommand::Object(TextObject::Sentence, include)
            }
            (PendingKey::Object(include), 'p') => {
                VisualCommand::Object(TextObject::Paragraph, include)
            }
            _ => VisualCommand::ClearPending,
        }
    }

    /// Classify one plain character in visual mode.
    fn visual_char_command(mode: VisualMode, character: char) -> Option<VisualCommand> {
        Some(match character {
            'v' if mode == VisualMode::Character => VisualCommand::Exit,
            'v' => VisualCommand::Switch(VisualMode::Character),
            'V' if mode == VisualMode::Line => VisualCommand::Exit,
            'V' => VisualCommand::Switch(VisualMode::Line),
            'd' | 'x' => VisualCommand::Delete,
            'y' => VisualCommand::Yank,
            'c' => VisualCommand::Change,
            '>' => VisualCommand::Indent(true),
            '<' => VisualCommand::Indent(false),
            'h' => VisualCommand::Motion(Motion::Left),
            'l' => VisualCommand::Motion(Motion::Right),
            'j' => VisualCommand::Motion(Motion::Down),
            'k' => VisualCommand::Motion(Motion::Up),
            '0' => VisualCommand::Motion(Motion::LineStart),
            '^' => VisualCommand::Motion(Motion::FirstNonBlank),
            '$' => VisualCommand::Motion(Motion::LineEnd),
            'w' => VisualCommand::Motion(Motion::WordForward),
            'b' => VisualCommand::Motion(Motion::WordBackward),
            'e' => VisualCommand::Motion(Motion::WordEnd),
            'G' => VisualCommand::Motion(Motion::Bottom),
            'g' => VisualCommand::Pending(PendingKey::G),
            'i' => VisualCommand::Pending(PendingKey::Object(false)),
            'a' => VisualCommand::Pending(PendingKey::Object(true)),
            'o' | 'O' => VisualCommand::SwapEnds,
            '/' => VisualCommand::Search(SearchDirection::Forward),
            '?' => VisualCommand::Search(SearchDirection::Backward),
            'n' => VisualCommand::NextMatch(false),
            'N' => VisualCommand::NextMatch(true),
            _ => return None,
        })
    }

    /// Run one visual-mode command on a selection of kind `mode`.
    pub(in crate::editor) fn execute_visual(
        &mut self,
        mode: VisualMode,
        command: VisualCommand,
        ctx: &mut dyn Context,
    ) {
        self.vi.set_pending(None);
        match command {
            VisualCommand::Exit => self.exit_visual(),
            VisualCommand::Switch(mode) => self.vi.set_mode(ViMode::Visual(mode)),
            VisualCommand::ClearPending => {}
            VisualCommand::Pending(pending) => self.vi.set_pending(Some(pending)),
            VisualCommand::Motion(motion) => self.apply_motion(motion, ctx),
            VisualCommand::SwapEnds => {
                let selection = self.buffer.selection();
                self.buffer
                    .set_selection(Selection::new(selection.head(), selection.anchor()));
                self.update_preferred_column();
                self.ensure_cursor_visible(ctx);
            }
            VisualCommand::Search(direction) => self.start_search_prompt(direction),
            VisualCommand::NextMatch(reverse) => self.jump_to_match(ctx, reverse),
            VisualCommand::Object(object, include) => {
                self.select_object(mode, object, include);
                self.ensure_cursor_visible(ctx);
            }
            VisualCommand::Delete => {
                self.visual_delete(mode);
                self.ensure_cursor_visible(ctx);
            }
            VisualCommand::Yank => {
                self.visual_yank(mode);
                self.ensure_cursor_visible(ctx);
            }
            VisualCommand::Change => {
                self.visual_change(mode);
                self.ensure_cursor_visible(ctx);
            }
            VisualCommand::Indent(right) => {
                self.visual_indent(mode, right);
                self.ensure_cursor_visible(ctx);
            }
        }
    }

    /// Select or extend by a text object, as Vim's `iw`, `aw`, `is`, `as`,
    /// `ip`, and `ap` do in visual mode.
    fn select_object(&mut self, mode: VisualMode, object: TextObject, include: bool) {
        let selection = self.buffer.selection();
        let mut area = VisualArea {
            anchor: selection.anchor(),
            cursor: selection.head(),
            linewise: mode == VisualMode::Line,
        };
        {
            let mut vim = Vim::new(&self.buffer, area.cursor);
            match object {
                TextObject::Word => vim.current_word(&mut area, include),
                TextObject::Sentence => vim.current_sent(&mut area, include),
                TextObject::Paragraph => vim.current_par(&mut area, include),
            };
        }
        self.buffer
            .set_selection(Selection::new(area.anchor, area.cursor));
        let mode = if area.linewise {
            VisualMode::Line
        } else {
            VisualMode::Character
        };
        self.vi.set_mode(ViMode::Visual(mode));
        self.update_preferred_column();
    }

    /// Return the first and last lines of the visual selection.
    fn visual_lines(&self) -> (usize, usize) {
        let (first, last) = self.visual_ends();
        (first.line, last.line)
    }

    /// Yank the selection, and leave visual mode at its start, as Vim does.
    fn visual_yank(&mut self, mode: VisualMode) {
        let range = self.visual_range(mode);
        let cursor = self.buffer.cursor();
        let target = match mode {
            VisualMode::Character => {
                self.set_yank(range, false);
                range.start
            }
            VisualMode::Line => {
                self.set_yank(self.linewise_range(range), true);
                // The cursor moves to the start of the first line, unless it
                // is already before it.
                let top = TextPosition::new(self.buffer.selection().anchor().line, 0);
                if top < cursor { top } else { cursor }
            }
        };
        self.leave_visual_at(target);
        self.update_preferred_column();
    }

    /// Delete the selection into the yank register, and leave visual mode.
    ///
    /// A character selection leaves the cursor at its start. A line
    /// selection deletes whole lines with their line breaks, and leaves the
    /// cursor on the line after them at the column that vertical moves
    /// keep.
    fn visual_delete(&mut self, mode: VisualMode) {
        if self.config.interaction.read_only() {
            self.exit_visual();
            return;
        }
        let range = self.visual_range(mode);
        match mode {
            VisualMode::Character => {
                self.set_yank(range, false);
                // A caret at the start is what undo restores.
                self.vi.set_mode(ViMode::Normal);
                self.buffer.set_cursor(range.start);
                if !range.is_empty() {
                    self.replace_range(range, "");
                }
                let cursor = motion::clamp_to_char(&self.buffer, range.start);
                self.buffer.set_cursor(cursor);
                self.update_preferred_column();
            }
            VisualMode::Line => {
                let column = self.preferred_column;
                let (first, last) = self.visual_lines();
                self.set_yank(self.linewise_range(range), true);
                let delete = if last + 1 < self.buffer.line_count() {
                    TextRange::new(range.start, TextPosition::new(last + 1, 0))
                } else if first > 0 {
                    TextRange::new(self.buffer.line_end_position(first - 1, false), range.end)
                } else {
                    range
                };
                self.vi.set_mode(ViMode::Normal);
                self.buffer.set_cursor(range.start);
                self.replace_range(delete, "");
                let line = first.min(self.buffer.line_count().saturating_sub(1));
                let cursor = motion::clamp_to_char(&self.buffer, self.column_in(line));
                self.buffer.set_cursor(cursor);
                self.preferred_column = column;
                self.row_goal = None;
            }
        }
        self.vi.set_last_edit(RepeatableEdit::DeleteChar);
    }

    /// Delete the selection into the yank register and enter insert mode.
    /// A line selection keeps one empty line for the new text.
    fn visual_change(&mut self, mode: VisualMode) {
        if self.config.interaction.read_only() {
            self.exit_visual();
            return;
        }
        let range = self.visual_range(mode);
        let yanked = match mode {
            VisualMode::Character => range,
            VisualMode::Line => self.linewise_range(range),
        };
        self.set_yank(yanked, mode == VisualMode::Line);
        self.vi.set_mode(ViMode::Normal);
        self.buffer.set_cursor(range.start);
        self.begin_text_entry_transaction();
        if !range.is_empty() {
            self.replace_range(range, "");
        }
        self.vi.begin_insert();
        self.vi.set_last_edit(RepeatableEdit::ChangeLine);
        self.update_preferred_column();
    }

    /// Indent the selected lines right or left by one tab stop of spaces,
    /// and leave visual mode on the first line.
    ///
    /// An indent skips empty lines. An outdent removes up to one tab stop
    /// of leading spaces.
    fn visual_indent(&mut self, mode: VisualMode, right: bool) {
        if self.config.interaction.read_only() || !self.config.multiline {
            return;
        }
        let column = self.preferred_column;
        let (first, last) = self.visual_lines();
        let tab_stop = self.config.tab_stop.max(1);
        let tab = " ".repeat(tab_stop);
        self.leave_visual_at(TextPosition::new(first, 0));
        {
            let mut transaction = self.buffer.transaction();
            for line in first..=last {
                let line_start = TextPosition::new(line, 0);
                let text = transaction.line_text(line);
                if right {
                    if !text.is_empty() {
                        transaction.replace_range(TextRange::new(line_start, line_start), &tab);
                    }
                } else {
                    let remove = text
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
        let cursor = motion::clamp_to_char(&self.buffer, self.column_in(first));
        self.buffer.set_cursor(cursor);
        match mode {
            VisualMode::Line => {
                self.preferred_column = column;
                self.row_goal = None;
            }
            VisualMode::Character => self.update_preferred_column(),
        }
    }
}
