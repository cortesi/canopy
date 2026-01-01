use super::{TextBuffer, TextPosition, TextRange};

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
