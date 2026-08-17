use canopy::{
    Context, EventOutcome,
    error::Result,
    event::{Event, key},
    geom::{Line, Point, Rect},
    render::Render,
};

use super::{
    Selection, TextBuffer, TextPosition, TextRange,
    widget::{Editor, prompt_text},
};

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
        self.recompute(buffer);
        self.current = if self.matches.is_empty() {
            None
        } else if direction == SearchDirection::Forward {
            Some(0)
        } else {
            Some(self.matches.len().saturating_sub(1))
        };
    }

    /// Update match cache if the buffer changed.
    pub fn update(&mut self, buffer: &TextBuffer) {
        if self.query.is_empty() {
            self.matches.clear();
            self.current = None;
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
    pub fn matches_for_line(&self, line: usize) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        for range in &self.matches {
            if range.start.line == line {
                ranges.push((range.start.column, range.end.column));
            }
        }
        ranges
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

    /// Recompute match cache for the current query.
    fn recompute(&mut self, buffer: &TextBuffer) {
        self.matches = find_matches(buffer, &self.query);
        self.revision = buffer.revision();
    }
}

impl Default for SearchState {
    fn default() -> Self {
        Self::new()
    }
}

/// Find text matches for a query within the buffer.
pub fn find_matches(buffer: &TextBuffer, query: &str) -> Vec<TextRange> {
    if query.is_empty() || query.contains('\n') {
        return Vec::new();
    }

    let mut out = Vec::new();
    for line_idx in 0..buffer.line_count() {
        let line = buffer.line_text(line_idx);
        let mut offset = 0usize;
        while let Some(found) = line[offset..].find(query) {
            let byte_start = offset.saturating_add(found);
            let byte_end = byte_start.saturating_add(query.len());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_finds_matches() {
        let buffer = TextBuffer::new("hello\nworld hello");
        let matches = find_matches(&buffer, "hello");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].start.line, 0);
        assert_eq!(matches[1].start.line, 1);
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
        /// Whether to replace all remaining matches.
        replace_all: bool,
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

    /// Handle prompt input events.
    pub(super) fn handle_prompt_event(
        &mut self,
        event: &Event,
        ctx: &mut dyn Context,
    ) -> EventOutcome {
        let Some(prompt) = self.prompt.clone() else {
            return EventOutcome::Ignore;
        };

        match (prompt, event) {
            (
                PromptState::Search { direction, query },
                Event::Key(key::Key {
                    key: key::KeyCode::Enter,
                    ..
                }),
            ) => {
                self.search.set_query(&self.buffer, query, direction);
                if let Some(pos) = self.search.current_match().map(|range| range.start) {
                    self.buffer.set_cursor(pos);
                    self.ensure_cursor_visible(ctx);
                }
                self.prompt = None;
                EventOutcome::Handle
            }
            (
                PromptState::Search {
                    direction,
                    mut query,
                },
                Event::Key(key::Key {
                    key: key::KeyCode::Backspace,
                    ..
                }),
            ) => {
                let _ = query.pop();
                self.prompt = Some(PromptState::Search { direction, query });
                EventOutcome::Handle
            }
            (
                PromptState::Search {
                    direction,
                    mut query,
                },
                Event::Key(key::Key {
                    key: key::KeyCode::Char(c),
                    mods,
                }),
            ) if !mods.ctrl && !mods.alt => {
                query.push(*c);
                self.prompt = Some(PromptState::Search { direction, query });
                EventOutcome::Handle
            }
            (
                PromptState::Search { .. },
                Event::Key(key::Key {
                    key: key::KeyCode::Esc,
                    ..
                }),
            ) => {
                self.prompt = None;
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceQuery { query },
                Event::Key(key::Key {
                    key: key::KeyCode::Enter,
                    ..
                }),
            ) => {
                self.prompt = Some(PromptState::ReplaceWith {
                    query,
                    replacement: String::new(),
                });
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceQuery { mut query },
                Event::Key(key::Key {
                    key: key::KeyCode::Backspace,
                    ..
                }),
            ) => {
                let _ = query.pop();
                self.prompt = Some(PromptState::ReplaceQuery { query });
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceQuery { mut query },
                Event::Key(key::Key {
                    key: key::KeyCode::Char(c),
                    mods,
                }),
            ) if !mods.ctrl && !mods.alt => {
                query.push(*c);
                self.prompt = Some(PromptState::ReplaceQuery { query });
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceQuery { .. },
                Event::Key(key::Key {
                    key: key::KeyCode::Esc,
                    ..
                }),
            ) => {
                self.prompt = None;
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceWith { query, replacement },
                Event::Key(key::Key {
                    key: key::KeyCode::Enter,
                    ..
                }),
            ) => {
                let matches = find_matches(&self.buffer, &query);
                self.prompt = Some(PromptState::ReplaceConfirm {
                    query,
                    replacement,
                    matches,
                    index: 0,
                    replace_all: false,
                });
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceWith {
                    query,
                    mut replacement,
                },
                Event::Key(key::Key {
                    key: key::KeyCode::Backspace,
                    ..
                }),
            ) => {
                let _ = replacement.pop();
                self.prompt = Some(PromptState::ReplaceWith { query, replacement });
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceWith {
                    query,
                    mut replacement,
                },
                Event::Key(key::Key {
                    key: key::KeyCode::Char(c),
                    mods,
                }),
            ) if !mods.ctrl && !mods.alt => {
                replacement.push(*c);
                self.prompt = Some(PromptState::ReplaceWith { query, replacement });
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceWith { .. },
                Event::Key(key::Key {
                    key: key::KeyCode::Esc,
                    ..
                }),
            ) => {
                self.prompt = None;
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceConfirm {
                    query,
                    replacement,
                    mut matches,
                    mut index,
                    mut replace_all,
                },
                Event::Key(key::Key {
                    key: key::KeyCode::Char(c),
                    ..
                }),
            ) => {
                match *c {
                    'y' => {
                        let (new_matches, next_index) =
                            self.replace_match(&query, &replacement, matches, index, ctx);
                        matches = new_matches;
                        index = next_index;
                    }
                    'n' => {
                        index = index.saturating_add(1);
                    }
                    'a' => {
                        replace_all = true;
                    }
                    'q' => {
                        self.prompt = None;
                        return EventOutcome::Handle;
                    }
                    _ => {}
                }

                if replace_all {
                    while index < matches.len() {
                        let (new_matches, next_index) =
                            self.replace_match(&query, &replacement, matches, index, ctx);
                        matches = new_matches;
                        index = next_index;
                    }
                }

                if index >= matches.len() {
                    self.prompt = None;
                } else {
                    self.prompt = Some(PromptState::ReplaceConfirm {
                        query,
                        replacement,
                        matches,
                        index,
                        replace_all,
                    });
                }
                EventOutcome::Handle
            }
            (
                PromptState::ReplaceConfirm { .. },
                Event::Key(key::Key {
                    key: key::KeyCode::Esc,
                    ..
                }),
            ) => {
                self.prompt = None;
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
        }
    }

    /// Replace a match at an index and return updated matches and next index.
    pub(super) fn replace_match(
        &mut self,
        query: &str,
        replacement: &str,
        matches: Vec<TextRange>,
        index: usize,
        ctx: &mut dyn Context,
    ) -> (Vec<TextRange>, usize) {
        let Some(range) = matches.get(index).copied() else {
            return (matches, index);
        };
        self.buffer
            .set_selection(Selection::new(range.start, range.end));
        self.handle_insert_text(replacement);
        self.ensure_cursor_visible(ctx);
        let updated = find_matches(&self.buffer, query);
        let next_index = updated
            .iter()
            .position(|candidate| candidate.start > range.start)
            .unwrap_or(updated.len());
        (updated, next_index)
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
        r.text("editor/prompt", line, &prompt_text(prompt))
    }
}
