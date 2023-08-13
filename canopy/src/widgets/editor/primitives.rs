use super::state::State;

/// A position in the editor. The offset may be one character beyond the bounds of the line.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Position {
    /// The offset of the chunk in the editor state.
    pub chunk: usize,
    /// The column offset within the chunk.
    pub offset: usize,
}

impl Position {
    pub fn new(chunk: usize, offset: usize) -> Self {
        Position { chunk, offset }
    }

    /// Return the next logical position after this one, without crossing chunk bounds. Does nothing if we abutt to the
    /// end of the line. If the current position is out of bounds, return a position at the end of the closest matching
    /// chunk.
    pub(super) fn right(&self, s: &State) -> Position {
        let last = s.last();
        if self.chunk > last.chunk {
            Position::new(s.chunks.len() - 1, s.chunks[s.chunks.len() - 1].len())
        } else if self.offset > s.chunks[self.chunk].len() {
            Position::new(self.chunk, s.chunks[self.chunk].len())
        } else {
            Position::new(
                self.chunk,
                (self.offset + 1).min(s.chunks[self.chunk].len()),
            )
        }
    }

    /// Constrain a `Position` to be within the exclusive range of `Core`.
    pub(super) fn cap_exclusive(&self, s: &State) -> Position {
        let ep = s.last();
        if self.chunk > ep.chunk {
            Position {
                chunk: ep.chunk,
                offset: s.chunks[ep.chunk].len(),
            }
        } else if s.chunks[self.chunk].len() < self.offset + 1 {
            Position {
                chunk: self.chunk,
                offset: s.chunks[self.chunk].len(),
            }
        } else {
            Position {
                chunk: self.chunk,
                offset: self.offset,
            }
        }
    }

    /// Constrain a `Position` to be within the inclusive range of `Core`.
    fn cap_inclusive(&self, s: &State) -> Position {
        let ep = s.last();
        if self.chunk > ep.chunk {
            Position {
                chunk: ep.chunk,
                offset: s.chunks[ep.chunk].len() - 1,
            }
        } else if s.chunks[self.chunk].len() < self.offset {
            Position {
                chunk: self.chunk,
                offset: s.chunks[self.chunk].len() - 1,
            }
        } else {
            Position {
                chunk: self.chunk,
                offset: self.offset,
            }
        }
    }
}

impl From<(usize, usize)> for Position {
    fn from((chunk, offset): (usize, usize)) -> Self {
        Position::new(chunk, offset)
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
        let line = s.line_from_offset(offset);
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

/// Split the input text into lines of the given width, and return the start and end offsets for each line.
fn wrap_offsets(s: &str, width: usize) -> Vec<(usize, usize)> {
    let mut offsets = Vec::new();
    let words = textwrap::core::break_words(
        textwrap::WordSeparator::UnicodeBreakProperties.find_words(s),
        width,
    );
    if words.is_empty() {
        return vec![(0, 0)];
    }
    let lines = textwrap::wrap_algorithms::wrap_first_fit(&words, &[width as f64]);
    for l in lines {
        let start = unsafe { l[0].word.as_ptr().offset_from(s.as_ptr()) };
        let last = l[l.len() - 1];
        let end = unsafe { last.word.as_ptr().offset_from(s.as_ptr()) as usize + last.word.len() };
        offsets.push((start as usize, end));
    }
    offsets
}

/// A chunk is a single piece of text with no newlines. An example might be a contiguous paragraph of text. A Chunk may
/// be wrapped into multiple Lines for display.
#[derive(Debug, Clone, Eq, Hash)]
pub(super) struct Chunk {
    /// The raw text of the line.
    text: String,
    /// The start and end offsets of each wrapped line in the chunk.
    pub wraps: Vec<(usize, usize)>,
    /// The width to which this chunk was wrapped
    // FIXME: This should not be stored in every line
    pub wrap_width: usize,
}

impl PartialEq for Chunk {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Chunk {
    pub fn new(s: &str, wrap: usize) -> Chunk {
        let mut l = Chunk {
            text: s.into(),
            wraps: vec![],
            wrap_width: wrap,
        };
        l.wrap(wrap);
        l
    }

    pub fn replace_range<R: std::ops::RangeBounds<usize>>(&mut self, range: R, s: &str) {
        self.text.replace_range(range, s);
        self.wrap(self.wrap_width);
    }

    pub fn push_str(&mut self, s: &str) {
        self.text.push_str(s);
        self.wrap(self.wrap_width);
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Insert a string at the given offset
    pub fn insert(&mut self, offset: usize, s: &str) {
        self.text.insert_str(offset, s);
        self.wrap(self.wrap_width);
    }

    /// Wrap the chunk into lines of the given width, and return the number of wrapped lines that resulted.
    pub fn wrap(&mut self, width: usize) -> usize {
        self.wraps = wrap_offsets(&self.text, width);
        self.wrap_width = width;
        self.wraps.len()
    }

    /// Return a wrapped line, by offset within this chunk. The offset must be within range, or this function will panic.
    pub fn wrapped_line(&self, off: usize) -> &str {
        let (start, end) = self.wraps[off];
        &self.text[start..end]
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
    fn position_right() {
        let s = State::new("a\nbb");
        assert_eq!(Position::new(0, 0).right(&s), Position::new(0, 1));
        assert_eq!(Position::new(0, 1).right(&s), Position::new(0, 1));
        assert_eq!(Position::new(1, 1).right(&s), Position::new(1, 2));
        assert_eq!(Position::new(1, 2).right(&s), Position::new(1, 2));

        // // Beyond bounds
        assert_eq!(Position::new(1, 3).right(&s), Position::new(1, 2));
        assert_eq!(Position::new(5, 0).right(&s), Position::new(1, 2));
    }

    #[test]
    fn position_ord() {
        assert!(Position::new(5, 5) == Position::new(5, 5));
        assert!(Position::new(4, 5) < Position::new(5, 5));
        assert!(Position::new(5, 4) < Position::new(5, 5));
    }

    fn twrap(s: &str, width: usize, expected: Vec<String>) {
        let offsets = wrap_offsets(s, width);
        assert_eq!(offsets.len(), expected.len());
        for i in 0..offsets.len() {
            let (start, end) = offsets[i];
            let line = &s[start..end];
            assert_eq!(line, expected[i]);
        }
    }

    #[test]
    fn test_wrap_offsets() {
        twrap("", 3, vec!["".into()]);
        twrap("one two three four", 100, vec!["one two three four".into()]);
        twrap("one two", 3, vec!["one".into(), "two".into()]);
        twrap(
            "one two three four",
            10,
            vec!["one two".into(), "three four".into()],
        );
    }
}
