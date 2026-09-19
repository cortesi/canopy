#[cfg(test)]
use std::mem;
use std::{cmp::Ordering, collections::HashSet, fmt, hash::Hash};

use crate::{
    ModalBindings,
    commands::CommandAction,
    error::{Error, Result},
    event::{key::Key, mouse::Mouse},
    path::{Path, PathFilter, PathMatch},
    script::LuauFunctionId,
};

mod action;
pub use action::{WidgetActionCatalog, WidgetActionName, WidgetActionSpec};

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
    /// Phase that sets when the binding runs relative to the widget.
    ///
    /// `None` means the phase was omitted at registration. Commands and
    /// callbacks then default to `after_widget`. Widget actions carry no
    /// phase, and an explicit phase on one is an error.
    pub phase: Option<BindingPhase>,
}

/// Action executed by a binding.
#[derive(Clone, Debug, PartialEq)]
pub enum BindingTarget {
    /// Stored Luau callback.
    Script(LuauFunctionId),
    /// Rust command invocation.
    Command(CommandAction),
    /// Named operation the route offers to widgets on the way up.
    WidgetAction(WidgetActionName),
}

/// Class of target a binding record owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingTargetKind {
    /// Stored Luau callback.
    Script,
    /// Rust command invocation.
    Command,
    /// Named operation offered to widgets on the route.
    WidgetAction,
}

impl BindingTargetKind {
    /// Return the kind of one binding target.
    #[must_use]
    pub fn of(target: &BindingTarget) -> Self {
        match target {
            BindingTarget::Script(_) => Self::Script,
            BindingTarget::Command(_) => Self::Command,
            BindingTarget::WidgetAction(_) => Self::WidgetAction,
        }
    }

    /// Return a stable target-kind label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Script => "script",
            Self::Command => "command",
            Self::WidgetAction => "widget_action",
        }
    }
}

impl BindingTarget {
    /// Return a stable target-kind label.
    pub fn label(&self) -> &'static str {
        BindingTargetKind::of(self).label()
    }

    /// Return the widget action name, when this is an action target.
    #[must_use]
    pub fn widget_action(&self) -> Option<&WidgetActionName> {
        match self {
            Self::WidgetAction(name) => Some(name),
            Self::Script(_) | Self::Command(_) => None,
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
    /// Phase that sets when the binding runs relative to the widget.
    ///
    /// `None` marks a widget action, which runs before the node's raw key
    /// handler and carries no phase choice.
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

    /// Return the path match for one route path.
    pub(crate) fn path_match(&self, path: &Path) -> Option<PathMatch> {
        self.path_matcher.check_match(path)
    }
}

/// Binding phase relative to widget input handling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BindingPhase {
    /// Execute before the focused widget.
    BeforeWidget,
    /// Execute only after the widget ignores the input.
    #[default]
    AfterWidget,
}

impl BindingPhase {
    /// Return a stable scripting and diagnostic label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::BeforeWidget => "before_widget",
            Self::AfterWidget => "after_widget",
        }
    }

    /// Parse a scripting label into a binding phase.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "before_widget" => Some(Self::BeforeWidget),
            "after_widget" => Some(Self::AfterWidget),
            _ => None,
        }
    }
}

/// Winner returned by the shared resolver.
#[derive(Clone, Debug)]
pub struct ResolvedBinding {
    /// Binding identifier.
    pub id: BindingId,
    /// Target to execute.
    pub target: BindingTarget,
    /// Routing phase, absent for a widget action.
    pub phase: Option<BindingPhase>,
    /// User-facing description.
    pub description: String,
}

/// Why the registry admits, blocks, or shadows one record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryStatus {
    /// The record is the effective winner.
    Effective,
    /// No record has this identifier.
    Missing,
    /// The active exclusive group does not admit this record.
    BlockedByExclusive(FrameworkBindingGroup),
    /// The record's exclusive group is not active.
    InactiveExclusive(FrameworkBindingGroup),
    /// The record's named mode is not active.
    InactiveMode(String),
    /// A transient mode ends resolution before this record.
    BlockedByTransient(String),
    /// The record's path never matches the route.
    PathMismatch,
    /// No route path admits the record in the active scope.
    NotEligible,
    /// An earlier route node shadows the record.
    ShadowedAtEarlierRoute {
        /// Binding that wins earlier on the route.
        winner: BindingId,
    },
    /// A higher-priority scope shadows the record.
    ShadowedByScope {
        /// Binding that wins in the higher scope.
        winner: BindingId,
    },
    /// A more specific path shadows the record.
    ShadowedByMoreSpecificPath {
        /// Binding that wins through path specificity.
        winner: BindingId,
    },
    /// A later insertion shadows the record.
    ShadowedByInsertion {
        /// Binding that wins through insertion order.
        winner: BindingId,
    },
}

