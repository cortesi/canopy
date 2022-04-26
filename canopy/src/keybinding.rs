use std::collections::HashSet;

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
