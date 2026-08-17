use std::{
    collections::{HashMap, HashSet},
    fmt, iter,
};

use crate::{
    error::{Error, Result},
    event::{
        key::Key,
        mouse::{self, Mouse},
    },
    path::*,
    script::LuauFunctionId,
};

/// Default input mode name.
const DEFAULT_MODE: &str = "";

/// Monotonic identifier for a binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingId(u64);

impl BindingId {
    /// Allocate the next binding ID.
    fn next(next: &mut u64) -> Result<Self> {
        let id = Self(*next);
        *next = next.checked_add(1).ok_or_else(|| {
            Error::InvalidOperation("binding identifier space exhausted".to_string())
        })?;
        Ok(id)
    }

    /// Return the numeric binding identifier.
    pub fn as_u64(self) -> u64 {
        self.0
    }

    /// Reconstruct a binding identifier from its numeric form.
    pub fn from_u64(id: u64) -> Self {
        Self(id)
    }
}

/// An action to be taken in response to an event, if the path matches.
#[derive(Clone, Debug)]
struct BoundAction {
    /// Unique identifier for the binding.
    id: BindingId,
    /// Compiled path matcher (includes original filter string).
    pathmatch: PathMatcher,
    /// Action to execute.
    action: LuauFunctionId,
}

/// Binding match precedence: the path-match score, then insertion order.
///
/// Higher values win, so a later insertion wins an otherwise exact tie.
type BindingPriority = ((usize, usize, usize), usize);

/// Tuple storing a binding match and its score.
type BindingCandidate = (BindingPriority, LuauFunctionId, PathMatch);

/// Binding mode/path filter used when removing or replacing bindings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BindingFilter<'a> {
    /// Optional mode name to match.
    pub mode: Option<&'a str>,
    /// Optional exact path filter string to match.
    pub path_filter: Option<&'a str>,
}

/// Input event used for bindings.
///
/// Key inputs are normalized when stored or matched so bindings are resilient
/// to terminal differences in Ctrl/Shift representations.
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
                let key_label = if matches!(m.button, mouse::Button::None) {
                    action
                } else {
                    let button = format!("{:?}", m.button);
                    format!("{button} {action}")
                };
                if parts.is_empty() {
                    write!(f, "{key_label}")
                } else {
                    write!(f, "{}+{key_label}", parts.join("+"))
                }
            }
        }
    }
}

