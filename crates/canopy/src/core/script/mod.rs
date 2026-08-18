use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap, HashSet},
    fmt, mem,
    pin::Pin,
    ptr::NonNull,
    rc::Rc,
    result::Result as StdResult,
    sync::{Arc, Mutex},
    time::Duration,
    vec,
};

use futures::executor;
use ruau::{
    bytecode::{BytecodeChunk, CompileOptions},
    session::{FunctionHandle, LifecycleError, RootHandle, Runtime},
    source::{ModuleId, Source, SourceProvider},
    surface::{CheckOptions, PrepareOptions, PreparedGraph, Surface, VmConfig},
    typecheck::{DiagnosticRecord, ModuleDiagnosticRecord, Severity},
    vm::{
        Ambient, CallOptions, Cancel, Limits, NativeModule, RuntimeCapabilities, Scope, SinkQuota,
        StashedClosure, ValueSnapshot,
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::{
    runtime::{Builder as RuntimeBuilder, Handle},
    task::yield_now,
};

use crate::{
    Canopy, ChangeOutcome, FixtureInfo, NodeId,
    commands::{self, ArgValue, CommandArgs, CommandInvocation, CommandSet, CommandSpec},
    core::{
        Core,
        context::{Context, CoreContext, CoreViewContext, FocusScope, ViewContext},
        inputmap,
        termbuf::Cell,
        widget_access,
    },
    error::{self, Result},
    event::{key, mouse},
    geom::{Point, RectI32, Size},
    path::PathFilter,
    style::{AttrSet, Color},
};

/// Base `canopy` scripting API declarations and native registration.
mod base_api;
/// Guards and the bridge between a running script scope and the live `Canopy`.
mod bridge;
/// Render Luau definition files from the current command set.
pub mod defs;
/// Command dispatch and call-into-Luau helpers.
mod dispatch;
/// Conversions between Luau host errors and canopy errors.
mod errors;
/// Luau module roots and their on-disk sources.
mod modules;
/// Builders that turn live canopy state into script-visible records.
mod records;
/// Conversions between Luau values and command argument values.
mod value;

use base_api::{build_base_module, build_owner_modules};
use bridge::*;
pub(crate) use bridge::{in_live_scope, validate_node_handle};
use dispatch::*;
use errors::*;
pub use modules::ScriptModuleRoots;
pub(crate) use modules::ScriptModuleSource;
use records::*;
use value::*;

/// Script identifier.
pub type ScriptId = u64;

/// One-shot fault-injection checkpoints for finalization tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FinalizeStep {
    /// Audited surfaces have been built but not staged.
    SurfacePrepared,
    /// Module declaration conformance has succeeded.
    DeclarationsValidated,
    /// Default binding scripts have compiled.
    DefaultBindingsCompiled,
    /// Startup scripts have compiled.
    StartupScriptsCompiled,
    /// The retained runtime has been built.
    RuntimeBuilt,
    /// A pending script is about to be loaded by sorted index.
    PendingScript(usize),
    /// All roots are loaded and publication is about to commit.
    BeforePublish,
}

/// Stable handle for a stored Luau closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LuauFunctionId(u64);

impl LuauFunctionId {
    /// Construct an identifier that no live closure owns.
    #[cfg(test)]
    pub(crate) fn for_test(id: u64) -> Self {
        Self(id)
    }
}

/// Recorded assertion outcome for a script evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptAssertion {
    /// Whether the assertion passed.
    pub passed: bool,
    /// Assertion message or fallback description.
    pub message: String,
}

/// Structured Luau typecheck diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptCheckDiagnostic {
    /// Diagnostic source name, when the diagnostic belongs to a named source.
    pub source: Option<String>,
    /// Diagnostic severity such as `error` or `warning`.
    pub severity: String,
    /// One-based line number, or zero when the diagnostic is not source-bound.
    pub line: usize,
    /// One-based column number, or zero when the diagnostic is not source-bound.
    pub column: usize,
    /// Human-readable diagnostic message.
    pub message: String,
}

impl ScriptCheckDiagnostic {
    /// Return true if this diagnostic should fail script evaluation.
    pub fn is_error(&self) -> bool {
        self.severity == "error"
    }
}

impl fmt::Display for ScriptCheckDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(source) = &self.source {
            write!(
                f,
                "{source}:{}:{}: {}",
                self.line, self.column, self.message
            )
        } else {
            write!(f, "{}:{}: {}", self.line, self.column, self.message)
        }
    }
}

/// Stable result returned by Luau typechecking APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptCheckResult {
    /// Diagnostics emitted by the checker.
    diagnostics: Vec<ScriptCheckDiagnostic>,
}

impl ScriptCheckResult {
    /// Construct a result from checker diagnostics.
    pub fn from_diagnostics(diagnostics: Vec<ScriptCheckDiagnostic>) -> Self {
        Self { diagnostics }
    }

    /// Return true if there are no failing diagnostics.
    pub fn is_ok(&self) -> bool {
        !self.has_errors()
    }

    /// Return all diagnostics.
    pub fn diagnostics(&self) -> &[ScriptCheckDiagnostic] {
        &self.diagnostics
    }

    /// Consume the result and return its diagnostics.
    pub fn into_diagnostics(self) -> Vec<ScriptCheckDiagnostic> {
        self.diagnostics
    }

