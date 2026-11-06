use std::mem;

use canopy::{
    Context,
    error::Result,
    geom::{Line, Point, Rect},
    input::key,
    render::Render,
    style::roles,
    text,
};

use super::widget::{Editor, prompt_text};
use crate::text_buffer::{Selection, TextBuffer, TextPosition, TextRange};

/// One prompt action in vi mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PromptCommand {
    /// Close the prompt without applying it.
    Cancel,
    /// Delete the last character of the edited field.
    Backspace,
    /// Append one character to the edited field.
    Push(char),
    /// Advance the prompt state.
    Enter,
    /// Answer a replace confirmation.
    Confirm(char),
}

/// Search direction for navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    /// Forward search direction.
    Forward,
    /// Backward search direction.
    Backward,
}

/// Search state and cached matches.
#[derive(Debug, Clone)]
pub struct SearchState {
    /// Current search query.
    query: String,
    /// Direction of the current search.
    direction: SearchDirection,
    /// Cached match ranges.
    matches: Vec<TextRange>,
    /// Current match index.
    current: Option<usize>,
    /// Buffer revision that matches were computed for.
    revision: u64,
    /// Whether matching ignores ASCII case.
    ignore_case: bool,
}

impl SearchState {
    /// Construct an empty search state.
    pub fn new() -> Self {
        Self {
            query: String::new(),
            direction: SearchDirection::Forward,
            matches: Vec::new(),
            current: None,
            revision: 0,
            ignore_case: false,
        }
    }

    /// Set the search query and compute matches.
    pub fn set_query(
        &mut self,
        buffer: &TextBuffer,
        query: impl Into<String>,
        direction: SearchDirection,
    ) {
        self.query = query.into();
        self.direction = direction;
        self.ignore_case = false;
        self.recompute(buffer);
        self.current = if self.matches.is_empty() {
            None
        } else if direction == SearchDirection::Forward {
            Some(0)
        } else {
            Some(self.matches.len().saturating_sub(1))
        };
    }

    /// Set a forward query with smart case, and make the first match at or
    /// after `from` current.
    ///
    /// A query without uppercase letters ignores ASCII case. When no match
    /// follows `from`, the search wraps to the first match.
    pub fn set_smart_query(
        &mut self,
        buffer: &TextBuffer,
        query: impl Into<String>,
        from: TextPosition,
    ) {
        self.query = query.into();
        self.direction = SearchDirection::Forward;
        self.ignore_case = !self.query.chars().any(char::is_uppercase);
        self.recompute(buffer);
        self.current = self.first_at_or_after(from);
    }

    /// Install precomputed match ranges, making the first match at or after
    /// `from` current.
    ///
    /// The ranges must arrive in ascending order without spanning lines, the
    /// way [`find_matches`] produces them, because the per-line renderer
    /// locates one line's slice by partition points. The query stays empty:
    /// the buffer is read-only under an installed set, and a new buffer
    /// replaces the search state outright, so no recompute can clobber these
    /// ranges. When no match follows `from`, the search wraps to the first.
    pub fn set_matches(
        &mut self,
        buffer: &TextBuffer,
        matches: Vec<TextRange>,
        from: TextPosition,
    ) {
        debug_assert!(
            matches
                .windows(2)
                .all(|pair| pair[0].start <= pair[1].start),
            "installed matches must arrive in ascending order"
        );
        self.query.clear();
        self.direction = SearchDirection::Forward;
        self.ignore_case = false;
        self.matches = matches;
        self.revision = buffer.revision();
        self.current = self.first_at_or_after(from);
    }

    /// Return the number of matches.
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Return all match ranges in ascending order.
    pub fn matches(&self) -> &[TextRange] {
        &self.matches
    }

    /// Return the index of the current match.
    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    /// Make match `index` current, or none.
    pub fn set_current(&mut self, index: Option<usize>) {
        self.current = index.filter(|&index| index < self.matches.len());
    }

    /// Update match cache if the buffer changed.
    pub fn update(&mut self, buffer: &TextBuffer) {
        if self.query.is_empty() {
            // Without a query there is nothing to recompute. An installed
            // match set pins its revision, so a matching revision keeps it;
            // only a changed buffer drops it. Clearing an already empty set
            // is a no-op either way, so no existing flow changes behavior.
            if self.revision != buffer.revision() {
                self.matches.clear();
                self.current = None;
            }
            return;
        }
        if self.revision != buffer.revision() {
            let current_range = self.current.and_then(|idx| self.matches.get(idx).copied());
            self.recompute(buffer);
            if let Some(range) = current_range {
                self.current = self
                    .matches
                    .iter()
                    .position(|candidate| *candidate == range)
                    .or({
                        if self.matches.is_empty() {
                            None
                        } else {
                            Some(0)
                        }
                    });
            }
        }
    }

