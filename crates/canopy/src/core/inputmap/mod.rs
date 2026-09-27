#[cfg(test)]
use std::mem;
use std::{cmp::Ordering, collections::HashSet, fmt, hash::Hash};

use crate::{
    commands::CommandCall,
    error::{Error, Result},
    input::{ModalBindings, key::Key, mouse::Mouse},
    path::{Path, PathFilter, PathMatch},
    script::LuauFunctionId,
};

mod intent;
pub use intent::{IntentCatalog, IntentName, IntentSpec, NavIntent};

/// Default mode name.
const DEFAULT_MODE: &str = "";

/// Reject an empty mode name, which would name the default mode.
fn check_mode_name(mode: &str) -> Result<()> {
    if mode.is_empty() {
        return Err(Error::Invalid(
            "a mode must have a name; the default mode has none".to_string(),
        ));
    }
    Ok(())
}

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

/// Resolution tier for one binding.
///
/// Variant order is resolution order: the framework group an open modal
/// admits, then the global tier, then active modes newest first, then the
/// default tier. Only framework-tier records belong to the framework; every
/// other tier holds application bindings that script APIs can change.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BindingTier {
    /// Framework-owned bindings that only a modal admits.
    Framework(FrameworkBindingGroup),
    /// Highest-priority application tier, which every modal admits.
    Global,
    /// Named application mode.
    Mode(String),
    /// Default application tier.
    Default,
}

impl BindingTier {
    /// Return the named mode, if this is a mode tier.
    pub fn mode(&self) -> Option<&str> {
        match self {
            Self::Mode(mode) => Some(mode),
            Self::Framework(_) | Self::Global | Self::Default => None,
        }
    }

    /// Return the framework group, if this is the framework tier.
    pub fn framework_group(&self) -> Option<FrameworkBindingGroup> {
        match self {
            Self::Framework(group) => Some(*group),
            Self::Global | Self::Mode(_) | Self::Default => None,
        }
    }

    /// Return whether this tier holds framework-owned bindings.
    pub fn is_framework(&self) -> bool {
        matches!(self, Self::Framework(_))
    }

    /// Return a stable scripting and diagnostic label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Framework(_) => "framework",
            Self::Global => "global",
            Self::Mode(_) => "mode",
            Self::Default => "default",
        }
    }
}

/// Options shared by native and scripted application bindings.
#[derive(Clone, Debug)]
pub struct BindingOptions {
    /// Optional validated path selector. Omission matches the current route.
    pub path: Option<PathFilter>,
    /// Resolution tier. Scripts install application tiers only; the
    /// framework tier is registered during setup.
    pub tier: BindingTier,
    /// Required user-facing description.
    pub description: String,
    /// Optional diagnostic source.
    pub source: Option<String>,
    /// Phase that sets when the binding runs relative to the widget.
    ///
    /// `None` means the phase was omitted at registration. Commands and
    /// callbacks then default to `after_widget`. An intent always runs
    /// before the widget, so an explicit `after_widget` on one is an error.
    pub phase: Option<BindingPhase>,
}

/// Action executed by a binding.
#[derive(Clone, Debug, PartialEq)]
pub enum BindingAction {
    /// Stored Luau callback.
    Script(LuauFunctionId),
    /// Rust command call.
    Command(CommandCall),
    /// Named operation the route offers to widgets on the way up.
    Intent(IntentName),
    /// Open a menu: push the named mode as a transient mode, which waits for
    /// one key. A binding that opens a menu leads to another choice rather
    /// than acting, so help marks it apart from the bindings that act.
    Menu(String),
}

/// Class of target a binding record owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingActionKind {
    /// Stored Luau callback.
    Script,
    /// Rust command call.
    Command,
    /// Named operation offered to widgets on the route.
    Intent,
    /// Transient mode that waits for one key.
    Menu,
}

