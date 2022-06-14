use std::collections::HashMap;

use crate::{error, event::key::Key, path::*, script, Result};

const DEFAULT_MODE: &str = "";

#[derive(Debug)]
struct BoundKey {
    pathmatch: PathMatcher,
    script: script::Script,
}

/// A KeyMode contains a set of bound keys.
#[derive(Debug)]
pub struct KeyMode {
    keys: HashMap<Key, Vec<BoundKey>>,
}

impl KeyMode {
    fn new() -> Self {
        KeyMode {
            keys: HashMap::new(),
        }
    }
    /// Insert a key binding into this mode
    fn insert(&mut self, path_filter: PathMatcher, key: Key, script: script::Script) {
        self.keys
            .entry(key)
            .or_insert_with(Vec::new)
            .push(BoundKey {
                pathmatch: path_filter,
                script,
            });
    }
    /// Resolve a key with a given path filter to a script.
    pub fn resolve(&self, path: &Path, key: Key) -> Option<&script::Script> {
        let mut ret = (0, None);
        for k in self.keys.get(&key)? {
            if let Some(p) = k.pathmatch.check(path) {
                if ret.1.is_none() || p > ret.0 {
                    ret = (p, Some(&k.script));
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
#[derive(Debug)]
pub struct KeyMap {
    modes: HashMap<String, KeyMode>,
    current_mode: String,
}

impl Default for KeyMap {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyMap {
    pub fn new() -> Self {
        let default = KeyMode::new();
        let mut modes = HashMap::new();
        modes.insert(DEFAULT_MODE.to_string(), default);
        KeyMap {
            current_mode: DEFAULT_MODE.into(),
            modes,
        }
    }

    pub fn set_mode(&mut self, mode: &str) -> Result<()> {
        if !mode.is_empty() && !self.modes.contains_key(mode) {
            Err(error::Error::Invalid(format!("Unknown mode: {}", mode)))
        } else {
            self.current_mode = mode.to_string();
            Ok(())
        }
    }

    pub fn resolve(&self, path: &Path, key: Key) -> Option<&script::Script> {
        // Unwrap is safe, because we make it impossible for our current mode to
        // be non-existent.
        let m = self.modes.get(&self.current_mode).unwrap();
        m.resolve(path, key)
    }

    /// Bind a key, within a given mode, with a given context to a list of commands.
    pub fn bind<K>(
        &mut self,
        mode: &str,
        key: K,
        path_filter: &str,
        script: script::Script,
    ) -> Result<()>
    where
        Key: From<K>,
    {
        let key = key.into();
        self.modes
            .entry(mode.to_string())
            .or_insert_with(KeyMode::new)
            .insert(PathMatcher::new(path_filter)?, key, script);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{script, Result};

    #[test]
    fn keymode() -> Result<()> {
        let e = script::ScriptHost::new();

        let mut m = KeyMode::new();
        m.insert(PathMatcher::new("foo")?, 'a'.into(), e.compile("a_foo()")?);
        m.insert(PathMatcher::new("bar")?, 'a'.into(), e.compile("a_bar()")?);
        m.insert(PathMatcher::new("")?, 'b'.into(), e.compile("b()")?);

        assert_eq!(
            m.resolve(&"foo".into(), 'a'.into()).unwrap().source(),
            "a_foo()"
        );
        assert_eq!(
            m.resolve(&"bar".into(), 'a'.into()).unwrap().source(),
            "a_bar()"
        );
        assert_eq!(
            m.resolve(&"bar/foo".into(), 'a'.into()).unwrap().source(),
            "a_foo()"
        );
        assert_eq!(
            m.resolve(&"foo/bar".into(), 'a'.into()).unwrap().source(),
            "a_bar()"
        );
        assert!(m.resolve(&"foo/bar".into(), 'x'.into()).is_none());
        assert!(m.resolve(&"nonexistent".into(), 'a'.into()).is_none());

        Ok(())
    }

    #[test]
    fn keymap() -> Result<()> {
        let mut m = KeyMap::new();
        let e = script::ScriptHost::new();

        m.bind("", 'a', "", e.compile("a_default()")?)?;
        m.bind("m", 'a', "", e.compile("a_m()")?)?;

        assert_eq!(
            m.resolve(&"foo/bar".into(), 'a'.into()).unwrap().source(),
            "a_default()"
        );
        m.set_mode("m")?;
        assert_eq!(
            m.resolve(&"foo/bar".into(), 'a'.into()).unwrap().source(),
            "a_m()"
        );

        Ok(())
    }
}