    /// Return the current match range, if any.
    pub fn current_match(&self) -> Option<TextRange> {
        self.current.and_then(|idx| self.matches.get(idx).copied())
    }

    /// Return match ranges for a line.
    ///
    /// Matches are stored in ascending order, so the slice for one line is
    /// located with two partition points and borrowed without allocating.
    pub fn matches_for_line(&self, line: usize) -> &[TextRange] {
        let start = self
            .matches
            .partition_point(|range| range.start.line < line);
        let end = self
            .matches
            .partition_point(|range| range.start.line <= line);
        &self.matches[start..end]
    }

    /// Move to the next match and return its position.
    pub fn move_next(&mut self, buffer: &TextBuffer, reverse: bool) -> Option<TextPosition> {
        self.update(buffer);
        if self.matches.is_empty() {
            return None;
        }
        let direction = if reverse {
            match self.direction {
                SearchDirection::Forward => SearchDirection::Backward,
                SearchDirection::Backward => SearchDirection::Forward,
            }
        } else {
            self.direction
        };

        let current = self.current.unwrap_or(0);
        let next = match direction {
            SearchDirection::Forward => (current + 1) % self.matches.len(),
            SearchDirection::Backward => {
                (current + self.matches.len().saturating_sub(1)) % self.matches.len()
            }
        };
        self.current = Some(next);
        self.matches.get(next).map(|range| range.start)
    }

    /// Make the next match from `from` current and return it, as Vim's `n`
    /// finds it: the first match that starts after `from` in the direction
    /// of the search, or the opposite direction with `reverse`. The search
    /// wraps around the ends of the text.
    pub fn next_from(
        &mut self,
        buffer: &TextBuffer,
        from: TextPosition,
        reverse: bool,
    ) -> Option<TextRange> {
        self.update(buffer);
        let count = self.matches.len();
        if count == 0 {
            return None;
        }
        let forward = (self.direction == SearchDirection::Forward) != reverse;
        let index = if forward {
            let after = self.matches.partition_point(|range| range.start <= from);
            if after < count { after } else { 0 }
        } else {
            self.matches
                .partition_point(|range| range.start < from)
                .checked_sub(1)
                .unwrap_or(count - 1)
        };
        self.current = Some(index);
        self.matches.get(index).copied()
    }

    /// Return the index of the first match at or after `from`, wrapping to
    /// the first match when none follows.
    fn first_at_or_after(&self, from: TextPosition) -> Option<usize> {
        (!self.matches.is_empty()).then(|| {
            self.matches
                .iter()
                .position(|range| range.start >= from)
                .unwrap_or(0)
        })
    }

    /// Recompute match cache for the current query.
    fn recompute(&mut self, buffer: &TextBuffer) {
        self.matches = find_matches_in(
            buffer,
            &self.query,
            TextPosition::new(0, 0),
            self.ignore_case,
        );
        self.revision = buffer.revision();
    }
}

/// Find text matches for a query within the buffer.
pub fn find_matches(buffer: &TextBuffer, query: &str) -> Vec<TextRange> {
    find_matches_from(buffer, query, TextPosition::new(0, 0))
}

/// Find matches entirely within the suffix beginning at a character position.
fn find_matches_from(buffer: &TextBuffer, query: &str, start: TextPosition) -> Vec<TextRange> {
    find_matches_in(buffer, query, start, false)
}