    /// Return true when the result contains failing diagnostics.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(ScriptCheckDiagnostic::is_error)
    }

    /// Return failing diagnostics.
    pub fn errors(&self) -> impl Iterator<Item = &ScriptCheckDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.is_error())
    }

    /// Render the failing diagnostics one per line.
    pub fn format_diagnostics(&self) -> String {
        self.errors()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Cached compiled script and its retained root once the host is finalized.
struct Script {
    /// Original source text.
    source: String,
    /// Strict source executed at runtime, including identity and diagnostic metadata.
    runtime_source: Source,
    /// Checked graph artifact used to produce the chunk, when the surface is finalized.
    prepared: Option<PreparedGraph>,
    /// Root loaded into the retained runtime.
    root: Option<RootHandle>,
}

/// Compiled script cache.
#[derive(Default)]
struct ScriptCache {
    /// Cached compiled scripts.
    scripts: HashMap<ScriptId, Script>,
    /// Next script identifier.
    next_script_id: ScriptId,
}

impl ScriptCache {
    /// Construct an empty cache with one-based script ids.
    fn new() -> Self {
        Self {
            next_script_id: 1,
            ..Self::default()
        }
    }

    /// Insert a compiled script and return its id.
    fn insert(
        &mut self,
        source: &str,
        runtime_source: Source,
        prepared: Option<PreparedGraph>,
    ) -> Result<ScriptId> {
        let id = self.next_script_id;
        self.next_script_id = self.next_script_id.checked_add(1).ok_or_else(|| {
            error::Error::InvalidOperation("script identifier space exhausted".to_string())
        })?;
        self.scripts.insert(
            id,
            Script {
                source: source.to_string(),
                runtime_source,
                prepared,
                root: None,
            },
        );
        Ok(id)
    }

    /// Return true when the cache holds a script.
    fn contains(&self, id: ScriptId) -> bool {
        self.scripts.contains_key(&id)
    }

    /// Return the retained root for a script, if it is loaded.
    fn root(&self, id: ScriptId) -> Option<RootHandle> {
        self.scripts.get(&id).and_then(|script| script.root.clone())
    }

    /// Return the prepared graph for a script.
    fn prepared(&self, id: ScriptId) -> Option<PreparedGraph> {
        self.scripts
            .get(&id)
            .and_then(|script| script.prepared.clone())
    }

    /// Record a script's checked graph artifact.
    fn set_prepared(&mut self, id: ScriptId, prepared: PreparedGraph) {
        if let Some(script) = self.scripts.get_mut(&id) {
            script.prepared = Some(prepared);
        }
    }

    /// Record the loaded root for a script.
    fn set_root(&mut self, id: ScriptId, root: RootHandle) {
        if let Some(script) = self.scripts.get_mut(&id) {
            script.root = Some(root);
        }
    }

    /// Clear every loaded root after source invalidation.
    fn clear_roots(&mut self) {
        for script in self.scripts.values_mut() {
            script.root = None;
            script.prepared = None;
        }
    }

    /// Return the original source for a script.
    fn source(&self, id: ScriptId) -> Option<String> {
        self.scripts.get(&id).map(|script| script.source.clone())
    }

    /// Return the source executed for a script.
    fn runtime_source(&self, id: ScriptId) -> Option<Source> {
        self.scripts
            .get(&id)
            .map(|script| script.runtime_source.clone())
    }
}

/// Stored Luau closure with a stable host-side id. The stash pins the closure
/// in the VM registry; dropping it queues the release for the VM's next step.
struct StoredFunction {
    /// Pending VM stash or retained generational handle.
    target: StoredFunctionTarget,
}

/// Callback state before and after promotion into the retained runtime.
#[derive(Clone)]
enum StoredFunctionTarget {
    /// Stash created during the currently active VM invocation.
    Pending(StashedClosure),
    /// Generational retained-runtime handle.
    Retained(FunctionHandle),
}

/// Stored Luau closure registry.
#[derive(Default)]
struct ClosureRegistry {
    /// Stored Luau closures keyed by stable id.
    functions: HashMap<LuauFunctionId, StoredFunction>,
    /// Next stored function identifier.
    next_function_id: u64,
    /// Retained handles queued for release after the current VM invocation.
    released: Vec<FunctionHandle>,
}

impl ClosureRegistry {
    /// Construct an empty registry with one-based function ids.
    fn new() -> Self {
        Self {
            next_function_id: 1,
            ..Self::default()
        }
    }

    /// Insert a stashed closure and return its stable function id.
    fn insert(&mut self, stashed: StashedClosure) -> Result<LuauFunctionId> {
        let id = LuauFunctionId(self.next_function_id);
        self.next_function_id = self.next_function_id.checked_add(1).ok_or_else(|| {
            error::Error::InvalidOperation("closure identifier space exhausted".to_string())
        })?;
        self.functions.insert(
            id,
            StoredFunction {
                target: StoredFunctionTarget::Pending(stashed),
            },
        );
        Ok(id)
    }

    /// Return a stored function target.
    fn target(&self, id: LuauFunctionId) -> Option<StoredFunctionTarget> {
        self.functions
            .get(&id)
            .map(|function| function.target.clone())
    }

    /// Remove a stored function and queue its retained handle for release.
    fn remove(&mut self, id: LuauFunctionId) {
        if let Some(function) = self.functions.remove(&id)
            && let StoredFunctionTarget::Retained(handle) = function.target
        {
            self.released.push(handle);
        }
    }

    /// Promote pending stashes and release removed handles between VM invocations.
    fn synchronize(&mut self, runtime: &mut Runtime) -> StdResult<(), LifecycleError> {
        for handle in self.released.drain(..) {
            match runtime.release(&handle) {
                Ok(()) | Err(LifecycleError::StaleHandle { .. }) => {}
                Err(error) => return Err(error),
            }
        }
        for function in self.functions.values_mut() {
            if let StoredFunctionTarget::Pending(stash) = &function.target {
                function.target = StoredFunctionTarget::Retained(runtime.retain(stash.clone()));
            }
        }
        Ok(())
    }

    /// Forget every callback after a source generation change.
    fn clear(&mut self) {
        self.functions.clear();
        self.released.clear();
    }
}

/// Shared mutable host state.
#[derive(Default)]
struct LuauState {
    /// Compiled script cache.
    scripts: ScriptCache,
    /// Stored closure registry.
    closures: ClosureRegistry,
    /// Log messages emitted by the most recent script evaluation.
    logs: Vec<String>,
    /// Assertion results emitted by the most recent script evaluation.
    assertions: Vec<ScriptAssertion>,
    /// Audited script surface used for checks and VM construction.
    surface: Option<Surface>,
    /// Script surface with startup-root global obligations.
    startup_surface: Option<Surface>,
    /// Typed globals every startup script root must define.
    startup_requirements: Vec<StartupRequirement>,
    /// Whether the command surface has been finalized.
    finalized: bool,
    /// Whether a top-level async eval is currently driving the retained VM.
    active_eval: bool,
    /// Deferred hooks to execute after the first live render.
    on_start_hooks: Vec<LuauFunctionId>,
    /// Optional one-shot finalization failure used by deterministic tests.
    finalize_failure: Option<FinalizeStep>,
}

/// Luau host state shared by the canopy runtime.
#[derive(Clone)]
pub(crate) struct LuauHost {
    /// Retained Ruau runtime, built by `finalize()`.
    runtime: Rc<RefCell<Option<Runtime>>>,
    /// Shared mutable host state.
    state: Rc<RefCell<LuauState>>,
}

impl fmt::Debug for LuauHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LuauHost").finish_non_exhaustive()
    }
}

