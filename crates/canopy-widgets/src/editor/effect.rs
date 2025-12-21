use super::{primitives, state};

/// Apply and revert editor state changes.
pub(super) trait Effector {
    /// Modifies the provided state and returns a new state to apply this effect.
    fn apply(&self, c: &mut state::State);

    /// Modifies the provided state and returns a new state to revert this effect.
    fn revert(&self, c: &mut state::State);
}

/// An editor state change that can be applied or reverted.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect {
    /// Insert text effect.
    Insert(Insert),
    /// Delete text effect.
    Delete(Delete),
}

impl Effector for Effect {
    fn apply(&self, s: &mut state::State) {
        match self {
            Self::Insert(i) => i.apply(s),
            Self::Delete(d) => d.apply(s),
        }
    }

    fn revert(&self, c: &mut state::State) {
        match self {
            Self::Insert(i) => i.revert(c),
            Self::Delete(d) => d.revert(c),
        }
    }
}

/// Insert text effect details.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Insert {
    /// Insert position.
    pos: primitives::InsertPos,
    /// Lines of inserted text.
    text: Vec<String>,
    /// Cursor before the insert.
    prev_cursor: primitives::Cursor,
}

impl Insert {
    /// Construct an insert effect.
    pub(super) fn new(s: &state::State, pos: primitives::InsertPos, text: &str) -> Self {
        Self {
            pos,
            text: text.split("\n").map(|s| s.to_string()).collect(),
            prev_cursor: s.cursor,
        }
    }
}

impl Effector for Insert {
    fn apply(&self, s: &mut state::State) {
        s.insert_lines(self.pos, &self.text)
    }

    fn revert(&self, s: &mut state::State) {
        s.delete(
            self.pos,
            primitives::InsertPos {
                chunk: self.pos.chunk + self.text.len(),
                offset: self.pos.offset,
            },
        );
        s.cursor = self.prev_cursor;
    }
}

/// Delete text effect details.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Delete {
    /// Starting position.
    start: primitives::InsertPos,
    /// Ending position.
    end: primitives::InsertPos,
    /// Cursor before the delete.
    prev_cursor: primitives::Cursor,
    /// Deleted lines.
    deleted_text: Vec<String>,
}

impl Delete {
    /// Construct a delete effect.
    pub(super) fn new(
        s: &state::State,
        start: primitives::InsertPos,
        end: primitives::InsertPos,
    ) -> Self {
        Self {
            start,
            end,
            prev_cursor: s.cursor,
            deleted_text: s.line_range(start, end),
        }
    }
}

impl Effector for Delete {
    fn apply(&self, s: &mut state::State) {
        s.delete(self.start, self.end)
    }

    fn revert(&self, s: &mut state::State) {
        s.insert_lines(self.start, &self.deleted_text);
        s.cursor = self.prev_cursor;
    }
}
