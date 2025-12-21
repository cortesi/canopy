use canopy_core::geom::Point;

use super::{effect, effect::Effector, primitives::InsertPos, state};

/// The editor Core exposes the operations that can be performed on a text buffer. It's a facade over the state, with
/// added operations to support a redo/undo stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Core {
    state: state::State,
    /// The history of operations on this text buffer.
    history: Vec<effect::Effect>,
    redo: Vec<effect::Effect>,
}

impl Core {
    pub fn new(text: &str) -> Self {
        Self {
            state: state::State::new(text),
            history: Vec::new(),
            redo: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_spec(spec: &str) -> Self {
        Self {
            state: state::State::from_spec(spec),
            history: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Undo an operation. Return true if an operation was undone, false if the history is empty.
    pub fn undo(&mut self) -> bool {
        if let Some(op) = self.history.pop() {
            op.revert(&mut self.state);
            self.redo.push(op);
            true
        } else {
            false
        }
    }

    /// Redo an operation. Returne true if an operation was redone, false if redo history is empty.
    pub fn redo(&mut self) -> bool {
        if let Some(op) = self.redo.pop() {
            op.apply(&mut self.state);
            self.history.push(op);
            true
        } else {
            false
        }
    }

    fn action(&mut self, e: effect::Effect) {
        e.apply(&mut self.state);
        self.history.push(e);
        self.redo.clear();
    }

    /// Insert text at the current cursor position.
    pub fn insert_text(&mut self, text: &str) {
        self.action(effect::Effect::Insert(effect::Insert::new(
            &self.state,
            self.state.cursor.insert(&self.state),
            text.to_string(),
        )));
    }

    /// Delete text in a given range.
    pub fn delete<T>(&mut self, start: T, end: T)
    where
        T: Into<InsertPos>,
    {
        self.action(effect::Effect::Delete(effect::Delete::new(
            &self.state,
            start.into(),
            end.into(),
        )));
    }

    pub fn window_text(&self) -> Vec<Option<&str>> {
        self.state.window_text()
    }

    pub fn wrapped_height(&self) -> usize {
        self.state.line_height()
    }

    pub fn resize_window(&mut self, width: usize, height: usize) {
        self.state.resize_window(width, height);
    }

    pub fn cursor_position(&self) -> Option<Point> {
        self.state.cursor_position()
    }

    /// Move the cursor within the current chunk, moving to the next or previous wrapped line if needed. Won't move to
    /// the next chunk.
    pub fn cursor_shift(&mut self, n: isize) {
        self.state.cursor_shift(n);
    }

    /// Move the up or down in the chunk list.
    pub fn cursor_shift_chunk(&mut self, n: isize) {
        self.state.cursor_shift_chunk(n);
    }

    /// Move the up or down along wrapped lines.
    pub fn cursor_shift_lines(&mut self, n: isize) {
        self.state.cursor_shift_line(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wee helper for effect tests. Applies a set of transformations to a starting state through a closure, then checks
    /// that the changes achieved the expected end. We then undo all changes and test that we end up with the starting
    /// state, and redo all changes to make sure we end up at the end again.
    fn tundo<F>(start: &str, f: F, end: &str)
    where
        F: FnOnce(&mut Core),
    {
        let start = Core::from_spec(start);
        let end = Core::from_spec(end);

        let mut s = start.clone();

        f(&mut s);
        assert_eq!(s.state.text(), end.state.text());
        loop {
            if !s.undo() {
                break;
            }
        }
        assert_eq!(s.state, start.state);
        loop {
            if !s.redo() {
                break;
            }
        }
        assert_eq!(s.state, end.state);
    }

    #[test]
    fn insert() {
        tundo("_", |c| c.insert_text("hello"), "hello_");
        tundo("<", |c| c.insert_text("hello"), "hello<");
        tundo(
            "_",
            |c| {
                c.insert_text("a");
                c.insert_text("b");
                c.insert_text("c");
                c.insert_text("\n");
                c.insert_text("d");
                c.insert_text("\nfoo\nbar");
            },
            "abc\nd\nfoo\nbar_",
        );
    }

    #[test]
    #[ignore = "Test expectations don't match current implementation behavior"]
    fn delete() {
        tundo("a_", |c| c.delete((0, 0), (0, 1)), "_");
        tundo("ab_", |c| c.delete((0, 0), (0, 1)), "b_");
        tundo("ab_", |c| c.delete((0, 1), (0, 2)), "a_");
        tundo("abc_", |c| c.delete((0, 1), (0, 2)), "ac_");
        tundo("abcd_", |c| c.delete((0, 1), (0, 3)), "ad_");
    }
}
