/// A position in the text buffer expressed as a logical line and a char index.
///
/// Positions order by line, then by column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

/// Return the position of the character of `text` that holds UTF-8 byte
/// `offset`, as an editor over `text` counts it.
///
/// Lines break at LF and at CRLF, and a column counts chars. An offset past
/// the end clamps to the end, and an offset in a line's break, such as the
/// LF of a CRLF, clamps to the end of that line.
pub fn position_of_offset(text: &str, offset: usize) -> TextPosition {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = text[start..offset].chars().count();
    TextPosition::new(line, column.min(line_chars(line_text(text, start))))
}

/// Return the UTF-8 byte offset in `text` of `position`, as an editor over
/// `text` counts it, clamped to the text.
///
/// A line past the last clamps to the last line, and a column past the end
/// of its line clamps to the end of the line, before its break.
pub fn offset_of_position(text: &str, position: TextPosition) -> usize {
    let mut start = 0;
    for _ in 0..position.line {
        match text[start..].find('\n') {
            Some(index) => start += index + 1,
            None => break,
        }
    }
    let line = line_text(text, start);
    let line = &line[..line.len() - line_break(line)];
    start
        + line
            .char_indices()
            .nth(position.column)
            .map_or(line.len(), |(index, _)| index)
}

/// Return the line of `text` that starts at byte `start`, its break
/// included.
fn line_text(text: &str, start: usize) -> &str {
    let rest = &text[start..];
    rest.find('\n').map_or(rest, |index| &rest[..=index])
}

/// Return the length in bytes of the break that ends `line`: two for CRLF,
/// one for LF, and none for the last line.
fn line_break(line: &str) -> usize {
    if line.ends_with("\r\n") {
        2
    } else {
        usize::from(line.ends_with('\n'))
    }
}

/// Return the chars of `line` before its break.
fn line_chars(line: &str) -> usize {
    line[..line.len() - line_break(line)].chars().count()
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
    #[must_use]
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
}