/// Builder-level execution ceilings for the retained VM. Gas bounds runaway
/// scripts even without an explicit timeout; the memory cap bounds script
/// allocations. Wall-clock timeouts are layered per invocation via `Cancel`.
fn default_vm_limits() -> Limits {
    Limits {
        gas: Some(500_000_000),
        max_memory_bytes: Some(256 * 1024 * 1024),
        ..Limits::unlimited()
    }
}

/// Per-invocation `print` output quota: enough for any reasonable diagnostic
/// run, small enough that a print loop cannot grow the log without bound.
const PRINT_QUOTA: SinkQuota = SinkQuota {
    max_bytes: Some(256 * 1024),
    max_calls: Some(4096),
};

/// Per-invocation limit override: builder defaults, plus a wall-clock watchdog
/// when the caller requested a timeout.
fn invocation_limits(timeout: Option<Duration>) -> Limits {
    Limits {
        cancel: timeout.map(Cancel::after),
        ..Limits::unlimited()
    }
}

/// Build per-invocation options and capture script output for host diagnostics.
fn invocation_options(
    timeout: Option<Duration>,
    print_lines: &Arc<Mutex<Vec<String>>>,
) -> CallOptions {
    let sink_lines = Arc::clone(print_lines);
    CallOptions::new()
        .limits(invocation_limits(timeout))
        .print_sink_with_quota(
            Box::new(move |bytes: &[u8]| {
                let line = String::from_utf8_lossy(bytes).trim_end().to_string();
                tracing::info!("{line}");
                if let Ok(mut lines) = sink_lines.lock() {
                    lines.push(line);
                }
            }),
            PRINT_QUOTA,
        )
}

/// Prefix scripts with strict mode unless they already declare a mode.
fn strict_source(source: &str) -> String {
    let trimmed = source.trim_start();
    if trimmed.starts_with("--!") {
        source.to_string()
    } else {
        format!("--!strict\n{source}")
    }
}

/// Build one named strict source for checking, compilation, loading, and tracebacks.
fn named_source(module_id: impl Into<ModuleId>, source: &str) -> Source {
    Source::text(module_id, strict_source(source))
}

/// Replace source text while retaining module identity and diagnostic metadata.
fn source_with_text(source: &Source, text: String) -> Source {
    Source::text(source.id().clone(), text).with_metadata(source.metadata().clone())
}

/// Apply Canopy strict mode to a source while preserving its full identity.
fn strict_named_source(source: &Source) -> Result<Source> {
    let text = source.as_str().ok_or_else(|| {
        error::Error::Invalid(format!(
            "script source {} is not valid UTF-8",
            source.display_name()
        ))
    })?;
    Ok(source_with_text(source, strict_source(text)))
}

/// Runtime source for a startup script: evaluate the root, then call its
/// obligated setup entry point from the same chunk environment.
fn startup_runtime_source(source: &str) -> String {
    let mut runtime = String::with_capacity(source.len() + "\nsetup()\n".len());
    runtime.push_str(source);
    if !runtime.ends_with('\n') {
        runtime.push('\n');
    }
    runtime.push_str("setup()\n");
    runtime
}

