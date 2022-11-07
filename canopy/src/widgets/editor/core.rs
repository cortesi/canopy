/// A position in the editor. When used as the end of a range, the column
/// offset, but not the line offset, may be beyond the bounds of the line.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Position {
    line: usize,
    column: usize,
}

impl Position {
    fn new(line: usize, column: usize) -> Self {
        Position { line, column }
    }

    /// Return the next logical position after this one. Returns the final
    /// position if this one is out of bounds.
    fn next(&self, c: &Core) -> Position {
        let last = c.last();
        if self.line > last.line {
            last
        } else if self.column < c.lines[self.line].raw.len() - 1 {
            Position::new(self.line, self.column + 1)
        } else {
            Position::new(self.line + 1, 0).cap_inclusive(c)
        }
    }

    /// Constrain a `Position` to be within the exclusive range of `Core`.
    fn cap_exclusive(&self, c: &Core) -> Position {
        let ep = c.last();
        if self.line > ep.line {
            Position {
                line: ep.line,
                column: c.lines[ep.line].raw.len(),
            }
        } else {
            if c.lines[self.line].raw.len() < self.column + 1 {
                Position {
                    line: self.line,
                    column: c.lines[self.line].raw.len(),
                }
            } else {
                Position {
                    line: self.line,
                    column: self.column,
                }
            }
        }
    }

    /// Constrain a `Position` to be within the inclusive range of `Core`.
    fn cap_inclusive(&self, c: &Core) -> Position {
        let ep = c.last();
        let start_line = if self.line > ep.line {
            c.last().line
        } else {
            self.line
        };
        if c.lines[start_line].raw.len() < self.column {
            Position {
                line: start_line,
                column: c.lines[start_line].raw.len() - 1,
            }
        } else {
            Position {
                line: start_line,
                column: self.column,
            }
        }
    }
}

impl From<(usize, usize)> for Position {
    fn from((line, column): (usize, usize)) -> Self {
        Self { line, column }
    }
}

/// A line is a single line of text, including any terminating line break characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Line {
    /// The raw text of the line.
    raw: String,
    /// The wrapped text of the line.
    wrapped: String,
    /// The number of lines in the wrapped text.
    height: usize,
}

