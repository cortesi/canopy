use crate::{NodeName, StatefulNode};
use std::hash::{Hash, Hasher};

use crate::Result;

/// The return type of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnTypes {
    /// No return type.
    Void,
    /// A canopy::Result<T> return.
    Result,
}

/// A command is an action that can be performed on a Node. Commands are used
/// for key bindings and other types of automation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The name of the node.
    pub node_name: NodeName,
    /// The name of the command.
    pub command: String,
    /// A doc string taken from the method comment.
    pub docs: String,
    /// The return type of the command.
    pub return_type: ReturnTypes,
}

impl Command {
    /// A full command name, of the form nodename.command
    pub fn fullname(&self) -> String {
        format!("{}.{}", self.node_name, self.command)
    }
}

impl Hash for Command {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fullname().hash(state);
    }
}

/// The Commands trait is implemented by all Nodes to expose the set of
/// supported commands. With very rare exceptions, this is done with the
/// `commands` macro.
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
