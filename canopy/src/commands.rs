use std::collections::HashMap;

use crate::{postorder, preorder, Core, Error, Node, NodeId, NodeName, Result, StatefulNode, Walk};

/// The return type of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnTypes {
    /// No return value.
    Void,
    String,
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
    /// Is the return value wrapped in a cargo::Result?
    pub return_result: bool,
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
    fn commands() -> Vec<CommandDefinition>
    where
        Self: Sized;

    /// Dispatch a command to this node.
    fn dispatch(&mut self, c: &mut dyn Core, cmd: &CommandInvocation) -> Result<ReturnValue>;
}

/// Dispatch a command relative to a node. This searches the node tree for a
/// matching node::command in the following order:
///     - A pre-order traversal of the current node subtree
///     - The path from the current node to the root
pub fn dispatch<T>(
    core: &mut dyn Core,
    current_id: T,
    root: &mut dyn Node,
    cmd: &CommandInvocation,
) -> Result<Option<ReturnValue>>
where
    T: Into<NodeId>,
{
    let mut seen = false;
    let uid = current_id.into();
    let v = postorder(root, &mut |x| -> Result<Walk<ReturnValue>> {
        if seen {
            // We're now on the path to the root
            match x.dispatch(core, cmd) {
                Err(Error::UnknownCommand(_)) => Ok(Walk::Continue),
                Err(e) => Err(e),
                Ok(v) => {
                    core.taint_tree(x);
                    Ok(Walk::Handle(v))
                }
            }
        } else if x.id() == uid {
            seen = true;
            // Preorder traversal from the focus node into its descendants. Our
            // focus node will be the first node visited.
            match preorder(x, &mut |x| -> Result<Walk<ReturnValue>> {
                match x.dispatch(core, cmd) {
                    Err(Error::UnknownCommand(_)) => Ok(Walk::Continue),
                    Err(e) => Err(e),
                    Ok(v) => {
                        core.taint_tree(x);
                        Ok(Walk::Handle(v))
                    }
                }
            }) {
                Err(Error::UnknownCommand(_)) => Ok(Walk::Continue),
                Err(e) => Err(e),
                Ok(Walk::Handle(t)) => Ok(Walk::Handle(t)),
                Ok(v) => Ok(v),
            }
        } else {
            Ok(Walk::Continue)
        }
    })?;
    Ok(v.value())
}

#[derive(Debug)]
pub struct CommandSet {
    pub commands: HashMap<String, CommandDefinition>,
}

impl CommandSet {
    pub fn new() -> Self {
        CommandSet {
            commands: HashMap::new(),
        }
    }

    pub fn commands(&mut self, cmds: &[CommandDefinition]) {
        for i in cmds {
            self.commands.insert(i.fullname(), i.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as canopy;
    use crate::tutils::*;
    use crate::{command, derive_commands, CommandNode, Result, StatefulNode};

    #[test]
    fn tdispatch() -> Result<()> {
        run(|c, _, mut root| {
            dispatch(
                c,
                root.id(),
                &mut root,
                &CommandInvocation {
                    node: "bb_la".try_into()?,
                    command: "c_leaf".into(),
                },
            )?;
            assert_eq!(state_path(), vec!["bb_la.c_leaf()"]);

            reset_state();
            dispatch(
                c,
                root.b.b.id(),
                &mut root,
                &CommandInvocation {
                    node: "bb_la".try_into()?,
                    command: "c_leaf".into(),
                },
            )?;
            assert!(state_path().is_empty());
            Ok(())
        })
    }

    #[test]
    fn load_commands() -> Result<()> {
        #[derive(canopy::StatefulNode)]
        struct Foo {
            state: canopy::NodeState,
            a_triggered: bool,
            b_triggered: bool,
        }

        impl canopy::Node for Foo {}

        #[derive_commands]
        impl Foo {
            #[command]
            /// This is a comment.
            //s Multiline too!
            fn a(&mut self, _core: &mut dyn Core) -> canopy::Result<()> {
                self.a_triggered = true;
                Ok(())
            }
            #[command]
            fn b(&mut self, _core: &mut dyn Core) -> canopy::Result<()> {
                self.b_triggered = true;
                Ok(())
            }
        }

        let mut cs = CommandSet::new();
        cs.commands(&Foo::commands());

        Ok(())
    }
}
