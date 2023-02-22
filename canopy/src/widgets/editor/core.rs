use super::effect::Effector;
use super::{effect, state};

/// Core implementation for a simple editor.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
        assert_eq!(s.state.raw_text(), end.state.raw_text());
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
            },
            "abc_",
        );
    }
}