/// Find matches within the suffix beginning at a character position,
/// optionally ignoring ASCII case.
///
/// ASCII case folding keeps every byte offset, so match columns index the
/// original text. A match starts and ends on grapheme boundaries, as in Vim,
/// so `e` does not match the `e` of an `e` with a combining accent.
fn find_matches_in(
    buffer: &TextBuffer,
    query: &str,
    start: TextPosition,
    ignore_case: bool,
) -> Vec<TextRange> {
    if query.is_empty() || query.contains('\n') {
        return Vec::new();
    }
    let folded_query;
    let query = if ignore_case {
        folded_query = query.to_ascii_lowercase();
        folded_query.as_str()
    } else {
        query
    };

    let mut out = Vec::new();
    for line_idx in start.line..buffer.line_count() {
        let mut line = buffer.line_text(line_idx);
        if ignore_case {
            line.make_ascii_lowercase();
        }
        let mut offset = if line_idx == start.line {
            line.char_indices()
                .nth(start.column)
                .map_or(line.len(), |(offset, _)| offset)
        } else {
            0
        };
        let boundaries = grapheme_boundaries(&line);
        let on_boundary = |byte: usize| {
            boundaries
                .as_ref()
                .is_none_or(|bytes| bytes.binary_search(&byte).is_ok())
        };
        while let Some(found) = line[offset..].find(query) {
            let byte_start = offset.saturating_add(found);
            let byte_end = byte_start.saturating_add(query.len());
            if !(on_boundary(byte_start) && on_boundary(byte_end)) {
                // The match splits a grapheme: look again from the next char.
                offset = byte_start + line[byte_start..].chars().next().map_or(1, char::len_utf8);
                continue;
            }
            let start_col = line[..byte_start].chars().count();
            let end_col = line[..byte_end].chars().count();
            out.push(TextRange::new(
                TextPosition::new(line_idx, start_col),
                TextPosition::new(line_idx, end_col),
            ));
            offset = byte_end;
        }
    }
    out
}

/// Return the byte offsets of the grapheme boundaries of `line`, both ends
/// included, or `None` for ASCII, where every byte is a boundary.
fn grapheme_boundaries(line: &str) -> Option<Vec<usize>> {
    if line.is_ascii() {
        return None;
    }
    let mut bytes: Vec<usize> = text::grapheme_indices(line).map(|(byte, _)| byte).collect();
    bytes.push(line.len());
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_match_never_splits_a_grapheme() {
        let buffer = TextBuffer::new("e\u{301}x ex e");
        let starts: Vec<usize> = find_matches(&buffer, "e")
            .iter()
            .map(|range| range.start.column)
            .collect();
        assert_eq!(starts, [4, 7]);
        assert_eq!(find_matches(&buffer, "e\u{301}").len(), 1);
        assert!(find_matches(&buffer, "\u{301}").is_empty());
        assert_eq!(find_matches(&buffer, "x").len(), 2);
    }

    #[test]
    fn search_finds_matches() {
        let buffer = TextBuffer::new("hello\nworld hello");
        let matches = find_matches(&buffer, "hello");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].start.line, 0);
        assert_eq!(matches[1].start.line, 1);
    }

    #[test]
    fn next_from_finds_the_match_after_a_position_and_wraps() {
        let buffer = TextBuffer::new("ab ab\nab");
        let at = |line, column| TextPosition::new(line, column);
        let mut state = SearchState::new();
        state.set_query(&buffer, "ab", SearchDirection::Forward);
        let start = |state: &mut SearchState, from, reverse| {
            state
                .next_from(&buffer, from, reverse)
                .map(|range| range.start)
        };
        assert_eq!(start(&mut state, at(0, 0), false), Some(at(0, 3)));
        assert_eq!(state.current_index(), Some(1));
        assert_eq!(start(&mut state, at(0, 1), false), Some(at(0, 3)));
        assert_eq!(start(&mut state, at(1, 0), false), Some(at(0, 0)));
        assert_eq!(start(&mut state, at(0, 3), true), Some(at(0, 0)));
        assert_eq!(start(&mut state, at(0, 0), true), Some(at(1, 0)));
        assert_eq!(start(&mut state, at(0, 4), true), Some(at(0, 3)));

        state.set_query(&buffer, "ab", SearchDirection::Backward);
        assert_eq!(start(&mut state, at(0, 3), false), Some(at(0, 0)));
        assert_eq!(start(&mut state, at(0, 3), true), Some(at(1, 0)));

        state.set_query(&buffer, "zz", SearchDirection::Forward);
        assert_eq!(start(&mut state, at(0, 0), false), None);
    }

    #[test]
    fn search_moves_forward_and_backward() {
        let buffer = TextBuffer::new("alpha beta alpha");
        let mut state = SearchState::new();
        state.set_query(&buffer, "alpha", SearchDirection::Forward);
        let first = state.current_match().unwrap().start;
        let second = state.move_next(&buffer, false).unwrap();
        assert_ne!(first, second);
        let back = state.move_next(&buffer, true).unwrap();
        assert_eq!(back, first);
    }

    #[test]
    fn installed_matches_make_the_first_match_at_or_after_from_current() {
        let buffer = TextBuffer::new("one two one");
        let ranges = find_matches(&buffer, "one");
        assert_eq!(ranges.len(), 2);
        let mut state = SearchState::new();
        state.set_matches(&buffer, ranges.clone(), TextPosition::new(0, 4));
        assert_eq!(state.match_count(), 2);
        assert_eq!(state.current_index(), Some(1));
        assert_eq!(state.current_match(), Some(ranges[1]));
    }

    #[test]
    fn installed_matches_wrap_and_survive_navigation() {
        let buffer = TextBuffer::new("one two one");
        let ranges = find_matches(&buffer, "one");
        let mut state = SearchState::new();
        state.set_matches(&buffer, ranges.clone(), TextPosition::new(0, 99));
        assert_eq!(state.current_match(), Some(ranges[0]));
        // The buffer did not change, so navigation keeps the installed set
        // instead of recomputing a literal query.
        state.move_next(&buffer, false);
        assert_eq!(state.current_match(), Some(ranges[1]));
        state.move_next(&buffer, false);
        assert_eq!(state.current_match(), Some(ranges[0]));
    }

    #[test]
    fn suffix_search_starts_before_nonoverlapping_match_collection() {
        let buffer = TextBuffer::new("ééé");
        assert_eq!(
            find_matches_from(&buffer, "éé", TextPosition::new(0, 1)),
            [TextRange::new(
                TextPosition::new(0, 1),
                TextPosition::new(0, 3)
            )]
        );
        assert!(find_matches_from(&buffer, "é", TextPosition::new(0, 99)).is_empty());
        assert!(find_matches_from(&buffer, "é", TextPosition::new(1, 0)).is_empty());
        assert!(find_matches(&buffer, "").is_empty());
    }

    #[test]
    fn crlf_and_lf_have_equivalent_search_matches() {
        let lf = TextBuffer::new("ab\nab\n");
        let crlf = TextBuffer::new("ab\r\nab\r\n");
        assert_eq!(find_matches(&lf, "ab"), find_matches(&crlf, "ab"));
        assert!(find_matches(&crlf, "\r").is_empty());
    }
}

