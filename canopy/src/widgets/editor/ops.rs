use super::core;

/// Operations are abstract commands to the editor. They are turned into
/// concrete, undoable Effects by way of the editor state.
enum Operation {
    Insert(String),
    DeleteChars(usize),
}

/// An effect represents an undo-able change to the editor state. Each effect
/// includes the information needed to undo and redo the operation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Effect {
    Insert(Insert),
    Delete(Delete),
}

impl Effector for Effect {
    fn apply(&self, s: &mut core::State) -> Option<Effect> {
        match self {
            Effect::Insert(o) => o.apply(s),
            Effect::Delete(o) => o.apply(s),
        }
    }
}

trait Effector {
    /// Modifies the provided state in-place, and returns an optional undo
    /// operation.
    fn apply(&self, s: &mut core::State) -> Option<Effect>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Insert {
    pos: core::Position,
    text: String,
}

impl Effector for Insert {
    fn apply(&self, s: &mut core::State) -> Option<Effect> {
        if self.text.contains("\n") {
            // If our text contains a newline, it's an expansion of the
            // current line into multiple lines.
            let mut m = s.lines.remove(self.pos.line).raw;
            m.insert_str(self.pos.column as usize, &self.text);
            let new: Vec<core::Line> = m.split("\n").map(|x| core::Line::new(x.clone())).collect();
            s.cursor = core::Position {
                line: s.cursor.line + new.len() - 1,
                column: self.text.len() - self.text.rfind("\n").unwrap() - 1,
            };
            s.lines.splice(self.pos.line..self.pos.line, new);
        } else {
            // If there are no newlines, we just insert the text in-place.
            s.lines[self.pos.line]
                .raw
                .insert_str(self.pos.column as usize, &self.text);
            s.cursor = (s.cursor.line, s.cursor.column + 1).into();
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Delete {
    start: core::Position,
    end: core::Position,
}

impl Effector for Delete {
    fn apply(&self, s: &mut core::State) -> Option<Effect> {
        if self.start.line > s.lines.len() || self.end == self.start {
            return None;
        } else if self.start.line == self.end.line {
            s.lines[self.start.line]
                .raw
                .replace_range(self.start.column..self.end.column, "");
        } else {
            let mut m = s.lines.remove(self.start.line).raw;
            m.replace_range(self.start.column.., "");

            let mut n = s.lines.remove(self.end.line - 1).raw;
            n.replace_range(..self.end.column, "");

            s.lines.drain(self.start.line..self.end.line - 1);

            m.push_str(&n);
            s.lines.insert(self.start.line, core::Line::new(&m));
        }
        s.cursor = core::Position {
            line: s.cursor.line,
            column: s.cursor.column.saturating_sub(1),
        };
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_spec(spec: &str) -> core::State {
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
        let mut n = core::State::new(&txt.join("\n"));
        if let Some(x) = cursor {
            n.cursor = x;
        }
        n
    }

    /// Compact test function - an underscore in the input text indicates cursor
    /// position, and is removed from the text when constructing state.
    fn test(text: &str, op: impl Effector, expected: &str) {
        let mut c = from_spec(text);
        let e = from_spec(expected);
        op.apply(&mut c);
        assert_eq!(c.raw_text(), e.raw_text());
        assert_eq!(c.cursor, e.cursor);
    }

    #[test]
    fn insert() {
        test(
            "_",
            Insert {
                pos: (0, 0).into(),
                text: "a".into(),
            },
            "a_",
        );
        test(
            "_",
            Insert {
                pos: (0, 0).into(),
                text: "a\nb".into(),
            },
            "a\nb_",
        );
    }

    #[test]
    fn delete() {
        // Nop, empty range
        test(
            "a",
            Delete {
                start: (0, 0).into(),
                end: (0, 0).into(),
            },
            "a",
        );
        test(
            "a",
            Delete {
                start: (10, 0).into(),
                end: (10, 0).into(),
            },
            "a",
        );
        // // Nop, beyond bounds
        test(
            "a",
            Delete {
                start: (1, 0).into(),
                end: (1, 0).into(),
            },
            "a",
        );
        test(
            "a",
            Delete {
                start: (0, 0).into(),
                end: (0, 1).into(),
            },
            "",
        );
        // Ranges
        test(
            "abc",
            Delete {
                start: (0, 0).into(),
                end: (0, 1).into(),
            },
            "bc",
        );
        test(
            "abc",
            Delete {
                start: (0, 1).into(),
                end: (0, 2).into(),
            },
            "ac",
        );
        test(
            "abc",
            Delete {
                start: (0, 2).into(),
                end: (0, 3).into(),
            },
            "ab",
        );
        test(
            "abc\ndef",
            Delete {
                start: (0, 0).into(),
                end: (1, 0).into(),
            },
            "def",
        );
        test(
            "abc\ndef\nghi",
            Delete {
                start: (0, 0).into(),
                end: (2, 0).into(),
            },
            "ghi",
        );
        test(
            "abc\ndef\nghi",
            Delete {
                start: (0, 1).into(),
                end: (2, 2).into(),
            },
            "ai",
        );
        test(
            "abc\ndef\nghi",
            Delete {
                start: (0, 2).into(),
                end: (2, 2).into(),
            },
            "abi",
        );
        test(
            "abc\ndef\nghi",
            Delete {
                start: (0, 3).into(),
                end: (2, 2).into(),
            },
            "abci",
        );
        test(
            "abc\ndef\nghi",
            Delete {
                start: (1, 0).into(),
                end: (2, 2).into(),
            },
            "abc\ni",
        );
    }
}