impl BindingActionKind {
    /// Return the kind of one binding target.
    #[must_use]
    pub(crate) fn of(target: &BindingAction) -> Self {
        match target {
            BindingAction::Script(_) => Self::Script,
            BindingAction::Command(_) => Self::Command,
            BindingAction::Intent(_) => Self::Intent,
            BindingAction::Menu(_) => Self::Menu,
        }
    }

    /// Return a stable target-kind label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Script => "script",
            Self::Command => "command",
            Self::Intent => "intent",
            Self::Menu => "menu",
        }
    }
}

impl BindingAction {
    /// Return a stable target-kind label.
    pub fn label(&self) -> &'static str {
        BindingActionKind::of(self).label()
    }

    /// Return the intent name, when this action is an intent.
    #[must_use]
    pub fn intent(&self) -> Option<&IntentName> {
        match self {
            Self::Intent(name) => Some(name),
            Self::Script(_) | Self::Command(_) | Self::Menu(_) => None,
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
    /// Resolution tier.
    pub tier: BindingTier,
    /// Required user-facing description.
    pub description: String,
    /// Optional diagnostic source.
    pub source: Option<String>,
    /// Phase that sets when the binding runs relative to the widget. A widget
    /// action is always `BeforeWidget`: the route offers it before the node's
    /// raw key handler.
    pub phase: BindingPhase,
    /// What the binding runs.
    pub action: BindingAction,
    /// Monotonic insertion order.
    pub insertion_id: u64,
    /// Compiled path matcher and its original filter.
    path_matcher: PathFilter,
}

impl BindingRecord {
    /// Copy the parts of this record that routing needs to act on it.
    pub(crate) fn resolved(&self) -> ResolvedBinding {
        let target = match &self.action {
            BindingAction::Intent(intent) => {
                return ResolvedBinding::Offer {
                    id: self.id,
                    intent: intent.clone(),
                };
            }
            BindingAction::Script(function) => RunTarget::Script(*function),
            BindingAction::Command(call) => RunTarget::Command(call.clone()),
            BindingAction::Menu(mode) => RunTarget::Menu(mode.clone()),
        };
        ResolvedBinding::Run(RunBinding {
            id: self.id,
            phase: self.phase,
            description: self.description.clone(),
            target,
        })
    }

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

/// Owned copy of a winning binding, kept while routing acts on it.
///
/// Resolution borrows records. Routing copies the winner here, because acting
/// on it can change the registry. The route offers an intent to the
/// node's widget, and runs a command or callback, so each carries only what
/// that needs.
#[derive(Clone, Debug)]
pub enum ResolvedBinding {
    /// An intent the route offers before the node's raw key handler.
    Offer {
        /// Binding identifier.
        id: BindingId,
        /// Offered intent.
        intent: IntentName,
    },
    /// A command or callback that runs at its phase.
    Run(RunBinding),
}

/// A resolved command or callback binding.
#[derive(Clone, Debug)]
pub struct RunBinding {
    /// Binding identifier.
    pub id: BindingId,
    /// Routing phase.
    pub phase: BindingPhase,
    /// User-facing description.
    pub description: String,
    /// What the binding runs.
    pub target: RunTarget,
}

/// What a running binding executes.
#[derive(Clone, Debug)]
pub enum RunTarget {
    /// Stored Luau callback.
    Script(LuauFunctionId),
    /// Rust command call.
    Command(CommandCall),
    /// Transient mode to push.
    Menu(String),
}

/// Why the registry admits, blocks, or shadows one record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryStatus {
    /// The record is the effective winner.
    Effective,
    /// No record has this identifier.
    Missing,
    /// The active framework group does not admit this record.
    BlockedByFrameworkGroup(FrameworkBindingGroup),
    /// The record's framework group is not active.
    InactiveFrameworkGroup(FrameworkBindingGroup),
    /// The record's named mode is not active.
    InactiveMode(String),
    /// A transient mode ends resolution before this record.
    BlockedByTransient(String),
    /// The record's path never matches the route.
    PathMismatch,
    /// No route path admits the record in the active tiers.
    NotEligible,
    /// An earlier route node shadows the record.
    ShadowedAtEarlierRoute {
        /// Binding that wins earlier on the route.
        winner: BindingId,
    },
    /// A higher-priority tier shadows the record.
    ShadowedByTier {
        /// Binding that wins in the higher tier.
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
            Self::BlockedByFrameworkGroup(group) => format!("blocked by framework group {group}"),
            Self::InactiveFrameworkGroup(group) => format!("inactive framework group {group}"),
            Self::InactiveMode(mode) => format!("inactive mode {mode}"),
            Self::BlockedByTransient(mode) => format!("blocked by transient mode {mode}"),
            Self::PathMismatch => "path does not match route".to_string(),
            Self::NotEligible => "not eligible in the active tiers".to_string(),
            Self::ShadowedAtEarlierRoute { .. } => "shadowed at an earlier route node".to_string(),
            Self::ShadowedByTier { .. } => "shadowed by a higher-priority tier".to_string(),
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
    /// Optional tier to match.
    pub tier: Option<BindingTier>,
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
    #[must_use]
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

/// One entry on the mode stack.
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
    /// Bindable intents the application registered.
    intents: IntentCatalog,
    /// Active application modes in push order.
    mode_stack: Vec<ActiveMode>,
    /// Number of mode stack updates, so observers can tell when to resync.
    mode_generation: u64,
    /// Number of binding record updates, so observers can tell when to resync.
    binding_generation: u64,
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
            intents: IntentCatalog::default(),
            mode_stack: Vec::new(),
            mode_generation: 0,
            binding_generation: 0,
            modal_bindings: None,
            next_id: 1,
            next_insertion_id: 1,
        }
    }

    /// Register one bindable intent for this application.
    pub(crate) fn register_intent(&mut self, spec: IntentSpec) -> Result<()> {
        self.intents.register(spec)
    }

    /// Return the registered intents in name order.
    #[must_use]
    pub(crate) fn intents(&self) -> &IntentCatalog {
        &self.intents
    }

    /// Install one binding, and return its identifier with the application
    /// bindings it replaced.
    ///
    /// The tier in `options` decides the semantics. An application tier
    /// replaces any binding with the same tier, input, and path. The framework
    /// tier is idempotent: the same record again returns the existing
    /// identifier, and a different record for the same group, input, and path
    /// is an error. Framework bindings take no script target, because nothing
    /// releases them.
    pub fn bind(
        &mut self,
        input: InputSpec,
        options: BindingOptions,
        target: BindingAction,
    ) -> Result<(BindingId, Vec<(BindingId, BindingAction)>)> {
        if matches!(&target, BindingAction::Menu(mode) if mode.is_empty()) {
            return Err(Error::Invalid("a menu must name its mode".to_string()));
        }
        if let BindingAction::Intent(name) = &target {
            if matches!(input, InputSpec::Mouse(_)) {
                return Err(Error::Invalid(
                    "intent bindings accept keys only".to_string(),
                ));
            }
            if !self.intents.contains(name) {
                return Err(Error::Invalid(format!(
                    "intent {name} is not registered in this application"
                )));
            }
        }
        // An intent is offered before the node's raw key handler.
        let phase = match target {
            BindingAction::Intent(_) => BindingPhase::BeforeWidget,
            BindingAction::Script(_) | BindingAction::Command(_) | BindingAction::Menu(_) => {
                options.phase.unwrap_or_default()
            }
        };
        let path_filter = options.path.as_ref().map_or("", PathFilter::as_str);
        let input = input.normalize();
        if let Some(group) = options.tier.framework_group() {
            validate_record(&options, target.intent())?;
            if matches!(target, BindingAction::Script(_)) {
                return Err(Error::Invalid(
                    "framework bindings take a command or an intent".to_string(),
                ));
            }
            if let Some(existing) = self.records.iter().find(|record| {
                record.tier == options.tier
                    && record.input == input
                    && record.path_filter() == path_filter
            }) {
                if existing.description == options.description
                    && existing.phase == phase
                    && existing.source == options.source
                    && existing.action == target
                {
                    return Ok((existing.id, Vec::new()));
                }
                return Err(Error::Invalid(format!(
                    "conflicting framework binding for {group}, {input}, and {path_filter}"
                )));
            }
        } else {
            validate_application_binding(&options, target.intent())?;
        }
        let path_matcher = options.path.clone().unwrap_or(PathFilter::new("")?);
        // Identifiers are allocated before anything is replaced, so exhaustion
        // leaves the registry as it was.
        let id = self.allocate_binding_id()?;
        let insertion_id = self.allocate_insertion_id()?;
        let removed = if options.tier.is_framework() {
            Vec::new()
        } else {
            self.unbind_input(
                input,
                &BindingSelector {
                    tier: Some(options.tier.clone()),
                    path_filter: Some(path_matcher.as_str()),
                },
            )
        };
        self.touch_bindings();
        self.records.push(BindingRecord {
            id,
            input,
            tier: options.tier,
            description: options.description,
            source: options.source,
            phase,
            action: target,
            insertion_id,
            path_matcher,
        });
        Ok((id, removed))
    }

    /// Remove one application binding.
    pub fn unbind(&mut self, id: BindingId) -> Result<Option<BindingAction>> {
        let Some(index) = self.records.iter().position(|record| record.id == id) else {
            return Ok(None);
        };
        if self.records[index].tier.is_framework() {
            return Err(Error::Invalid(format!(
                "binding {} is framework-owned",
                id.as_u64()
            )));
        }
        let record = self.records.remove(index);
        self.touch_bindings();
        Ok(Some(record.action))
    }

    /// Remove application bindings for an input and selector.
    pub fn unbind_input(
        &mut self,
        input: InputSpec,
        selector: &BindingSelector<'_>,
    ) -> Vec<(BindingId, BindingAction)> {
        let input = input.normalize();
        self.remove_application_records(|record| {
            record.input == input
                && selector
                    .tier
                    .as_ref()
                    .is_none_or(|tier| record.tier == *tier)
                && selector
                    .path_filter
                    .is_none_or(|path| record.path_filter() == path)
        })
    }

    /// Remove all application bindings and reset application modes.
    pub fn clear_application(&mut self) -> Vec<(BindingId, BindingAction)> {
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
    ) -> Vec<(BindingId, BindingAction)> {
        let mut removed = Vec::new();
        self.records.retain(|record| {
            if record.tier.is_framework() || !selected(record) {
                return true;
            }
            removed.push((record.id, record.action.clone()));
            false
        });
        if !removed.is_empty() {
            self.touch_bindings();
        }
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
    pub fn resolve_match(&self, path: &Path, input: InputSpec) -> Option<&BindingRecord> {
        self.candidates(path, input)
            .into_iter()
            .next()
            .map(|candidate| candidate.record)
    }

    /// Return ranked binding candidates at one route node, best first.
    ///
    /// The order is the framework groups an open modal admits, then the
    /// global tier, the newest active modes, and the default tier. A framework
    /// modal admits every global binding, and only its listed intents from
    /// the other application tiers. Within a tier, path specificity and
    /// insertion order rank the candidates. A transient mode ends the walk,
    /// so nothing older follows it, unless a framework group suspends it.
    pub(crate) fn candidates(&self, path: &Path, input: InputSpec) -> Vec<BindingCandidate<'_>> {
        let input = input.normalize();
        let mut out = Vec::new();
        match &self.modal_bindings {
            Some(ModalBindings::Framework { groups, intents }) => {
                for group in *groups {
                    self.extend_framework_group(&mut out, path, input, *group);
                }
                if intents.is_empty() {
                    self.extend_tier(&mut out, path, input, |tier| *tier == BindingTier::Global);
                } else {
                    self.extend_application_tiers(&mut out, path, input, false);
                }
            }
            Some(ModalBindings::Application) | None => {
                self.extend_application_tiers(&mut out, path, input, true);
            }
        }
        out
    }

    /// Append the application tiers in resolution order.
    ///
    /// `transient_ends` is false while a framework group suspends transient
    /// modes, so a waiting mode does not cut off the tiers below it.
    fn extend_application_tiers<'a>(
        &'a self,
        out: &mut Vec<BindingCandidate<'a>>,
        path: &Path,
        input: InputSpec,
        transient_ends: bool,
    ) {
        self.extend_tier(out, path, input, |tier| *tier == BindingTier::Global);
        for mode in self.mode_stack.iter().rev() {
            self.extend_tier(out, path, input, |tier| {
                tier.mode() == Some(mode.name.as_str())
            });
            if transient_ends && mode.transient {
                // A transient mode ends the walk, so older modes and the
                // default tier stay out of reach.
                return;
            }
        }
        self.extend_tier(out, path, input, |tier| *tier == BindingTier::Default);
    }

    /// Append the candidates of one framework group, best first.
    fn extend_framework_group<'a>(
        &'a self,
        out: &mut Vec<BindingCandidate<'a>>,
        path: &Path,
        input: InputSpec,
        group: FrameworkBindingGroup,
    ) {
        self.extend_tier(out, path, input, |tier| {
            tier.framework_group() == Some(group)
        });
    }

    /// Append the candidates of one tier, best first. `in_tier` selects the
    /// tier's records.
    ///
    /// The tier's candidates are ranked in place at the end of `out`, so a
    /// query allocates nothing per tier.
    fn extend_tier<'a>(
        &'a self,
        out: &mut Vec<BindingCandidate<'a>>,
        path: &Path,
        input: InputSpec,
        in_tier: impl Fn(&BindingTier) -> bool,
    ) {
        let start = out.len();
        out.extend(
            self.records
                .iter()
                .filter(|record| record.input == input && in_tier(&record.tier))
                .filter(|record| self.admits_record(record))
                .filter_map(|record| {
                    record
                        .path_matcher
                        .check_match(path)
                        .map(|path_match| BindingCandidate { record, path_match })
                }),
        );
        out[start..].sort_by(|left, right| compare_candidates(*left, *right).reverse());
    }

    /// Return whether the active modal admission admits one record.
    ///
    /// A framework modal admits its groups, every global binding, and
    /// application bindings to its listed intents. Global bindings reach
    /// every modal, as they reach past a transient mode, so a key such as
    /// help works everywhere. Admission ignores route position and widget
    /// state. Route selection and
    /// `candidate_keys` share this predicate.
    pub(crate) fn admits_record(&self, record: &BindingRecord) -> bool {
        match &self.modal_bindings {
            Some(ModalBindings::Framework { groups, intents }) => match &record.tier {
                BindingTier::Framework(record_group) => groups.contains(record_group),
                BindingTier::Global => true,
                BindingTier::Mode(_) | BindingTier::Default => record
                    .action
                    .intent()
                    .is_some_and(|name| intents.contains(&name.as_str())),
            },
            Some(ModalBindings::Application) | None => !record.tier.is_framework(),
        }
    }

    /// Return normalized key inputs the active modal admission admits.
    ///
    /// The result is a safe superset for discovery. It includes dormant action
    /// keys and does not decide which binding wins.
    pub(crate) fn candidate_keys(&self) -> Vec<Key> {
        self.candidate_inputs(|input| match input {
            InputSpec::Key(key) => Some(key),
            InputSpec::Mouse(_) => None,
        })
    }

    /// Return normalized mouse inputs the active modal admission admits.
    pub(crate) fn eligible_mouse_inputs(&self) -> Vec<Mouse> {
        self.candidate_inputs(|input| match input {
            InputSpec::Mouse(mouse) => Some(mouse),
            InputSpec::Key(_) => None,
        })
    }

    /// Return the distinct inputs `select` keeps from the records the active
    /// modal admission admits.
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
        if let Some(group) = self.active_framework_group() {
            if !self.admits_record(record) {
                return RegistryStatus::BlockedByFrameworkGroup(group);
            }
        } else {
            match &record.tier {
                BindingTier::Framework(group) => {
                    return RegistryStatus::InactiveFrameworkGroup(*group);
                }
                BindingTier::Mode(mode)
                    if !self.mode_stack.iter().any(|active| active.name == *mode) =>
                {
                    return RegistryStatus::InactiveMode(mode.clone());
                }
                BindingTier::Global | BindingTier::Mode(_) | BindingTier::Default => {}
            }
            if let Some(mode) = self.transient_blocker(&record.tier) {
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
        if winner.tier != record.tier {
            return RegistryStatus::ShadowedByTier { winner: winner.id };
        }
        let path = &route[winner_route];
        let record_match = record
            .path_matcher
            .check_match(path)
            .expect("record must match its first route node");
        let winner_match = winner
            .path_matcher
            .check_match(path)
            .expect("winner must match its route node");
        if winner_match.score() > record_match.score() {
            RegistryStatus::ShadowedByMoreSpecificPath { winner: winner.id }
        } else {
            RegistryStatus::ShadowedByInsertion { winner: winner.id }
        }
    }

    /// Apply or remove the admission owned by the top modal.
    pub(crate) fn set_modal_bindings(&mut self, bindings: Option<ModalBindings>) {
        self.modal_bindings = bindings;
    }

    /// Return the first framework group the top modal admits, which names
    /// the modal.
    pub fn active_framework_group(&self) -> Option<FrameworkBindingGroup> {
        match self.modal_bindings {
            Some(ModalBindings::Framework { groups, .. }) => groups.first().copied(),
            Some(ModalBindings::Application) | None => None,
        }
    }

    /// Make `mode` the newest active mode. A mode that is already active
    /// moves to the top.
    ///
    /// An empty name is an error. So is entering a mode while a menu waits
    /// for its key, because the mode would bury the menu. A failed call
    /// changes nothing.
    pub fn enter_mode(&mut self, mode: &str) -> Result<()> {
        check_mode_name(mode)?;
        if let Some(menu) = self.transient_mode() {
            return Err(Error::Invalid(format!(
                "mode {mode} cannot enter while menu {menu} waits for its key"
            )));
        }
        if self.mode() == mode {
            return Ok(());
        }
        self.mode_stack.retain(|active| active.name != mode);
        self.mode_stack.push(ActiveMode {
            name: mode.to_string(),
            transient: false,
        });
        self.touch_modes();
        Ok(())
    }

    /// Remove `mode` from the active modes, wherever it is in the stack. A
    /// mode that is not active stays that way, and the call changes nothing.
    ///
    /// An empty name is an error, and so is the name of a menu that waits for
    /// its key: the menu closes when its key arrives.
    pub fn leave_mode(&mut self, mode: &str) -> Result<()> {
        check_mode_name(mode)?;
        if self.transient_mode() == Some(mode) {
            return Err(Error::Invalid(format!(
                "menu {mode} waits for its key, so it cannot be left"
            )));
        }
        let before = self.mode_stack.len();
        self.mode_stack.retain(|active| active.name != mode);
        if self.mode_stack.len() != before {
            self.touch_modes();
        }
        Ok(())
    }

    /// Open `mode` as a menu that waits for one key.
    ///
    /// Keys the menu does not bind never fall through to older modes or the
    /// default tier. Key routing closes the menu before it runs the binding.
    /// A mode that is already active cannot open as a menu, because one name
    /// never names both.
    pub(crate) fn open_menu(&mut self, mode: &str) -> Result<()> {
        check_mode_name(mode)?;
        if self.mode_stack.iter().any(|active| active.name == mode) {
            return Err(Error::Invalid(format!(
                "menu {mode} cannot open while {mode} is an active mode"
            )));
        }
        self.mode_stack.push(ActiveMode {
            name: mode.to_string(),
            transient: true,
        });
        self.touch_modes();
        Ok(())
    }

    /// Close the menu that waits for a key, if one does.
    pub(crate) fn close_menu(&mut self) {
        if self.transient_mode().is_some() {
            self.mode_stack.pop();
            self.touch_modes();
        }
    }

    /// Return the newest active mode.
    pub fn mode(&self) -> &str {
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

    /// Return a count that changes whenever a binding or the mode stack does.
    pub(crate) fn input_generation(&self) -> u64 {
        self.mode_generation.wrapping_add(self.binding_generation)
    }

    /// Record a binding record update.
    fn touch_bindings(&mut self) {
        self.binding_generation = self.binding_generation.wrapping_add(1);
    }

    /// Return the transient mode that keeps resolution from reaching `tier`.
    fn transient_blocker(&self, tier: &BindingTier) -> Option<&str> {
        let floor = match tier {
            BindingTier::Default => 0,
            BindingTier::Mode(mode) => {
                self.mode_stack
                    .iter()
                    .rposition(|active| active.name == *mode)?
                    + 1
            }
            BindingTier::Framework(_) | BindingTier::Global => return None,
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
                .filter(|record| !record.tier.is_framework())
                .cloned()
                .collect(),
            mode_stack: self.mode_stack.clone(),
        }
    }

    /// Restore application records without changing framework state.
    pub(crate) fn restore_application(&mut self, snapshot: ApplicationBindingSnapshot) {
        self.records.retain(|record| record.tier.is_framework());
        self.records.extend(snapshot.records);
        self.records.sort_by_key(|record| record.insertion_id);
        self.mode_stack = snapshot.mode_stack;
        self.touch_modes();
        self.touch_bindings();
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
            .filter(|record| !record.tier.is_framework())
            .filter(|record| !baseline.contains(&record.id))
            .filter_map(|record| match record.action {
                BindingAction::Script(target) => Some(target),
                BindingAction::Command(_) | BindingAction::Intent(_) | BindingAction::Menu(_) => {
                    None
                }
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
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("binding identifier space exhausted".to_string()))?;
        Ok(id)
    }

    /// Allocate one insertion ID.
    fn allocate_insertion_id(&mut self) -> Result<u64> {
        let id = self.next_insertion_id;
        self.next_insertion_id = self
            .next_insertion_id
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("binding insertion space exhausted".to_string()))?;
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
/// `action` is the intent name for an action target, and `None` for a
/// command or callback target.
pub fn validate_application_binding(
    options: &BindingOptions,
    action: Option<&IntentName>,
) -> Result<()> {
    let path_filter = options.path.as_ref().map_or("", PathFilter::as_str);
    validate_application_tier(&options.tier, path_filter)?;
    validate_record(options, action)
}

/// Validate the description and phase of one binding in any tier.
fn validate_record(options: &BindingOptions, action: Option<&IntentName>) -> Result<()> {
    validate_description(&options.description)?;
    if action.is_some() && options.phase == Some(BindingPhase::AfterWidget) {
        return Err(Error::Invalid(
            "intent bindings run before the widget and cannot take after_widget".to_string(),
        ));
    }
    Ok(())
}

/// Validate an application binding tier.
fn validate_application_tier(tier: &BindingTier, path_filter: &str) -> Result<()> {
    match tier {
        BindingTier::Global => {
            if !path_filter.starts_with('/') || !path_filter.ends_with('/') {
                return Err(Error::Invalid(
                    "global bindings require a start- and end-anchored path".to_string(),
                ));
            }
        }
        BindingTier::Mode(mode) if mode.is_empty() => {
            return Err(Error::Invalid(
                "named binding mode cannot be empty".to_string(),
            ));
        }
        BindingTier::Default | BindingTier::Mode(_) => {}
        BindingTier::Framework(_) => {
            return Err(Error::Invalid(
                "application bindings cannot use the framework tier".to_string(),
            ));
        }
    }
    Ok(())
}

/// Validate required user-facing binding text.
fn validate_description(description: &str) -> Result<()> {
    if description.trim().is_empty() {
        Err(Error::Invalid(
            "binding description cannot be empty".to_string(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
