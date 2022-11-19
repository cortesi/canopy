use super::core;

/// An operation on the text buffer. Each operation includes the information
/// needed to undo and redo the operation, which might be computed on
/// application from the underlying text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Operation {
    Insert(Insert),
    Delete(Delete),
}

impl Operator for Operation {
    fn apply(&self, s: &mut core::State) -> Option<Operation> {
        match self {
            Operation::Insert(o) => o.apply(s),
            Operation::Delete(o) => o.apply(s),
        }
    }
}

trait Operator {
    /// Modifies the provided state in-place, and returns an optional undo
    /// operation.
    fn apply(&self, s: &mut core::State) -> Option<Operation>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Insert {
    pos: core::Position,
    text: String,
}

impl Operator for Insert {
    fn apply(&self, s: &mut core::State) -> Option<Operation> {
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

impl Operator for Delete {
    fn apply(&self, s: &mut core::State) -> Option<Operation> {
        if self.start.line > s.lines.len() || self.end == self.start {
            return None;
        } else if self.start.line == self.end.line {
            s.lines[self.start.line]
                .raw
                .replace_range(self.start.column..self.end.column, "");
        }
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
    fn test(text: &str, op: impl Operator, expected: &str) {
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
}
