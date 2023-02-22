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
        } else if self.column < s.lines[self.line].raw.len() - 1 {
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
                column: s.lines[ep.line].raw.len(),
            }
        } else if s.lines[self.line].raw.len() < self.column + 1 {
            Position {
                line: self.line,
                column: s.lines[self.line].raw.len(),
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
                column: s.lines[ep.line].raw.len() - 1,
            }
        } else if s.lines[self.line].raw.len() < self.column {
            Position {
                line: self.line,
                column: s.lines[self.line].raw.len() - 1,
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

/// A line is a single line of text, including any terminating line break characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Line {
    /// The raw text of the line.
    pub raw: String,
}

impl Line {
    pub fn new(s: &str) -> Line {
        Line { raw: s.into() }
    }
}

/// The current state of the editor
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct State {
    /// The underlying raw text being edited.
    pub lines: Vec<Line>,
    /// The current cursor position.
    pub cursor: Position,
}

impl State {
    pub fn new(text: &str) -> Self {
        let cursor = (0, 0).into();
        let mut t: Vec<Line> = text.split("\n").map(|x| Line::new(x)).collect();
        if t.is_empty() {
            t.push(Line::new(""))
        }
        State { lines: t, cursor }
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

    /// The complete raw text of this editor.
    pub fn raw_text(&self) -> String {
        self.lines
            .iter()
            .map(|x| x.raw.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Insert a set of lines at the cursor, then update the cursor. The first line is spliced into the line the cursor
    /// is on, all other lines are added as whole new lines.
    pub fn insert_lines<T, S, I>(&mut self, pos: T, s: S)
    where
        S: AsRef<[I]>,
        I: ToString,
        T: Into<Position>,
    {
        let pos = pos.into();
        let s = s.as_ref();
        if s.len() > 1 {
            // If our text contains a newline, it's an expansion of the
            // current line into multiple lines.
            self.lines[pos.line]
                .raw
                .insert_str(pos.column as usize, &s[0].to_string());
            self.lines.splice(
                pos.line + 1..pos.line + 1,
                s[1..].iter().map(|x| Line::new(&x.to_string())),
            );
            self.cursor = Position {
                line: self.cursor.line + s.len() - 1,
                column: s.last().unwrap().to_string().len(),
            };
        } else {
            // If there are no newlines, we just insert the text in-place.
            let s = &s[0].to_string();
            self.lines[pos.line].raw.insert_str(pos.column as usize, s);
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
        if start.line > self.lines.len() || end == start {
            return;
        } else if start.line == end.line {
            self.lines[start.line]
                .raw
                .replace_range(start.column..end.column, "");
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
            let mut m = self.lines.remove(start.line).raw;
            m.replace_range(start.column.., "");

            if self.lines.len() > end.line - 1 {
                let mut n = self.lines.remove(end.line - 1).raw;
                n.replace_range(..end.column.min(n.len()), "");
                self.lines.drain(start.line..end.line - 1);
                m.push_str(&n);
            }

            self.lines.insert(start.line, Line::new(&m));

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
            self.lines.len() - 1,
            self.lines[self.lines.len() - 1].raw.len() - 1,
        )
            .into()
    }

    /// Retrieve the text from inclusive start to exclusive end.
    pub fn text_range<T>(&self, start: T, end: T) -> String
    where
        T: Into<Position>,
    {
        let start = start.into().cap_exclusive(self);
        let end = end.into().cap_exclusive(self);

        let mut buf: String = String::new();
        if start.line == end.line {
            buf.push_str(&self.lines[start.line].raw[start.column..end.column]);
        } else {
            buf.push_str(&self.lines[start.line].raw[start.column..]);
            buf.push_str("\n");
            if end.line - start.line > 1 {
                for l in &self.lines[(start.line + 1)..(end.line - 1)] {
                    buf.push_str(&l.raw);
                    buf.push_str("\n");
                }
            }
            buf.push_str(&self.lines[end.line].raw[..end.column]);
        }
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wee helper for state equality tests
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
}
