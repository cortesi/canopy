#[cfg(test)]
use std::mem;
use std::{cmp::Ordering, collections::HashSet, fmt};

use crate::{
    ModalBindings,
    commands::CommandAction,
    core::NodeId,
    error::{Error, Result},
    event::{key::Key, mouse::Mouse},
    path::{Path, PathFilter, PathMatch},
    script::LuauFunctionId,
};

/// Default input mode name.
const DEFAULT_MODE: &str = "";

/// Monotonic identifier for a binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingId(u64);

impl BindingId {
    /// Return the numeric binding identifier.
    pub fn as_u64(self) -> u64 {
        self.0
    }

    /// Reconstruct a binding identifier from its numeric form.
    pub fn from_u64(id: u64) -> Self {
        Self(id)
    }
}

/// Stable name for one framework-owned binding group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameworkBindingGroup(&'static str);

impl FrameworkBindingGroup {
    /// Construct a framework binding group.
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    /// Return the diagnostic group name.
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for FrameworkBindingGroup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

/// Opaque token for one active exclusive binding frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExclusiveFrameToken(u64);

impl ExclusiveFrameToken {
    /// Construct a placeholder token for test contexts.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) const fn for_test(id: u64) -> Self {
        Self(id)
    }
}

/// Owner of one binding record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindingOwner {
    /// Application-owned binding that script APIs can mutate.
    Application,
    /// Framework-owned binding in a private group.
    Framework(FrameworkBindingGroup),
}

/// Resolution scope for one binding.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BindingScope {
    /// Highest-priority application tier.
    Global,
    /// Named application mode.
    Mode(String),
    /// Default application mode.
    Default,
    /// Framework-only exclusive group.
    Exclusive(FrameworkBindingGroup),
}

impl BindingScope {
    /// Return the named mode, if this is a mode scope.
    pub fn mode(&self) -> Option<&str> {
        match self {
            Self::Mode(mode) => Some(mode),
            _ => None,
        }
    }

    /// Return a stable scripting and diagnostic label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Mode(_) => "mode",
            Self::Default => "default",
            Self::Exclusive(_) => "exclusive",
        }
    }
}

/// Options shared by native and scripted application bindings.
#[derive(Clone, Debug)]
pub struct BindingOptions {
    /// Optional validated path selector. Omission matches the current route.
    pub path: Option<PathFilter>,
    /// Application scope and optional named mode.
    pub scope: BindingScope,
    /// Required user-facing description.
    pub description: String,
    /// Optional diagnostic source.
    pub source: Option<String>,
    /// Explicit routing phase, or the legacy selector-derived phase.
    pub phase: Option<BindingPhase>,
}

/// Action executed by a binding.
#[derive(Clone, Debug, PartialEq)]
pub enum BindingTarget {
    /// Stored Luau callback.
    Script(LuauFunctionId),
    /// Rust command invocation.
    Command(CommandAction),
}

impl BindingTarget {
    /// Return a stable target-kind label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Script(_) => "script",
            Self::Command(_) => "command",
        }
    }
}

/// One complete binding record used by routing and introspection.
#[derive(Clone, Debug)]
pub struct BindingRecord {
    /// Stable binding identifier.
    pub id: BindingId,
    /// Normalized input selector.
    pub input: InputSpec,
    /// Record owner.
    pub owner: BindingOwner,
    /// Resolution scope.
    pub scope: BindingScope,
    /// Required user-facing description.
    pub description: String,
    /// Optional diagnostic source.
    pub source: Option<String>,
    /// Explicit phase override, if supplied at registration.
    pub phase: Option<BindingPhase>,
    /// Binding target.
    pub target: BindingTarget,
    /// Monotonic insertion order.
    pub insertion_id: u64,
    /// Compiled path matcher and its original filter.
    path_matcher: PathFilter,
}

impl BindingRecord {
    /// Return the original path filter.
    pub fn path_filter(&self) -> &str {
        self.path_matcher.as_str()
    }
}

/// Binding phase relative to widget input handling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingPhase {
    /// Execute before the focused widget.
    BeforeWidget,
    /// Execute only after the widget ignores the input.
    AfterIgnore,
}

