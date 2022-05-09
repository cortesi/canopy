use regex;
use std::{collections::HashMap, f32::consts::E};

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
}

impl PathMatcher {
    pub fn new(path: &str) -> Result<Self> {
        let parts = path.split("/");
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
        Ok(PathMatcher { expr })
    }

    /// Check whether the path filter matches a given path. Returns the position
    /// of the final match character in the path string. We use this returned
    /// value to disambiguate when mulitple matches are active for a key - the
    /// path with the largest match position wins.
    pub fn check(&self, path: &str) -> Option<usize> {
        Some(self.expr.find(path)?.end())
    }
}

struct BoundKey {
    pathmatch: PathMatcher,
    commands: Vec<String>,
}

/// A KeyMode contains a set of bound keys.
pub struct KeyMode {
    keys: HashMap<Key, Vec<BoundKey>>,
}

impl KeyMode {
    fn new() -> Self {
        KeyMode {
            keys: HashMap::new(),
        }
    }
    /// Insert a key binding into this set
    fn insert(&mut self, path_filter: PathMatcher, key: Key, commands: Vec<String>) {
        self.keys
            .entry(key)
            .or_insert_with(Vec::new)
            .push(BoundKey {
                pathmatch: path_filter,
                commands,
            });
    }
    /// Resolve a key with a given path filter to a list of commands.
    pub fn resolve(&self, path: &str, key: Key) -> Option<Vec<String>> {
        let mut ret = (0, None);
        for k in self.keys.get(&key)? {
            if let Some(p) = k.pathmatch.check(path) {
                if ret.1.is_none() || p > ret.0 {
                    ret = (p, Some(k.commands.clone()));
                }
            }
        }
        ret.1
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
    modes: HashMap<String, KeyMode>,
    current_mode: String,
}

impl KeyBindings {
    pub fn new() -> Self {
        let default = KeyMode::new();
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
        mode: &str,
        key: K,
        path_filter: &str,
        commands: Vec<String>,
    ) -> Result<()>
    where
        Key: From<K>,
    {
        let key = key.into();
        self.modes
            .entry(mode.to_string())
            .or_insert_with(KeyMode::new)
            .insert(PathMatcher::new(path_filter)?, key, commands);
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
        assert_eq!(v.check("/any/thing/"), Some(0));
        assert_eq!(v.check("/"), Some(0));

        let v = PathMatcher::new("bar")?;
        assert_eq!(v.check("/foo/bar/"), Some(8));
        assert_eq!(v.check("/bar/foo/"), Some(4));
        assert!(v.check("/foo/foo/").is_none());

        let v = PathMatcher::new("foo/*/bar")?;
        assert_eq!(v.check("/foo/oink/oink/bar/"), Some(18));
        assert_eq!(v.check("/foo/bar/"), Some(8));
        assert_eq!(v.check("/oink/foo/bar/oink/"), Some(13));
        assert_eq!(v.check("/foo/oink/oink/bar/"), Some(18));
        assert_eq!(v.check("/foo/bar/voing/"), Some(8));

        let v = PathMatcher::new("/foo")?;
        assert_eq!(v.check("/foo/"), Some(4));
        assert_eq!(v.check("/foo/bar/"), Some(4));
        assert!(v.check("/bar/foo/bar/").is_none());

        let v = PathMatcher::new("foo/")?;
        assert_eq!(v.check("/foo/"), Some(5));
        assert_eq!(v.check("/bar/foo/"), Some(9));
        assert!(v.check("/foo/bar/").is_none());

        let v = PathMatcher::new("foo/*/bar/*/voing/")?;
        assert_eq!(v.check("/foo/bar/voing/"), Some(15));
        assert_eq!(v.check("/foo/x/bar/voing/"), Some(17));
        assert_eq!(v.check("/foo/x/bar/x/voing/"), Some(19));
        assert_eq!(v.check("/x/foo/x/bar/x/voing/"), Some(21));
        assert!(v.check("/foo/x/bar/x/voing/x").is_none());

        Ok(())
    }

    #[test]
    fn keymode() -> Result<()> {
        let mut m = KeyMode::new();
        m.insert(
            PathMatcher::new("foo")?,
            'a'.into(),
            vec!["a-foo".to_string()],
        );
        m.insert(
            PathMatcher::new("bar")?,
            'a'.into(),
            vec!["a-bar".to_string()],
        );
        m.insert(PathMatcher::new("")?, 'b'.into(), vec!["b".to_string()]);

        assert_eq!(
            m.resolve("foo", 'a'.into()).unwrap(),
            vec!["a-foo".to_string()]
        );
        assert_eq!(
            m.resolve("bar", 'a'.into()).unwrap(),
            vec!["a-bar".to_string()]
        );
        assert_eq!(
            m.resolve("bar/foo", 'a'.into()).unwrap(),
            vec!["a-foo".to_string()]
        );
        assert_eq!(
            m.resolve("foo/bar", 'a'.into()).unwrap(),
            vec!["a-bar".to_string()]
        );

        Ok(())
    }
}
