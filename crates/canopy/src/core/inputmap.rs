use std::{collections::HashMap, fmt};

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
    /// Compiled path matcher (includes original filter string).
    pathmatch: PathMatcher,
    /// Action to execute.
    action: BindingTarget,
}

/// A resolved input binding target.
#[derive(Debug, Clone, PartialEq)]
pub enum BindingTarget {
    /// Script identifier to execute.
    Script(script::ScriptId),
    /// Direct command invocation.
    Command(CommandInvocation),
}

/// Input event used for bindings.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
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

impl fmt::Display for InputSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Key(k) => write!(f, "{k}"),
            Self::Mouse(m) => {
                let mut parts = Vec::new();
                if m.modifiers.ctrl {
                    parts.push("Ctrl");
                }
                if m.modifiers.alt {
                    parts.push("Alt");
                }
                if m.modifiers.shift {
                    parts.push("Shift");
                }
                let action = format!("{:?}", m.action);
                let button = format!("{:?}", m.button);
                if parts.is_empty() {
                    write!(f, "{button} {action}")
                } else {
                    write!(f, "{}+{button} {action}", parts.join("+"))
                }
            }
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

    /// Insert a key binding into this mode.
    ///
    /// The input is normalized before storing.
    fn insert(&mut self, pathmatch: PathMatcher, input: InputSpec, action: BindingTarget) {
        let input = input.normalize();
        self.inputs
            .entry(input)
            .or_default()
            .push(BoundAction { pathmatch, action });
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

    /// Return all bindings in this mode.
    fn bindings(&self) -> Vec<BindingInfo<'_>> {
        let mut out = Vec::new();
        for (input, actions) in &self.inputs {
            for a in actions {
                out.push(BindingInfo {
                    input: *input,
                    path_filter: a.pathmatch.filter(),
                    target: &a.action,
                });
            }
        }
        out
    }

    /// Return bindings that match a specific path.
    fn bindings_for_path(&self, path: &Path) -> Vec<MatchedBindingInfo<'_>> {
        let mut out = Vec::new();
        for (input, actions) in &self.inputs {
            for a in actions {
                if let Some(m) = a.pathmatch.check_match(path) {
                    out.push(MatchedBindingInfo {
                        info: BindingInfo {
                            input: *input,
                            path_filter: a.pathmatch.filter(),
                            target: &a.action,
                        },
                        m,
                    });
                }
            }
        }
        out
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
        let pathmatch = PathMatcher::new(path_filter)?;
        self.modes
            .entry(mode.to_string())
            .or_insert_with(InputMode::new)
            .insert(pathmatch, input, action);
        Ok(())
    }

    /// Return the name of the current input mode.
    pub fn current_mode(&self) -> &str {
        &self.current_mode
    }

    /// Return all bindings defined for a mode.
    pub fn bindings_for_mode(&self, mode: &str) -> Vec<BindingInfo<'_>> {
        self.modes
            .get(mode)
            .map(|m| m.bindings())
            .unwrap_or_default()
    }

    /// Return bindings in a mode that match a specific path.
    pub fn bindings_matching_path(&self, mode: &str, path: &Path) -> Vec<MatchedBindingInfo<'_>> {
        self.modes
            .get(mode)
            .map(|m| m.bindings_for_path(path))
            .unwrap_or_default()
    }
}

/// Metadata about a single input binding.
#[derive(Debug, Clone)]
pub struct BindingInfo<'a> {
    /// Input that triggers this binding.
    pub input: InputSpec,
    /// Original path filter string (e.g., "editor/*").
    pub path_filter: &'a str,
    /// Target action (script or command).
    pub target: &'a BindingTarget,
}

/// Binding info with match metadata.
#[derive(Debug, Clone)]
pub struct MatchedBindingInfo<'a> {
    /// The binding info.
    pub info: BindingInfo<'a>,
    /// Match metadata from the path matcher.
    pub m: PathMatch,
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
