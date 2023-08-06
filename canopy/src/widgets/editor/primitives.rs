use super::state::State;

/// A position in the editor. The column offset, but not the chunk offset, may be beyond the bounds of the line.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Position {
    pub chunk: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Position {
            chunk: line,
            column,
        }
    }

    /// Return the next logical position after this one. Returns the final
    /// position if this one is out of bounds.
    fn next(&self, s: &State) -> Position {
        let last = s.last();
        if self.chunk > last.chunk {
            last
        } else if self.column < s.chunks[self.chunk].len() - 1 {
            Position::new(self.chunk, self.column + 1)
        } else {
            Position::new(self.chunk + 1, 0).cap_inclusive(s)
        }
    }

    /// Constrain a `Position` to be within the exclusive range of `Core`.
    pub(super) fn cap_exclusive(&self, s: &State) -> Position {
        let ep = s.last();
        if self.chunk > ep.chunk {
            Position {
                chunk: ep.chunk,
                column: s.chunks[ep.chunk].len(),
            }
        } else if s.chunks[self.chunk].len() < self.column + 1 {
            Position {
                chunk: self.chunk,
                column: s.chunks[self.chunk].len(),
            }
        } else {
            Position {
                chunk: self.chunk,
                column: self.column,
            }
        }
    }

    /// Constrain a `Position` to be within the inclusive range of `Core`.
    fn cap_inclusive(&self, s: &State) -> Position {
        let ep = s.last();
        if self.chunk > ep.chunk {
            Position {
                chunk: ep.chunk,
                column: s.chunks[ep.chunk].len() - 1,
            }
        } else if s.chunks[self.chunk].len() < self.column {
            Position {
                chunk: self.chunk,
                column: s.chunks[self.chunk].len() - 1,
            }
        } else {
            Position {
                chunk: self.chunk,
                column: self.column,
            }
        }
    }
}

impl From<(usize, usize)> for Position {
    fn from((line, column): (usize, usize)) -> Self {
        Position::new(line, column)
    }
}

/// A wrapped line in the editor, represented as a chunk index and a line offset within that chunk.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Line {
    pub chunk: usize,
    pub offset: usize,
}

impl Line {
    /// Add a number of lines to this one, returning the resulting line. If the line is beyond bounds, return None.
    pub(super) fn add(&self, s: &State, n: usize) -> Option<Line> {
        // FIXME: Make this more efficient
        let mut chunk = self.chunk;
        let mut offset = self.offset;
        for _ in 0..n {
            if offset + 1 < s.chunks[chunk].wraps.len() {
                offset += 1;
            } else if chunk + 1 < s.chunks.len() {
                chunk += 1;
                offset = 0;
            } else {
                return None;
            }
        }
        Some(Line { chunk, offset })
    }
}

impl From<(usize, usize)> for Line {
    fn from((chunk, offset): (usize, usize)) -> Self {
        Line { chunk, offset }
    }
}

/// A window of wrapped lines, represented as a line offset and a height.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Window {
    pub line: Line,
    pub height: usize,
}

impl Window {
    /// Create a Window from an offset and a screen height.
    pub(super) fn from_offset(s: &State, offset: usize, height: usize) -> Self {
        let line = s.wrapped_line_offset(offset);
        Window { line, height }
    }

    /// Return the lines within the window. Lines can be Null if they are beyond
    /// the bounds of the document.
    pub(super) fn lines(&self, s: &State) -> Vec<Option<Line>> {
        let mut lines = Vec::with_capacity(self.height);
        let mut line = Some(self.line);
        for _ in 0..self.height {
            lines.push(line);
            if let Some(l) = line {
                line = l.add(s, 1);
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_position() {
        let s = State::new("a\nbb");
        assert_eq!(Position::new(0, 0).cap_inclusive(&s), (0, 0).into());
        assert_eq!(Position::new(0, 2).cap_inclusive(&s), (0, 0).into());
        assert_eq!(Position::new(3, 0).cap_inclusive(&s), (1, 1).into());
        assert_eq!(Position::new(3, 3).cap_inclusive(&s), (1, 1).into());

        assert_eq!(Position::new(0, 0).cap_exclusive(&s), (0, 0).into());
        assert_eq!(Position::new(3, 3).cap_exclusive(&s), (1, 2).into());
    }

    #[test]
    fn position_ord() {
        assert!(Position::new(5, 5) == Position::new(5, 5));
        assert!(Position::new(4, 5) < Position::new(5, 5));
        assert!(Position::new(5, 4) < Position::new(5, 5));
    }
}
