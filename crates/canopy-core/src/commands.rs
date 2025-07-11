use std::collections::HashMap;

use crate::{
    Context, Error, Node, NodeId, Result,
    state::{NodeName, StatefulNode},
    tree,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgTypes {
    Context,
    ISize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Args {
    Context,
    ISize(isize),
}

impl Args {
    pub fn as_isize(&self) -> Result<isize> {
        match self {
            Args::ISize(i) => Ok(*i),
            _ => Err(Error::Internal(format!("Expected isize, got {self:?}"))),
        }
    }
}

/// The return type of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnTypes {
    /// No return value.
    Void,
    String,
}

/// The return type of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReturnSpec {
    /// What is the ultimate type of the return?
    pub typ: ReturnTypes,
    /// Is the return wrapped in a `Result`? That is, is the method fallible?
    pub result: bool,
}

impl ReturnSpec {
    pub fn new(typ: ReturnTypes, result: bool) -> Self {
        Self { typ, result }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnValue {
    Void,
    String(String),
}

/// A parsed command invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInvocation {
    /// The name of the node.
    pub node: NodeName,
    /// The name of the command.
    pub command: String,
    /// Arguments to the command.
    pub args: Vec<Args>,
}

/// CommandDefinition encapsulates the definition of a command that can be
/// performed on a Node. Commands are used for key bindings, mouse actions and
/// general automation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    /// The name of the node.
    pub node: NodeName,
    /// The name of the command.
    pub command: String,
    /// A doc string taken from the method comment.
    pub docs: String,
    /// The return type of the command.
    pub ret: ReturnSpec,

    pub args: Vec<ArgTypes>,
}

impl CommandSpec {
    /// A full command name, of the form nodename.command
    pub fn fullname(&self) -> String {
        format!("{}.{}", self.node, self.command)
    }
}

/// The CommandNode trait is implemented by all Nodes to expose the set of supported commands. With rare exceptions,
/// this is done with the `commands` macro.
pub trait CommandNode: StatefulNode {
    /// Return a list of commands for this node.
    fn commands() -> Vec<CommandSpec>
    where
        Self: Sized;

    /// Dispatch a command to this node.
    fn dispatch(&mut self, c: &mut dyn Context, cmd: &CommandInvocation) -> Result<ReturnValue>;
}

/// Dispatch a command relative to a node. This searches the node tree for a
/// matching node::command in the following order:
///     - A pre-order traversal of the current node subtree
///     - The path from the current node to the root
pub fn dispatch<T>(
    core: &mut dyn Context,
    current_id: T,
    root: &mut dyn Node,
    cmd: &CommandInvocation,
) -> Result<Option<ReturnValue>>
where
    T: Into<NodeId>,
{
    let mut seen = false;
    let uid = current_id.into();
    let v = tree::postorder(root, &mut |x| -> Result<tree::Walk<ReturnValue>> {
        if seen {
            // We're now on the path to the root
            let _pre_focus = core.current_focus_gen();
            match x.dispatch(core, cmd) {
                Err(Error::UnknownCommand(_)) => Ok(tree::Walk::Continue),
                Err(e) => Err(e),
                Ok(v) => Ok(tree::Walk::Handle(v)),
            }
        } else if x.id() == uid {
            seen = true;
            // Preorder traversal from the focus node into its descendants. Our
            // focus node will be the first node visited.
            match tree::preorder(x, &mut |x| -> Result<tree::Walk<ReturnValue>> {
                let _pre_focus = core.current_focus_gen();
                match x.dispatch(core, cmd) {
                    Err(Error::UnknownCommand(_)) => Ok(tree::Walk::Continue),
                    Err(e) => Err(e),
                    Ok(v) => {
                        // No longer need to check taint status since everything is always tainted
                        Ok(tree::Walk::Handle(v))
                    }
                }
            }) {
                Err(Error::UnknownCommand(_)) => Ok(tree::Walk::Continue),
                Err(e) => Err(e),
                Ok(tree::Walk::Handle(t)) => Ok(tree::Walk::Handle(t)),
                Ok(v) => Ok(v),
            }
        } else {
            Ok(tree::Walk::Continue)
        }
    })?;
    Ok(v.value())
}

#[derive(Debug)]
pub struct CommandSet {
    pub commands: HashMap<String, CommandSpec>,
}

impl Default for CommandSet {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandSet {
    pub fn new() -> Self {
        CommandSet {
            commands: HashMap::new(),
        }
    }

    pub fn commands(&mut self, cmds: &[CommandSpec]) {
        for i in cmds {
            self.commands.insert(i.fullname(), i.clone());
        }
    }
}

// Tests moved to canopy crate to avoid circular dependency
#[cfg(test)]
mod tests {
    // TODO: Move command dispatch tests from canopy-core to canopy crate
}
