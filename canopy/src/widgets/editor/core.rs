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

    /// Insert text at the current cursor position.
    pub fn insert_text(&mut self, text: &str) {
        let ins = effect::Effect::Insert(effect::Insert::new(
            &self.state,
            self.state.cursor,
            text.to_string(),
        ));
        ins.apply(&mut self.state);
        self.history.push(ins);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wee helper for state equality tests
    fn tundo<F>(a: &str, f: F, b: &str)
    where
        F: FnOnce(&mut Core) -> (),
    {
        let mut a = Core::from_spec(a);
        let post = a.clone();
        let b = Core::from_spec(b);
        f(&mut a);
        assert_eq!(a.state.raw_text(), b.state.raw_text());
        loop {
            if !a.undo() {
                break;
            }
        }
        assert_eq!(a.state, post.state);
    }

    #[test]
    fn insert() {
        tundo("", |c| c.insert_text("hello"), "hello");
        tundo(
            "",
            |c| {
                c.insert_text("a");
                c.insert_text("b");
                c.insert_text("c");
            },
            "abc",
        );
    }
}