/// Winner returned by the shared resolver.
#[derive(Clone, Debug)]
pub struct ResolvedBinding {
    /// Binding identifier.
    pub id: BindingId,
    /// Target to execute.
    pub target: BindingTarget,
    /// Routing phase.
    pub phase: BindingPhase,
    /// User-facing description.
    pub description: String,
}

/// Binding selector used by application mutation APIs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BindingSelector<'a> {
    /// Optional scope to match.
    pub scope: Option<BindingScope>,
    /// Optional exact path filter string to match.
    pub path_filter: Option<&'a str>,
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
    pub fn normalize(self) -> Self {
        match self {
            Self::Mouse(mouse) => Self::Mouse(mouse),
            Self::Key(key) => Self::Key(key.normalize()),
        }
    }
}

impl From<Key> for InputSpec {
    fn from(key: Key) -> Self {
        Self::Key(key)
    }
}

impl From<char> for InputSpec {
    fn from(key: char) -> Self {
        Self::Key(key.into())
    }
}

impl From<Mouse> for InputSpec {
    fn from(mouse: Mouse) -> Self {
        Self::Mouse(mouse)
    }
}

impl fmt::Display for InputSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Key(key) => write!(formatter, "{key}"),
            Self::Mouse(mouse) => write!(formatter, "{mouse}"),
        }
    }
}

/// Application-owned state restored after a failed startup script.
#[derive(Clone, Debug)]
pub struct ApplicationBindingSnapshot {
    /// Application records captured at the start of an attempt.
    records: Vec<BindingRecord>,
    /// Application mode stack captured at the start of an attempt.
    mode_stack: Vec<String>,
}

/// One active exclusive binding frame.
#[derive(Clone, Copy, Debug)]
struct ExclusiveFrame {
    /// Unique token used for ordered removal.
    token: ExclusiveFrameToken,
    /// Framework group admitted by this frame.
    group: FrameworkBindingGroup,
    /// Node that owns the frame.
    owner: NodeId,
}

/// Registry for application bindings, framework controls, and active modes.
#[derive(Clone, Debug)]
pub struct InputMap {
    /// Flat application and framework binding records.
    records: Vec<BindingRecord>,
    /// Active application modes in push order.
    mode_stack: Vec<String>,
    /// Active exclusive framework frames in push order.
    exclusive_frames: Vec<ExclusiveFrame>,
    /// Admission imposed by the active modal, overriding legacy frames.
    modal_bindings: Option<ModalBindings>,
    /// Next binding identifier.
    next_id: u64,
    /// Next insertion-order identifier.
    next_insertion_id: u64,
    /// Next exclusive-frame token.
    next_token: u64,
}

impl Default for InputMap {
    fn default() -> Self {
        Self::new()
    }
}

