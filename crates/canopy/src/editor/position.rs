use std::cmp::Ordering;

/// A position in the text buffer expressed as a logical line and a char index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextPosition {
    /// Logical line index (0-based).
    pub line: usize,
    /// Char index within the line (0-based).
    pub column: usize,
}

impl TextPosition {
    /// Create a new text position.
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

impl Ord for TextPosition {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.line.cmp(&other.line) {
            Ordering::Equal => self.column.cmp(&other.column),
            ordering => ordering,
        }
    }
}

impl PartialOrd for TextPosition {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A half-open text range expressed in buffer coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextRange {
    /// Range start position (inclusive).
    pub start: TextPosition,
    /// Range end position (exclusive).
    pub end: TextPosition,
}

impl TextRange {
    /// Construct a text range.
    pub fn new(start: TextPosition, end: TextPosition) -> Self {
        Self { start, end }
    }

    /// Return a range with start/end ordered.
    pub fn normalized(self) -> Self {
        if self.start <= self.end {
            self
        } else {
            Self {
                start: self.end,
                end: self.start,
            }
        }
    }

    /// Return true if the range is empty.
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Return the range start and end ordered.
    pub fn ordered(self) -> (TextPosition, TextPosition) {
        let normalized = self.normalized();
        (normalized.start, normalized.end)
    }
}
