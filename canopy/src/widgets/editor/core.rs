use std::f32::consts::E;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct Position {
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
    txt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Delete {
    start: Position,
    end: Position,
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
                if v.txt.contains("\n") {
                    // If our contains a newline, it's an expansion of the
                    // current line into multiple lines.
                    let mut m = self.text.remove(v.pos.line).raw;
                    m.insert_str(v.pos.column as usize, &v.txt);
                    let new: Vec<Line> = m.split("\n").map(|x| Line::new(x.clone())).collect();
                    self.cursor = Position {
                        line: self.cursor.line + new.len() - 1,
                        column: v.txt.len() - v.txt.rfind("\n").unwrap() - 1,
                    };
                    self.text.splice(v.pos.line..v.pos.line, new);
                } else {
                    // If there are no newlines, we just insert the text
                    self.text[v.pos.line]
                        .raw
                        .insert_str(v.pos.column as usize, &v.txt);
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

    pub fn raw_text(&self) -> String {
        self.text
            .iter()
            .map(|x| x.raw.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn insert(&mut self, s: &str) {
        self.execute_op(Operation::Insert(Insert {
            pos: self.cursor,
            txt: s.into(),
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
    fn basic_ops() {
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
