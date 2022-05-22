use crate::{postorder, Node, NodeId, NodeName, SkipWalker, StatefulNode};

use crate::Result;

/// The return type of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnTypes {
    /// No return value.
    Void,
    /// A canopy::Result<T> return.
    Result,
}

/// A parsed command invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The name of the node.
    pub node: NodeName,
    /// The name of the command.
    pub command: String,
}

/// CommandDefinition encapsulates the definition of a command that can be
/// performed on a Node. Commands are used for key bindings, mouse actions and
/// general automation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDefinition {
    /// The name of the node.
    pub node: NodeName,
    /// The name of the command.
    pub command: String,
    /// A doc string taken from the method comment.
    pub docs: String,
    /// The return type of the command.
    pub return_type: ReturnTypes,
}

impl CommandDefinition {
    /// A full command name, of the form nodename.command
    pub fn fullname(&self) -> String {
        format!("{}.{}", self.node, self.command)
    }
}

/// The CommandNode trait is implemented by all Nodes to expose the set of
/// supported commands. With rare exceptions, this is done with the `commands`
/// macro.
pub trait CommandNode: StatefulNode {
    /// Returns a list of commands for this node. If a name is specified, it
    /// is used as the node name for the commands, otherwise we use the struct
    /// name converted to snake case. This method is used to pre-load our key
    /// binding map, and the optional name specifier lets us cater for nodes
    /// that may be renamed at runtime.
    fn load_commands(name: Option<&str>) -> Vec<CommandDefinition>
    where
        Self: Sized;

    /// Returns a list of commands for this node.
    fn commands(&self) -> Vec<CommandDefinition>;

    /// Dispatch a command to this node.
    fn dispatch(&mut self, cmd: &Command) -> Result<()>;
}

/// Dispatch a command relative to a node. This searches the node tree for a
/// matching node::command in the following order:
///     - A pre-order traversal of the current node subtree
///     - The path from the current node to the root
pub fn dispatch<T>(current_id: T, root: &mut dyn Node, _cmd: &Command) -> Result<()>
where
    T: Into<NodeId>,
{
    let mut seen = false;
    let uid = current_id.into();
    postorder(root, &mut |x| -> Result<SkipWalker> {
        Ok(if seen {
            // if x.dispatch(cmd).is_ok() {
            //     SkipWalker::Stop
            // } else {
            //     SkipWalker::Continue
            // }
            // Path to root
            //ret = ret.join(f(x)?);
            SkipWalker::new(false)
        } else if x.id() == uid {
            seen = true;
            // ret = ret.join(f(x)?);
            SkipWalker::new(true)
        } else {
            SkipWalker::new(false)
        })
    })?;
    Ok(())
}
