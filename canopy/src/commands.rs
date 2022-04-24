use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub name: String,
    pub docs: String,
}

pub trait Commands {
    fn commands() -> Vec<Command>
    where
        Self: Sized;

    fn dispatch(&mut self, _name: &str) -> Result<()>;
}
