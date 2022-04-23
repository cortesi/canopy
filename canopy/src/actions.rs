use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub name: String,
    pub docs: String,
}

pub trait Actions {
    fn actions() -> Vec<Action>
    where
        Self: Sized,
    {
        vec![]
    }
    fn dispatch(&mut self, _name: &str) -> Result<()> {
        Ok(())
    }
}