/// Prompt modes for search and replace interactions.
#[derive(Debug, Clone)]
pub(super) enum PromptState {
    /// Search query input.
    Search {
        /// Search direction.
        direction: SearchDirection,
        /// Current query text.
        query: String,
    },
    /// Replace query input.
    ReplaceQuery {
        /// Current query text.
        query: String,
    },
    /// Replace replacement input.
    ReplaceWith {
        /// Query text.
        query: String,
        /// Replacement text.
        replacement: String,
    },
    /// Confirm replacements one by one.
    ReplaceConfirm {
        /// Query text.
        query: String,
        /// Replacement text.
        replacement: String,
        /// Match list.
        matches: Vec<TextRange>,
        /// Current match index.
        index: usize,
    },
}

impl Editor {
    /// Start a search prompt in the specified direction.
    pub(super) fn start_search_prompt(&mut self, direction: SearchDirection) {
        self.prompt = Some(PromptState::Search {
            direction,
            query: String::new(),
        });
    }

    /// Start a replace prompt.
    pub(super) fn start_replace_prompt(&mut self) {
        self.prompt = Some(PromptState::ReplaceQuery {
            query: String::new(),
        });
    }

    /// Return the string field the current prompt is editing, if any.
    fn prompt_edit_field(prompt: &mut PromptState) -> Option<&mut String> {
        match prompt {
            PromptState::Search { query, .. } | PromptState::ReplaceQuery { query } => Some(query),
            PromptState::ReplaceWith { replacement, .. } => Some(replacement),
            PromptState::ReplaceConfirm { .. } => None,
        }
    }

    /// Clear the field of the open prompt, if it has one. Returns whether a
    /// prompt is open.
    pub(super) fn clear_prompt(&mut self) -> bool {
        let Some(prompt) = self.prompt.as_mut() else {
            return false;
        };
        if let Some(field) = Self::prompt_edit_field(prompt) {
            field.clear();
        }
        true
    }

    /// Classify one key while a prompt is open.
    ///
    /// Esc cancels every prompt. Editable prompts take backspace, plain text,
    /// and enter; a replace confirmation takes the characters it answers.
    pub(super) fn prompt_command(&self, key: key::Key) -> Option<PromptCommand> {
        let prompt = self.prompt.as_ref()?;
        if matches!(key.key, key::KeyCode::Esc) {
            return Some(PromptCommand::Cancel);
        }
        match prompt {
            PromptState::ReplaceConfirm { .. } => match key.key {
                key::KeyCode::Char(character) => Some(PromptCommand::Confirm(character)),
                _ => None,
            },
            _ => match key.key {
                key::KeyCode::Backspace => Some(PromptCommand::Backspace),
                key::KeyCode::Char(character) if !key.mods.ctrl && !key.mods.alt => {
                    Some(PromptCommand::Push(character))
                }
                key::KeyCode::Enter => Some(PromptCommand::Enter),
                _ => None,
            },
        }
    }

