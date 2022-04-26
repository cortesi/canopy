use crate::StatefulNode;
use std::hash::{Hash, Hasher};

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub node_name: String,
    pub command: String,
    pub docs: String,
}

impl Hash for Command {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.command.hash(state);
    }
}

pub trait Commands: StatefulNode {
    /// Returns a list of commands for this struct. If a name is specified, it
    /// is used as the node name for the commands, otherwise we use the struct
    /// name converted to snake case. This method is used to pre-load our key
    /// binding map, and the optional name specifier lets us cater for nodes
    /// that may be renamed at runtime.
    fn load_commands(name: Option<&str>) -> Vec<Command>
    where
        Self: Sized;

    /// Returns a list of commands for this node.
    fn commands(&self) -> Vec<Command>;

    /// Dispatch a command to this node.
    fn dispatch(&mut self, _name: &str) -> Result<()>;
}