/// Compile Luau source under the canopy profile before the surface is finalized.
fn compile_chunk(source: &str) -> Result<BytecodeChunk> {
    RuntimeCapabilities::default()
        .compile_source(source.as_bytes(), &CompileOptions::new())
        .map_err(|err| compile_error_to_canopy(&err))
}

/// Convert one owned Ruau diagnostic record to Canopy's stable script shape.
pub(crate) fn diagnostic_record_to_script(
    source: Option<String>,
    diagnostic: DiagnosticRecord,
) -> ScriptCheckDiagnostic {
    let (line, column) = if diagnostic.primary_location.is_missing() {
        (0, 0)
    } else {
        let begin = diagnostic.primary_location.begin;
        (begin.line as usize + 1, begin.column as usize + 1)
    };
    ScriptCheckDiagnostic {
        source,
        severity: match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warning | Severity::Info => "warning",
        }
        .to_string(),
        line,
        column,
        message: diagnostic.message,
    }
}

/// Convert one module-qualified record through the shared Canopy adapter.
fn module_diagnostic_to_script(diagnostic: ModuleDiagnosticRecord) -> ScriptCheckDiagnostic {
    diagnostic_record_to_script(Some(diagnostic.display_name), diagnostic.diagnostic)
}

/// Check a named source and convert its owned diagnostics through the Canopy adapter.
fn check_source_with_surface(surface: &Surface, source: &Source) -> ScriptCheckResult {
    let source_name = source.display_name().to_string();
    let checked = surface.check(source, CheckOptions::default());
    ScriptCheckResult {
        diagnostics: checked
            .diagnostics()
            .records()
            .map(|diagnostic| diagnostic_record_to_script(Some(source_name.clone()), diagnostic))
            .collect(),
    }
}

/// Return the Luau-safe global name for a command owner.
pub(crate) fn luau_global_owner_name(owner: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "and", "break", "continue", "do", "else", "elseif", "end", "export", "false", "for",
        "function", "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true",
        "type", "until", "while",
    ];
    if KEYWORDS.contains(&owner) {
        format!("{owner}_cmd")
    } else {
        owner.to_string()
    }
}

impl LuauHost {
    /// Construct a new Luau host. The runtime is built by `finalize()`, once
    /// the full command surface is known.
    pub fn new() -> Self {
        Self {
            runtime: Rc::new(RefCell::new(None)),
            state: Rc::new(RefCell::new(LuauState::new())),
        }
    }

    /// Return true if the API has been finalized.
    pub fn is_finalized(&self) -> bool {
        self.state.borrow().finalized
    }

    /// Configure a one-shot finalization failure.
    #[cfg(test)]
    pub(crate) fn inject_finalize_failure(&self, step: FinalizeStep) {
        self.state.borrow_mut().finalize_failure = Some(step);
    }

    /// Fail once when the configured finalization checkpoint is reached.
    pub(crate) fn finalize_checkpoint(&self, step: FinalizeStep) -> Result<()> {
        let mut state = self.state.borrow_mut();
        if state.finalize_failure == Some(step) {
            state.finalize_failure = None;
            Err(error::Error::Script(format!(
                "fault injected at finalization step {step:?}"
            )))
        } else {
            Ok(())
        }
    }

    /// Return the finalized script surface.
    pub(crate) fn surface(&self) -> Option<Surface> {
        self.state.borrow().surface.clone()
    }

    /// Add a required global definition for startup script roots.
    pub fn require_startup_global(&self, name: &str, type_text: &str) -> Result<()> {
        let mut state = self.state.borrow_mut();
        if state.finalized {
            return Err(error::Error::InvalidOperation(
                "startup global requirements are sealed after finalize_api()".into(),
            ));
        }
        if name.trim().is_empty() {
            return Err(error::Error::Invalid(
                "startup global requirement name cannot be empty".into(),
            ));
        }
        state.startup_requirements.push(StartupRequirement {
            name: name.to_string(),
            type_text: type_text.to_string(),
        });
        Ok(())
    }

    /// Mark the retained runtime as busy with a top-level async evaluation.
    fn begin_active_eval(&self) -> Result<ActiveEvalGuard> {
        let mut state = self.state.borrow_mut();
        if state.active_eval {
            return Err(error::Error::ScriptStructured {
                kind: error::ScriptErrorKind::ScriptBusy,
                command: None,
                owner: None,
                message: "a script evaluation is already active".to_string(),
            });
        }
        state.active_eval = true;
        Ok(ActiveEvalGuard {
            state: Rc::clone(&self.state),
        })
    }

    /// Type-check a named Luau source against the finalized canopy API.
    pub fn check_script(&self, source_name: &str, source: &str) -> Result<ScriptCheckResult> {
        let surface = self.state.borrow().surface.clone().ok_or_else(|| {
            error::Error::InvalidOperation(
                "cannot type-check scripts before finalize_api()".to_string(),
            )
        })?;
        let source = named_source(ModuleId::new(source_name), source);
        Ok(check_source_with_surface(&surface, &source))
    }

    /// Type-check a named startup script against its runtime module identity.
    fn check_startup_source(&self, source: &Source) -> Result<ScriptCheckResult> {
        let surface = self.state.borrow().startup_surface.clone().ok_or_else(|| {
            error::Error::InvalidOperation(
                "cannot type-check startup scripts before finalize_api()".to_string(),
            )
        })?;
        Ok(check_source_with_surface(&surface, source))
    }

