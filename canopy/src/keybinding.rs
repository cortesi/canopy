use std::collections::HashSet;

use comfy_table::{ContentArrangement, Table};

use crate::Command;

/// The Keybindings struct manages the global set of key bindings for the app.
///
/// When a key is pressed, it is first translated through the global key map
/// into a set of possible action specifications. We then walk the tree of nodes
/// from the focus to the root, trying each action specification in turn, until
/// an action is handled by a node. If no action is handled, the key is ignored.
pub struct KeyBindings {
    commands: HashSet<Command>,
}

impl KeyBindings {
    pub fn new() -> Self {
        KeyBindings {
            commands: HashSet::new(),
        }
    }

    pub fn load(&mut self, cmds: Vec<Command>) {
        for i in cmds {
            self.commands.insert(i);
        }
    }

    /// Output keybindings to the terminal, formatted in a nice table. Make sure
    /// the terminal is not being controlled by Canopy when you call this.
    pub fn pretty_print(&self) {
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.load_preset(comfy_table::presets::UTF8_FULL);

        let mut cmds: Vec<&Command> = self.commands.iter().collect();
        cmds.sort_by(|a, b| a.fullname().cmp(&b.fullname()));

        for i in cmds {
            table.add_row(vec![
                comfy_table::Cell::new(i.fullname()).fg(comfy_table::Color::Green),
                comfy_table::Cell::new(i.docs.clone()),
            ]);
        }

        println!("{table}");
    }
}

impl std::fmt::Display for KeyBindings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for i in self.commands.iter() {
            write!(f, "{}\n", i)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as canopy;
    use crate::{command, derive_commands, Commands, Result, StatefulNode};

    #[test]
    fn kb_load() -> Result<()> {
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

        let mut kb = KeyBindings::new();
        kb.load(Foo::load_commands(None));

        Ok(())
    }
}
