use super::state::State;

/// A position that can be clamped within the bounds of a `State`.
pub trait Pos: Sized {
    /// Create a new item and clamp it
    fn new(s: &State, chunk: usize, offset: usize) -> Self;
    /// Constrain within state bounds, and return a new item
    fn constrain(&self, s: &State) -> Self;
    /// Retrieve the chunk and offset of the cursor.
    fn chunk_offset(&self) -> (usize, usize);
    /// Is this cursor between wrapped lines?
    fn is_between(&self, s: &State) -> bool;

    /// Shift the cursor by an offset within a chunk. If the new position is out of bounds, return the closest matching
    /// position within the chunk. If the new offset lands on a character that is between lines, we continue in the same
    /// direction until we find a character that is in bounds.
    fn shift(&self, s: &State, n: isize) -> Self {
        let (chunk, offset) = self.chunk_offset();
        let mut ret = Self::new(s, chunk, offset.saturating_add_signed(n));
        let btw = if n < 0 { -1 } else { 1 };
        // If we're between wraps we look for the next wrapped location.
        while ret.is_between(s) {
            let (c, o) = ret.chunk_offset();
            ret = Self::new(s, c, o.saturating_add_signed(btw));
        }
        ret
    }

    /// Shift the chunk offset. If the new position is out of bounds, return the closest matching position.
    fn shift_chunk(&self, s: &State, n: isize) -> Self {
        let (chunk, offset) = self.chunk_offset();
        Self::new(s, chunk.saturating_add_signed(n), offset)
    }

    fn shift_line(&self, s: &State, n: isize) -> Self {
        let (chunk, offset) = self.chunk_offset();
        if let Some(l) = Line::from_position(s, (chunk, offset)) {
            if let Some(ret) = l.shift(s, n) {
                let line_offset = offset - l.first_pos(s).offset;
                return if line_offset > ret.len(s) {
                    // If our line offset takes us beyond the end of the line, we return the last position.
                    Self::new(s, ret.chunk, ret.first_pos(s).offset + ret.len(s))
                } else {
                    Self::new(s, ret.chunk, ret.first_pos(s).offset + line_offset)
                };
            }
        }
        Self::new(s, chunk, offset)
    }
}

/// A Cursor, which can either be in insert or character mode. In insert mode, we can point one offset beyond the last
/// character in the chunk.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cursor {
    /// An insert cursor
    Insert(InsertPos),
    /// An visual cursor
    Char(CharPos),
}

impl Cursor {
    /// Shift left or right within a chunk
    pub fn shift(&self, s: &State, n: isize) -> Self {
        match self {
            Cursor::Insert(p) => Cursor::Insert(p.shift(s, n)),
            Cursor::Char(p) => Cursor::Char(p.shift(s, n)),
        }
    }

    /// Shift up and down in the list of chunks.
    pub fn shift_chunk(&self, s: &State, n: isize) -> Self {
        match self {
            Cursor::Insert(p) => Cursor::Insert(p.shift_chunk(s, n)),
            Cursor::Char(p) => Cursor::Char(p.shift_chunk(s, n)),
        }
    }

    /// Shift up and down along wrapped lines.
    pub fn shift_line(&self, s: &State, n: isize) -> Self {
        match self {
            Cursor::Insert(p) => Cursor::Insert(p.shift_line(s, n)),
            Cursor::Char(p) => Cursor::Char(p.shift_line(s, n)),
        }
    }

    /// Return an insert position for the cursor. If the cursor is already in insert mode, this just returns the cursor.
    /// If the cursor is a char cursor, we return the insert point after the current character, capped to the length of
    /// the line.
    pub fn insert(&self, s: &State) -> InsertPos {
        match self {
            Cursor::Insert(p) => *p,
            Cursor::Char(p) => (*p).into(),
        }
        .constrain(s)
    }

