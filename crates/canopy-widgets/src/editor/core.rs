use canopy_core as canopy;
use super::effect::Effector;
use super::{effect, primitives::InsertPos, state};
use canopy_core::geom::Point;

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