    /// Enforce the startup-script obligation before compiling a startup root.
    fn typecheck_startup_source(&self, source: &Source) -> Result<()> {
        let result = self.check_startup_source(source)?;
        if result.is_ok() {
            Ok(())
        } else {
            Err(error::Error::Parse(error::ParseError::new(
                result.format_diagnostics(),
            )))
        }
    }

    /// Enforce Luau type checking for finalized APIs in debug builds.
    fn maybe_typecheck(&self, source: &str) -> Result<()> {
        if !cfg!(debug_assertions) || !self.is_finalized() {
            return Ok(());
        }
        let result = self.check_script("canopy/eval", source)?;
        if result.is_ok() {
            Ok(())
        } else {
            Err(error::Error::Parse(error::ParseError::new(
                result.format_diagnostics(),
            )))
        }
    }

    /// Clear recorded logs and assertions for the next script evaluation.
    fn clear_diagnostics(&self) {
        let mut state = self.state.borrow_mut();
        state.logs.clear();
        state.assertions.clear();
    }

    /// Append a log line to the current evaluation state.
    fn push_log(&self, message: String) {
        self.state.borrow_mut().logs.push(message);
    }

    /// Append an assertion result to the current evaluation state.
    fn push_assertion(&self, passed: bool, message: String) {
        self.state
            .borrow_mut()
            .assertions
            .push(ScriptAssertion { passed, message });
    }

    /// Drain deferred `on_start` hooks in registration order.
    pub fn drain_on_start_hooks(&self) -> Vec<LuauFunctionId> {
        self.state.borrow_mut().drain_on_start_hooks()
    }

    /// Snapshot deferred startup hooks without consuming them.
    pub(crate) fn on_start_hooks(&self) -> Vec<LuauFunctionId> {
        self.state.borrow().on_start_hooks.clone()
    }

    /// Restore a deferred startup-hook snapshot and return the replaced queue.
    pub(crate) fn replace_on_start_hooks(&self, hooks: Vec<LuauFunctionId>) -> Vec<LuauFunctionId> {
        mem::replace(&mut self.state.borrow_mut().on_start_hooks, hooks)
    }

    /// Return true when deferred `on_start` hooks are pending.
    pub fn has_on_start_hooks(&self) -> bool {
        !self.state.borrow().on_start_hooks.is_empty()
    }

    /// Take the logs collected during the most recent evaluation.
    pub fn take_logs(&self) -> Vec<String> {
        mem::take(&mut self.state.borrow_mut().logs)
    }

    /// Return log lines collected during the most recent evaluation.
    pub fn logs(&self) -> Vec<String> {
        self.state.borrow().logs.clone()
    }

    /// Take the assertions collected during the most recent evaluation.
    pub fn take_assertions(&self) -> Vec<ScriptAssertion> {
        mem::take(&mut self.state.borrow_mut().assertions)
    }

    /// Return assertions collected during the most recent evaluation.
    pub fn assertions(&self) -> Vec<ScriptAssertion> {
        self.state.borrow().assertions.clone()
    }

    /// Return current (log, assertion) counts for journal baselines.
    pub(crate) fn diagnostics_counts(&self) -> (usize, usize) {
        let state = self.state.borrow();
        (state.logs.len(), state.assertions.len())
    }

    /// Audit and stage the command and startup surfaces without publishing a runtime.
    ///
    /// Returns the rendered Luau definition file for the installed modules.
    pub(crate) fn prepare_finalize(
        &self,
        commands: &CommandSet,
        default_binding_owners: &BTreeSet<String>,
        extra_modules: &[Arc<dyn NativeModule>],
        module_source: Option<Arc<dyn SourceProvider>>,
        fixtures: &[FixtureInfo],
    ) -> Result<String> {
        if self.is_finalized() || self.state.borrow().surface.is_some() {
            return Err(error::Error::InvalidOperation(
                "Luau API finalization is already active or complete".into(),
            ));
        }
        let mut modules = vec![build_base_module()?];
        modules.extend(extra_modules.iter().map(Arc::clone));
        modules.extend(build_owner_modules(commands, default_binding_owners)?);
        let definitions = defs::render_definitions(&modules, fixtures);

        let mut builder = Surface::builder();
        if let Some(source) = module_source {
            builder = builder.module_source(source);
        }
        for module in modules {
            builder = builder.module(module);
        }
        let surface = builder.build().map_err(|err| {
            error::Error::Script(format!("building script surface failed: {err}"))
        })?;
        let mut startup_surface = surface.clone();
        for requirement in &self.state.borrow().startup_requirements {
            startup_surface
                .require_global(&requirement.name, &requirement.type_text)
                .map_err(|err| {
                    error::Error::Script(format!("building startup script checker failed: {err}"))
                })?;
        }
        self.finalize_checkpoint(FinalizeStep::SurfacePrepared)?;
        let mut state = self.state.borrow_mut();
        state.surface = Some(surface);
        state.startup_surface = Some(startup_surface);
        Ok(definitions)
    }