    /// Return a cursor of matching type at the given chunk and offset.
    pub fn at(&self, s: &State, chunk: usize, offset: usize) -> Self {
        match self {
            Cursor::Insert(_) => Cursor::Insert(InsertPos::new(s, chunk, offset)),
            Cursor::Char(_) => Cursor::Char(CharPos::new(s, chunk, offset)),
        }
    }

    pub fn constrain(&self, s: &State) -> Self {
        match self {
            Cursor::Insert(p) => Cursor::Insert(p.constrain(s)),
            Cursor::Char(p) => Cursor::Char(p.constrain(s)),
        }
    }
}

/// An insert position. The offset 0 is before the first character in the chunk, and offset `len` is after the last.
///
/// So, given the string abc, where _ is the insertion point, we can have the following possible positions:
///
///    abc_
///    ab_c
///    a_bc
///    _abc
///
/// On empty lines, an insertion point at offset 0 is valid.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InsertPos {
    /// The offset of the chunk in the editor state.
    pub chunk: usize,
    /// The column offset within the chunk.
    pub offset: usize,
}

impl Pos for InsertPos {
    /// Create a new InsertPos and constrain it within the state.
    fn new(s: &State, chunk: usize, offset: usize) -> Self {
        InsertPos { chunk, offset }.constrain(s)
    }

    fn is_between(&self, s: &State) -> bool {
        let c = &s.chunks[self.chunk];
        c.offset_is_between(self.offset)
    }

    fn chunk_offset(&self) -> (usize, usize) {
        (self.chunk, self.offset)
    }

    fn constrain(&self, s: &State) -> Self {
        let ep = s.last();
        if self.chunk > ep.chunk {
            InsertPos {
                chunk: ep.chunk,
                offset: s.chunks[ep.chunk].len(),
            }
        } else if self.offset + 1 > s.chunks[self.chunk].len() {
            InsertPos {
                chunk: self.chunk,
                offset: s.chunks[self.chunk].len(),
            }
        } else {
            *self
        }
    }
}

impl From<(usize, usize)> for InsertPos {
    fn from((chunk, offset): (usize, usize)) -> Self {
        InsertPos { chunk, offset }
    }
}

impl From<CharPos> for InsertPos {
    fn from(cp: CharPos) -> Self {
        let (chunk, offset) = cp.chunk_offset();
        InsertPos { chunk, offset }
    }
}

/// A characgter position. Offset 0 is the first character in the chunk, and offset `len - 1` is the last.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CharPos {
    /// The offset of the chunk in the editor state.
    pub chunk: usize,
    /// The column offset within the chunk.
    pub offset: usize,
}

impl Pos for CharPos {
    /// Create a new CharPos and constrain it within the state.
    fn new(s: &State, chunk: usize, offset: usize) -> Self {
        CharPos { chunk, offset }.constrain(s)
    }

    fn chunk_offset(&self) -> (usize, usize) {
        (self.chunk, self.offset)
    }

    fn is_between(&self, s: &State) -> bool {
        let c = &s.chunks[self.chunk];
        c.offset_is_between(self.offset)
    }

    fn constrain(&self, s: &State) -> Self {
        let ep = s.last();
        if self.chunk > ep.chunk {
            CharPos {
                chunk: ep.chunk,
                offset: s.chunks[ep.chunk].len() - 1,
            }
        } else if s.chunks[self.chunk].len() <= self.offset {
            CharPos {
                chunk: self.chunk,
                offset: s.chunks[self.chunk].len().saturating_sub(1),
            }
        } else {
            *self
        }
    }
}

impl From<(usize, usize)> for CharPos {
    fn from((chunk, offset): (usize, usize)) -> Self {
        CharPos { chunk, offset }
    }
}

impl From<InsertPos> for CharPos {
    fn from(cp: InsertPos) -> Self {
        let (chunk, offset) = cp.chunk_offset();
        CharPos {
            chunk,
            offset: offset.saturating_sub(1),
        }
    }
}