impl RegistryStatus {
    /// Return the stable human diagnostic label.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Effective => "effective".to_string(),
            Self::Missing => "missing".to_string(),
            Self::BlockedByExclusive(group) => format!("blocked by exclusive group {group}"),
            Self::InactiveExclusive(group) => format!("inactive exclusive group {group}"),
            Self::InactiveMode(mode) => format!("inactive mode {mode}"),
            Self::BlockedByTransient(mode) => format!("blocked by transient mode {mode}"),
            Self::PathMismatch => "path does not match route".to_string(),
            Self::NotEligible => "not eligible in the active scope".to_string(),
            Self::ShadowedAtEarlierRoute { .. } => "shadowed at an earlier route node".to_string(),
            Self::ShadowedByScope { .. } => "shadowed by a higher-priority scope".to_string(),
            Self::ShadowedByMoreSpecificPath { .. } => {
                "shadowed by a more specific path".to_string()
            }
            Self::ShadowedByInsertion { .. } => "shadowed by later insertion".to_string(),
        }
    }
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

/// One entry on the input mode stack.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ActiveMode {
    /// Mode name.
    name: String,
    /// Whether the mode takes only the next key.
    transient: bool,
}

/// Application-owned state restored after a failed startup script.
#[derive(Clone, Debug)]
pub struct ApplicationBindingSnapshot {
    /// Application records captured at the start of an attempt.
    records: Vec<BindingRecord>,
    /// Application mode stack captured at the start of an attempt.
    mode_stack: Vec<ActiveMode>,
}

