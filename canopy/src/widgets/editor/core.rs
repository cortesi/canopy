use std::f32::consts::E;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Position {
    line: usize,
    column: usize,
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
    text: Vec<Line>,
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
        let cursor = Position { line: 0, column: 0 };
        let mut t: Vec<Line> = text.split("\n").map(|x| Line::new(x)).collect();
        if t.is_empty() {
            t.push(Line::new(""))
        }
        Core {
            text: t,
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
                    // If our contains a newline, it's an expansion of the
                    // current line into multiple lines.
                    let mut m = self.text.remove(v.pos.line).raw;
                    m.insert_str(v.pos.column as usize, &v.text);
                    let new: Vec<Line> = m.split("\n").map(|x| Line::new(x.clone())).collect();
                    self.cursor = Position {
                        line: self.cursor.line + new.len() - 1,
                        column: v.text.len() - v.text.rfind("\n").unwrap() - 1,
                    };
                    self.text.splice(v.pos.line..v.pos.line, new);
                } else {
                    // If there are no newlines, we just insert the text in-place.
                    self.text[v.pos.line]
                        .raw
                        .insert_str(v.pos.column as usize, &v.text);
                    self.cursor = Position {
                        line: self.cursor.line,
                        column: self.cursor.column + 1,
                    };
                }
            }
            Operation::Delete(v) => {
                self.cursor = Position {
                    line: self.cursor.line,
                    column: self.cursor.column - 1,
                };
            }
        }
        self.history.push(op);
    }

    /// The complete raw text of this editor.
    pub fn raw_text(&self) -> String {
        self.text
            .iter()
            .map(|x| x.raw.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Retrieve the text from inclusive start to exclusive end.
    pub fn text_range(&self, start: Position, end: Position) -> String {
        let mut buf: String = String::new();
        if start.line > end.line {
            panic!("start.line > end.line");
        }
        if start.line == end.line {
            buf.push_str(&self.text[start.line].raw[start.column..end.column]);
        } else {
            buf.push_str(&self.text[start.line].raw[start.column..]);
            buf.push_str("\n");
            if end.line - start.line > 1 {
                for l in &self.text[(start.line + 1)..(end.line - 1)] {
                    buf.push_str(&l.raw);
                    buf.push_str("\n");
                }
            }
            buf.push_str(&self.text[end.line].raw[..end.column]);
        }
        buf
    }

    pub fn insert(&mut self, s: &str) {
        self.execute_op(Operation::Insert(Insert {
            pos: self.cursor,
            text: s.into(),
        }));
    }

    pub fn delete(&mut self, start: Position, end: Position) {
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
                cursor = Some(Position {
                    line: cnt,
                    column: x,
                });
                txt.push(i.replace("_", ""))
            } else {
                txt.push(i.into());
            }
        }
        let mut n = Core::new(&txt.join("\n"));
        n.set_cursor(cursor.unwrap());
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
    fn textrange() {
        let c = Core::new("one two\nthree four\nx");
        assert_eq!(
            c.text_range(
                Position { line: 0, column: 0 },
                Position { line: 0, column: 3 }
            ),
            "one"
        );
        assert_eq!(
            c.text_range(
                Position { line: 0, column: 4 },
                Position { line: 0, column: 7 }
            ),
            "two"
        );
        assert_eq!(
            c.text_range(
                Position { line: 0, column: 1 },
                Position { line: 0, column: 2 }
            ),
            "n"
        );
        assert_eq!(
            c.text_range(
                Position { line: 0, column: 0 },
                Position { line: 1, column: 0 }
            ),
            "one two\n"
        );
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
}