/// A wrapped line in the editor, represented as a chunk index and a line offset within that chunk. The length of the
/// line is always the set width of the editor.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Line {
    pub chunk: usize,
    pub wrap_idx: usize,
}

impl Line {
    pub(super) fn from_position<T: Into<InsertPos>>(s: &State, pos: T) -> Option<Line> {
        let pos = pos.into();
        for (i, (wstart, wend)) in s.chunks[pos.chunk].wraps.iter().enumerate() {
            if *wstart <= pos.offset && (pos.offset < *wend || *wstart == *wend) {
                return Some(Line {
                    chunk: pos.chunk,
                    wrap_idx: i,
                });
            }
        }
        None
    }

    pub fn len(&self, s: &State) -> usize {
        let (start, end) = s.chunks[self.chunk].wraps[self.wrap_idx];
        end - start
    }

    /// Return the first insert position in this line.
    pub fn first_pos(&self, s: &State) -> InsertPos {
        let (start, _) = s.chunks[self.chunk].wraps[self.wrap_idx];
        (self.chunk, start).into()
    }

    /// Get a Line from a given wrapped line number. If the specified offset is out of range, the last line is returned.
    pub fn from_lineno(s: &State, line_number: usize) -> Line {
        let mut wrapped_offset = 0;
        for (i, c) in s.chunks.iter().enumerate() {
            if wrapped_offset + c.wraps.len() > line_number {
                return (i, line_number - wrapped_offset).into();
            }
            wrapped_offset += c.wraps.len();
        }
        (
            s.chunks.len() - 1,
            s.chunks[s.chunks.len() - 1].wraps.len() - 1,
        )
            .into()
    }

    /// Shift by a number of lines, returning the resulting line. If the line is beyond bounds, return None.
    pub(super) fn shift(&self, s: &State, n: isize) -> Option<Line> {
        // FIXME: Make this more efficient
        let mut chunk = self.chunk;
        let mut wrap_idx = self.wrap_idx;
        if n < 0 {
            for _ in 0..n.abs() {
                if wrap_idx > 0 {
                    wrap_idx -= 1;
                } else if chunk > 0 {
                    chunk -= 1;
                    wrap_idx = 0;
                } else {
                    return None;
                }
            }
        } else {
            for _ in 0..n {
                if wrap_idx + 1 < s.chunks[chunk].wraps.len() {
                    wrap_idx += 1;
                } else if chunk + 1 < s.chunks.len() {
                    chunk += 1;
                    wrap_idx = 0;
                } else {
                    return None;
                }
            }
        }
        Some(Line { chunk, wrap_idx })
    }
}

impl From<(usize, usize)> for Line {
    fn from((chunk, offset): (usize, usize)) -> Self {
        Line {
            chunk,
            wrap_idx: offset,
        }
    }
}

/// A window of wrapped lines, represented as a line offset and a height.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Window {
    pub line: Line,
    pub height: usize,
}

impl Window {
    /// Create a Window from a line number and a screen height.
    #[cfg(test)]
    pub(super) fn from_offset(s: &State, lineno: usize, height: usize) -> Self {
        let line = Line::from_lineno(s, lineno);
        Window { line, height }
    }

    /// A window starting at a specific offset line, with the same dimensions as this one.
    #[cfg(test)]
    pub(super) fn at_line(&self, s: &State, lineno: usize) -> Self {
        let line = Line::from_lineno(s, lineno);
        Window {
            line,
            height: self.height,
        }
    }

    /// A window with a specified height, and the same dimensions as this one.
    pub(super) fn with_height(&self, height: usize) -> Self {
        Window {
            line: self.line,
            height,
        }
    }

