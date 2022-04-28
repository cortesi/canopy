use std::collections::HashMap;

use comfy_table::{ContentArrangement, Table};

use crate::{event::key::Key, Command};

const DEFAULT_MODE: &str = "default";

struct BoundKey {
    context: String,
    commands: Vec<String>,
}

struct KeySet {
    keys: HashMap<Key, BoundKey>,
}

impl KeySet {
    fn new() -> Self {
        KeySet {
            keys: HashMap::new(),
        }
    }
    fn insert(&mut self, key: Key, context: String, commands: Vec<String>) {
        self.keys.insert(key, BoundKey { context, commands });
    }
    pub fn resolve(&self, path: Vec<String>, key: Key) -> Option<Vec<String>> {
        let key_binding = self.keys.get(&key)?;
        Some(key_binding.commands.clone())
    }
}

/// The Keybindings struct manages the global set of key bindings for the app.
///
/// When a key is pressed, it is first translated through the global key map
/// into a set of possible action specifications. We then walk the tree of nodes
/// from the focus to the root, trying each action specification in turn, until
/// an action is handled by a node. If no action is handled, the key is ignored.
pub struct KeyBindings {
    commands: HashMap<String, Command>,
    modes: HashMap<String, KeySet>,
    current_mode: String,
}

impl KeyBindings {
    pub fn new() -> Self {
        let default = KeySet::new();
        let mut modes = HashMap::new();
        modes.insert(DEFAULT_MODE.to_string(), default);
        KeyBindings {
            commands: HashMap::new(),
            current_mode: DEFAULT_MODE.into(),
            modes,
        }
    }

    pub fn resolve(&self, path: Vec<String>, key: Key) -> Option<Vec<String>> {
        let mode = self.modes.get(&self.current_mode)?;
        let key_binding = mode.keys.get(&key)?;
        Some(key_binding.commands.clone())
    }

    /// Bind a key, within a given mode, with a given context to a list of commands.
    pub fn bind<K>(&mut self, key: K, mode: &str, context: &str, commands: Vec<String>)
    where
        Key: From<K>,
    {
        let key = key.into();
        self.modes
            .entry(mode.to_string())
            .or_insert_with(KeySet::new)
            .insert(key, context.to_string(), commands);
    }

    pub fn load_commands(&mut self, cmds: Vec<Command>) {
        for i in cmds {
            self.commands.insert(i.fullname(), i);
        }
    }

    /// Output keybindings to the terminal, formatted in a nice table. Make sure
    /// the terminal is not being controlled by Canopy when you call this.
    pub fn pretty_print_commands(&self) {
        let mut cmds: Vec<&Command> = self.commands.values().collect();
        cmds.sort_by(|a, b| a.fullname().cmp(&b.fullname()));

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
        kb.load_commands(Foo::load_commands(None));

        Ok(())
    }
}
