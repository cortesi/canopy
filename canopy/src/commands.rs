use std::hash::{Hash, Hasher};

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub command: String,
    pub docs: String,
}

impl Hash for Command {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.command.hash(state);
    }
}

pub trait Commands {
    fn commands() -> Vec<Command>
    where
        Self: Sized;

    fn dispatch(&mut self, _name: &str) -> Result<()>;
}