/// Registry for application bindings, framework controls, and active modes.
#[derive(Clone, Debug)]
pub struct InputMap {
    /// Flat application and framework binding records.
    records: Vec<BindingRecord>,
    /// Bindable widget actions the application registered.
    actions: WidgetActionCatalog,
    /// Active application modes in push order.
    mode_stack: Vec<ActiveMode>,
    /// Number of mode stack updates, so observers can tell when to resync.
    mode_generation: u64,
    /// Admission imposed by the active modal.
    modal_bindings: Option<ModalBindings>,
    /// Next binding identifier.
    next_id: u64,
    /// Next insertion-order identifier.
    next_insertion_id: u64,
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
            actions: WidgetActionCatalog::default(),
            mode_stack: Vec::new(),
            mode_generation: 0,
            modal_bindings: None,
            next_id: 1,
            next_insertion_id: 1,
        }
    }

    /// Register one bindable widget action for this application.
    pub(crate) fn register_widget_action(&mut self, spec: WidgetActionSpec) -> Result<()> {
        self.actions.register(spec)
    }

    /// Freeze the widget action catalog when the script API finalizes.
    pub(crate) fn freeze_widget_actions(&mut self) {
        self.actions.freeze();
    }

    /// Return the registered widget actions in name order.
    #[must_use]
    pub(crate) fn widget_actions(&self) -> &WidgetActionCatalog {
        &self.actions
    }

    /// Store or replace an application binding.
    pub fn replace_application_binding(
        &mut self,
        input: InputSpec,
        options: BindingOptions,
        target: BindingTarget,
    ) -> Result<(BindingId, Vec<(BindingId, BindingTarget)>)> {
        validate_application_binding(&options, target.widget_action())?;
        if let BindingTarget::WidgetAction(name) = &target {
            if matches!(input, InputSpec::Mouse(_)) {
                return Err(Error::InvalidOperation(
                    "widget action bindings accept keys only".to_string(),
                ));
            }
            if !self.actions.contains_name(name.as_str()) {
                return Err(Error::InvalidOperation(format!(
                    "widget action {name} is not registered in this application"
                )));
            }
        }
        let phase = match target {
            BindingTarget::WidgetAction(_) => None,
            BindingTarget::Script(_) | BindingTarget::Command(_) => {
                Some(options.phase.unwrap_or_default())
            }
        };
        let path_filter = options.path.as_ref().map_or("", PathFilter::as_str);
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
            phase,
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
        let scope = BindingScope::Exclusive(group);
        if options.scope != scope {
            return Err(Error::InvalidOperation(
                "framework binding scope must match its exclusive group".to_string(),
            ));
        }
        let path_filter = options.path.as_ref().map_or("", PathFilter::as_str);
        let path_matcher = options.path.clone().unwrap_or(PathFilter::new("")?);
        let input = input.normalize();
        let phase = Some(options.phase.unwrap_or_default());
        if let Some(existing) = self.records.iter().find(|record| {
            record.owner == BindingOwner::Framework(group)
                && record.input == input
                && record.path_filter() == path_filter
        }) {
            if existing.scope == scope
                && existing.description == options.description
                && existing.phase == phase
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
            phase,
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
        self.touch_modes();
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
    ///
    /// This is the structural winner. It ignores widget acceptance, so an
    /// action candidate that no widget consumes still resolves here. Route
    /// selection uses [`Core::select_key_binding`] instead.
    pub fn resolve_match(&self, path: &Path, input: InputSpec) -> Option<ResolvedBinding> {
        let candidate = self.candidates(path, input).into_iter().next()?;
        Some(ResolvedBinding {
            id: candidate.record.id,
            target: candidate.record.target.clone(),
            phase: candidate.record.phase,
            description: candidate.record.description.clone(),
        })
    }

    /// Return ranked binding candidates at one route node, best first.
    ///
    /// The order is modal admission, then the global tier, the newest active
    /// modes, and the default tier. Within a tier, path specificity and
    /// insertion order rank the candidates. A transient mode ends the walk,
    /// so nothing older follows it.
    pub(crate) fn candidates(&self, path: &Path, input: InputSpec) -> Vec<BindingCandidate<'_>> {
        let input = input.normalize();
        let mut out = Vec::new();
        match &self.modal_bindings {
            Some(ModalBindings::Framework(group)) => {
                let group = *group;
                self.extend_scope(
                    &mut out,
                    path,
                    input,
                    &BindingScope::Exclusive(group),
                    Some(group),
                );
            }
            Some(ModalBindings::FrameworkWithActions { group, .. }) => {
                let group = *group;
                self.extend_scope(
                    &mut out,
                    path,
                    input,
                    &BindingScope::Exclusive(group),
                    Some(group),
                );
                self.extend_application_tiers(&mut out, path, input);
            }
            Some(ModalBindings::Application) | None => {
                self.extend_application_tiers(&mut out, path, input);
            }
        }
        out
    }

    /// Append the application tiers in resolution order.
    fn extend_application_tiers<'a>(
        &'a self,
        out: &mut Vec<BindingCandidate<'a>>,
        path: &Path,
        input: InputSpec,
    ) {
        self.extend_scope(out, path, input, &BindingScope::Global, None);
        for mode in self.mode_stack.iter().rev() {
            let scope = BindingScope::Mode(mode.name.clone());
            self.extend_scope(out, path, input, &scope, None);
            if mode.transient {
                // A transient mode ends the walk, so older modes and the
                // default tier stay out of reach.
                return;
            }
        }
        self.extend_scope(out, path, input, &BindingScope::Default, None);
    }

    /// Append the candidates of one exact scope, best first.
    fn extend_scope<'a>(
        &'a self,
        out: &mut Vec<BindingCandidate<'a>>,
        path: &Path,
        input: InputSpec,
        scope: &BindingScope,
        framework_group: Option<FrameworkBindingGroup>,
    ) {
        let mut found = self
            .records
            .iter()
            .filter(|record| record.input == input && record.scope == *scope)
            .filter(|record| match framework_group {
                Some(group) => record.owner == BindingOwner::Framework(group),
                None => matches!(record.owner, BindingOwner::Application),
            })
            .filter(|record| self.admits_record(record))
            .filter_map(|record| {
                record
                    .path_matcher
                    .check_match(path)
                    .map(|path_match| BindingCandidate { record, path_match })
            })
            .collect::<Vec<_>>();
        found.sort_by(|left, right| compare_candidates(*left, *right).reverse());
        out.extend(found);
    }

    /// Return whether the active scope state admits one record.
    ///
    /// Admission ignores route position and widget state. Route selection and
    /// `candidate_keys` share this predicate.
    pub(crate) fn admits_record(&self, record: &BindingRecord) -> bool {
        match &self.modal_bindings {
            Some(ModalBindings::Framework(group)) => {
                record.owner == BindingOwner::Framework(*group)
                    && record.scope == BindingScope::Exclusive(*group)
            }
            Some(ModalBindings::FrameworkWithActions { group, actions }) => {
                if record.owner == BindingOwner::Framework(*group)
                    && record.scope == BindingScope::Exclusive(*group)
                {
                    return true;
                }
                matches!(record.owner, BindingOwner::Application)
                    && record.phase.is_none()
                    && record
                        .target
                        .widget_action()
                        .is_some_and(|name| actions.contains(&name.as_str()))
            }
            Some(ModalBindings::Application) | None => {
                matches!(record.owner, BindingOwner::Application)
                    && !matches!(record.scope, BindingScope::Exclusive(_))
            }
        }
    }

    /// Return normalized key inputs the active scope state admits.
    ///
    /// The result is a safe superset for discovery. It includes dormant action
    /// keys and does not decide which binding wins.
    pub(crate) fn candidate_keys(&self) -> Vec<Key> {
        self.candidate_inputs(|input| match input {
            InputSpec::Key(key) => Some(key),
            InputSpec::Mouse(_) => None,
        })
    }

    /// Return normalized mouse inputs that can participate in the current
    /// scope state.
    pub(crate) fn eligible_mouse_inputs(&self) -> Vec<Mouse> {
        self.candidate_inputs(|input| match input {
            InputSpec::Mouse(mouse) => Some(mouse),
            InputSpec::Key(_) => None,
        })
    }

    /// Return the distinct inputs `select` keeps from the records the active
    /// scope state admits.
    ///
    /// The order is by label alone, for stable presentation. Precedence between
    /// records belongs to the resolver, which ranks one winner per input.
    fn candidate_inputs<I: Eq + Hash + ToString>(
        &self,
        select: impl Fn(InputSpec) -> Option<I>,
    ) -> Vec<I> {
        let mut inputs = HashSet::new();
        for record in &self.records {
            if self.admits_record(record)
                && let Some(input) = select(record.input.normalize())
            {
                inputs.insert(input);
            }
        }
        let mut inputs = inputs.into_iter().collect::<Vec<_>>();
        inputs.sort_by_key(ToString::to_string);
        inputs
    }

    /// Return why the registry admits, blocks, or shadows one record.
    pub(crate) fn registry_status(&self, id: BindingId, route: &[Path]) -> RegistryStatus {
        let Some(record) = self.binding(id) else {
            return RegistryStatus::Missing;
        };
        if let Some(group) = self.active_exclusive_group() {
            if !self.admits_record(record) {
                return RegistryStatus::BlockedByExclusive(group);
            }
        } else {
            match &record.scope {
                BindingScope::Exclusive(group) => {
                    return RegistryStatus::InactiveExclusive(*group);
                }
                BindingScope::Mode(mode)
                    if !self.mode_stack.iter().any(|active| active.name == *mode) =>
                {
                    return RegistryStatus::InactiveMode(mode.clone());
                }
                BindingScope::Global | BindingScope::Mode(_) | BindingScope::Default => {}
            }
            if let Some(mode) = self.transient_blocker(&record.scope) {
                return RegistryStatus::BlockedByTransient(mode.to_string());
            }
        }

        let record_route = route
            .iter()
            .position(|path| record.path_matcher.check_match(path).is_some());
        let Some(record_route) = record_route else {
            return RegistryStatus::PathMismatch;
        };
        let winner = route.iter().enumerate().find_map(|(index, path)| {
            self.resolve_match(path, record.input)
                .map(|winner| (index, winner))
        });
        let Some((winner_route, winner)) = winner else {
            return RegistryStatus::NotEligible;
        };
        if winner.id == id {
            return RegistryStatus::Effective;
        }
        if winner_route < record_route {
            return RegistryStatus::ShadowedAtEarlierRoute { winner: winner.id };
        }
        let winning_record = self
            .binding(winner.id)
            .expect("resolved binding record must remain registered");
        if winning_record.scope != record.scope {
            return RegistryStatus::ShadowedByScope { winner: winner.id };
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
            RegistryStatus::ShadowedByMoreSpecificPath { winner: winner.id }
        } else {
            RegistryStatus::ShadowedByInsertion { winner: winner.id }
        }
    }

    /// Apply or remove the admission owned by the top modal scope.
    pub(crate) fn set_modal_bindings(&mut self, bindings: Option<ModalBindings>) {
        self.modal_bindings = bindings;
    }

    /// Return the active exclusive group admitted by the top modal scope.
    pub fn active_exclusive_group(&self) -> Option<FrameworkBindingGroup> {
        match self.modal_bindings {
            Some(ModalBindings::Framework(group))
            | Some(ModalBindings::FrameworkWithActions { group, .. }) => Some(group),
            Some(ModalBindings::Application) | None => None,
        }
    }

    /// Set the active input mode.
    pub fn set_mode(&mut self, mode: &str) {
        self.mode_stack.clear();
        self.push_active(mode, false);
    }

    /// Push a named input mode.
    pub fn push_mode(&mut self, mode: &str) {
        self.push_active(mode, false);
    }

    /// Push a named input mode that takes only the next key.
    ///
    /// Keys the mode does not bind never fall through to older modes or the
    /// default scope. Key routing pops the mode before it runs the binding.
    pub fn push_transient_mode(&mut self, mode: &str) {
        self.push_active(mode, true);
    }

    /// Push one mode stack entry. The empty default mode is never pushed.
    fn push_active(&mut self, mode: &str, transient: bool) {
        if !mode.is_empty() {
            self.mode_stack.push(ActiveMode {
                name: mode.to_string(),
                transient,
            });
        }
        self.touch_modes();
    }

    /// Pop the newest input mode and return the active mode.
    pub fn pop_mode(&mut self) -> &str {
        self.mode_stack.pop();
        self.touch_modes();
        self.current_mode()
    }

    /// Return the newest active input mode.
    pub fn current_mode(&self) -> &str {
        self.mode_stack
            .last()
            .map_or(DEFAULT_MODE, |mode| mode.name.as_str())
    }

    /// Return the newest active mode when it is transient.
    pub fn transient_mode(&self) -> Option<&str> {
        self.mode_stack
            .last()
            .filter(|mode| mode.transient)
            .map(|mode| mode.name.as_str())
    }

    /// Return active non-default modes in resolution order.
    pub fn active_modes(&self) -> Vec<&str> {
        self.mode_stack
            .iter()
            .rev()
            .map(|mode| mode.name.as_str())
            .collect()
    }

    /// Return the number of mode stack updates so far.
    pub(crate) fn mode_generation(&self) -> u64 {
        self.mode_generation
    }

    /// Record a mode stack update.
    fn touch_modes(&mut self) {
        self.mode_generation = self.mode_generation.wrapping_add(1);
    }

    /// Return the transient mode that keeps resolution from reaching `scope`.
    fn transient_blocker(&self, scope: &BindingScope) -> Option<&str> {
        let floor = match scope {
            BindingScope::Default => 0,
            BindingScope::Mode(mode) => {
                self.mode_stack
                    .iter()
                    .rposition(|active| active.name == *mode)?
                    + 1
            }
            BindingScope::Global | BindingScope::Exclusive(_) => return None,
        };
        self.mode_stack[floor..]
            .iter()
            .rev()
            .find(|active| active.transient)
            .map(|active| active.name.as_str())
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
        self.touch_modes();
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
                BindingTarget::Command(_) | BindingTarget::WidgetAction(_) => None,
            })
            .collect()
    }

    /// Replace the next binding identifier for deterministic exhaustion tests.
    #[cfg(test)]
    pub(crate) fn replace_next_id(&mut self, next_id: u64) -> u64 {
        mem::replace(&mut self.next_id, next_id)
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

/// One ranked binding candidate at one route node.
#[derive(Clone, Copy, Debug)]
pub struct BindingCandidate<'a> {
    /// Candidate record.
    pub(crate) record: &'a BindingRecord,
    /// Path-match score against the route path.
    pub(crate) path_match: PathMatch,
}

/// Compare two candidates by path specificity and insertion order.
fn compare_candidates(left: BindingCandidate<'_>, right: BindingCandidate<'_>) -> Ordering {
    left.path_match
        .score()
        .cmp(&right.path_match.score())
        .then_with(|| left.record.insertion_id.cmp(&right.record.insertion_id))
}

/// Validate the options and target of one application binding without
/// installing it.
///
/// `action` is the widget action name for an action target, and `None` for a
/// command or callback target.
pub fn validate_application_binding(
    options: &BindingOptions,
    action: Option<&WidgetActionName>,
) -> Result<()> {
    let path_filter = options.path.as_ref().map_or("", PathFilter::as_str);
    validate_application_scope(&options.scope, path_filter)?;
    validate_description(&options.description)?;
    if action.is_some() && options.phase.is_some() {
        return Err(Error::InvalidOperation(
            "widget action bindings do not take a phase".to_string(),
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
