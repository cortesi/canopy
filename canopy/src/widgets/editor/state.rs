use super::chunk::Chunk;

const DEFAULT_WRAP: usize = 80;

/// A position in the editor. The column offset, but not the line offset, may be
/// beyond the bounds of the line.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    fn new(line: usize, column: usize) -> Self {
        Position { line, column }
    }

    /// Return the next logical position after this one. Returns the final
    /// position if this one is out of bounds.
    fn next(&self, s: &State) -> Position {
        let last = s.last();
        if self.line > last.line {
            last
        } else if self.column < s.chunks[self.line].len() - 1 {
            Position::new(self.line, self.column + 1)
        } else {
            Position::new(self.line + 1, 0).cap_inclusive(s)
        }
    }

    /// Constrain a `Position` to be within the exclusive range of `Core`.
    pub(super) fn cap_exclusive(&self, s: &State) -> Position {
        let ep = s.last();
        if self.line > ep.line {
            Position {
                line: ep.line,
                column: s.chunks[ep.line].len(),
            }
        } else if s.chunks[self.line].len() < self.column + 1 {
            Position {
                line: self.line,
                column: s.chunks[self.line].len(),
            }
        } else {
            Position {
                line: self.line,
                column: self.column,
            }
        }
    }

    /// Constrain a `Position` to be within the inclusive range of `Core`.
    fn cap_inclusive(&self, s: &State) -> Position {
        let ep = s.last();
        if self.line > ep.line {
            Position {
                line: ep.line,
                column: s.chunks[ep.line].len() - 1,
            }
        } else if s.chunks[self.line].len() < self.column {
            Position {
                line: self.line,
                column: s.chunks[self.line].len() - 1,
            }
        } else {
            Position {
                line: self.line,
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

/// The current state of the editor
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct State {
    /// The underlying raw text being edited.
    pub chunks: Vec<Chunk>,
    /// The current cursor position.
    pub cursor: Position,
    pub width: usize,
}

impl State {
    pub fn new(text: &str) -> Self {
        let cursor = (0, 0).into();
        let mut t: Vec<Chunk> = text
            .split("\n")
            .map(|x| Chunk::new(x, DEFAULT_WRAP))
            .collect();
        if t.is_empty() {
            t.push(Chunk::new("", DEFAULT_WRAP))
        }
        State {
            chunks: t,
            cursor,
            width: DEFAULT_WRAP,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_spec(spec: &str) -> Self {
        let mut txt = vec![];
        let mut cursor = None;
        for (cnt, i) in spec.lines().enumerate() {
            if let Some(x) = i.find("_") {
                cursor = Some((cnt, x).into());
                txt.push(i.replace("_", ""))
            } else {
                txt.push(i.into());
            }
        }
        let mut n = State::new(&txt.join("\n"));
        if let Some(x) = cursor {
            n.cursor = x;
        }
        n
    }

    /// Insert a set of lines at the cursor, then update the cursor to point just beyond the last inserted character.
    pub fn insert_lines<T, S, I>(&mut self, pos: T, s: S)
    where
        S: AsRef<[I]>,
        I: ToString,
        T: Into<Position>,
    {
        let pos = pos.into();
        let s = s.as_ref();
        if s.len() > 1 {
            // Start by snipping the line at the insert point into start and end chunks.
            let start = &self.chunks[pos.line].as_str()[..pos.column];
            let end = &self.chunks[pos.line].as_str()[pos.column..].to_string();

            self.chunks[pos.line] =
                Chunk::new(&format!("{}{}", start, s[0].to_string()), self.width);

            let mut trailer = s[1..].iter().map(|x| x.to_string()).collect::<Vec<_>>();
            let last = trailer.pop().unwrap();
            trailer.push(format!("{}{}", last, end));

            self.chunks.splice(
                pos.line + 1..pos.line + 1,
                trailer.iter().map(|x| Chunk::new(x, self.width)),
            );
            self.cursor = Position {
                line: pos.line + s.len() - 1,
                column: last.len(),
            };
        } else {
            // If there are no line, we just insert the text in-place.
            let s = &s[0].to_string();
            self.chunks[pos.line].insert(pos.column as usize, s);
            self.cursor = (self.cursor.line, self.cursor.column + s.len()).into();
        }
    }

    /// Insert the given text at the given position, and update the cursor.
    pub fn insert<T>(&mut self, pos: T, s: &str)
    where
        T: Into<Position>,
    {
        self.insert_lines(pos, s.split("\n").collect::<Vec<&str>>())
    }

    /// Insert the given text at the given position, and update the cursor if necessary.
    pub fn delete<T>(&mut self, start: T, end: T)
    where
        T: Into<Position>,
    {
        let start = start.into();
        let end = end.into();
        if start.line > self.chunks.len() || end == start {
            return;
        } else if start.line == end.line {
            self.chunks[start.line].replace_range(start.column..end.column, "");
            if self.cursor > start {
                if self.cursor <= end {
                    self.cursor = start;
                } else if self.cursor.line == start.line {
                    self.cursor = Position {
                        line: self.cursor.line,
                        column: self.cursor.column.saturating_sub(end.column - start.column),
                    };
                }
            }
        } else {
            let mut m = self.chunks.remove(start.line);
            m.replace_range(start.column.., "");

            if self.chunks.len() > end.line - 1 {
                let mut n = self.chunks.remove(end.line - 1);
                n.replace_range(..end.column.min(n.len()), "");
                self.chunks.drain(start.line..end.line - 1);
                m.push_str(n.as_str());
            }

            self.chunks.insert(start.line, m);

            if self.cursor > start {
                if self.cursor <= end {
                    self.cursor = start;
                } else if self.cursor.line == start.line {
                    self.cursor = Position {
                        line: self.cursor.line.saturating_sub(end.line - start.line),
                        column: self.cursor.column.saturating_sub(end.column),
                    };
                } else {
                    self.cursor = Position {
                        line: self.cursor.line.saturating_sub(end.line - start.line),
                        column: self.cursor.column.saturating_sub(end.column),
                    };
                    // We've ended moving the cursor onto our partially snipped starting line, so adjust the offset.
                    if self.cursor.line == start.line {
                        self.cursor = Position {
                            line: self.cursor.line,
                            column: self.cursor.column + start.column,
                        };
                    }
                }
            }
        }
    }

    /// What's the position of the final character in the text?
    pub(super) fn last(&self) -> Position {
        (
            self.chunks.len() - 1,
            self.chunks[self.chunks.len() - 1].len() - 1,
        )
            .into()
    }

    /// Retrieve lines of text from inclusive start to exclusive end. The first and last line returned may be partial if
    /// the offsets are not on line boundaries.
    pub fn line_range<T>(&self, start: T, end: T) -> Vec<String>
    where
        T: Into<Position>,
    {
        let start = start.into().cap_exclusive(self);
        let end = end.into().cap_exclusive(self);

        let mut buf = vec![];
        if start.line == end.line {
            buf.push(self.chunks[start.line].as_str()[start.column..end.column].to_string());
        } else {
            buf.push(self.chunks[start.line].as_str()[start.column..].to_string());
            if end.line - start.line > 1 {
                for l in &self.chunks[(start.line + 1)..(end.line - 1)] {
                    buf.push(l.as_str().into());
                }
            }
            buf.push(self.chunks[end.line].as_str()[..end.column].to_string());
        }
        buf
    }

    /// The complete text of this editor, with chunks separated by newlines.
    pub fn text(&self) -> String {
        self.chunks
            .iter()
            .map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Retrieve the text from inclusive start to exclusive end. The first and last line returned may be partial if the
    /// offsets are not on line boundaries.
    pub fn text_range<T>(&self, start: T, end: T) -> String
    where
        T: Into<Position>,
    {
        self.line_range(start, end).join("\n")
    }

    /// Find the position of a given wrapped line offset. The return value is a tuple (chunk offset, wrapped line
    /// offset), where the wrapped line offset is the offset within the returned chunk. If the specified offset is out
    /// of range, the last line is returned.
    pub fn wrapped_offset(&self, wrapped_line: usize) -> (usize, usize) {
        let mut offset = 0;
        for (i, c) in self.chunks.iter().enumerate() {
            if offset + c.wraps.len() > wrapped_line {
                return (i, wrapped_line - offset);
            }
            offset += c.wraps.len();
        }
        (
            self.chunks.len() - 1,
            self.chunks[self.chunks.len() - 1].wraps.len() - 1,
        )
    }

    /// Return the wrapped lines in a given window. The start offset is in terms of the wrapped text. The returned Vec
    /// may be shorter than length if the end of the text is reached.
    pub fn wrapped_window(&self, start: usize, length: usize) -> Vec<&str> {
        let mut buf = vec![];
        let (chunk_offset, mut wrapped_offset) = self.wrapped_offset(start);
        let end = self.wrapped_offset(start + length);
        for coff in chunk_offset..self.chunks.len() {
            for woff in wrapped_offset..self.chunks[coff].wraps.len() {
                if (coff, woff) == end {
                    return buf;
                }
                buf.push(self.chunks[coff].wrapped_line(woff));
                if buf.len() >= length {
                    return buf;
                }
            }
            wrapped_offset = 0;
        }
        buf
    }

    /// Set the width of the editor for wrapping, and return the total number of wrapped lines that resulted.
    pub fn set_width(&mut self, width: usize) -> usize {
        // FIXME: This needs to be a as close to a nop as possible if the width hasn't changed.
        self.width = width;
        self.chunks.iter_mut().map(|x| x.wrap(width)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Check if a specification given as a string containing newlines is equal to a Vec<&str>.
    fn str_eq(b: Vec<&str>, a: &str) {
        if a.is_empty() {
            assert!(b.is_empty());
            return;
        }
        let av = a.split('\n').collect::<Vec<_>>();
        assert_eq!(av, b)
    }

    /// Take a state specification a, turn it into a State object, apply the transformation f, then check if the result
    /// is equal to the state specification b.
    fn seq<F>(a: &str, f: F, b: &str)
    where
        F: FnOnce(&mut State) -> (),
    {
        let mut a = State::from_spec(a);
        let b = State::from_spec(b);
        f(&mut a);
        assert_eq!(a, b);
    }

    #[test]
    fn insert() {
        seq("_", |x| x.insert((0, 0), "a"), "a_");
        seq("_", |x| x.insert((0, 0), "a\nb"), "a\nb_");
        seq("abc\ndef_", |x| x.insert((0, 2), "x\ny"), "abx\ny_c\ndef");
    }

    #[test]
    fn delete() {
        // Nop, empty
        seq("a_", |x| x.delete((0, 0), (0, 0)), "a_");

        // Nop, beyond bounds
        seq("a_", |x| x.delete((10, 0), (10, 0)), "a_");
        seq("a_", |x| x.delete((1, 0), (1, 0)), "a_");

        // Single line deletes
        seq("a_", |x| x.delete((0, 0), (0, 1)), "_");
        seq("abc_", |x| x.delete((0, 0), (0, 1)), "bc_");
        seq("abc_", |x| x.delete((0, 1), (0, 2)), "ac_");
        seq("abc_", |x| x.delete((0, 2), (0, 3)), "ab_");
        seq("_abc", |x| x.delete((0, 2), (0, 3)), "_ab");
        seq("ab_c", |x| x.delete((0, 1), (0, 3)), "a_");
        seq("ab_c\nfoo", |x| x.delete((0, 1), (0, 3)), "a_\nfoo");
        seq(
            "foo\nab_c\nfoo",
            |x| x.delete((1, 1), (1, 3)),
            "foo\na_\nfoo",
        );
        seq(
            "foo\nab_c\nfoo",
            |x| x.delete((1, 0), (1, 3)),
            "foo\n_\nfoo",
        );

        // Multi line deletes
        seq(
            "one_\ntwo\nthree",
            |x| x.delete((1, 0), (2, 1)),
            "one_\nhree",
        );
        seq(
            "one\ntw_o\nthree",
            |x| x.delete((1, 0), (2, 1)),
            "one\n_hree",
        );
        seq(
            "one\ntwo\nthre_e",
            |x| x.delete((1, 0), (2, 1)),
            "one\nhre_e",
        );
        seq("one\ntwo\nthre_e", |x| x.delete((0, 1), (2, 4)), "o_e");
        seq("one\ntwo\nthre_e", |x| x.delete((0, 3), (2, 2)), "onere_e");
        seq("one\ntwo\nthre_e", |x| x.delete((0, 3), (2, 3)), "onee_e");
        seq("one\ntwo\nthre_e", |x| x.delete((0, 3), (2, 4)), "one_e");
        seq("one\ntwo\nthre_e", |x| x.delete((0, 3), (2, 5)), "one_");
    }

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

    #[test]
    fn text_range() {
        let s = State::new("one two\nthree four\nx");
        assert_eq!(s.text_range((0, 0), (0, 3)), "one");
        assert_eq!(s.text_range((0, 4), (0, 7)), "two");
        assert_eq!(s.text_range((0, 1), (0, 2)), "n");
        assert_eq!(s.text_range((0, 0), (1, 0)), "one two\n");
        // // Beyond bounds
        assert_eq!(s.text_range((10, 0), (11, 0)), "");
        assert_eq!(s.text_range((1, 6), (11, 0)), "four\nx");
    }

    #[test]
    fn text_width() {
        let mut s = State::new("one two\nthree four\nx");
        assert_eq!(s.set_width(3), 7);
    }

    #[test]
    fn wrapped_offset() {
        let mut s = State::new("one two\nthree four\nx");
        assert_eq!(s.set_width(3), 7);
        assert_eq!(s.wrapped_offset(0), (0, 0));
        assert_eq!(s.wrapped_offset(1), (0, 1));
        assert_eq!(s.wrapped_offset(2), (1, 0));
        assert_eq!(s.wrapped_offset(100), (2, 0));
    }

    #[test]
    fn wrapped_window() {
        let mut s = State::new("one two\nthree four\nx");
        assert_eq!(s.set_width(3), 7);
        str_eq(s.wrapped_window(0, 0), "");
        str_eq(s.wrapped_window(0, 1), "one");
        str_eq(s.wrapped_window(0, 2), "one\ntwo");
        str_eq(s.wrapped_window(0, 3), "one\ntwo\nthr");

        str_eq(s.wrapped_window(1, 1), "two");
        str_eq(s.wrapped_window(1, 2), "two\nthr");

        str_eq(s.wrapped_window(2, 1), "thr");
        str_eq(s.wrapped_window(2, 2), "thr\nee");
        str_eq(s.wrapped_window(2, 3), "thr\nee\nfou");
    }
}
