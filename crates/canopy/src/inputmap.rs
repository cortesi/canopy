use std::collections::HashMap;

use crate::script;
use canopy_core::{Result, error, event::key::Key, event::mouse::Mouse, path::*};

const DEFAULT_MODE: &str = "";

/// An action to be taken in response to an event, if the path matches.
#[derive(Debug)]
struct BoundAction {
    pathmatch: PathMatcher,
    script: script::ScriptId,
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub enum Input {
    Mouse(Mouse),
    Key(Key),
}

impl Input {
    fn normalize(&self) -> Input {
        match *self {
            Input::Mouse(m) => Input::Mouse(m),
            Input::Key(k) => Input::Key(k.normalize()),
        }
    }
}

/// A InputMode contains a set of bound keys and mouse actions.
#[derive(Debug)]
pub struct InputMode {
    inputs: HashMap<Input, Vec<BoundAction>>,
}

impl InputMode {
    fn new() -> Self {
        InputMode {
            inputs: HashMap::new(),
        }
    }

    /// Insert a key binding into this mode
    fn insert(&mut self, path_filter: PathMatcher, input: Input, script: script::ScriptId) {
        self.inputs.entry(input).or_default().push(BoundAction {
            pathmatch: path_filter,
            script,
        });
    }

    /// Resolve a key with a given path filter to a script.
    pub fn resolve(&self, path: &Path, input: Input) -> Option<script::ScriptId> {
        let input = input.normalize();
        let mut ret = (0, None);
        for k in self.inputs.get(&input)? {
            if let Some(p) = k.pathmatch.check(path) {
                if ret.1.is_none() || p > ret.0 {
                    ret = (p, Some(k.script));
                }
            }
        }
        ret.1
    }
}

/// The InputMap struct manages the global set of key and mouse bindings for the
/// app.
///
/// When a key is pressed, it is first translated through the global key map
/// into a set of possible action specifications. We then walk the tree of nodes
/// from the focus to the root, trying each action specification in turn, until
/// an action is handled by a node. If no action is handled, the key is ignored.
#[derive(Debug)]
pub struct InputMap {
    modes: HashMap<String, InputMode>,
    current_mode: String,
}

impl Default for InputMap {
    fn default() -> Self {
        Self::new()
    }
}

impl InputMap {
    pub fn new() -> Self {
        let default = InputMode::new();
        let mut modes = HashMap::new();
        modes.insert(DEFAULT_MODE.to_string(), default);
        InputMap {
            current_mode: DEFAULT_MODE.into(),
            modes,
        }
    }

    #[allow(dead_code)]
    pub fn set_mode(&mut self, mode: &str) -> Result<()> {
        if !mode.is_empty() && !self.modes.contains_key(mode) {
            Err(error::Error::Invalid(format!("Unknown mode: {mode}")))
        } else {
            self.current_mode = mode.to_string();
            Ok(())
        }
    }

    pub fn resolve(&self, path: &Path, input: Input) -> Option<script::ScriptId> {
        // Unwrap is safe, because we make it impossible for our current mode to
        // be non-existent.
        let m = self.modes.get(&self.current_mode).unwrap();
        m.resolve(path, input)
    }

    /// Bind a key, within a given mode, with a given context to a list of commands.
    pub fn bind(
        &mut self,
        mode: &str,
        input: Input,
        path_filter: &str,
        script: script::ScriptId,
    ) -> Result<()> {
        self.modes
            .entry(mode.to_string())
            .or_insert_with(InputMode::new)
            .insert(PathMatcher::new(path_filter)?, input, script);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script;
    use canopy_core::{Result, event::key};

    #[test]
    fn caseconfusion() -> Result<()> {
        let mut e = script::ScriptHost::new();
        let mut m = InputMode::new();
        let a_foo = e.compile("x()")?;

        m.insert(PathMatcher::new("foo")?, Input::Key('A'.into()), a_foo);

        assert_eq!(
            m.resolve(&"foo".into(), Input::Key(key::Shift + 'A'))
                .unwrap(),
            a_foo
        );
        assert_eq!(
            m.resolve(&"foo".into(), Input::Key(key::Shift + 'a'))
                .unwrap(),
            a_foo
        );

        Ok(())
    }

    #[test]
    fn keymode() -> Result<()> {
        let mut e = script::ScriptHost::new();

        let mut m = InputMode::new();
        let a_foo = e.compile("x()")?;
        let a_bar = e.compile("x()")?;
        let b = e.compile("x()")?;
        m.insert(PathMatcher::new("foo")?, Input::Key('a'.into()), a_foo);
        m.insert(PathMatcher::new("bar")?, Input::Key('a'.into()), a_bar);
        m.insert(PathMatcher::new("")?, Input::Key('b'.into()), b);

        assert_eq!(
            m.resolve(&"foo".into(), Input::Key('a'.into())).unwrap(),
            a_foo
        );
        assert_eq!(
            m.resolve(&"bar".into(), Input::Key('a'.into())).unwrap(),
            a_bar,
        );
        assert_eq!(
            m.resolve(&"bar/foo".into(), Input::Key('a'.into()))
                .unwrap(),
            a_foo,
        );
        assert_eq!(
            m.resolve(&"foo/bar".into(), Input::Key('a'.into()))
                .unwrap(),
            a_bar
        );
        assert!(
            m.resolve(&"foo/bar".into(), Input::Key('x'.into()))
                .is_none()
        );
        assert!(
            m.resolve(&"nonexistent".into(), Input::Key('a'.into()))
                .is_none()
        );

        Ok(())
    }

    #[test]
    fn keymap() -> Result<()> {
        let mut m = InputMap::new();
        let mut e = script::ScriptHost::new();

        let a_default = e.compile("x()")?;
        let a_m = e.compile("x()")?;

        m.bind("", Input::Key('a'.into()), "", a_default)?;
        m.bind("m", Input::Key('a'.into()), "", a_m)?;

        assert_eq!(
            m.resolve(&"foo/bar".into(), Input::Key('a'.into()))
                .unwrap(),
            a_default
        );
        m.set_mode("m")?;
        assert_eq!(
            m.resolve(&"foo/bar".into(), Input::Key('a'.into()))
                .unwrap(),
            a_m
        );

        Ok(())
    }
}
