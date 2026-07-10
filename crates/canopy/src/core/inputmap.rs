use std::{
    collections::{HashMap, HashSet},
    fmt, iter,
};

use crate::{
    commands::CommandInvocation,
    error::{Error, Result},
    event::{
        key::Key,
        mouse::{self, Mouse},
    },
    path::*,
    script,
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
    action: BindingTarget,
}

/// Binding match priority. Higher values win; later insertion wins exact ties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct BindingPriority {
    /// Count of literal path segments.
    literals: usize,
    /// Whether the match ends at the focused path terminus.
    anchored_end: bool,
    /// Number of path components matched.
    depth: usize,
    /// Insertion position for stable replacement on otherwise equal matches.
    insertion_order: usize,
}

impl BindingPriority {
    /// Build priority from path match metadata and insertion order.
    fn new(path_match: PathMatch, insertion_order: usize) -> Self {
        Self {
            literals: path_match.literals,
            anchored_end: path_match.anchored_end,
            depth: path_match.depth,
            insertion_order,
        }
    }
}

/// Tuple storing a binding match and its score.
type BindingCandidate = (BindingPriority, BindingTarget, PathMatch);

/// Binding mode/path filter used when removing or replacing bindings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BindingFilter<'a> {
    /// Optional mode name to match.
    pub mode: Option<&'a str>,
    /// Optional exact path filter string to match.
    pub path_filter: Option<&'a str>,
}

