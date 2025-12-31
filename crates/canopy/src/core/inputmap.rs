use std::collections::HashMap;

use crate::{
    commands::CommandInvocation,
    error::Result,
    event::{key::Key, mouse::Mouse},
    path::*,
    script,
};

/// Default input mode name.
const DEFAULT_MODE: &str = "";

/// An action to be taken in response to an event, if the path matches.
#[derive(Debug)]
struct BoundAction {
    /// Path matcher for the binding.
    pathmatch: PathMatcher,
    /// Action to execute.
    action: BindingTarget,
}

/// A resolved input binding target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingTarget {
    /// Script identifier to execute.
    Script(script::ScriptId),
    /// Direct command invocation.
    Command(CommandInvocation),
}

/// Input event used for bindings.
#[derive(Debug, Hash, PartialEq, Eq)]
pub enum InputSpec {
    /// Mouse input.
    Mouse(Mouse),
    /// Keyboard input.
    Key(Key),
}

impl InputSpec {
    /// Normalize key variants for matching.
    fn normalize(&self) -> Self {
        match *self {
            Self::Mouse(m) => Self::Mouse(m),
            Self::Key(k) => Self::Key(k.normalize()),
        }
    }
}

/// A InputMode contains a set of bound keys and mouse actions.
#[derive(Debug)]
pub struct InputMode {
    /// Input bindings for this mode.
    inputs: HashMap<InputSpec, Vec<BoundAction>>,
}

impl InputMode {
    /// Construct an empty input mode.
    fn new() -> Self {
        Self {
            inputs: HashMap::new(),
        }
    }

    /// Insert a key binding into this mode
    fn insert(&mut self, path_filter: PathMatcher, input: InputSpec, action: BindingTarget) {
        self.inputs.entry(input).or_default().push(BoundAction {
            pathmatch: path_filter,
            action,
        });
    }

    /// Resolve a key with a given path filter to a binding target.
    pub fn resolve(&self, path: &Path, input: &InputSpec) -> Option<BindingTarget> {
        self.resolve_match(path, input)
            .map(|(action, _match)| action)
    }

    /// Resolve a key with a given path filter, returning the match metadata.
    pub fn resolve_match(
        &self,
        path: &Path,
        input: &InputSpec,
    ) -> Option<(BindingTarget, PathMatch)> {
        let input = input.normalize();
        let mut best: Option<(usize, usize, usize, BindingTarget, PathMatch)> = None;
        for (idx, k) in self.inputs.get(&input)?.iter().enumerate() {
            if let Some(m) = k.pathmatch.check_match(path) {
                let score = (m.end, m.len, idx);
                let replace = match best {
                    Some((best_end, best_len, best_idx, _, _)) => {
                        score > (best_end, best_len, best_idx)
                    }
                    None => true,
                };
                if replace {
                    best = Some((score.0, score.1, score.2, k.action.clone(), m));
                }
            }
        }
        best.map(|(_, _, _, action, m)| (action, m))
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
    /// Registered modes and bindings.
    modes: HashMap<String, InputMode>,
    /// Current active mode name.
    current_mode: String,
}

impl Default for InputMap {
    fn default() -> Self {
        Self::new()
    }
}

impl InputMap {
    /// Construct a new input map with the default mode.
    pub fn new() -> Self {
        let default = InputMode::new();
        let mut modes = HashMap::new();
        modes.insert(DEFAULT_MODE.to_string(), default);
        Self {
            current_mode: DEFAULT_MODE.into(),
            modes,
        }
    }

    #[allow(dead_code)]
    /// Set the current input mode.
    pub fn set_mode(&mut self, mode: &str) -> Result<()> {
        if mode.is_empty() {
            self.current_mode = DEFAULT_MODE.into();
            return Ok(());
        }
        self.modes
            .entry(mode.to_string())
            .or_insert_with(InputMode::new);
        self.current_mode = mode.to_string();
        Ok(())
    }

    /// Resolve a binding in the current mode.
    pub fn resolve(&self, path: &Path, input: &InputSpec) -> Option<BindingTarget> {
        // Unwrap is safe, because we make it impossible for our current mode to
        // be non-existent.
        let m = self.modes.get(&self.current_mode).unwrap();
        if let Some(action) = m.resolve(path, input) {
            return Some(action);
        }
        if self.current_mode != DEFAULT_MODE {
            return self.modes.get(DEFAULT_MODE)?.resolve(path, input);
        }
        None
    }

    /// Resolve a binding in the current mode, returning match metadata.
    pub fn resolve_match(
        &self,
        path: &Path,
        input: &InputSpec,
    ) -> Option<(BindingTarget, PathMatch)> {
        let m = self.modes.get(&self.current_mode).unwrap();
        if let Some(action) = m.resolve_match(path, input) {
            return Some(action);
        }
        if self.current_mode != DEFAULT_MODE {
            return self.modes.get(DEFAULT_MODE)?.resolve_match(path, input);
        }
        None
    }

