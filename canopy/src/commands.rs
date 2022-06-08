use std::collections::HashMap;

use comfy_table::{ContentArrangement, Table};

use crate::{postorder, preorder, Error, Node, NodeId, NodeName, Result, StatefulNode, Walk};

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
pub fn dispatch<T>(current_id: T, root: &mut dyn Node, cmd: &Command) -> Result<()>
where
    T: Into<NodeId>,
{
    let mut seen = false;
    let uid = current_id.into();
    postorder(root, &mut |x| -> Result<Walk<()>> {
        if seen {
            // We're now on the path to the root
            match x.dispatch(cmd) {
                Err(Error::UnknownCommand(_)) => Ok(Walk::Continue),
                Err(e) => Err(e),
                Ok(_) => Ok(Walk::Handle(())),
            }
        } else if x.id() == uid {
            seen = true;
            // Preorder traversal from the focus node into its descendants. Our
            // focus node will be the first node visited.
            match preorder(x, &mut |x| -> Result<Walk<()>> {
                match x.dispatch(cmd) {
                    Err(Error::UnknownCommand(_)) => Ok(Walk::Continue),
                    Err(e) => Err(e),
                    Ok(_) => Ok(Walk::Handle(())),
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
    Ok(())
}

pub struct CommandSet {
    pub commands: HashMap<String, CommandDefinition>,
}

impl CommandSet {
    pub fn new() -> Self {
        CommandSet {
            commands: HashMap::new(),
        }
    }

    pub fn load_commands(&mut self, cmds: Vec<CommandDefinition>) {
        for i in cmds {
            self.commands.insert(i.fullname(), i);
        }
    }

    /// Output keybindings to the terminal, formatted in a nice table. Make sure
    /// the terminal is not being controlled by Canopy when you call this.
    pub fn pretty_print(&self) {
        let mut cmds: Vec<&CommandDefinition> = self.commands.values().collect();

        cmds.sort_by_key(|a| a.fullname());

        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.load_preset(comfy_table::presets::UTF8_FULL);
        for i in cmds {
            table.add_row(vec![
                comfy_table::Cell::new(i.fullname()).fg(comfy_table::Color::Green),
                comfy_table::Cell::new(i.docs.clone()),
            ]);
        }
        println!("{table}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as canopy;
    use crate::tutils::utils;
    use crate::{command, derive_commands, CommandNode, Result, StatefulNode};

    #[test]
    fn tdispatch() -> Result<()> {
        let mut root = utils::TRoot::new();

        dispatch(
            root.id(),
            &mut root,
            &Command {
                node: "bb_la".try_into()?,
                command: "c_leaf".into(),
            },
        )?;
        assert_eq!(utils::state_path(), vec!["bb_la.c_leaf()"]);

        utils::reset_state();
        dispatch(
            root.b.b.id(),
            &mut root,
            &Command {
                node: "bb_la".try_into()?,
                command: "c_leaf".into(),
            },
        )?;
        assert!(utils::state_path().is_empty());

        Ok(())
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
            fn a(&mut self) -> canopy::Result<()> {
                self.a_triggered = true;
                Ok(())
            }
            #[command]
            fn b(&mut self) -> canopy::Result<()> {
                self.b_triggered = true;
                Ok(())
            }
        }

        let mut cs = CommandSet::new();
        cs.load_commands(Foo::load_commands(None));

        Ok(())
    }
}