    /// Run one classified prompt command.
    pub(super) fn execute_prompt(&mut self, command: PromptCommand, ctx: &mut dyn Context) {
        match command {
            PromptCommand::Cancel => self.close_prompt(),
            PromptCommand::Backspace => {
                if let Some(field) = self.prompt.as_mut().and_then(Self::prompt_edit_field) {
                    let _ = field.pop();
                }
            }
            PromptCommand::Push(character) => {
                if let Some(field) = self.prompt.as_mut().and_then(Self::prompt_edit_field) {
                    field.push(character);
                }
            }
            PromptCommand::Enter => self.handle_prompt_enter(ctx),
            PromptCommand::Confirm(character) => self.handle_replace_confirm(character, ctx),
        }
    }

    /// Advance a search or replace prompt on Enter.
    fn handle_prompt_enter(&mut self, ctx: &mut dyn Context) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        match prompt {
            PromptState::Search { direction, query } => {
                self.search.set_query(&self.buffer, query, direction);
                self.jump_to_match(ctx, false);
            }
            PromptState::ReplaceQuery { query } => {
                self.prompt = Some(PromptState::ReplaceWith {
                    query,
                    replacement: String::new(),
                });
            }
            PromptState::ReplaceWith { query, replacement } => {
                let matches = find_matches(&self.buffer, &query);
                self.prompt = Some(PromptState::ReplaceConfirm {
                    query,
                    replacement,
                    matches,
                    index: 0,
                });
            }
            prompt => self.prompt = Some(prompt),
        }
    }

    /// Handle y/n/a/q during replace confirmation.
    fn handle_replace_confirm(&mut self, c: char, ctx: &mut dyn Context) {
        if self.config.interaction.read_only()
            && matches!(self.prompt, Some(PromptState::ReplaceConfirm { .. }))
        {
            self.close_prompt();
            return;
        }
        let Some(PromptState::ReplaceConfirm {
            query,
            replacement,
            matches,
            index,
        }) = self.prompt.as_mut()
        else {
            return;
        };

        match c {
            'y' => {
                if let Some(range) = matches.get(*index).copied() {
                    let query = query.clone();
                    let replacement = replacement.clone();
                    let new_matches = self.replace_match(&query, &replacement, range, ctx);
                    if let Some(PromptState::ReplaceConfirm { matches, index, .. }) =
                        self.prompt.as_mut()
                    {
                        *matches = new_matches;
                        *index = 0;
                    }
                }
            }
            'n' => {
                *index = index.saturating_add(1);
            }
            'a' => {
                // Replace every remaining match, and close.
                let query = query.clone();
                let replacement = replacement.clone();
                let mut matches = mem::take(matches);
                let mut index = *index;
                while let Some(range) = matches.get(index).copied() {
                    matches = self.replace_match(&query, &replacement, range, ctx);
                    index = 0;
                }
                self.close_prompt();
                return;
            }
            'q' => {
                self.close_prompt();
                return;
            }
            _ => {}
        }

        if let Some(PromptState::ReplaceConfirm { matches, index, .. }) = &self.prompt
            && *index >= matches.len()
        {
            self.close_prompt();
        }
    }

    /// Replace a match range and return the remaining matches from the cursor.
    pub(super) fn replace_match(
        &mut self,
        query: &str,
        replacement: &str,
        range: TextRange,
        ctx: &mut dyn Context,
    ) -> Vec<TextRange> {
        self.buffer
            .set_selection(Selection::new(range.start, range.end));
        self.handle_insert_text(replacement);
        self.ensure_cursor_visible(ctx);
        find_matches_from(&self.buffer, query, self.buffer.cursor())
    }

    /// Render the search/replace prompt overlay.
    pub(super) fn render_prompt(
        &self,
        r: &mut Render,
        view_rect: Rect,
        origin: Point,
    ) -> Result<()> {
        let Some(prompt) = &self.prompt else {
            return Ok(());
        };
        let y = origin.y.saturating_add(view_rect.h.saturating_sub(1));
        let line = Line::new(origin.x, y, view_rect.w);
        r.text(roles::PROMPT, line, &prompt_text(prompt))
    }
}