/// A resolved input binding target.
#[derive(Debug, Clone, PartialEq)]
pub enum BindingTarget {
    /// Script identifier to execute.
    Script(script::ScriptId),
    /// Direct command invocation.
    Command(CommandInvocation),
    /// Sequence of commands executed in order.
    CommandSequence(Vec<CommandInvocation>),
    /// Switch to another input mode.
    SetInputMode(String),
    /// Stored Luau closure owned by the script host.
    LuauFunction(script::LuauFunctionId),
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
        action: BindingTarget,
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
    ) -> Option<(BindingTarget, PathMatch)> {
        let input = input.normalize();
        let mut best: Option<BindingCandidate> = None;
        for (idx, k) in self.inputs.get(&input)?.iter().enumerate() {
            if let Some(m) = k.pathmatch.check_match(path) {
                let score = BindingPriority::new(m, idx);
                let replace = match best {
                    Some((best_score, _, _)) => score > best_score,
                    None => true,
                };
                if replace {
                    best = Some((score, k.action.clone(), m));
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
                    target: &a.action,
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
                            target: &a.action,
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
    fn unbind_with_targets(&mut self, id: BindingId) -> Vec<BindingTarget> {
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
    ) -> Vec<(BindingId, BindingTarget)> {
        let input = input.normalize();
        let Some(actions) = self.inputs.get_mut(&input) else {
            return Vec::new();
        };

        let mut removed = Vec::new();
        actions.retain(|action| {
            let matches = path_filter.is_none_or(|filter| action.pathmatch.filter() == filter);
            if matches {
                removed.push((action.id, action.action.clone()));
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
    fn clear(&mut self) -> Vec<(BindingId, BindingTarget)> {
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
    fn remove_luau_functions(&mut self) -> Vec<(BindingId, BindingTarget)> {
        let mut removed = Vec::new();
        for actions in self.inputs.values_mut() {
            actions.retain(|action| {
                if matches!(action.action, BindingTarget::LuauFunction(_)) {
                    removed.push((action.id, action.action.clone()));
                    false
                } else {
                    true
                }
            });
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
    pub fn mode_stack(&self) -> &[String] {
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
    ) -> Option<(BindingTarget, PathMatch)> {
        self.resolve_in_modes(|mode| mode.resolve_match(path, input))
    }

    /// Bind a key or mouse input to switch the active input mode.
    ///
    /// The input is normalized before storing.
    ///
    /// Returns the new binding ID.
    pub fn bind_input_mode(
        &mut self,
        mode: &str,
        input: InputSpec,
        path_filter: &str,
        next_mode: &str,
    ) -> Result<BindingId> {
        self.bind_action(
            mode,
            input,
            path_filter,
            BindingTarget::SetInputMode(next_mode.to_string()),
        )
    }

    /// Store a key binding action for a mode and path filter.
    ///
    /// Returns the new binding ID.
    fn bind_action(
        &mut self,
        mode: &str,
        input: InputSpec,
        path_filter: &str,
        action: BindingTarget,
    ) -> Result<BindingId> {
        let pathmatch = PathMatcher::new(path_filter)?;
        validate_binding_target(&action)?;
        let id = BindingId::next(&mut self.next_id)?;
        self.modes
            .entry(mode.to_string())
            .or_insert_with(InputMode::new)
            .insert(id, pathmatch, input, action);
        Ok(id)
    }

    /// Remove a binding by ID and return removed targets.
    pub fn unbind_with_targets(&mut self, id: BindingId) -> Vec<BindingTarget> {
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
    ) -> Vec<(BindingId, BindingTarget)> {
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
        target: BindingTarget,
    ) -> Result<(BindingId, Vec<(BindingId, BindingTarget)>)> {
        let pathmatch = PathMatcher::new(path_filter)?;
        validate_binding_target(&target)?;
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
    pub fn clear(&mut self) -> Vec<(BindingId, BindingTarget)> {
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
    pub(crate) fn remove_luau_functions(&mut self) -> Vec<(BindingId, BindingTarget)> {
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
    pub(crate) fn targets_not_in(&self, baseline: &HashSet<BindingId>) -> Vec<BindingTarget> {
        self.modes
            .values()
            .flat_map(|mode| mode.inputs.values())
            .flatten()
            .filter(|action| !baseline.contains(&action.id))
            .map(|action| action.action.clone())
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

    /// Replace the next binding identifier for deterministic exhaustion tests.
    #[cfg(test)]
    pub(crate) fn replace_next_id(&mut self, next_id: u64) -> u64 {
        let previous = self.next_id;
        self.next_id = next_id;
        previous
    }
}

/// Validate a binding target before mutating the input map.
fn validate_binding_target(target: &BindingTarget) -> Result<()> {
    let valid_command = |command: &CommandInvocation| !command.id.0.is_empty();
    let valid = match target {
        BindingTarget::Script(id) => *id != 0,
        BindingTarget::Command(command) => valid_command(command),
        BindingTarget::CommandSequence(commands) => {
            !commands.is_empty() && commands.iter().all(valid_command)
        }
        BindingTarget::SetInputMode(_) | BindingTarget::LuauFunction(_) => true,
    };
    if valid {
        Ok(())
    } else {
        Err(Error::Invalid("invalid input binding target".to_string()))
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

/// Binding info annotated with the mode it belongs to.
#[derive(Debug, Clone)]
pub struct ModeBindingInfo<'a> {
    /// Input mode name.
    pub mode: &'a str,
    /// Binding metadata.
    pub info: BindingInfo<'a>,
}

#[cfg(test)]
mod tests {
    use proptest::{prelude::*, test_runner::TestCaseResult};

    use super::*;
    use crate::{core::testing::model::trace_result, error::Result, event::key, script};

    #[derive(Clone, Debug)]
    enum RegistryOperation {
        Bind {
            mode: u8,
            key: u8,
            path: u8,
            target: u8,
        },
        Replace {
            mode: u8,
            key: u8,
            path: u8,
            target: u8,
        },
        Unbind(u8),
        UnbindInput {
            key: u8,
            mode: Option<u8>,
            path: Option<u8>,
        },
        SetMode(u8),
        PushMode(u8),
        PopMode,
        Clear,
        ExhaustIdentifier,
    }

    #[derive(Clone, Debug, PartialEq)]
    struct ModelBinding {
        id: BindingId,
        mode: String,
        input: InputSpec,
        path: String,
        target: BindingTarget,
    }

    #[derive(Debug)]
    struct RegistryModel {
        bindings: Vec<ModelBinding>,
        mode_stack: Vec<String>,
        next_id: u64,
    }

    impl RegistryModel {
        fn new() -> Self {
            Self {
                bindings: Vec::new(),
                mode_stack: Vec::new(),
                next_id: 1,
            }
        }
    }

    fn registry_operation_strategy() -> impl Strategy<Value = Vec<RegistryOperation>> {
        prop::collection::vec(
            prop_oneof![
                (0_u8..3, 0_u8..3, 0_u8..4, 0_u8..3).prop_map(|(mode, key, path, target)| {
                    RegistryOperation::Bind {
                        mode,
                        key,
                        path,
                        target,
                    }
                },),
                (0_u8..3, 0_u8..3, 0_u8..4, 0_u8..3).prop_map(|(mode, key, path, target)| {
                    RegistryOperation::Replace {
                        mode,
                        key,
                        path,
                        target,
                    }
                },),
                (0_u8..12).prop_map(RegistryOperation::Unbind),
                (
                    0_u8..3,
                    prop::option::of(0_u8..3),
                    prop::option::of(0_u8..3)
                )
                    .prop_map(|(key, mode, path)| RegistryOperation::UnbindInput {
                        key,
                        mode,
                        path
                    },),
                (0_u8..3).prop_map(RegistryOperation::SetMode),
                (0_u8..3).prop_map(RegistryOperation::PushMode),
                Just(RegistryOperation::PopMode),
                Just(RegistryOperation::Clear),
                Just(RegistryOperation::ExhaustIdentifier),
            ],
            1..80,
        )
    }

    fn model_mode(index: u8) -> &'static str {
        ["", "normal", "modal"][usize::from(index) % 3]
    }

    fn model_input(index: u8) -> InputSpec {
        InputSpec::Key(['a', 'b', 'c'][usize::from(index) % 3].into())
    }

    fn model_path(index: u8) -> &'static str {
        ["", "foo", "foo/**", "invalid-name"][usize::from(index) % 4]
    }

    fn model_target(index: u8) -> BindingTarget {
        match index % 3 {
            0 => BindingTarget::Script(u64::from(index) + 1),
            1 => BindingTarget::SetInputMode(model_mode(index).to_string()),
            _ => BindingTarget::Script(0),
        }
    }

    fn registry_signature(map: &InputMap) -> Vec<ModelBinding> {
        let mut bindings = map
            .bindings()
            .into_iter()
            .map(|binding| ModelBinding {
                id: binding.info.id,
                mode: binding.mode.to_string(),
                input: binding.info.input,
                path: binding.info.path_filter.to_string(),
                target: binding.info.target.clone(),
            })
            .collect::<Vec<_>>();
        bindings.sort_by_key(|binding| binding.id.as_u64());
        bindings
    }

    fn assert_registry_model(map: &InputMap, model: &RegistryModel) -> TestCaseResult {
        let mut expected = model.bindings.clone();
        expected.sort_by_key(|binding| binding.id.as_u64());
        prop_assert_eq!(registry_signature(map), expected);
        prop_assert_eq!(map.mode_stack(), model.mode_stack.as_slice());
        prop_assert_eq!(
            map.current_mode(),
            model.mode_stack.last().map_or("", String::as_str)
        );
        prop_assert_eq!(map.next_id, model.next_id);
        Ok(())
    }

    fn apply_registry_operation(
        map: &mut InputMap,
        model: &mut RegistryModel,
        operation: &RegistryOperation,
    ) -> TestCaseResult {
        match *operation {
            RegistryOperation::Bind {
                mode,
                key,
                path,
                target,
            } => {
                let mode = model_mode(mode);
                let input = model_input(key);
                let path = model_path(path);
                let target = model_target(target);
                let result = map.bind_action(mode, input, path, target.clone());
                let valid = path != "invalid-name" && !matches!(target, BindingTarget::Script(0));
                prop_assert_eq!(result.is_ok(), valid);
                if let Ok(id) = result {
                    model.bindings.push(ModelBinding {
                        id,
                        mode: mode.to_string(),
                        input,
                        path: path.to_string(),
                        target,
                    });
                    model.next_id += 1;
                }
            }
            RegistryOperation::Replace {
                mode,
                key,
                path,
                target,
            } => {
                let mode = model_mode(mode);
                let input = model_input(key);
                let path = model_path(path);
                let target = model_target(target);
                let result = map.replace_binding(mode, input, path, target.clone());
                let valid = path != "invalid-name" && !matches!(target, BindingTarget::Script(0));
                prop_assert_eq!(result.is_ok(), valid);
                if let Ok((id, _)) = result {
                    model.bindings.retain(|binding| {
                        binding.mode != mode || binding.input != input || binding.path != path
                    });
                    model.bindings.push(ModelBinding {
                        id,
                        mode: mode.to_string(),
                        input,
                        path: path.to_string(),
                        target,
                    });
                    model.next_id += 1;
                }
            }
            RegistryOperation::Unbind(index) => {
                let id = model
                    .bindings
                    .get(usize::from(index) % model.bindings.len().max(1))
                    .map_or(BindingId::from_u64(u64::MAX - 1), |binding| binding.id);
                let removed = map.unbind_with_targets(id);
                let before = model.bindings.len();
                model.bindings.retain(|binding| binding.id != id);
                prop_assert_eq!(!removed.is_empty(), before != model.bindings.len());
            }
            RegistryOperation::UnbindInput { key, mode, path } => {
                let input = model_input(key);
                let mode = mode.map(model_mode);
                let path = path.map(model_path);
                map.unbind_input(
                    input,
                    BindingFilter {
                        mode,
                        path_filter: path,
                    },
                );
                model.bindings.retain(|binding| {
                    binding.input != input
                        || mode.is_some_and(|mode| binding.mode != mode)
                        || path.is_some_and(|path| binding.path != path)
                });
            }
            RegistryOperation::SetMode(mode) => {
                let mode = model_mode(mode);
                map.set_mode(mode)?;
                model.mode_stack.clear();
                if !mode.is_empty() {
                    model.mode_stack.push(mode.to_string());
                }
            }
            RegistryOperation::PushMode(mode) => {
                let mode = model_mode(mode);
                map.push_mode(mode)?;
                if !mode.is_empty() {
                    model.mode_stack.push(mode.to_string());
                }
            }
            RegistryOperation::PopMode => {
                map.pop_mode();
                model.mode_stack.pop();
            }
            RegistryOperation::Clear => {
                map.clear();
                model.bindings.clear();
                model.mode_stack.clear();
            }
            RegistryOperation::ExhaustIdentifier => {
                let before = registry_signature(map);
                map.next_id = u64::MAX;
                let result =
                    map.bind_action("", InputSpec::Key('z'.into()), "", BindingTarget::Script(1));
                prop_assert!(result.is_err());
                prop_assert_eq!(registry_signature(map), before);
                map.next_id = model.next_id;
            }
        }
        assert_registry_model(map, model)
    }

    proptest! {
        #[test]
        fn input_registry_state_machine_matches_model(operations in registry_operation_strategy()) {
            let mut map = InputMap::new();
            let mut model = RegistryModel::new();
            for (index, operation) in operations.iter().enumerate() {
                trace_result(
                    apply_registry_operation(&mut map, &mut model, operation),
                    &operations,
                    index,
                )?;
            }
        }
    }

    trait ResolveTarget {
        fn resolve(&self, path: &Path, input: &InputSpec) -> Option<BindingTarget>;
    }

    impl ResolveTarget for InputMode {
        fn resolve(&self, path: &Path, input: &InputSpec) -> Option<BindingTarget> {
            self.resolve_match(path, input).map(|(target, _)| target)
        }
    }

    impl ResolveTarget for InputMap {
        fn resolve(&self, path: &Path, input: &InputSpec) -> Option<BindingTarget> {
            self.resolve_match(path, input).map(|(target, _)| target)
        }
    }

    trait BindScript {
        fn bind(
            &mut self,
            mode: &str,
            input: InputSpec,
            path_filter: &str,
            script: script::ScriptId,
        ) -> Result<BindingId>;
    }

    impl BindScript for InputMap {
        fn bind(
            &mut self,
            mode: &str,
            input: InputSpec,
            path_filter: &str,
            script: script::ScriptId,
        ) -> Result<BindingId> {
            self.bind_action(mode, input, path_filter, BindingTarget::Script(script))
        }
    }

    trait Unbind {
        fn unbind(&mut self, id: BindingId) -> bool;
    }

    impl Unbind for InputMap {
        fn unbind(&mut self, id: BindingId) -> bool {
            !self.unbind_with_targets(id).is_empty()
        }
    }

    #[test]
    fn replacement_errors_preserve_the_old_binding() -> Result<()> {
        let mut map = InputMap::new();
        let input = InputSpec::Key('a'.into());
        let original = map.bind("", input, "", 1)?;

        assert!(
            map.replace_binding("", input, "invalid-name", BindingTarget::Script(2))
                .is_err()
        );
        assert_eq!(
            map.resolve(&Path::empty(), &input),
            Some(BindingTarget::Script(1))
        );
        assert_eq!(map.bindings()[0].info.id, original);

        assert!(
            map.replace_binding("", input, "", BindingTarget::Script(0))
                .is_err()
        );
        assert_eq!(
            map.resolve(&Path::empty(), &input),
            Some(BindingTarget::Script(1))
        );

        map.next_id = u64::MAX;
        assert!(
            map.replace_binding("", input, "", BindingTarget::Script(2))
                .is_err()
        );
        assert_eq!(
            map.resolve(&Path::empty(), &input),
            Some(BindingTarget::Script(1))
        );
        Ok(())
    }

    #[test]
    fn binding_views_are_independent_of_insertion_order() -> Result<()> {
        fn build(inputs: [char; 2]) -> Result<InputMap> {
            let mut map = InputMap::new();
            for input in inputs {
                map.bind_input_mode("", InputSpec::Key(input.into()), "", &input.to_string())?;
            }
            Ok(map)
        }

        let forward = build(['a', 'b'])?;
        let reverse = build(['b', 'a'])?;
        let signature = |map: &InputMap| {
            map.bindings()
                .into_iter()
                .map(|binding| (binding.mode.to_string(), binding.info.input.to_string()))
                .collect::<Vec<_>>()
        };
        assert_eq!(signature(&forward), signature(&reverse));
        assert_eq!(
            signature(&forward),
            [
                ("".to_string(), "a".to_string()),
                ("".to_string(), "b".to_string())
            ]
        );

        let matched = |map: &InputMap| {
            map.bindings_matching_path("", &Path::empty())
                .into_iter()
                .map(|binding| binding.info.input.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(matched(&forward), matched(&reverse));
        Ok(())
    }

    #[test]
    fn caseconfusion() -> Result<()> {
        let e = script::LuauHost::new();
        let mut m = InputMode::new();
        let a_foo = e.compile("x()")?;

        m.insert(
            BindingId(1),
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
        let e = script::LuauHost::new();

        let mut m = InputMode::new();
        let a_foo = e.compile("x()")?;
        let a_bar = e.compile("x()")?;
        let b = e.compile("x()")?;
        m.insert(
            BindingId(1),
            PathMatcher::new("foo")?,
            InputSpec::Key('a'.into()),
            BindingTarget::Script(a_foo),
        );
        m.insert(
            BindingId(2),
            PathMatcher::new("bar")?,
            InputSpec::Key('a'.into()),
            BindingTarget::Script(a_bar),
        );
        m.insert(
            BindingId(3),
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
        let e = script::LuauHost::new();

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
    fn mode_stack_resolves_top_to_bottom_then_default() -> Result<()> {
        let mut m = InputMap::new();
        let e = script::LuauHost::new();
        let a_default = e.compile("default()")?;
        let a_normal = e.compile("normal()")?;
        let a_modal = e.compile("modal()")?;
        let b_normal = e.compile("normal_b()")?;
        let c_default = e.compile("default_c()")?;

        m.bind("", InputSpec::Key('a'.into()), "", a_default)?;
        m.bind("normal", InputSpec::Key('a'.into()), "", a_normal)?;
        m.bind("modal", InputSpec::Key('a'.into()), "", a_modal)?;
        m.bind("normal", InputSpec::Key('b'.into()), "", b_normal)?;
        m.bind("", InputSpec::Key('c'.into()), "", c_default)?;

        m.push_mode("normal")?;
        m.push_mode("modal")?;
        assert_eq!(m.current_mode(), "modal");
        assert_eq!(m.mode_stack(), &["normal".to_string(), "modal".to_string()]);
        assert_eq!(m.active_modes(), vec!["modal", "normal", ""]);
        assert_eq!(
            m.resolve(&"foo".into(), &InputSpec::Key('a'.into()))
                .unwrap(),
            BindingTarget::Script(a_modal)
        );
        assert_eq!(
            m.resolve(&"foo".into(), &InputSpec::Key('b'.into()))
                .unwrap(),
            BindingTarget::Script(b_normal)
        );
        assert_eq!(
            m.resolve(&"foo".into(), &InputSpec::Key('c'.into()))
                .unwrap(),
            BindingTarget::Script(c_default)
        );

        assert_eq!(m.pop_mode(), "normal");
        assert_eq!(
            m.resolve(&"foo".into(), &InputSpec::Key('a'.into()))
                .unwrap(),
            BindingTarget::Script(a_normal)
        );
        assert_eq!(m.pop_mode(), "");
        assert_eq!(
            m.resolve(&"foo".into(), &InputSpec::Key('a'.into()))
                .unwrap(),
            BindingTarget::Script(a_default)
        );

        Ok(())
    }

    #[test]
    fn layered_modes_fall_back_to_default() -> Result<()> {
        let mut m = InputMap::new();
        let e = script::LuauHost::new();
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
    fn missing_active_mode_falls_back_to_default() -> Result<()> {
        let mut m = InputMap::new();
        let e = script::LuauHost::new();
        let a_default = e.compile("x()")?;
        m.bind("", InputSpec::Key('b'.into()), "", a_default)?;
        m.current_mode = "missing".to_string();

        assert_eq!(
            m.resolve(&"foo".into(), &InputSpec::Key('b'.into()))
                .unwrap(),
            BindingTarget::Script(a_default)
        );
        assert!(
            m.resolve_match(&"foo".into(), &InputSpec::Key('x'.into()))
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn unbind_removes_binding() -> Result<()> {
        let mut m = InputMap::new();
        let e = script::LuauHost::new();
        let a_default = e.compile("x()")?;
        let id = m.bind("", InputSpec::Key('a'.into()), "", a_default)?;

        assert!(m.unbind(id));
        assert!(!m.unbind(id));
        assert!(
            m.resolve(&"foo".into(), &InputSpec::Key('a'.into()))
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn binding_precedence_prefers_anchored_end() -> Result<()> {
        let mut m = InputMode::new();
        let e = script::LuauHost::new();
        let a_loose = e.compile("x()")?;
        let a_anchor = e.compile("x()")?;

        m.insert(
            BindingId(1),
            PathMatcher::new("foo")?,
            InputSpec::Key('a'.into()),
            BindingTarget::Script(a_loose),
        );
        m.insert(
            BindingId(2),
            PathMatcher::new("bar")?,
            InputSpec::Key('a'.into()),
            BindingTarget::Script(a_anchor),
        );

        assert_eq!(
            m.resolve(&"/foo/bar".into(), &InputSpec::Key('a'.into()))
                .unwrap(),
            BindingTarget::Script(a_anchor)
        );
        Ok(())
    }

    #[test]
    fn binding_precedence_prefers_depth_when_literals_equal() -> Result<()> {
        let mut m = InputMode::new();
        let e = script::LuauHost::new();
        let a_shallow = e.compile("x()")?;
        let a_deep = e.compile("x()")?;

        m.insert(
            BindingId(1),
            PathMatcher::new("bar/**")?,
            InputSpec::Key('a'.into()),
            BindingTarget::Script(a_shallow),
        );
        m.insert(
            BindingId(2),
            PathMatcher::new("foo/**")?,
            InputSpec::Key('a'.into()),
            BindingTarget::Script(a_deep),
        );

        assert_eq!(
            m.resolve(&"/foo/bar/baz".into(), &InputSpec::Key('a'.into()))
                .unwrap(),
            BindingTarget::Script(a_deep)
        );
        Ok(())
    }

    #[test]
    fn binding_precedence_prefers_insertion_order_on_tie() -> Result<()> {
        let mut m = InputMode::new();
        let e = script::LuauHost::new();
        let a_first = e.compile("x()")?;
        let a_last = e.compile("x()")?;

        m.insert(
            BindingId(1),
            PathMatcher::new("bar/foo")?,
            InputSpec::Key('a'.into()),
            BindingTarget::Script(a_first),
        );
        m.insert(
            BindingId(2),
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