impl Line {
    fn new(s: &str) -> Line {
        Line {
            raw: s.into(),
            wrapped: s.into(),
            height: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Insert {
    pos: Position,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Delete {
    start: Position,
    end: Position,
    text: String,
}

/// An operation on the text buffer. Each operation includes the information
/// needed to undo and redo the operation, which might be computed on
/// application from the underlying text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Operation {
    Insert(Insert),
    Delete(Delete),
}

/// Core implementation for a simple editor.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Core {
    /// The underlying raw text being edited.
    lines: Vec<Line>,
    /// The history of operations on this text buffer.
    history: Vec<Operation>,
    /// The current cursor position.
    cursor: Position,
    /// Width of the viewport.
    width: u64,
}

impl Core {
    pub fn new(text: &str) -> Self {
        let history = Vec::new();
        let cursor = (0, 0).into();
        let mut t: Vec<Line> = text.split("\n").map(|x| Line::new(x)).collect();
        if t.is_empty() {
            t.push(Line::new(""))
        }
        Core {
            lines: t,
            history,
            cursor,
            width: 0,
        }
    }

    fn set_cursor(&mut self, pos: Position) {
        self.cursor = pos;
    }

    fn execute_op(&mut self, op: Operation) {
        match &op {
            Operation::Insert(v) => {
                if v.text.contains("\n") {
                    // If our text contains a newline, it's an expansion of the
                    // current line into multiple lines.
                    let mut m = self.lines.remove(v.pos.line).raw;
                    m.insert_str(v.pos.column as usize, &v.text);
                    let new: Vec<Line> = m.split("\n").map(|x| Line::new(x.clone())).collect();
                    self.cursor = Position {
                        line: self.cursor.line + new.len() - 1,
                        column: v.text.len() - v.text.rfind("\n").unwrap() - 1,
                    };
                    self.lines.splice(v.pos.line..v.pos.line, new);
                } else {
                    // If there are no newlines, we just insert the text in-place.
                    self.lines[v.pos.line]
                        .raw
                        .insert_str(v.pos.column as usize, &v.text);
                    self.cursor = (self.cursor.line, self.cursor.column + 1).into();
                }
            }
            Operation::Delete(v) => {
                if v.start.line > self.lines.len() || v.end == v.start {
                    return;
                } else if v.start.line == v.end.line {
                    self.lines[v.start.line]
                        .raw
                        .replace_range(v.start.column..v.end.column, "");
                }
                // self.cursor = Position {
                //     line: self.cursor.line,
                //     column: self.cursor.column - 1,
                // };
            }
        }
        self.history.push(op);
    }

    /// The complete raw text of this editor.
    pub fn raw_text(&self) -> String {
        self.lines
            .iter()
            .map(|x| x.raw.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// What's the position of the final character in the text?
    fn last(&self) -> Position {
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
        let start = start.into().cap_inclusive(self);
        let end = end.into().cap_exclusive(self);
        println!("{:?}", end);

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

    pub fn insert(&mut self, s: &str) {
        self.execute_op(Operation::Insert(Insert {
            pos: self.cursor,
            text: s.into(),
        }));
    }

    /// Delete a text range. The range may extend beyond the end of the text
    /// or line with no error
    pub fn delete<T>(&mut self, start: T, end: T)
    where
        T: Into<Position>,
    {
        let start = start.into();
        let end = end.into();
        self.execute_op(Operation::Delete(Delete {
            start,
            end,
            text: self.text_range(start, end),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_spec(spec: &str) -> Core {
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
        let mut n = Core::new(&txt.join("\n"));
        if let Some(x) = cursor {
            n.set_cursor(x);
        }
        n
    }

    /// Compact test function - the underscore indicates cursor position.
    fn test<F>(text: &str, ops: F, expected: &str)
    where
        F: Fn(&mut Core),
    {
        let mut c = from_spec(text);
        let e = from_spec(expected);
        ops(&mut c);
        assert_eq!(c.raw_text(), e.raw_text());
        assert_eq!(c.cursor, e.cursor);
    }

    #[test]
    fn cap_position() {
        let c = Core::new("a\nbb");
        assert_eq!(Position::new(0, 0).cap_inclusive(&c), (0, 0).into());
        assert_eq!(Position::new(0, 2).cap_inclusive(&c), (0, 0).into());
        assert_eq!(Position::new(3, 0).cap_inclusive(&c), (1, 0).into());
        assert_eq!(Position::new(3, 3).cap_inclusive(&c), (1, 1).into());

        assert_eq!(Position::new(0, 0).cap_exclusive(&c), (0, 0).into());
        assert_eq!(Position::new(3, 3).cap_exclusive(&c), (1, 2).into());
    }

    #[test]
    fn text_range() {
        let c = Core::new("one two\nthree four\nx");
        // assert_eq!(c.text_range((0, 0), (0, 3)), "one");
        // assert_eq!(c.text_range((0, 4), (0, 7)), "two");
        // assert_eq!(c.text_range((0, 1), (0, 2)), "n");
        // assert_eq!(c.text_range((0, 0), (1, 0)), "one two\n");
        // // Beyond bounds
        // assert_eq!(c.text_range((10, 0), (11, 0)), "");
        assert_eq!(c.text_range((1, 6), (11, 0)), "four\nx");
    }

    #[test]
    fn insert() {
        test(
            "_",
            |c| {
                c.insert("a");
                c.insert("b");
                c.insert("c");
            },
            "abc_",
        );
        test(
            "a_",
            |c| {
                c.insert("\n");
            },
            "a\n_",
        );
        test(
            "_",
            |c| {
                c.insert("\n");
            },
            "\n_",
        );
        test(
            "a_",
            |c| {
                c.insert("\nb\n");
            },
            "a\nb\n_",
        );
        test(
            "a_",
            |c| {
                c.insert("\nb");
            },
            "a\nb_",
        );
    }

    #[test]
    fn delete() {
        // Nop, empty range
        test(
            "a",
            |c| {
                c.delete((0, 0), (0, 0));
            },
            "a",
        );
        // Nop, beyond bounds
        test(
            "a",
            |c| {
                c.delete((1, 0), (1, 0));
            },
            "a",
        );
        test(
            "a",
            |c| {
                c.delete((0, 0), (0, 1));
            },
            "",
        );
    }
}
