use super::state;

pub(super) trait Effector {
    /// Modifies the provided state and returns a new state to apply this effect.
    fn apply(&self, c: &mut state::State);

    /// Modifies the provided state and returns a new state to revert this effect.
    fn revert(&self, c: &mut state::State);
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect {
    Insert(Insert),
    Delete(Delete),
}

impl Effector for Effect {
    fn apply(&self, s: &mut state::State) {
        match self {
            Effect::Insert(i) => i.apply(s),
            Effect::Delete(d) => d.apply(s),
        }
    }

    fn revert(&self, c: &mut state::State) {
        match self {
            Effect::Insert(i) => i.revert(c),
            Effect::Delete(d) => d.revert(c),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Insert {
    pos: state::Position,
    text: Vec<String>,
    prev_cursor: state::Position,
}

impl Insert {
    pub(super) fn new(s: &state::State, pos: state::Position, text: String) -> Self {
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
            state::Position {
                line: self.pos.line + self.text.len(),
                column: self.pos.column,
            },
        );
        s.cursor = self.prev_cursor;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Delete {
    start: state::Position,
    end: state::Position,
    prev_cursor: state::Position,
    deleted_text: Vec<String>,
}

impl Delete {
    pub(super) fn new(s: &state::State, start: state::Position, end: state::Position) -> Self {
        Self {
            start,
            end,
            prev_cursor: s.cursor,
            deleted_text: s.text_lines(start, end),
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
