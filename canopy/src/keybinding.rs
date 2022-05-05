use regex;
use std::collections::HashMap;

use comfy_table::{ContentArrangement, Table};

use crate::{error, event::key::Key, Command, NodeName, Result};

const DEFAULT_MODE: &str = "default";

/// A match expression that can be applied to paths.
///
/// Examples:
///
///  - "foo" any path containing "foo"
///  - "foo/*/bar" any path containing "foo" followed by "bar"
///  - "foo/*/bar/" any path containing "foo" folowed by "bar" as a final component
///  - "/foo/*/bar/" any path starting with "foo" folowed by "bar" as a final component
///
/// The specificity of the matcher is a rough measure of the number of
/// significant match components in the specification. When disambiguating key
/// bindings, we prefer more specific matches.
#[derive(Debug, Clone)]
pub struct PathMatcher {
    expr: regex::Regex,
    pub specificity: usize,
}

impl PathMatcher {
    pub fn new(path: &str) -> Result<Self> {
        let parts = path.split("/");
        let specificity = parts.clone().filter(|x| !(*x == "*" || *x == "**")).count();

        let mut pattern = parts
            .filter_map(|x| {
                if x == "*" {
                    Some(String::from(r"[a-z0-9_/]*"))
                } else if x != "" {
                    Some(format!("{}/", regex::escape(x)))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");
        if path.starts_with("/") {
            pattern = "^/".to_string() + &pattern;
        }
        if path.ends_with("/") {
            pattern = pattern + "$";
        } else {
            pattern = pattern.trim_end_matches('/').to_string();
        }
        let expr = regex::Regex::new(&pattern).map_err(|e| error::Error::Invalid(e.to_string()))?;
        Ok(PathMatcher { specificity, expr })
    }

    /// Check whether the path filter matches a given path.
    pub fn is_match(&self, path: &str) -> bool {
        self.expr.is_match(path)
    }
}

struct BoundKey {
    matcher: PathMatcher,
    commands: Vec<String>,
}

/// A Mode contains a set of bound keys.
struct Mode {
    keys: HashMap<Key, Vec<BoundKey>>,
}

impl Mode {
    fn new() -> Self {
        Mode {
            keys: HashMap::new(),
        }
    }
    /// Insert a key binding into this set
    fn insert(&mut self, key: Key, path_filter: PathMatcher, commands: Vec<String>) {
        self.keys
            .entry(key)
            .or_insert_with(Vec::new)
            .push(BoundKey {
                matcher: path_filter,
                commands,
            });
    }
    /// Resolve a key with a given path filter to a list of commands.
    pub fn resolve(&self, path: Vec<NodeName>, key: Key) -> Option<Vec<String>> {
        unimplemented!();
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
    modes: HashMap<String, Mode>,
    current_mode: String,
}

impl KeyBindings {
    pub fn new() -> Self {
        let default = Mode::new();
        let mut modes = HashMap::new();
        modes.insert(DEFAULT_MODE.to_string(), default);
        KeyBindings {
            commands: HashMap::new(),
            current_mode: DEFAULT_MODE.into(),
            modes,
        }
    }

    pub fn resolve(&self, path: Vec<String>, key: Key) -> Option<Vec<String>> {
        unimplemented!();
    }

    /// Bind a key, within a given mode, with a given context to a list of commands.
    pub fn bind<K>(
        &mut self,
        key: K,
        mode: &str,
        context: &str,
        commands: Vec<String>,
    ) -> Result<()>
    where
        Key: From<K>,
    {
        let key = key.into();
        self.modes
            .entry(mode.to_string())
            .or_insert_with(Mode::new)
            .insert(key, PathMatcher::new(context)?, commands);
        Ok(())
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

    #[test]
    fn pathfilter() -> Result<()> {
        let v = PathMatcher::new("")?;
        assert!(v.is_match("/any/thing/"));
        assert!(v.is_match("/"));

        let v = PathMatcher::new("bar")?;
        assert!(v.is_match("/foo/bar/"));
        assert!(v.is_match("/bar/foo/"));
        assert!(!v.is_match("/foo/foo/"));

        let v = PathMatcher::new("foo/*/bar")?;
        assert!(v.is_match("/foo/oink/oink/bar/"));
        assert!(v.is_match("/foo/bar/"));
        assert!(v.is_match("/oink/foo/bar/oink/"));
        assert!(v.is_match("/foo/oink/oink/bar/"));
        assert!(v.is_match("/foo/bar/voing/"));

        let v = PathMatcher::new("/foo")?;
        assert!(v.is_match("/foo/"));
        assert!(v.is_match("/foo/bar/"));
        assert!(!v.is_match("/bar/foo/bar/"));

        let v = PathMatcher::new("foo/")?;
        assert!(v.is_match("/foo/"));
        assert!(v.is_match("/bar/foo/"));
        assert!(!v.is_match("/foo/bar/"));

        Ok(())
    }
}