impl InputMap {
    /// Construct an empty binding registry.
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            mode_stack: Vec::new(),
            exclusive_frames: Vec::new(),
            modal_bindings: None,
            next_id: 1,
            next_insertion_id: 1,
            next_token: 1,
        }
    }

    /// Store or replace an application action, preserving its explicit phase.
    pub fn replace_application_action(
        &mut self,
        input: InputSpec,
        options: BindingOptions,
        target: BindingTarget,
    ) -> Result<(BindingId, Vec<(BindingId, BindingTarget)>)> {
        let path_filter = options.path.as_ref().map_or("", PathFilter::as_str);
        validate_application_scope(&options.scope, path_filter)?;
        validate_description(&options.description)?;
        validate_phase(input, options.phase)?;
        let path_matcher = options.path.clone().unwrap_or(PathFilter::new("")?);
        let id = self.allocate_binding_id()?;
        let insertion_id = self.allocate_insertion_id()?;
        let input = input.normalize();
        let removed = self.unbind_input(
            input,
            &BindingSelector {
                scope: Some(options.scope.clone()),
                path_filter: Some(path_filter),
            },
        );
        self.records.push(BindingRecord {
            id,
            input,
            owner: BindingOwner::Application,
            scope: options.scope,
            description: options.description,
            source: options.source,
            phase: options.phase,
            target,
            insertion_id,
            path_matcher,
        });
        Ok((id, removed))
    }

    /// Store one idempotent framework binding.
    pub fn bind_framework(
        &mut self,
        group: FrameworkBindingGroup,
        input: impl Into<InputSpec>,
        options: BindingOptions,
        command: CommandAction,
    ) -> Result<BindingId> {
        let input = input.into();
        validate_description(&options.description)?;
        validate_phase(input, options.phase)?;
        let scope = BindingScope::Exclusive(group);
        if options.scope != scope {
            return Err(Error::InvalidOperation(
                "framework binding scope must match its exclusive group".to_string(),
            ));
        }
        let path_filter = options.path.as_ref().map_or("", PathFilter::as_str);
        let path_matcher = options.path.clone().unwrap_or(PathFilter::new("")?);
        let input = input.normalize();
        if let Some(existing) = self.records.iter().find(|record| {
            record.owner == BindingOwner::Framework(group)
                && record.input == input
                && record.path_filter() == path_filter
        }) {
            if existing.scope == scope
                && existing.description == options.description
                && existing.phase == options.phase
                && existing.source == options.source
                && existing.target == BindingTarget::Command(command)
            {
                return Ok(existing.id);
            }
            return Err(Error::InvalidOperation(format!(
                "conflicting framework binding for {group}, {input}, and {path_filter}"
            )));
        }
        let id = self.allocate_binding_id()?;
        let insertion_id = self.allocate_insertion_id()?;
        self.records.push(BindingRecord {
            id,
            input,
            owner: BindingOwner::Framework(group),
            scope,
            description: options.description,
            source: options.source,
            phase: options.phase,
            target: BindingTarget::Command(command),
            insertion_id,
            path_matcher,
        });
        Ok(id)
    }

    /// Remove one application binding.
    pub fn unbind(&mut self, id: BindingId) -> Result<Option<BindingTarget>> {
        let Some(index) = self.records.iter().position(|record| record.id == id) else {
            return Ok(None);
        };
        if !matches!(self.records[index].owner, BindingOwner::Application) {
            return Err(Error::InvalidOperation(format!(
                "binding {} is framework-owned",
                id.as_u64()
            )));
        }
        let record = self.records.remove(index);
        Ok(Some(record.target))
    }

    /// Remove application bindings for an input and selector.
    pub fn unbind_input(
        &mut self,
        input: InputSpec,
        selector: &BindingSelector<'_>,
    ) -> Vec<(BindingId, BindingTarget)> {
        let input = input.normalize();
        self.remove_application_records(|record| {
            record.input == input
                && selector
                    .scope
                    .as_ref()
                    .is_none_or(|scope| record.scope == *scope)
                && selector
                    .path_filter
                    .is_none_or(|path| record.path_filter() == path)
        })
    }

    /// Remove all application bindings and reset application modes.
    pub fn clear_application(&mut self) -> Vec<(BindingId, BindingTarget)> {
        let removed = self.remove_application_records(|_| true);
        self.mode_stack.clear();
        removed
    }

    /// Drop selected application records and report their targets in registry
    /// order.
    fn remove_application_records(
        &mut self,
        selected: impl Fn(&BindingRecord) -> bool,
    ) -> Vec<(BindingId, BindingTarget)> {
        let mut removed = Vec::new();
        self.records.retain(|record| {
            if !matches!(record.owner, BindingOwner::Application) || !selected(record) {
                return true;
            }
            removed.push((record.id, record.target.clone()));
            false
        });
        removed
    }

    /// Return every record in insertion order.
    pub fn bindings(&self) -> &[BindingRecord] {
        &self.records
    }

    /// Return one binding record by ID.
    pub(crate) fn binding(&self, id: BindingId) -> Option<&BindingRecord> {
        self.records.iter().find(|record| record.id == id)
    }

    /// Resolve one input at one route node.
    pub fn resolve_match(&self, path: &Path, input: InputSpec) -> Option<ResolvedBinding> {
        let input = input.normalize();
        let winner = if let Some(group) = self.active_exclusive_group() {
            self.best_in_scope(path, input, &BindingScope::Exclusive(group), Some(group))
        } else {
            self.best_in_scope(path, input, &BindingScope::Global, None)
                .or_else(|| {
                    self.mode_stack.iter().rev().find_map(|mode| {
                        self.best_in_scope(path, input, &BindingScope::Mode(mode.clone()), None)
                    })
                })
                .or_else(|| self.best_in_scope(path, input, &BindingScope::Default, None))
        }?;
        Some(ResolvedBinding {
            id: winner.0.id,
            target: winner.0.target.clone(),
            phase: winner.0.phase.unwrap_or_else(|| binding_phase(winner.1)),
            description: winner.0.description.clone(),
        })
    }

    /// Return normalized key inputs that can participate in the current scope
    /// state.
    pub(crate) fn eligible_keys(&self) -> Vec<Key> {
        let active_group = self.active_exclusive_group();
        let mut keys = HashSet::new();
        for record in &self.records {
            let eligible = match active_group {
                Some(group) => {
                    record.owner == BindingOwner::Framework(group)
                        && record.scope == BindingScope::Exclusive(group)
                }
                None => matches!(record.owner, BindingOwner::Application),
            };
            if eligible && let InputSpec::Key(key) = record.input {
                keys.insert(key.normalize());
            }
        }
        let mut keys = keys.into_iter().collect::<Vec<_>>();
        keys.sort_by_key(ToString::to_string);
        keys
    }

    /// Explain one record's state for a route from the target to the root.
    pub(crate) fn diagnostic_state(&self, id: BindingId, route: &[Path]) -> String {
        let Some(record) = self.binding(id) else {
            return "missing".to_string();
        };
        if let Some(group) = self.active_exclusive_group() {
            if record.owner != BindingOwner::Framework(group)
                || record.scope != BindingScope::Exclusive(group)
            {
                return format!("blocked by exclusive group {group}");
            }
        } else {
            match &record.scope {
                BindingScope::Exclusive(group) => {
                    return format!("inactive exclusive group {group}");
                }
                BindingScope::Mode(mode)
                    if !self.mode_stack.iter().any(|active| active == mode) =>
                {
                    return format!("inactive mode {mode}");
                }
                BindingScope::Global | BindingScope::Mode(_) | BindingScope::Default => {}
            }
        }

        let record_route = route
            .iter()
            .position(|path| record.path_matcher.check_match(path).is_some());
        let Some(record_route) = record_route else {
            return "path does not match route".to_string();
        };
        let winner = route.iter().enumerate().find_map(|(index, path)| {
            self.resolve_match(path, record.input)
                .map(|winner| (index, winner))
        });
        let Some((winner_route, winner)) = winner else {
            return "not eligible in the active scope".to_string();
        };
        if winner.id == id {
            return "effective".to_string();
        }
        if winner_route < record_route {
            return "shadowed at an earlier route node".to_string();
        }
        let winning_record = self
            .binding(winner.id)
            .expect("resolved binding record must remain registered");
        if winning_record.scope != record.scope {
            return "shadowed by a higher-priority scope".to_string();
        }
        let path = &route[winner_route];
        let record_match = record
            .path_matcher
            .check_match(path)
            .expect("record must match its first route node");
        let winner_match = winning_record
            .path_matcher
            .check_match(path)
            .expect("winner must match its route node");
        if winner_match.score() > record_match.score() {
            "shadowed by a more specific path".to_string()
        } else {
            "shadowed by later insertion".to_string()
        }
    }

    /// Push one exclusive frame for its owning node.
    pub fn push_exclusive_bindings(
        &mut self,
        group: FrameworkBindingGroup,
        owner: NodeId,
    ) -> Result<ExclusiveFrameToken> {
        let token = ExclusiveFrameToken(self.next_token);
        self.next_token = self.next_token.checked_add(1).ok_or_else(|| {
            Error::InvalidOperation("exclusive frame token space exhausted".to_string())
        })?;
        self.exclusive_frames.push(ExclusiveFrame {
            token,
            group,
            owner,
        });
        Ok(token)
    }

    /// Remove one exclusive frame without disturbing newer frames.
    pub fn pop_exclusive_bindings(&mut self, token: ExclusiveFrameToken) -> Result<()> {
        let Some(index) = self
            .exclusive_frames
            .iter()
            .position(|frame| frame.token == token)
        else {
            return Err(Error::InvalidOperation(
                "exclusive binding frame token is not active".to_string(),
            ));
        };
        self.exclusive_frames.remove(index);
        Ok(())
    }

    /// Apply or remove the admission owned by the top modal scope.
    pub(crate) fn set_modal_bindings(&mut self, bindings: Option<ModalBindings>) {
        self.modal_bindings = bindings;
    }

    /// Return the newest active exclusive group.
    pub fn active_exclusive_group(&self) -> Option<FrameworkBindingGroup> {
        match self.modal_bindings {
            Some(ModalBindings::Framework(group)) => Some(group),
            Some(ModalBindings::Application) => None,
            None => self.exclusive_frames.last().map(|frame| frame.group),
        }
    }

    /// Remove frames whose owner is not attached to the active tree.
    pub(crate) fn retain_exclusive_owners(&mut self, attached: &HashSet<NodeId>) {
        self.exclusive_frames
            .retain(|frame| attached.contains(&frame.owner));
    }

    /// Return active exclusive tokens for a tree-edit baseline.
    pub(crate) fn exclusive_frame_tokens(&self) -> HashSet<ExclusiveFrameToken> {
        self.exclusive_frames
            .iter()
            .map(|frame| frame.token)
            .collect()
    }

    /// Remove pre-edit frames whose owning widget identity was replaced.
    pub(crate) fn remove_replaced_exclusive_owners(
        &mut self,
        owners: &HashSet<NodeId>,
        before: &HashSet<ExclusiveFrameToken>,
    ) {
        self.exclusive_frames
            .retain(|frame| !before.contains(&frame.token) || !owners.contains(&frame.owner));
    }

    /// Set the active input mode.
    pub fn set_mode(&mut self, mode: &str) -> Result<()> {
        self.mode_stack.clear();
        if !mode.is_empty() {
            self.mode_stack.push(mode.to_string());
        }
        Ok(())
    }

    /// Push a named input mode.
    pub fn push_mode(&mut self, mode: &str) -> Result<()> {
        if !mode.is_empty() {
            self.mode_stack.push(mode.to_string());
        }
        Ok(())
    }

    /// Pop the newest input mode and return the active mode.
    pub fn pop_mode(&mut self) -> &str {
        self.mode_stack.pop();
        self.current_mode()
    }

    /// Return the newest active input mode.
    pub fn current_mode(&self) -> &str {
        self.mode_stack.last().map_or(DEFAULT_MODE, String::as_str)
    }

    /// Return active non-default modes in resolution order.
    pub fn active_modes(&self) -> Vec<&str> {
        self.mode_stack.iter().rev().map(String::as_str).collect()
    }

    /// Snapshot only application-owned registry state.
    pub(crate) fn snapshot_application(&self) -> ApplicationBindingSnapshot {
        ApplicationBindingSnapshot {
            records: self
                .records
                .iter()
                .filter(|record| matches!(record.owner, BindingOwner::Application))
                .cloned()
                .collect(),
            mode_stack: self.mode_stack.clone(),
        }
    }

    /// Restore application records without changing framework state.
    pub(crate) fn restore_application(&mut self, snapshot: ApplicationBindingSnapshot) {
        self.records
            .retain(|record| !matches!(record.owner, BindingOwner::Application));
        self.records.extend(snapshot.records);
        self.records.sort_by_key(|record| record.insertion_id);
        self.mode_stack = snapshot.mode_stack;
    }

    /// Return script targets added after an application snapshot was captured.
    pub(crate) fn targets_not_in(
        &self,
        baseline: &ApplicationBindingSnapshot,
    ) -> Vec<LuauFunctionId> {
        let baseline: HashSet<BindingId> =
            baseline.records.iter().map(|record| record.id).collect();
        self.records
            .iter()
            .filter(|record| matches!(record.owner, BindingOwner::Application))
            .filter(|record| !baseline.contains(&record.id))
            .filter_map(|record| match record.target {
                BindingTarget::Script(target) => Some(target),
                BindingTarget::Command(_) => None,
            })
            .collect()
    }

    /// Replace the next binding identifier for deterministic exhaustion tests.
    #[cfg(test)]
    pub(crate) fn replace_next_id(&mut self, next_id: u64) -> u64 {
        mem::replace(&mut self.next_id, next_id)
    }

    /// Select the best matching record in one exact scope.
    fn best_in_scope(
        &self,
        path: &Path,
        input: InputSpec,
        scope: &BindingScope,
        framework_group: Option<FrameworkBindingGroup>,
    ) -> Option<(&BindingRecord, PathMatch)> {
        self.records
            .iter()
            .filter(|record| record.input == input && record.scope == *scope)
            .filter(|record| match framework_group {
                Some(group) => record.owner == BindingOwner::Framework(group),
                None => matches!(record.owner, BindingOwner::Application),
            })
            .filter_map(|record| {
                record
                    .path_matcher
                    .check_match(path)
                    .map(|path_match| (record, path_match))
            })
            .max_by(|left, right| compare_candidates(*left, *right))
    }

    /// Allocate one binding ID without mutating the registry on exhaustion.
    fn allocate_binding_id(&mut self) -> Result<BindingId> {
        let id = BindingId(self.next_id);
        self.next_id = self.next_id.checked_add(1).ok_or_else(|| {
            Error::InvalidOperation("binding identifier space exhausted".to_string())
        })?;
        Ok(id)
    }

    /// Allocate one insertion ID.
    fn allocate_insertion_id(&mut self) -> Result<u64> {
        let id = self.next_insertion_id;
        self.next_insertion_id = self.next_insertion_id.checked_add(1).ok_or_else(|| {
            Error::InvalidOperation("binding insertion space exhausted".to_string())
        })?;
        Ok(id)
    }
}