    /// Build and publish the retained runtime after every other preparation step succeeds.
    pub(crate) fn publish_finalize(&self) -> Result<()> {
        if self.is_finalized() {
            return Err(error::Error::InvalidOperation(
                "Luau API already finalized".into(),
            ));
        }
        let surface = self.state.borrow().surface.clone().ok_or_else(|| {
            error::Error::InvalidOperation("Luau API surface is not prepared".into())
        })?;
        let mut runtime = Runtime::new(
            surface.clone(),
            &VmConfig::untrusted(Ambient::production(0), default_vm_limits()),
        )
        .map_err(|err| error::Error::Script(format!("building script VM failed: {err}")))?;
        self.finalize_checkpoint(FinalizeStep::RuntimeBuilt)?;

        let pending = self
            .state
            .borrow()
            .scripts
            .scripts
            .iter()
            .map(|(id, script)| (*id, script.runtime_source.clone(), script.prepared.clone()))
            .collect::<Vec<_>>();
        let mut pending = pending;
        pending.sort_by_key(|(id, _, _)| *id);
        let mut loaded = Vec::with_capacity(pending.len());
        for (index, (id, source, prepared)) in pending.into_iter().enumerate() {
            self.finalize_checkpoint(FinalizeStep::PendingScript(index))?;
            let prepared = match prepared {
                Some(prepared) => prepared,
                None => surface
                    .prepare_graph_ready(source)
                    .map_err(|error| prepare_graph_error_to_canopy(&error))?,
            };
            let root = runtime.load_prepared(&prepared).map_err(|error| {
                retained_runtime_error_to_canopy(&error, "loading prepared script", None)
            })?;
            loaded.push((id, prepared, root));
        }
        self.finalize_checkpoint(FinalizeStep::BeforePublish)?;
        *self.runtime.borrow_mut() = Some(runtime);
        {
            let mut state = self.state.borrow_mut();
            for (id, prepared, root) in loaded {
                state.scripts.set_prepared(id, prepared);
                state.scripts.set_root(id, root);
            }
            state.publish();
        }
        Ok(())
    }

    /// Discard a failed finalization attempt and scripts compiled only for that attempt.
    pub(crate) fn abort_finalize(&self, existing_scripts: &HashSet<ScriptId>) {
        *self.runtime.borrow_mut() = None;
        let mut state = self.state.borrow_mut();
        state
            .scripts
            .scripts
            .retain(|id, _| existing_scripts.contains(id));
        state.surface = None;
        state.startup_surface = None;
        state.finalized = false;
    }

    /// Return the script IDs that existed before a finalization attempt.
    pub(crate) fn script_ids(&self) -> HashSet<ScriptId> {
        self.state
            .borrow()
            .scripts
            .scripts
            .keys()
            .copied()
            .collect()
    }

    /// Compile a script and return its id.
    pub fn compile(&self, source: &str) -> Result<ScriptId> {
        self.compile_source(&Source::text(ModuleId::new(b"canopy".to_vec()), source))
    }

    /// Compile a source while preserving its module identity and diagnostic metadata.
    pub(crate) fn compile_source(&self, source: &Source) -> Result<ScriptId> {
        let original = source.as_str().ok_or_else(|| {
            error::Error::Invalid(format!(
                "script source {} is not valid UTF-8",
                source.display_name()
            ))
        })?;
        let original = original.to_string();
        let runtime_source = strict_named_source(source)?;
        let prepared = if let Some(surface) = self.state.borrow().surface.clone() {
            Some(
                surface
                    .prepare_graph_ready(runtime_source.clone())
                    .map_err(|error| prepare_graph_error_to_canopy(&error))?,
            )
        } else {
            self.maybe_typecheck(&original)?;
            // Compiling before finalization proves the source is well formed; the retained
            // runtime recompiles it from the prepared graph.
            compile_chunk(runtime_source.as_str().expect("strict source is UTF-8"))?;
            None
        };
        let sid = self
            .state
            .borrow_mut()
            .scripts
            .insert(&original, runtime_source, prepared)?;
        if self.is_finalized() {
            self.load_script(sid)?;
        }
        Ok(sid)
    }

    /// Compile a startup script after enforcing its typed entry-point contract.
    pub fn compile_startup_named(
        &self,
        source: &str,
        module_name: impl AsRef<[u8]>,
    ) -> Result<ScriptId> {
        self.compile_startup_source(&Source::text(
            ModuleId::new(module_name.as_ref().to_vec()),
            source,
        ))
    }

    /// Compile a startup source while preserving its mounted identity and metadata.
    pub(crate) fn compile_startup_source(&self, source: &Source) -> Result<ScriptId> {
        let original = source.as_str().ok_or_else(|| {
            error::Error::Invalid(format!(
                "startup source {} is not valid UTF-8",
                source.display_name()
            ))
        })?;
        let original = original.to_string();
        self.typecheck_startup_source(&strict_named_source(source)?)?;
        let runtime_source = source_with_text(source, startup_runtime_source(&original));
        let runtime_source = strict_named_source(&runtime_source)?;
        let surface = self.state.borrow().surface.clone().ok_or_else(|| {
            error::Error::InvalidOperation(
                "cannot compile startup scripts before finalize_api()".to_string(),
            )
        })?;
        let prepared = surface
            .prepare_graph_ready(runtime_source.clone())
            .map_err(|error| prepare_graph_error_to_canopy(&error))?;
        let sid =
            self.state
                .borrow_mut()
                .scripts
                .insert(&original, runtime_source, Some(prepared))?;
        if self.is_finalized() {
            self.load_script(sid)?;
        }
        Ok(sid)
    }