    /// Return the lines within the window. Lines can be Null if they are beyond
    /// the bounds of the document.
    pub(super) fn lines(&self, s: &State) -> Vec<Option<Line>> {
        let mut lines = Vec::with_capacity(self.height);
        let mut line = Some(self.line);
        for _ in 0..self.height {
            lines.push(line);
            if let Some(l) = line {
                line = l.shift(s, 1);
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
pub struct Chunk {
    /// The raw text of the line.
    text: String,
    /// The start and end offsets of each wrapped line in the chunk. Not all characters are necessarily included in the
    /// wrapped lines - for instance, whitespace at the end of a line might be elided.
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
    pub fn new(s: &str, wrap_width: usize) -> Chunk {
        let mut l = Chunk {
            text: s.into(),
            wraps: vec![],
            wrap_width,
        };
        l.wrap(wrap_width);
        l
    }

    /// Find the wrapped line for the matching offset. Return None if the offset is out of bounds or if the character is
    /// between wrapped lines.
    pub fn find_wrap(&self, off: usize) -> Option<(usize, usize)> {
        for i in &self.wraps {
            if i.0 <= off && off < i.1 {
                return Some(*i);
            } else if i.0 > off {
                // If we're past the offset, we can stop checking.
                break;
            }
        }
        None
    }

    /// A character is "between" if it is a) within the normal range of the chunk, and b) not part of a wrapped line.
    /// This happens due to the wrapping algorithm eliding whitespace at the end of the line.
    pub fn offset_is_between(&self, off: usize) -> bool {
        if off >= self.text.len() {
            return false;
        }
        self.find_wrap(off).is_none()
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

    // Tiny helper to create an InsertPos
    fn ip(chunk: usize, off: usize) -> InsertPos {
        (chunk, off).into()
    }

    // Tiny helper to create a CharPos
    fn cp(chunk: usize, off: usize) -> CharPos {
        (chunk, off).into()
    }

    /// A variant of the assert_eq macro that coerces its second argument to match the type of the first argument with
    /// .into().
    macro_rules! assert_eqi {
        ($a:expr, $b:expr) => {
            assert_eq!($a, $b.into())
        };
    }

    #[test]
    fn offset_is_between() {
        let c = Chunk::new("foo bar voing", 3);
        assert!(!c.offset_is_between(0));
        assert!(c.offset_is_between(3));
        assert!(!c.offset_is_between(4));
        assert!(c.offset_is_between(7));
        assert!(!c.offset_is_between(20));
    }

    #[test]
    fn insertpos_cap() {
        let s = State::new("a\nbb");
        assert_eqi!(ip(0, 0).constrain(&s), (0, 0));
        assert_eqi!(ip(0, 2).constrain(&s), (0, 1));
        assert_eqi!(ip(3, 0).constrain(&s), (1, 2));
        assert_eqi!(ip(3, 3).constrain(&s), (1, 2));
    }

    #[test]
    fn insertpos_shift() {
        let s = State::new("a\nbb");
        assert_eqi!(ip(0, 0).shift(&s, 1), (0, 1));
        assert_eqi!(ip(0, 0).shift(&s, 100), (0, 1));
        assert_eqi!(ip(0, 0).shift(&s, 100).shift(&s, isize::MAX), (0, 1));
        assert_eqi!(ip(0, 1).shift(&s, 1), (0, 1));
        assert_eqi!(ip(1, 1).shift(&s, 1), (1, 2));
        assert_eqi!(ip(1, 2).shift(&s, 1), (1, 2));

        // Beyond bounds
        assert_eqi!(ip(1, 3).shift(&s, 1), (1, 2));
        assert_eqi!(ip(5, 0).shift(&s, 1), (1, 2));

        // Negative
        assert_eqi!(ip(0, 0).shift(&s, -1), (0, 0));
        assert_eqi!(ip(0, 1).shift(&s, -1), (0, 0));
        assert_eqi!(ip(1, 2).shift(&s, -1), (1, 1));
        assert_eqi!(ip(1, 2).shift(&s, isize::MIN), (1, 0));
    }

    #[test]
    fn charpos_shift() {
        let s = State::new("a\nbb");
        assert_eqi!(cp(0, 0).shift(&s, 1), (0, 0));
        assert_eqi!(cp(0, 0).shift(&s, 100), (0, 0));
        assert_eqi!(cp(0, 0).shift(&s, 100).shift(&s, isize::MAX), (0, 0));
        assert_eqi!(cp(1, 0).shift(&s, 100).shift(&s, isize::MAX), (1, 1));
        assert_eqi!(cp(0, 1).shift(&s, 1), (0, 0));
        assert_eqi!(cp(1, 0).shift(&s, 1), (1, 1));
        assert_eqi!(cp(1, 1).shift(&s, 1), (1, 1));

        // Beyond bounds
        assert_eqi!(cp(1, 3).shift(&s, 1), (1, 1));
        assert_eqi!(cp(5, 0).shift(&s, 1), (1, 1));

        // Negative
        assert_eqi!(cp(0, 0).shift(&s, -1), (0, 0));
        assert_eqi!(cp(0, 1).shift(&s, -1), (0, 0));
        assert_eqi!(cp(1, 2).shift(&s, -1), (1, 1));
        assert_eqi!(cp(1, 2).shift(&s, isize::MIN), (1, 0));
    }

    #[test]
    fn shift_line() {
        let mut s = State::new("a\nbb\n\nccc");
        s.resize_window(3, 10);
        // +ive
        assert_eqi!(ip(0, 0).shift_line(&s, 1), (1, 0));
        assert_eqi!(ip(1, 0).shift_line(&s, 1), (2, 0));
        assert_eqi!(ip(2, 0).shift_line(&s, 1), (3, 0));
        assert_eqi!(ip(3, 0).shift_line(&s, 1), (3, 0));
        // Now at an offset in every line
        assert_eqi!(ip(1, 1).shift_line(&s, 1), (2, 0));
        assert_eqi!(ip(1, 1).shift_line(&s, 2), (3, 1));

        // -ive
        assert_eqi!(ip(0, 0).shift_line(&s, -1), (0, 0));
        assert_eqi!(ip(1, 0).shift_line(&s, -1), (0, 0));
        assert_eqi!(cp(1, 1).shift_line(&s, -1), (0, 0));
        assert_eqi!(ip(1, 1).shift_line(&s, -1), (0, 1));

        let mut s = State::new("one two\nthree four\n\nccc");
        s.resize_window(3, 10);
        // +ive
        assert_eqi!(ip(0, 0).shift_line(&s, 1), (0, 4));
        assert_eqi!(ip(0, 4).shift_line(&s, 1), (1, 0));

        assert_eqi!(ip(0, 1).shift_line(&s, 1), (0, 5));
        assert_eqi!(ip(0, 5).shift_line(&s, 1), (1, 1));
        assert_eqi!(ip(1, 1).shift_line(&s, 1), (1, 4));
        // -ive
        assert_eqi!(ip(0, 4).shift_line(&s, -1), (0, 0));
    }

    #[test]
    fn line_from_position() {
        let mut s = State::new("one two\nthree four\n\nccc");
        s.resize_window(3, 10);

        assert_eq!(Line::from_position(&s, (0, 0)), Some((0, 0).into()));
        assert_eq!(Line::from_position(&s, (0, 3)), None);
        assert_eq!(Line::from_position(&s, (0, 4)), Some((0, 1).into()));
        assert_eq!(Line::from_position(&s, (1, 0)), Some((1, 0).into()));
        // Offset 0 on an empty line
        assert_eq!(Line::from_position(&s, (2, 0)), Some((2, 0).into()));
    }

    #[test]
    fn line_from_lineno() {
        let mut s = State::new("one two\nthree four\nx");
        assert_eq!(s.resize_window(3, 10), 7);
        assert_eq!(Line::from_lineno(&s, 0), (0, 0).into());
        assert_eq!(Line::from_lineno(&s, 1), (0, 1).into());
        assert_eq!(Line::from_lineno(&s, 2), (1, 0).into());
        assert_eq!(Line::from_lineno(&s, 100), (2, 0).into());
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
