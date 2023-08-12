use super::effect::Effector;
use super::{
    effect,
    primitives::{Position, Window},
    state,
};
use crate::geom::Point;

/// Core implementation for a simple editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Core {
    pub(super) state: state::State,
    /// The history of operations on this text buffer.
    history: Vec<effect::Effect>,
    redo: Vec<effect::Effect>,
}

impl Core {
    pub fn new(text: &str) -> Self {
        Core {
            state: state::State::new(text),
            history: Vec::new(),
            redo: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_spec(spec: &str) -> Self {
        Core {
            state: state::State::from_spec(spec),
            history: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Undo an operation. Return true if an operation was performed, false if the history is empty.
    pub fn undo(&mut self) -> bool {
        if let Some(op) = self.history.pop() {
            op.revert(&mut self.state);
            self.redo.push(op);
            true
        } else {
            false
        }
    }

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
            self.state.cursor,
            text.to_string(),
        )));
    }

    /// Insert text at the current cursor position.
    pub fn delete<T>(&mut self, start: T, end: T)
    where
        T: Into<Position>,
    {
        self.action(effect::Effect::Delete(effect::Delete::new(
            &self.state,
            start.into(),
            end.into(),
        )));
    }

    pub fn set_width(&mut self, width: usize) {
        self.state.set_width(width);
    }

    pub fn cursor_position(&self, win: Window) -> Option<Point> {
        self.state.coords_in_window(win, self.state.cursor)
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
        F: FnOnce(&mut Core) -> (),
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
        tundo("", |c| c.insert_text("hello"), "hello_");
        tundo(
            "",
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
    fn delete() {
        tundo("hello_", |c| c.delete((0, 0), (0, 1)), "ello_");
        tundo("hello\nworld_", |c| c.delete((0, 0), (1, 1)), "orld_");
    }
}