    /// Load a compiled script into the retained runtime.
    fn load_script(&self, sid: ScriptId) -> Result<RootHandle> {
        let (source, prepared) = {
            let state = self.state.borrow();
            (
                state.scripts.runtime_source(sid),
                state.scripts.prepared(sid),
            )
        };
        let source =
            source.ok_or_else(|| error::Error::Script(format!("script {sid} not found")))?;
        let mut runtime_cell = self.runtime.try_borrow_mut().map_err(|_| {
            error::Error::Script("cannot load a script while the script VM is executing".into())
        })?;
        let runtime = runtime_cell.as_mut().ok_or_else(|| {
            error::Error::InvalidOperation("cannot load scripts before finalize_api()".to_string())
        })?;
        if runtime.invalidate_if_source_changed().is_some() {
            let mut state = self.state.borrow_mut();
            state.scripts.clear_roots();
            state.closures.clear();
        }
        let prepared = match prepared {
            Some(prepared) => prepared,
            None => runtime
                .prepare_ready(source, PrepareOptions::new())
                .map_err(|error| prepare_graph_error_to_canopy(&error))?,
        };
        let root = runtime.load_prepared(&prepared).map_err(|error| {
            retained_runtime_error_to_canopy(&error, "loading prepared script", None)
        })?;
        let mut state = self.state.borrow_mut();
        state.scripts.set_prepared(sid, prepared);
        state.scripts.set_root(sid, root.clone());
        Ok(root)
    }

    /// Return the loaded root for a script, reloading after source invalidation.
    fn loaded_root(&self, sid: ScriptId) -> Result<RootHandle> {
        let invalidated = {
            let mut runtime = self.runtime.try_borrow_mut().map_err(|_| {
                error::Error::Script(
                    "cannot inspect scripts while the script VM is executing".into(),
                )
            })?;
            runtime
                .as_mut()
                .and_then(Runtime::invalidate_if_source_changed)
                .is_some()
        };
        if invalidated {
            let mut state = self.state.borrow_mut();
            state.scripts.clear_roots();
            state.closures.clear();
        }
        if let Some(root) = self.state.borrow().scripts.root(sid) {
            return Ok(root);
        }
        if !self.state.borrow().scripts.contains(sid) {
            return Err(error::Error::Script(format!("script {sid} not found")));
        }
        self.load_script(sid)
    }

    /// Execute a compiled script and return its value.
    ///
    /// A `timeout` bounds cooperative execution; `None` runs without one.
    pub fn execute(
        &self,
        canopy: &mut Canopy,
        node_id: impl Into<NodeId>,
        sid: ScriptId,
        timeout: Option<Duration>,
    ) -> Result<ArgValue> {
        let node_id = node_id.into();
        let root = self.loaded_root(sid)?;
        // Diagnostics accumulate per top-level evaluation: a nested run
        // triggered from inside a live script must not erase the logs and
        // assertions the outer evaluation has already collected.
        if !in_live_scope(canopy) {
            self.clear_diagnostics();
        }
        let label = format!("script {sid} on node {node_id:?}");
        self.run_target(canopy, node_id, &CallTarget::Root(root), &label, timeout)
    }

    /// Execute a compiled script inside an existing VM scope.
    pub(crate) fn execute_in_scope(
        &self,
        scope: &Scope<'_>,
        node_id: impl Into<NodeId>,
        sid: ScriptId,
    ) -> Result<()> {
        let root = self.state.borrow().scripts.root(sid).ok_or_else(|| {
            error::Error::Script(format!(
                "script {sid} is not loaded for reentrant execution"
            ))
        })?;
        let node_id = node_id.into();
        let label = format!("script {sid} on node {node_id:?}");
        self.run_target_in_scope(scope, node_id, &CallTarget::Root(root), &label, None)
            .map(|_| ())
    }

    /// Run a script callable through a fresh limited scope step.
    fn run_target(
        &self,
        canopy: &mut Canopy,
        node_id: NodeId,
        target: &CallTarget,
        label: &str,
        timeout: Option<Duration>,
    ) -> Result<ArgValue> {
        if let CallTarget::Root(root) = target {
            return self.run_root_async(canopy, node_id, root, label, timeout);
        }
        let _active_eval = self.begin_active_eval()?;
        let runtime = self.runtime.clone();
        let mut runtime_cell = runtime.try_borrow_mut().map_err(|_| {
            error::Error::Script("script VM re-entered without a live scope".into())
        })?;
        let runtime = runtime_cell.as_mut().ok_or_else(|| {
            error::Error::InvalidOperation(
                "cannot execute scripts before finalize_api()".to_string(),
            )
        })?;
        let print_lines = Arc::new(Mutex::new(Vec::new()));
        let options = invocation_options(timeout, &print_lines);
        let mut outcome: Option<Result<ArgValue>> = None;
        let step = runtime.step_with_context(canopy, &options, |scope| {
            let _guard = match ScriptAnchorGuard::push(scope, node_id) {
                Ok(guard) => guard,
                Err(error) => return Err(error),
            };
            let result = match target.resolve(scope, label, timeout) {
                Ok(function) => call_in_scope(scope, function, label, timeout),
                Err(err) => Err(err),
            };
            outcome = Some(result);
            Ok(())
        });
        let synchronized = self.synchronize_closures(runtime, label, timeout);
        self.push_print_lines(&print_lines);
        match step {
            Ok(()) => {
                synchronized?;
                outcome.unwrap_or_else(|| {
                    Err(error::Error::Script(format!("{label} produced no result")))
                })
            }
            Err(error) => Err(retained_runtime_error_to_canopy(&error, label, timeout)),
        }
    }

