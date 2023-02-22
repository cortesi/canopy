use super::{
    effect,
    state::{Line, Position},
};

/// Core implementation for a simple editor.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Core {
    /// The underlying raw text being edited.
    pub(super) lines: Vec<Line>,
    /// The history of operations on this text buffer.
    history: Vec<effect::Effect>,
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

    /// The complete raw text of this editor.
    pub fn raw_text(&self) -> String {
        self.lines
            .iter()
            .map(|x| x.raw.clone())
            .collect::<Vec<_>>()
            .join("\n")
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
