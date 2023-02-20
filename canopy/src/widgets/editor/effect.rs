use super::core;

trait Effector {
    /// Modifies the provided state in-place, and returns an optional undo
    /// operation.
    fn apply(&self, c: core::State) -> core::State;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect {
    Insert(Insert),
    Delete(Delete),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Insert {
    pub(crate) pos: core::Position,
    pub(crate) text: String,
}

impl Effector for Insert {
    fn apply(&self, s: core::State) -> core::State {
        s.insert(self.pos, &self.text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Delete {
    start: core::Position,
    end: core::Position,
}

impl Effector for Delete {
    fn apply(&self, s: core::State) -> core::State {
        s.delete(self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