    /// Run a retained root through Ruau's async owned-result driver.
    fn run_root_async(
        &self,
        canopy: &mut Canopy,
        node_id: NodeId,
        root: &RootHandle,
        label: &str,
        timeout: Option<Duration>,
    ) -> Result<ArgValue> {
        let _active_eval = self.begin_active_eval()?;
        let runtime = self.runtime.clone();
        let mut runtime_cell = runtime.try_borrow_mut().map_err(|_| {
            error::Error::Script("script VM re-entered without a live scope".into())
        })?;
        let runtime = runtime_cell.as_mut().ok_or_else(|| {
            error::Error::InvalidOperation(
                "cannot execute scripts before finalize_api()".to_string(),
            )
        })?;
        let print_lines = Arc::new(Mutex::new(Vec::new()));
        let options = invocation_options(timeout, &print_lines);
        canopy.script_context_stack.push(node_id);
        let future = runtime.run_with_context(root, canopy, options);
        let outcome = if Handle::try_current().is_ok() {
            executor::block_on(future)
        } else {
            let runtime = RuntimeBuilder::new_current_thread()
                .enable_time()
                .build()
                .map_err(|err| {
                    error::Error::Script(format!("script async runtime failed: {err}"))
                })?;
            runtime.block_on(future)
        };
        let popped = canopy.script_context_stack.pop();
        debug_assert_eq!(popped, Some(node_id));
        let synchronized = self.synchronize_closures(runtime, label, timeout);
        self.push_print_lines(&print_lines);
        match outcome {
            Ok(values) => {
                synchronized?;
                let value = values.first().unwrap_or(&ValueSnapshot::Nil);
                marshaled_to_arg_value(value)
                    .map_err(|message| error::Error::Script(format!("{label}: {message}")))
            }
            Err(error) => Err(retained_runtime_error_to_canopy(&error, label, timeout)),
        }
    }

    /// Run a script callable inside an existing live scope.
    fn run_target_in_scope<'s>(
        &self,
        scope: &Scope<'s>,
        node_id: NodeId,
        target: &CallTarget,
        label: &str,
        timeout: Option<Duration>,
    ) -> Result<ArgValue> {
        let _guard = ScriptAnchorGuard::push(scope, node_id)
            .map_err(|err| runtime_error_to_canopy(&err, label, timeout))?;
        let function = target.resolve(scope, label, timeout)?;
        call_in_scope(scope, function, label, timeout)
    }

    /// Promote and release callback handles between retained-runtime invocations.
    fn synchronize_closures(
        &self,
        runtime: &mut Runtime,
        label: &str,
        timeout: Option<Duration>,
    ) -> Result<()> {
        self.state
            .borrow_mut()
            .closures
            .synchronize(runtime)
            .map_err(|error| retained_runtime_error_to_canopy(&error, label, timeout))
    }

    /// Push lines captured from `print` into the diagnostics buffer.
    fn push_print_lines(&self, print_lines: &Mutex<Vec<String>>) {
        if let Ok(mut lines) = print_lines.lock() {
            for line in lines.drain(..) {
                self.push_log(line);
            }
        }
    }

    /// Return the source for a cached script.
    pub fn script_source(&self, sid: ScriptId) -> Option<String> {
        self.state.borrow().scripts.source(sid)
    }

    /// Store a stashed Luau closure and return a stable host-side id.
    fn store_function(&self, stashed: StashedClosure) -> Result<LuauFunctionId> {
        self.state.borrow_mut().closures.insert(stashed)
    }

    /// Release a stored function reference. The underlying registry pin is
    /// released on the VM's next step.
    pub fn release_function(&self, id: LuauFunctionId) {
        self.state.borrow_mut().closures.remove(id);
    }

    /// Execute a stored Luau closure in the current script context.
    pub fn call_function(
        &self,
        canopy: &mut Canopy,
        node_id: NodeId,
        id: LuauFunctionId,
    ) -> Result<()> {
        let target = self
            .state
            .borrow()
            .closures
            .target(id)
            .ok_or_else(|| error::Error::Script(format!("Luau function {id:?} not found")))?;
        let label = format!("Luau binding on node {node_id:?}");
        self.run_target(canopy, node_id, &CallTarget::Stored(target), &label, None)
            .map(|_| ())
    }

    /// Execute a stored Luau closure inside an existing live scope.
    pub(crate) fn call_function_in_scope(
        &self,
        scope: &Scope<'_>,
        node_id: NodeId,
        id: LuauFunctionId,
    ) -> Result<()> {
        let target = self
            .state
            .borrow()
            .closures
            .target(id)
            .ok_or_else(|| error::Error::Script(format!("Luau function {id:?} not found")))?;
        let label = format!("Luau binding on node {node_id:?}");
        self.run_target_in_scope(scope, node_id, &CallTarget::Stored(target), &label, None)
            .map(|_| ())
    }
}

/// Tests for the Luau scripting host.
#[cfg(test)]
mod tests;