/// Compare two candidates by path specificity and insertion order.
fn compare_candidates(
    left: (&BindingRecord, PathMatch),
    right: (&BindingRecord, PathMatch),
) -> Ordering {
    left.1
        .score()
        .cmp(&right.1.score())
        .then_with(|| left.0.insertion_id.cmp(&right.0.insertion_id))
}

/// Classify one path match for routing and help presentation.
fn binding_phase(path_match: PathMatch) -> BindingPhase {
    if path_match.anchored_end {
        BindingPhase::BeforeWidget
    } else {
        BindingPhase::AfterIgnore
    }
}

/// Reject pre-widget mouse bindings, which the mouse route cannot execute.
fn validate_phase(input: InputSpec, phase: Option<BindingPhase>) -> Result<()> {
    if matches!(input, InputSpec::Mouse(_)) && phase == Some(BindingPhase::BeforeWidget) {
        return Err(Error::InvalidOperation(
            "mouse bindings cannot use the before_widget phase".to_string(),
        ));
    }
    Ok(())
}

/// Validate an application binding scope.
fn validate_application_scope(scope: &BindingScope, path_filter: &str) -> Result<()> {
    match scope {
        BindingScope::Global => {
            if !path_filter.starts_with('/') || !path_filter.ends_with('/') {
                return Err(Error::InvalidOperation(
                    "global bindings require a start- and end-anchored path".to_string(),
                ));
            }
        }
        BindingScope::Mode(mode) if mode.is_empty() => {
            return Err(Error::InvalidOperation(
                "named binding mode cannot be empty".to_string(),
            ));
        }
        BindingScope::Default | BindingScope::Mode(_) => {}
        BindingScope::Exclusive(_) => {
            return Err(Error::InvalidOperation(
                "application bindings cannot use an exclusive scope".to_string(),
            ));
        }
    }
    Ok(())
}

/// Validate required user-facing binding text.
fn validate_description(description: &str) -> Result<()> {
    if description.trim().is_empty() {
        Err(Error::InvalidOperation(
            "binding description cannot be empty".to_string(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