/// A InputMode contains a set of bound keys and mouse actions.
#[derive(Clone, Debug)]
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
    fn insert(
        &mut self,
        id: BindingId,
        pathmatch: PathMatcher,
        input: InputSpec,
        action: LuauFunctionId,
    ) {
        let input = input.normalize();
        self.inputs.entry(input).or_default().push(BoundAction {
            id,
            pathmatch,
            action,
        });
    }

    /// Resolve a key with a given path filter, returning the match metadata.
    ///
    /// The input is normalized before matching.
    pub fn resolve_match(
        &self,
        path: &Path,
        input: &InputSpec,
    ) -> Option<(LuauFunctionId, PathMatch)> {
        let input = input.normalize();
        let mut best: Option<BindingCandidate> = None;
        for (idx, k) in self.inputs.get(&input)?.iter().enumerate() {
            if let Some(m) = k.pathmatch.check_match(path) {
                let score: BindingPriority = (m.score(), idx);
                let replace = match best {
                    Some((best_score, _, _)) => score > best_score,
                    None => true,
                };
                if replace {
                    best = Some((score, k.action, m));
                }
            }
        }
        best.map(|(_, action, m)| (action, m))
    }

    /// Return all bindings in this mode.
    fn bindings(&self) -> Vec<BindingInfo<'_>> {
        let mut out = Vec::new();
        for (input, actions) in &self.inputs {
            for a in actions {
                out.push(BindingInfo {
                    id: a.id,
                    input: *input,
                    path_filter: a.pathmatch.filter(),
                    target: a.action,
                });
            }
        }
        out.sort_by(|left, right| {
            left.input
                .to_string()
                .cmp(&right.input.to_string())
                .then_with(|| left.path_filter.cmp(right.path_filter))
                .then_with(|| left.id.0.cmp(&right.id.0))
        });
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
                            id: a.id,
                            input: *input,
                            path_filter: a.pathmatch.filter(),
                            target: a.action,
                        },
                        m,
                    });
                }
            }
        }
        out.sort_by(|left, right| {
            left.info
                .input
                .to_string()
                .cmp(&right.info.input.to_string())
                .then_with(|| left.info.path_filter.cmp(right.info.path_filter))
                .then_with(|| left.info.id.0.cmp(&right.info.id.0))
        });
        out
    }

    /// Remove a binding by ID and return removed targets.
    fn unbind_with_targets(&mut self, id: BindingId) -> Vec<LuauFunctionId> {
        let mut removed = false;
        let mut targets = Vec::new();
        for actions in self.inputs.values_mut() {
            let mut retained = Vec::new();
            for action in actions.drain(..) {
                if action.id == id {
                    removed = true;
                    targets.push(action.action);
                } else {
                    retained.push(action);
                }
            }
            *actions = retained;
        }
        self.inputs.retain(|_, actions| !actions.is_empty());
        if removed { targets } else { Vec::new() }
    }

    /// Remove bindings for an input, optionally filtered by path.
    fn unbind_input(
        &mut self,
        input: InputSpec,
        path_filter: Option<&str>,
    ) -> Vec<(BindingId, LuauFunctionId)> {
        let input = input.normalize();
        let Some(actions) = self.inputs.get_mut(&input) else {
            return Vec::new();
        };

        let mut removed = Vec::new();
        actions.retain(|action| {
            let matches = path_filter.is_none_or(|filter| action.pathmatch.filter() == filter);
            if matches {
                removed.push((action.id, action.action));
                false
            } else {
                true
            }
        });

        if actions.is_empty() {
            self.inputs.remove(&input);
        }

        removed
    }

    /// Remove all bindings from this mode.
    fn clear(&mut self) -> Vec<(BindingId, LuauFunctionId)> {
        let mut removed = Vec::new();
        for actions in self.inputs.values_mut() {
            for action in actions.drain(..) {
                removed.push((action.id, action.action));
            }
        }
        self.inputs.clear();
        removed
    }

    /// Remove every Luau-backed binding from this mode.
    fn remove_luau_functions(&mut self) -> Vec<(BindingId, LuauFunctionId)> {
        let mut removed = Vec::new();
        for actions in self.inputs.values_mut() {
            for action in actions.drain(..) {
                removed.push((action.id, action.action));
            }
        }
        self.inputs.retain(|_, actions| !actions.is_empty());
        removed
    }
}