    /// Bind a key, within a given mode, with a given context to a list of commands.
    pub fn bind(
        &mut self,
        mode: &str,
        input: InputSpec,
        path_filter: &str,
        script: script::ScriptId,
    ) -> Result<()> {
        self.bind_action(mode, input, path_filter, BindingTarget::Script(script))
    }

    /// Bind a key, within a given mode, with a given context to a direct command invocation.
    pub fn bind_command(
        &mut self,
        mode: &str,
        input: InputSpec,
        path_filter: &str,
        command: CommandInvocation,
    ) -> Result<()> {
        self.bind_action(mode, input, path_filter, BindingTarget::Command(command))
    }

    /// Store a key binding action for a mode and path filter.
    fn bind_action(
        &mut self,
        mode: &str,
        input: InputSpec,
        path_filter: &str,
        action: BindingTarget,
    ) -> Result<()> {
        self.modes
            .entry(mode.to_string())
            .or_insert_with(InputMode::new)
            .insert(PathMatcher::new(path_filter)?, input, action);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error::Result, event::key, script};

    #[test]
    fn caseconfusion() -> Result<()> {
        let mut e = script::ScriptHost::new();
        let mut m = InputMode::new();
        let a_foo = e.compile("x()")?;

        m.insert(
            PathMatcher::new("foo")?,
            InputSpec::Key('A'.into()),
            BindingTarget::Script(a_foo),
        );

        assert_eq!(
            m.resolve(&"foo".into(), &InputSpec::Key(key::Shift + 'A'))
                .unwrap(),
            BindingTarget::Script(a_foo)
        );
        assert_eq!(
            m.resolve(&"foo".into(), &InputSpec::Key(key::Shift + 'a'))
                .unwrap(),
            BindingTarget::Script(a_foo)
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
        m.insert(
            PathMatcher::new("foo")?,
            InputSpec::Key('a'.into()),
            BindingTarget::Script(a_foo),
        );
        m.insert(
            PathMatcher::new("bar")?,
            InputSpec::Key('a'.into()),
            BindingTarget::Script(a_bar),
        );
        m.insert(
            PathMatcher::new("")?,
            InputSpec::Key('b'.into()),
            BindingTarget::Script(b),
        );

        assert_eq!(
            m.resolve(&"foo".into(), &InputSpec::Key('a'.into()))
                .unwrap(),
            BindingTarget::Script(a_foo)
        );
        assert_eq!(
            m.resolve(&"bar".into(), &InputSpec::Key('a'.into()))
                .unwrap(),
            BindingTarget::Script(a_bar),
        );
        assert_eq!(
            m.resolve(&"bar/foo".into(), &InputSpec::Key('a'.into()))
                .unwrap(),
            BindingTarget::Script(a_foo),
        );
        assert_eq!(
            m.resolve(&"foo/bar".into(), &InputSpec::Key('a'.into()))
                .unwrap(),
            BindingTarget::Script(a_bar)
        );
        assert!(
            m.resolve(&"foo/bar".into(), &InputSpec::Key('x'.into()))
                .is_none()
        );
        assert!(
            m.resolve(&"nonexistent".into(), &InputSpec::Key('a'.into()))
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

        m.bind("", InputSpec::Key('a'.into()), "", a_default)?;
        m.bind("m", InputSpec::Key('a'.into()), "", a_m)?;

        assert_eq!(
            m.resolve(&"foo/bar".into(), &InputSpec::Key('a'.into()))
                .unwrap(),
            BindingTarget::Script(a_default)
        );
        m.set_mode("m")?;
        assert_eq!(
            m.resolve(&"foo/bar".into(), &InputSpec::Key('a'.into()))
                .unwrap(),
            BindingTarget::Script(a_m)
        );

        Ok(())
    }

    #[test]
    fn layered_modes_fall_back_to_default() -> Result<()> {
        let mut m = InputMap::new();
        let mut e = script::ScriptHost::new();
        let a_default = e.compile("x()")?;
        m.bind("", InputSpec::Key('b'.into()), "", a_default)?;
        m.set_mode("m")?;
        assert_eq!(
            m.resolve(&"foo".into(), &InputSpec::Key('b'.into()))
                .unwrap(),
            BindingTarget::Script(a_default)
        );
        Ok(())
    }

    #[test]
    fn binding_precedence_uses_match_length_then_order() -> Result<()> {
        let mut m = InputMode::new();
        let mut e = script::ScriptHost::new();
        let a_short = e.compile("x()")?;
        let a_long = e.compile("x()")?;
        let a_last = e.compile("x()")?;

        m.insert(
            PathMatcher::new("foo")?,
            InputSpec::Key('a'.into()),
            BindingTarget::Script(a_short),
        );
        m.insert(
            PathMatcher::new("bar/foo")?,
            InputSpec::Key('a'.into()),
            BindingTarget::Script(a_long),
        );
        m.insert(
            PathMatcher::new("bar/foo")?,
            InputSpec::Key('a'.into()),
            BindingTarget::Script(a_last),
        );

        assert_eq!(
            m.resolve(&"/root/bar/foo".into(), &InputSpec::Key('a'.into()))
                .unwrap(),
            BindingTarget::Script(a_last)
        );
        Ok(())
    }
}
