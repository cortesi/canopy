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

    pub fn insert_text(&mut self, pos: state::Position, text: &str) {
        let ins = effect::Insert::new(&self.state, pos, text.to_string());
        ins.apply(&mut self.state);
    }
}