/// The InputMap struct manages the global set of key and mouse bindings for the
/// app.
///
/// When a key is pressed, it is first translated through the global key map
/// into a set of possible action specifications. We then walk the tree of nodes
/// from the focus to the root, trying each action specification in turn, until
/// an action is handled by a node. If no action is handled, the key is ignored.
#[derive(Clone, Debug)]
pub struct InputMap {
    /// Registered modes and bindings.
    modes: HashMap<String, InputMode>,
    /// Current active mode name.
    current_mode: String,
    /// Active non-default modes, ordered from oldest to newest.
    mode_stack: Vec<String>,
    /// Next binding identifier.
    next_id: u64,
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
            mode_stack: Vec::new(),
            modes,
            next_id: 1,
        }
    }

    /// Set the current input mode.
    pub fn set_mode(&mut self, mode: &str) -> Result<()> {
        if mode.is_empty() {
            self.current_mode = DEFAULT_MODE.into();
            self.mode_stack.clear();
            return Ok(());
        }
        self.ensure_mode(mode);
        self.mode_stack.clear();
        self.mode_stack.push(mode.to_string());
        self.refresh_current_mode();
        Ok(())
    }

    /// Push an input mode on top of the active mode stack.
    pub fn push_mode(&mut self, mode: &str) -> Result<()> {
        if mode.is_empty() {
            return Ok(());
        }
        self.ensure_mode(mode);
        self.mode_stack.push(mode.to_string());
        self.refresh_current_mode();
        Ok(())
    }

    /// Pop the top input mode and return the newly-active mode name.
    pub fn pop_mode(&mut self) -> &str {
        self.mode_stack.pop();
        self.refresh_current_mode();
        self.current_mode()
    }

    /// Return active non-default modes from oldest to newest.
    #[cfg(test)]
    pub(crate) fn mode_stack(&self) -> &[String] {
        &self.mode_stack
    }

    /// Return active mode names in binding-resolution order.
    pub fn active_modes(&self) -> Vec<&str> {
        let mut modes = Vec::new();
        for mode in self.resolution_modes() {
            if !modes.contains(&mode) {
                modes.push(mode);
            }
        }
        modes
    }

    /// Ensure a mode exists in the registry.
    fn ensure_mode(&mut self, mode: &str) {
        self.modes
            .entry(mode.to_string())
            .or_insert_with(InputMode::new);
    }

    /// Refresh the cached current mode name from the stack top.
    fn refresh_current_mode(&mut self) {
        self.current_mode = self
            .mode_stack
            .last()
            .cloned()
            .unwrap_or_else(|| DEFAULT_MODE.to_string());
    }

    /// Return active mode names in resolution order.
    fn resolution_modes(&self) -> impl Iterator<Item = &str> {
        self.mode_stack
            .iter()
            .rev()
            .map(String::as_str)
            .chain(iter::once(DEFAULT_MODE))
    }

    /// Resolve a binding using a supplied mode sequence.
    fn resolve_in_modes<R>(&self, mut resolve: impl FnMut(&InputMode) -> Option<R>) -> Option<R> {
        for mode in self.resolution_modes() {
            if let Some(result) = self.modes.get(mode).and_then(&mut resolve) {
                return Some(result);
            }
        }
        None
    }

    /// Resolve a binding in the current mode, returning match metadata.
    ///
    /// The input is normalized before matching.
    pub fn resolve_match(
        &self,
        path: &Path,
        input: &InputSpec,
    ) -> Option<(LuauFunctionId, PathMatch)> {
        self.resolve_in_modes(|mode| mode.resolve_match(path, input))
    }

    /// Store a binding for a mode and path filter without a live script host.
    ///
    /// Returns the new binding ID.
    #[cfg(test)]
    pub(crate) fn bind_test(
        &mut self,
        mode: &str,
        input: InputSpec,
        path_filter: &str,
        action: LuauFunctionId,
    ) -> Result<BindingId> {
        let pathmatch = PathMatcher::new(path_filter)?;
        let id = BindingId::next(&mut self.next_id)?;
        self.modes
            .entry(mode.to_string())
            .or_insert_with(InputMode::new)
            .insert(id, pathmatch, input, action);
        Ok(id)
    }

    /// Remove a binding by ID and return removed targets.
    pub fn unbind_with_targets(&mut self, id: BindingId) -> Vec<LuauFunctionId> {
        let mut removed = false;
        let mut targets = Vec::new();
        for mode in self.modes.values_mut() {
            let removed_targets = mode.unbind_with_targets(id);
            if !removed_targets.is_empty() {
                removed = true;
                targets.extend(removed_targets);
            }
        }
        if removed { targets } else { Vec::new() }
    }

    /// Remove bindings matching an input/mode/path filter.
    pub fn unbind_input(
        &mut self,
        input: InputSpec,
        filter: BindingFilter<'_>,
    ) -> Vec<(BindingId, LuauFunctionId)> {
        let mut removed = Vec::new();

        if let Some(mode) = filter.mode {
            if let Some(entry) = self.modes.get_mut(mode) {
                removed.extend(entry.unbind_input(input, filter.path_filter));
            }
        } else {
            for entry in self.modes.values_mut() {
                removed.extend(entry.unbind_input(input, filter.path_filter));
            }
        }

        self.modes
            .retain(|mode, actions| mode == DEFAULT_MODE || !actions.inputs.is_empty());
        removed
    }

    /// Replace any bindings matching an input/mode/path filter, then insert the new binding.
    pub fn replace_binding(
        &mut self,
        mode: &str,
        input: InputSpec,
        path_filter: &str,
        target: LuauFunctionId,
    ) -> Result<(BindingId, Vec<(BindingId, LuauFunctionId)>)> {
        let pathmatch = PathMatcher::new(path_filter)?;
        let id = BindingId::next(&mut self.next_id)?;
        let removed = self.unbind_input(
            input,
            BindingFilter {
                mode: Some(mode),
                path_filter: Some(path_filter),
            },
        );
        self.modes
            .entry(mode.to_string())
            .or_insert_with(InputMode::new)
            .insert(id, pathmatch, input, target);
        Ok((id, removed))
    }

    /// Remove every binding from every mode.
    pub fn clear(&mut self) -> Vec<(BindingId, LuauFunctionId)> {
        let mut removed = Vec::new();
        for mode in self.modes.values_mut() {
            removed.extend(mode.clear());
        }
        self.modes.retain(|mode, _| mode == DEFAULT_MODE);
        self.current_mode = DEFAULT_MODE.to_string();
        self.mode_stack.clear();
        removed
    }

    /// Remove every Luau-backed binding while preserving command and mode bindings.
    pub(crate) fn remove_luau_functions(&mut self) -> Vec<(BindingId, LuauFunctionId)> {
        let mut removed = Vec::new();
        for mode in self.modes.values_mut() {
            removed.extend(mode.remove_luau_functions());
        }
        self.modes
            .retain(|mode, actions| mode == DEFAULT_MODE || !actions.inputs.is_empty());
        removed
    }

    /// Return every binding ID currently stored in the map.
    pub(crate) fn binding_ids(&self) -> HashSet<BindingId> {
        self.modes
            .values()
            .flat_map(|mode| mode.inputs.values())
            .flatten()
            .map(|action| action.id)
            .collect()
    }

    /// Clone targets owned by bindings absent from a baseline ID set.
    pub(crate) fn targets_not_in(&self, baseline: &HashSet<BindingId>) -> Vec<LuauFunctionId> {
        self.modes
            .values()
            .flat_map(|mode| mode.inputs.values())
            .flatten()
            .filter(|action| !baseline.contains(&action.id))
            .map(|action| action.action)
            .collect()
    }

    /// Return the name of the current input mode.
    pub fn current_mode(&self) -> &str {
        &self.current_mode
    }

    /// Return all bindings across all modes.
    pub fn bindings(&self) -> Vec<ModeBindingInfo<'_>> {
        let mut modes = self.modes.iter().collect::<Vec<_>>();
        modes.sort_by_key(|(left, _)| *left);
        let mut out = Vec::new();
        for (mode, bindings) in modes {
            for info in bindings.bindings() {
                out.push(ModeBindingInfo { mode, info });
            }
        }
        out
    }

    /// Return bindings in a mode that match a specific path.
    pub fn bindings_matching_path(&self, mode: &str, path: &Path) -> Vec<MatchedBindingInfo<'_>> {
        self.modes
            .get(mode)
            .map(|m| m.bindings_for_path(path))
            .unwrap_or_default()
    }

    /// Replace the next binding identifier for deterministic exhaustion tests.
    #[cfg(test)]
    pub(crate) fn replace_next_id(&mut self, next_id: u64) -> u64 {
        let previous = self.next_id;
        self.next_id = next_id;
        previous
    }
}

/// Metadata about a single input binding.
#[derive(Debug, Clone)]
pub struct BindingInfo<'a> {
    /// Binding identifier.
    pub id: BindingId,
    /// Input that triggers this binding.
    pub input: InputSpec,
    /// Original path filter string (e.g., "editor/*").
    pub path_filter: &'a str,
    /// Target action (script, command, command sequence, or Luau closure).
    pub target: LuauFunctionId,
}

/// Binding info with match metadata.
#[derive(Debug, Clone)]
pub struct MatchedBindingInfo<'a> {
    /// The binding info.
    pub info: BindingInfo<'a>,
    /// Match metadata from the path matcher.
    pub m: PathMatch,
}

/// Binding info annotated with the mode it belongs to.
#[derive(Debug, Clone)]
pub struct ModeBindingInfo<'a> {
    /// Input mode name.
    pub mode: &'a str,
    /// Binding metadata.
    pub info: BindingInfo<'a>,
}

/// Tests for the input map.
#[cfg(test)]
mod tests;
