use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    future::Future,
    mem,
    pin::Pin,
    ptr::NonNull,
    rc::Rc,
    result::Result as StdResult,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
    vec,
};

use futures::executor;
use ruau::{
    bytecode::{BytecodeChunk, CompileError, CompileOptions},
    module::{self, Binding},
    session::{FunctionHandle, LifecycleError, RootHandle, Runtime},
    source::{ModuleId, Source, SourceProvider},
    surface::{CheckOptions, PrepareGraphError, PrepareOptions, PreparedGraph, Surface, VmConfig},
    typecheck::{DiagnosticRecord, ModuleDiagnosticRecord, Severity},
    vm::{
        Ambient, AsyncHostContext, AsyncHostFunction, CallOptions, Cancel, ExecError, FromLuaMulti,
        Function, HostReturn, HostType, HostTypeBuilder, IntoLua, Limits, MarshaledPair,
        MarshaledScriptError, MultiValue, NativeModule, OwnedValue, RuntimeCapabilities,
        RuntimeError, RuntimeErrorKind, Scope, ScopedValue, ScriptError, ScriptErrorField,
        SinkQuota, StashedClosure, Table, TableLayout, UnsupportedTableKey, ValueSnapshot,
        async_host_fn, classify_marshaled_table,
    },
};
use serde::{Deserialize, Serialize};
use tokio::{
    runtime::{Builder as RuntimeBuilder, Handle},
    task::yield_now,
};

thread_local! {
    static REENTRANT_CANOPY: RefCell<Vec<NonNull<Canopy>>> = const { RefCell::new(Vec::new()) };
}

use crate::{
    Canopy, ChangeOutcome, FixtureInfo, NodeId,
    commands::{self, ArgValue, CommandArgs, CommandInvocation, CommandSet, CommandSpec},
    core::{
        Core,
        context::{Context, CoreContext, CoreViewContext, FocusScope, ViewContext},
        help::BindingKind,
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
/// Render Luau definition files from the current command set.
pub mod defs;
/// Persistent script module roots and module source.
mod modules;

pub use modules::ScriptModuleRoots;
pub(crate) use modules::ScriptModuleSource;

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptAssertion {
    /// Whether the assertion passed.
    pub passed: bool,
    /// Assertion message or fallback description.
    pub message: String,
}

/// Structured Luau typecheck diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Help/debug label for the closure.
    label: Option<String>,
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
    fn insert(&mut self, stashed: StashedClosure, label: Option<String>) -> Result<LuauFunctionId> {
        let id = LuauFunctionId(self.next_function_id);
        self.next_function_id = self.next_function_id.checked_add(1).ok_or_else(|| {
            error::Error::InvalidOperation("closure identifier space exhausted".to_string())
        })?;
        self.functions.insert(
            id,
            StoredFunction {
                target: StoredFunctionTarget::Pending(stashed),
                label,
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

    /// Return the help/debug label for a stored function.
    fn label(&self, id: LuauFunctionId) -> Option<String> {
        self.functions
            .get(&id)
            .and_then(|function| function.label.clone())
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

impl LuauState {
    /// Construct empty script host state.
    fn new() -> Self {
        Self {
            scripts: ScriptCache::new(),
            closures: ClosureRegistry::new(),
            startup_requirements: vec![StartupRequirement {
                name: "setup".to_string(),
                type_text: "() -> ()".to_string(),
            }],
            ..Self::default()
        }
    }

    /// Mark a fully prepared script API as ready.
    fn publish(&mut self) {
        self.finalized = true;
    }

    /// Drain deferred `on_start` hooks in registration order.
    fn drain_on_start_hooks(&mut self) -> Vec<LuauFunctionId> {
        mem::take(&mut self.on_start_hooks)
    }
}

/// One required global definition for startup script roots.
#[derive(Clone)]
struct StartupRequirement {
    /// Global name the root script must define.
    name: String,
    /// Luau type text the definition must satisfy.
    type_text: String,
}

/// Clears the retained VM active-eval flag when a top-level async eval exits.
struct ActiveEvalGuard {
    /// Shared state whose active-eval flag should be cleared.
    state: Rc<RefCell<LuauState>>,
}

impl Drop for ActiveEvalGuard {
    fn drop(&mut self) {
        self.state.borrow_mut().active_eval = false;
    }
}

/// Stack guard for the script dispatch anchor inside the borrowed Canopy context.
struct ScriptAnchorGuard<'a, 's> {
    /// Scope that owns the Canopy context borrow.
    scope: &'a Scope<'s>,
}

impl<'a, 's> ScriptAnchorGuard<'a, 's> {
    /// Push the active command dispatch anchor for this script call.
    fn push(scope: &'a Scope<'s>, node_id: NodeId) -> StdResult<Self, RuntimeError> {
        push_script_anchor(scope, node_id)?;
        Ok(Self { scope })
    }
}

impl Drop for ScriptAnchorGuard<'_, '_> {
    fn drop(&mut self) {
        pop_script_anchor(self.scope);
    }
}

/// Guard exposing the current Canopy to nested script callbacks during routing.
struct ReentrantCanopyGuard;

impl ReentrantCanopyGuard {
    /// Push a Canopy pointer for reentrant host calls in the same VM stack.
    fn push(canopy: &mut Canopy) -> Self {
        REENTRANT_CANOPY.with(|stack| stack.borrow_mut().push(NonNull::from(canopy)));
        Self
    }
}

impl Drop for ReentrantCanopyGuard {
    fn drop(&mut self) {
        REENTRANT_CANOPY.with(|stack| {
            let _ = stack.borrow_mut().pop();
        });
    }
}

/// Execute a closure with the reentrant Canopy pointer, when one is installed.
fn with_reentrant_canopy<R>(f: impl FnOnce(&mut Canopy) -> Result<R>) -> Option<Result<R>> {
    REENTRANT_CANOPY.with(|stack| {
        let canopy = stack.borrow().last().copied()?;
        // SAFETY: `ReentrantCanopyGuard` is installed only while the script-originated
        // routing call owns the live `&mut Canopy` on this thread, and is popped before
        // that borrow returns to Ruau.
        Some(f(unsafe { &mut *canopy.as_ptr() }))
    })
}

/// Execute a closure with the live Canopy, through the normal context or the reentrant bridge.
fn with_canopy<R>(scope: &Scope<'_>, f: impl FnOnce(&mut Canopy) -> Result<R>) -> Result<R> {
    if let Some(mut canopy) = scope.context_mut::<Canopy>() {
        return f(&mut canopy);
    }
    with_reentrant_canopy(f)
        .unwrap_or_else(|| Err(error::Error::Script("no active canopy context".into())))
}

/// Push the active script anchor.
fn push_script_anchor(scope: &Scope<'_>, node_id: NodeId) -> StdResult<(), RuntimeError> {
    Ok(with_canopy(scope, |canopy| {
        canopy.script_context_stack.push(node_id);
        Ok(())
    })?)
}

/// Pop the active script anchor.
fn pop_script_anchor(scope: &Scope<'_>) {
    with_canopy(scope, |canopy| {
        canopy.script_context_stack.pop();
        Ok(())
    })
    .ok();
}

/// Return true when a live script scope is active on this thread.
///
/// True means the current Rust code was reached from inside a running script,
/// so any script execution started now is nested within that evaluation.
pub(crate) fn in_live_scope(canopy: &Canopy) -> bool {
    !canopy.script_context_stack.is_empty()
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

/// Convert an Ruau compile error to Canopy's parse error shape.
fn compile_error_to_canopy(err: &CompileError) -> error::Error {
    let begin = err.location().map(|location| location.begin);
    error::Error::Parse(error::ParseError::with_position(
        err.message(),
        begin.map(|position| position.line as usize + 1),
        begin.map(|position| position.column as usize + 1),
    ))
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

/// Convert preparation failures into Canopy's existing public error categories.
fn prepare_graph_error_to_canopy(error: &PrepareGraphError) -> error::Error {
    if let Some(diagnostics) = error.diagnostics()
        && diagnostics.has_errors()
    {
        let result = ScriptCheckResult {
            diagnostics: diagnostics
                .records()
                .map(module_diagnostic_to_script)
                .collect(),
        };
        return error::Error::Parse(error::ParseError::new(result.format_diagnostics()));
    }
    if let Some(error) = error.compile_error() {
        return compile_error_to_canopy(error);
    }
    error::Error::Script(format!("preparing script graph failed: {error}"))
}

/// Convert a displayable error into a canopy script error.
fn lua_to_canopy(err: impl fmt::Display) -> error::Error {
    error::Error::Script(err.to_string())
}

/// Convert raw integer coordinates into a canopy point.
fn point_from_coords(x: i64, y: i64) -> Result<Point> {
    let x = u32::try_from(x)
        .map_err(|_| error::Error::Script(format!("x coordinate must be >= 0, got {x}")))?;
    let y = u32::try_from(y)
        .map_err(|_| error::Error::Script(format!("y coordinate must be >= 0, got {y}")))?;
    Ok(Point { x, y })
}

/// Execute a closure with mutable access to the active canopy instance.
fn with_current_canopy<R>(
    scope: &Scope<'_>,
    f: impl FnOnce(&mut Canopy, NodeId) -> Result<R>,
) -> Result<R> {
    with_canopy(scope, |canopy| {
        let node_id = canopy
            .script_context_stack
            .last()
            .copied()
            .ok_or_else(|| error::Error::Script("no active script context".into()))?;
        f(canopy, node_id)
    })
}

/// Script-side opaque handle for a canopy node.
#[derive(Clone, Copy, Debug)]
struct NodeHandle {
    /// Backing arena id.
    id: NodeId,
}

/// Build the host userdata descriptor for `NodeId` handles.
fn node_handle_type() -> HostType {
    HostTypeBuilder::<NodeHandle>::new("NodeId")
        .class(&commands::declaration::Class::new("NodeId"))
        .eq_by(|left, right| left.id == right.id)
        .marshal(node_handle_marshal)
        .tostring(|handle| node_token(handle.id))
        .build()
}

/// Return the external automation token for a node id.
fn node_token(node_id: NodeId) -> String {
    format!("{node_id:?}")
}

/// Marshal a node handle to the external automation token record.
fn node_handle_marshal(handle: &NodeHandle) -> ValueSnapshot {
    ValueSnapshot::Table(vec![
        marshaled_string_pair("type", "NodeId"),
        marshaled_string_pair("token", node_token(handle.id)),
    ])
}

/// Build a string-keyed marshaled table pair.
fn marshaled_string_pair(key: &str, value: impl Into<String>) -> MarshaledPair {
    MarshaledPair {
        key: ValueSnapshot::String(key.as_bytes().to_vec()),
        value: ValueSnapshot::String(value.into().into_bytes()),
    }
}

/// Convert a node identifier into its scripting representation.
fn node_id_to_arg(node_id: NodeId) -> ArgValue {
    ArgValue::Node(node_id)
}

/// Convert a script node handle back into a node identifier.
fn node_id_from_value<'s>(
    scope: &Scope<'s>,
    value: ScopedValue<'s>,
) -> StdResult<NodeId, RuntimeError> {
    match value {
        ScopedValue::Userdata(userdata) => Ok(userdata.borrow::<NodeHandle>(scope)?.id),
        other => Err(RuntimeError::structured(
            format!("expected NodeId, got {}", other.type_name()),
            [
                ScriptErrorField::new("kind", "type_mismatch"),
                ScriptErrorField::new("expected", "NodeId"),
                ScriptErrorField::new("got", other.type_name()),
            ],
        )),
    }
}

/// Validate a script-held node handle against the live arena.
pub(crate) fn validate_node_handle(core: &Core, node_id: NodeId) -> Result<()> {
    if core.nodes.contains_key(node_id) {
        Ok(())
    } else {
        Err(error::Error::from(commands::CommandError::InvalidNode {
            id: node_id,
        }))
    }
}

/// Return a display name for a scoped value's type.
/// Copy the text behind a scoped string value.
fn scoped_value_to_string<'s>(
    scope: &Scope<'s>,
    value: ScopedValue<'s>,
) -> StdResult<String, String> {
    match value {
        ScopedValue::String(text) => scope
            .string_bytes(text)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .map_err(|err| err.to_string()),
        other => Err(format!("expected string, got {}", other.type_name())),
    }
}

/// Convert a scoped value into a displayable string for diagnostics.
fn scoped_value_to_display<'s>(scope: &Scope<'s>, value: ScopedValue<'s>) -> String {
    match value {
        ScopedValue::Nil => "nil".to_string(),
        ScopedValue::Boolean(value) => value.to_string(),
        ScopedValue::Integer(value) => value.to_string(),
        ScopedValue::Number(value) => value.to_string(),
        ScopedValue::String(text) => scope
            .string_bytes(text)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_else(|_| "<string>".to_string()),
        other => format!("<{}>", other.type_name()),
    }
}

/// Canopy-owned location within a nested command value.
#[derive(Clone)]
struct ValuePath(String);

impl ValuePath {
    /// Start a value path at a named boundary.
    fn root(name: &str) -> Self {
        Self(name.to_string())
    }

    /// Extend this path with a one-based sequence index.
    fn index(&self, index: usize) -> Self {
        Self(format!("{}[{index}]", self.0))
    }

    /// Extend this path with a string map field.
    fn field(&self, field: &str) -> Self {
        if is_luau_identifier(field) {
            Self(format!("{}.{field}", self.0))
        } else {
            let field = field.replace('\\', "\\\\").replace('"', "\\\"");
            Self(format!("{}[\"{field}\"]", self.0))
        }
    }

    /// Prefix a conversion failure with this path.
    fn error(&self, message: impl fmt::Display) -> String {
        format!("{}: {message}", self.0)
    }
}

/// Return whether a string can use dotted Luau field notation.
fn is_luau_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

/// Apply Canopy's shared numeric policy to a script number.
fn number_to_arg_value(value: f64, path: &ValuePath) -> StdResult<ArgValue, String> {
    if !value.is_finite() {
        return Err(path.error("non-finite numbers are not supported"));
    }
    if value.fract() == 0.0 && value >= i64::MIN as f64 && value < -(i64::MIN as f64) {
        Ok(ArgValue::Int(value as i64))
    } else {
        Ok(ArgValue::Float(value))
    }
}

/// Reject table layouts outside Canopy's array-or-string-map domain model.
fn reject_unsupported_layout(layout: &TableLayout, path: &ValuePath) -> StdResult<(), String> {
    match layout {
        TableLayout::Empty | TableLayout::Sequence { .. } | TableLayout::StringMap { .. } => Ok(()),
        TableLayout::Sparse { first_missing, .. } => {
            Err(path.error(format!("sparse table missing index {first_missing}")))
        }
        TableLayout::Mixed { .. } => {
            Err(path.error("mixed integer and string table keys are not supported"))
        }
        TableLayout::UnsupportedKey { key } => Err(path.error(unsupported_key_message(key))),
    }
}

/// Describe one table key rejected by the shared Ruau classifier.
fn unsupported_key_message(key: &UnsupportedTableKey) -> String {
    match key {
        UnsupportedTableKey::NonPositiveInteger { value } => {
            format!("table index must be positive, got {value}")
        }
        UnsupportedTableKey::FractionalNumber { display } => {
            format!("table index must be integral, got {display}")
        }
        UnsupportedTableKey::IndexOutOfRange { display } => {
            format!("table index is out of range: {display}")
        }
        UnsupportedTableKey::DuplicateIndex { index } => {
            format!("duplicate table index {index}")
        }
        UnsupportedTableKey::Type { type_name } => {
            format!("unsupported table key type: {type_name}")
        }
    }
}

/// Read a sequence key after `TableLayout` has established a dense layout.
fn scoped_sequence_index(key: ScopedValue<'_>, path: &ValuePath) -> StdResult<usize, String> {
    match key {
        ScopedValue::Integer(index) => usize::try_from(index)
            .map_err(|_| path.error(format!("table index is out of range: {index}"))),
        ScopedValue::Number(index) => Ok(index as usize),
        other => Err(path.error(format!(
            "expected sequence index, got {}",
            other.type_name()
        ))),
    }
}

/// Read a strict UTF-8 string key after layout classification.
fn scoped_table_key<'s>(
    scope: &Scope<'s>,
    key: ScopedValue<'s>,
    path: &ValuePath,
) -> StdResult<String, String> {
    let ScopedValue::String(key) = key else {
        return Err(path.error(format!("expected string key, got {}", key.type_name())));
    };
    let bytes = scope
        .string_bytes(key)
        .map_err(|error| path.error(error.to_string()))?;
    String::from_utf8(bytes).map_err(|error| path.error(format!("invalid UTF-8 key: {error}")))
}

/// Convert a scoped value into a dynamic command argument.
fn scoped_to_arg_value<'s>(
    scope: &Scope<'s>,
    value: ScopedValue<'s>,
) -> StdResult<ArgValue, String> {
    scoped_to_arg_value_at(scope, value, &ValuePath::root("value"))
}

/// Convert a scoped value at one nested command-value path.
fn scoped_to_arg_value_at<'s>(
    scope: &Scope<'s>,
    value: ScopedValue<'s>,
    path: &ValuePath,
) -> StdResult<ArgValue, String> {
    match value {
        ScopedValue::Nil => Ok(ArgValue::Null),
        ScopedValue::Boolean(value) => Ok(ArgValue::Bool(value)),
        ScopedValue::Integer(value) => Ok(ArgValue::Int(value)),
        ScopedValue::Number(value) => number_to_arg_value(value, path),
        ScopedValue::String(text) => scope
            .string_bytes(text)
            .map_err(|error| path.error(error.to_string()))
            .and_then(|bytes| {
                String::from_utf8(bytes)
                    .map(ArgValue::String)
                    .map_err(|error| path.error(format!("invalid UTF-8 string: {error}")))
            }),
        ScopedValue::Table(table) => table_to_arg_value(scope, table, path),
        ScopedValue::Userdata(userdata) => userdata
            .borrow::<NodeHandle>(scope)
            .map(|handle| ArgValue::Node(handle.id))
            .map_err(|_| path.error("expected NodeId userdata")),
        other => Err(path.error(format!(
            "unsupported script value type: {}",
            other.type_name()
        ))),
    }
}

/// Convert a scoped table into an `ArgValue`.
fn table_to_arg_value<'s>(
    scope: &Scope<'s>,
    table: Table<'s>,
    path: &ValuePath,
) -> StdResult<ArgValue, String> {
    let layout = table
        .layout(scope)
        .map_err(|error| path.error(error.to_string()))?;
    reject_unsupported_layout(&layout, path)?;
    match layout {
        TableLayout::Empty => Ok(ArgValue::Map(BTreeMap::new())),
        TableLayout::Sequence { len } => {
            let mut values = vec![None; len];
            for (key, value) in table
                .pairs(scope)
                .map_err(|error| path.error(error.to_string()))?
            {
                let index = scoped_sequence_index(key, path)?;
                values[index - 1] = Some(scoped_to_arg_value_at(scope, value, &path.index(index))?);
            }
            Ok(ArgValue::Array(
                values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        value.ok_or_else(|| path.error(format!("missing index {}", index + 1)))
                    })
                    .collect::<StdResult<_, _>>()?,
            ))
        }
        TableLayout::StringMap { .. } => {
            let mut values = BTreeMap::new();
            for (key, value) in table
                .pairs(scope)
                .map_err(|error| path.error(error.to_string()))?
            {
                let key = scoped_table_key(scope, key, path)?;
                values.insert(
                    key.clone(),
                    scoped_to_arg_value_at(scope, value, &path.field(&key))?,
                );
            }
            Ok(ArgValue::Map(values))
        }
        _ => unreachable!("unsupported layouts were rejected"),
    }
}

/// Convert an `ArgValue` into a scoped Luau value.
fn arg_value_to_scoped<'s>(
    scope: &Scope<'s>,
    value: &ArgValue,
) -> StdResult<ScopedValue<'s>, RuntimeError> {
    Ok(match value {
        ArgValue::Null => ScopedValue::Nil,
        ArgValue::Bool(value) => ScopedValue::Boolean(*value),
        // Host numbers always enter Luau as `number`: the VM's native integer
        // type does not mix with number literals in comparisons or arithmetic,
        // and scripts are written against plain numbers.
        ArgValue::Int(value) => ScopedValue::Number(*value as f64),
        ArgValue::UInt(value) => ScopedValue::Number(*value as f64),
        ArgValue::Float(value) => ScopedValue::Number(*value),
        ArgValue::String(value) => ScopedValue::String(scope.create_string(value)?),
        ArgValue::Node(id) => ScopedValue::Userdata(scope.create_userdata(NodeHandle { id: *id })?),
        ArgValue::Array(values) => {
            let array = values
                .iter()
                .map(|value| arg_value_to_scoped(scope, value))
                .collect::<StdResult<Vec<_>, _>>()?;
            array.into_lua(scope)?
        }
        ArgValue::Map(values) => {
            let table = scope.create_table()?;
            for (key, value) in values {
                let value = arg_value_to_scoped(scope, value)?;
                if !matches!(value, ScopedValue::Nil) {
                    table.set(scope, key.as_str(), value)?;
                }
            }
            ScopedValue::Table(table)
        }
    })
}

/// Convert a point into its scripting record.
fn point_to_arg(point: Point) -> ArgValue {
    ArgValue::Map(BTreeMap::from([
        ("x".to_string(), ArgValue::Int(i64::from(point.x))),
        ("y".to_string(), ArgValue::Int(i64::from(point.y))),
    ]))
}

/// Convert a size into its scripting record.
fn size_to_arg(size: Size) -> ArgValue {
    ArgValue::Map(BTreeMap::from([
        ("w".to_string(), ArgValue::Int(i64::from(size.w))),
        ("h".to_string(), ArgValue::Int(i64::from(size.h))),
    ]))
}

/// Convert a screen rect into its scripting record.
fn rect_to_arg(rect: RectI32) -> ArgValue {
    ArgValue::Map(BTreeMap::from([
        ("x".to_string(), ArgValue::Int(i64::from(rect.tl.x))),
        ("y".to_string(), ArgValue::Int(i64::from(rect.tl.y))),
        ("w".to_string(), ArgValue::Int(i64::from(rect.w))),
        ("h".to_string(), ArgValue::Int(i64::from(rect.h))),
    ]))
}

/// Convert a list of node ids into a scripting array.
fn node_list_to_arg(nodes: impl IntoIterator<Item = NodeId>) -> ArgValue {
    ArgValue::Array(nodes.into_iter().map(node_id_to_arg).collect())
}

/// Convert a node into the `NodeInfo` scripting record.
fn node_info_to_arg(canopy: &Canopy, node_id: NodeId) -> Result<BTreeMap<String, ArgValue>> {
    let Some(node) = canopy.core.nodes.get(node_id) else {
        return Err(error::Error::NotFound(format!("node {node_id:?}")));
    };
    let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
    let rect = if node.view.outer.w == 0 || node.view.outer.h == 0 {
        ArgValue::Null
    } else {
        rect_to_arg(node.view.outer)
    };
    let content_rect = if node.view.content.w == 0 || node.view.content.h == 0 {
        ArgValue::Null
    } else {
        rect_to_arg(node.view.content)
    };
    let accept_focus = widget_access::accepts_focus(&canopy.core, node_id);
    Ok(BTreeMap::from([
        ("id".to_string(), node_id_to_arg(node_id)),
        ("name".to_string(), ArgValue::String(node.name.to_string())),
        (
            "focused".to_string(),
            ArgValue::Bool(root_ctx.node_is_focused(node_id)),
        ),
        (
            "on_focus_path".to_string(),
            ArgValue::Bool(root_ctx.node_is_on_focus_path(node_id)),
        ),
        ("hidden".to_string(), ArgValue::Bool(node.hidden)),
        ("visible".to_string(), ArgValue::Bool(!node.hidden)),
        (
            "children".to_string(),
            node_list_to_arg(node.children.iter().copied()),
        ),
        ("rect".to_string(), rect),
        ("content_rect".to_string(), content_rect),
        ("canvas".to_string(), size_to_arg(node.canvas)),
        ("scroll".to_string(), point_to_arg(node.scroll)),
        ("accept_focus".to_string(), ArgValue::Bool(accept_focus)),
    ]))
}

/// Convert a node into a recursive tree record.
fn tree_node_to_arg(canopy: &Canopy, node_id: NodeId) -> Result<ArgValue> {
    let mut info = node_info_to_arg(canopy, node_id)?;
    let Some(node) = canopy.core.nodes.get(node_id) else {
        return Err(error::Error::NotFound(format!("node {node_id:?}")));
    };
    let children = node
        .children
        .iter()
        .copied()
        .map(|child_id| tree_node_to_arg(canopy, child_id))
        .collect::<Result<Vec<_>>>()?;
    info.insert("children".to_string(), ArgValue::Array(children));
    Ok(ArgValue::Map(info))
}

/// Convert registered fixtures into a scripting array.
fn fixtures_to_arg(canopy: &Canopy) -> ArgValue {
    ArgValue::Array(
        canopy
            .fixture_infos()
            .into_iter()
            .map(|fixture| {
                ArgValue::Map(BTreeMap::from([
                    ("name".to_string(), ArgValue::String(fixture.name)),
                    (
                        "description".to_string(),
                        ArgValue::String(fixture.description),
                    ),
                ]))
            })
            .collect(),
    )
}

/// Label for a script-declared callback: the caller's script line when the
/// declaration site is known, so binding introspection points at the source.
fn script_callback_label(scope: &Scope<'_>) -> String {
    match scope.caller_location(0) {
        Some(location) => format!("script:{}", location.line),
        None => "script".to_string(),
    }
}

/// Convert one binding record into its scripting record.
fn binding_info_to_arg(
    canopy: &Canopy,
    mode: &str,
    binding: &inputmap::BindingInfo<'_>,
) -> ArgValue {
    let input_type = match binding.input {
        inputmap::InputSpec::Key(_) => "key",
        inputmap::InputSpec::Mouse(_) => "mouse",
    };
    let mut record = BTreeMap::from([
        (
            "input".to_string(),
            ArgValue::String(binding.input.to_string()),
        ),
        (
            "input_type".to_string(),
            ArgValue::String(input_type.to_string()),
        ),
        ("mode".to_string(), ArgValue::String(mode.to_string())),
        (
            "path".to_string(),
            ArgValue::String(binding.path_filter.to_string()),
        ),
        ("target".to_string(), ArgValue::String("luau".to_string())),
    ]);
    if let Some(desc) = canopy.script_host.function_label(binding.target) {
        record.insert("desc".to_string(), ArgValue::String(desc));
    }
    ArgValue::Map(record)
}

/// Convert a command parameter specification into its scripting record.
fn command_param_to_arg(param: &commands::CommandParamSpec) -> ArgValue {
    let mut record = BTreeMap::from([
        ("name".to_string(), ArgValue::String(param.name.to_string())),
        (
            "kind".to_string(),
            ArgValue::String(
                match param.kind {
                    commands::CommandParamKind::Injected => "injected",
                    commands::CommandParamKind::User => "user",
                }
                .to_string(),
            ),
        ),
        (
            "rust_type".to_string(),
            ArgValue::String(param.ty.rust.to_string()),
        ),
        (
            "luau_type".to_string(),
            ArgValue::String(defs::command_type_to_luau(&param.ty)),
        ),
        ("optional".to_string(), ArgValue::Bool(param.optional)),
    ]);
    if let Some(doc) = param.doc {
        record.insert("doc".to_string(), ArgValue::String(doc.to_string()));
    }
    if let Some(default) = param.default {
        record.insert("default".to_string(), ArgValue::String(default.to_string()));
    }
    ArgValue::Map(record)
}

/// Convert a command specification into its scripting record.
fn command_info_to_arg(
    spec: &CommandSpec,
    resolution: Option<commands::CommandResolution>,
) -> ArgValue {
    let owner = match spec.dispatch {
        commands::CommandDispatchKind::Node { owner } => owner,
        commands::CommandDispatchKind::Free => "",
    };
    let mut record = BTreeMap::from([
        ("name".to_string(), ArgValue::String(spec.name.to_string())),
        ("owner".to_string(), ArgValue::String(owner.to_string())),
        (
            "params".to_string(),
            ArgValue::Array(spec.params.iter().map(command_param_to_arg).collect()),
        ),
        (
            "ret".to_string(),
            ArgValue::String(match spec.ret {
                commands::CommandReturnSpec::Unit => "()".to_string(),
                commands::CommandReturnSpec::Value(ty) => defs::command_type_to_luau(&ty),
            }),
        ),
        (
            "available".to_string(),
            ArgValue::Bool(resolution.is_some()),
        ),
    ]);
    if let Some(doc) = spec.doc.long.or(spec.doc.short) {
        record.insert("doc".to_string(), ArgValue::String(doc.to_string()));
    }
    if let commands::CommandReturnSpec::Value(ty) = spec.ret
        && let Some(doc) = ty.doc
    {
        record.insert("ret_doc".to_string(), ArgValue::String(doc.to_string()));
    }
    if let Some(target) = resolution.and_then(commands::CommandResolution::target) {
        record.insert("target".to_string(), node_id_to_arg(target));
    }
    ArgValue::Map(record)
}

/// Convert the current rendered screen buffer into its scripting record.
fn screen_to_arg(canopy: &mut Canopy) -> Result<ArgValue> {
    canopy.refresh_snapshot()?;
    let Some(buffer) = canopy.buf() else {
        return Err(error::Error::Script(
            "screen unavailable before render".into(),
        ));
    };
    Ok(ArgValue::Array(
        buffer
            .rows()
            .into_iter()
            .map(|row| ArgValue::Array(row.into_iter().map(ArgValue::String).collect()))
            .collect(),
    ))
}

/// Convert the current rendered screen buffer into styled cell records.
fn screen_cells_to_arg(canopy: &mut Canopy) -> Result<ArgValue> {
    canopy.refresh_snapshot()?;
    let Some(buffer) = canopy.buf() else {
        return Err(error::Error::Script(
            "screen unavailable before render".into(),
        ));
    };
    let size = buffer.size();
    let mut rows = Vec::with_capacity(size.h as usize);
    for y in 0..size.h {
        let mut row = Vec::with_capacity(size.w as usize);
        for x in 0..size.w {
            let cell = buffer
                .get(Point { x, y })
                .expect("buffer coordinates should always be valid");
            row.push(cell_to_arg(x, y, cell));
        }
        rows.push(ArgValue::Array(row));
    }
    Ok(ArgValue::Array(rows))
}

/// Convert one terminal cell into a scripting record.
fn cell_to_arg(x: u32, y: u32, cell: &Cell) -> ArgValue {
    ArgValue::Map(BTreeMap::from([
        ("x".to_string(), ArgValue::UInt(u64::from(x))),
        ("y".to_string(), ArgValue::UInt(u64::from(y))),
        ("text".to_string(), ArgValue::String(cell.rendered_text())),
        ("fg".to_string(), color_to_arg(cell.style.fg)),
        ("bg".to_string(), color_to_arg(cell.style.bg)),
        ("attrs".to_string(), attrs_to_arg(cell.style.attrs)),
        (
            "continuation".to_string(),
            ArgValue::Bool(cell.continuation),
        ),
    ]))
}

/// Convert a color to a stable RGB string.
fn color_to_arg(color: Color) -> ArgValue {
    let (r, g, b) = color.rgb();
    ArgValue::String(format!("#{r:02x}{g:02x}{b:02x}"))
}

/// Convert text attributes to stable lowercase names.
fn attrs_to_arg(attrs: AttrSet) -> ArgValue {
    let mut names = Vec::new();
    if attrs.bold {
        names.push(ArgValue::String("bold".to_string()));
    }
    if attrs.crossedout {
        names.push(ArgValue::String("crossedout".to_string()));
    }
    if attrs.dim {
        names.push(ArgValue::String("dim".to_string()));
    }
    if attrs.italic {
        names.push(ArgValue::String("italic".to_string()));
    }
    if attrs.overline {
        names.push(ArgValue::String("overline".to_string()));
    }
    if attrs.underline {
        names.push(ArgValue::String("underline".to_string()));
    }
    ArgValue::Array(names)
}

/// Return the rendered screen text inside a signed rectangle, clipped to the screen.
fn screen_text_for_rect(canopy: &mut Canopy, rect: RectI32) -> Result<String> {
    canopy.refresh_snapshot()?;
    let Some(buffer) = canopy.buf() else {
        return Err(error::Error::Script(
            "screen unavailable before render".into(),
        ));
    };
    let Some(rect) = rect.intersect_rect(buffer.rect()) else {
        return Ok(String::new());
    };
    let mut rows = Vec::with_capacity(rect.h as usize);
    for y in rect.tl.y..rect.tl.y + rect.h {
        let mut row = String::new();
        for x in rect.tl.x..rect.tl.x + rect.w {
            let cell = buffer
                .get(Point { x, y })
                .expect("buffer coordinates should always be valid");
            row.push_str(&cell.rendered_text());
        }
        rows.push(row);
    }
    Ok(rows.join("\n"))
}

/// Return the rendered screen as plain text.
fn screen_text(canopy: &mut Canopy) -> Result<String> {
    canopy.refresh_snapshot()?;
    let Some(buffer) = canopy.buf() else {
        return Err(error::Error::Script(
            "screen unavailable before render".into(),
        ));
    };
    Ok(buffer.screen_text())
}

/// Convert the most recent route trace to scripting records.
fn route_trace_to_arg(canopy: &Canopy) -> ArgValue {
    ArgValue::Array(
        canopy
            .route_trace()
            .iter()
            .map(|entry| {
                let mut record = BTreeMap::from([
                    (
                        "phase".to_string(),
                        ArgValue::String(entry.phase.as_str().to_string()),
                    ),
                    ("path".to_string(), ArgValue::String(entry.path.clone())),
                    ("detail".to_string(), ArgValue::String(entry.detail.clone())),
                ]);
                if let Some(node) = entry.node {
                    record.insert("node".to_string(), node_id_to_arg(node));
                }
                ArgValue::Map(record)
            })
            .collect(),
    )
}

/// Convert the current help snapshot to a scripting record.
fn help_snapshot_to_arg(canopy: &Canopy) -> ArgValue {
    let snapshot = canopy.help_snapshot();
    let bindings = snapshot
        .bindings
        .iter()
        .map(|binding| {
            ArgValue::Map(BTreeMap::from([
                (
                    "input".to_string(),
                    ArgValue::String(binding.input.to_string()),
                ),
                (
                    "mode".to_string(),
                    ArgValue::String(binding.mode.to_string()),
                ),
                (
                    "path".to_string(),
                    ArgValue::String(binding.path_filter.to_string()),
                ),
                (
                    "kind".to_string(),
                    ArgValue::String(
                        match binding.kind {
                            BindingKind::PreEventOverride => "pre",
                            BindingKind::PostEventFallback => "post",
                        }
                        .to_string(),
                    ),
                ),
                ("target".to_string(), ArgValue::String("luau".to_string())),
                ("label".to_string(), ArgValue::String(binding.label.clone())),
            ]))
        })
        .collect();
    let commands = snapshot
        .commands
        .iter()
        .filter(|command| !command.spec.doc.hidden)
        .map(|command| command_info_to_arg(command.spec, command.resolution))
        .collect();
    ArgValue::Map(BTreeMap::from([
        ("focus".to_string(), node_id_to_arg(snapshot.focus)),
        (
            "focus_path".to_string(),
            ArgValue::String(snapshot.focus_path.to_string()),
        ),
        (
            "input_mode".to_string(),
            ArgValue::String(snapshot.input_mode.to_string()),
        ),
        ("bindings".to_string(), ArgValue::Array(bindings)),
        ("commands".to_string(), ArgValue::Array(commands)),
    ]))
}

/// Convert the script journal to scripting records.
fn script_journal_to_arg(canopy: &Canopy) -> ArgValue {
    ArgValue::Array(
        canopy
            .script_journal()
            .iter()
            .map(|entry| {
                ArgValue::Map(BTreeMap::from([
                    ("id".to_string(), ArgValue::UInt(entry.id)),
                    ("origin".to_string(), ArgValue::String(entry.origin.clone())),
                    ("source".to_string(), ArgValue::String(entry.source.clone())),
                    ("ok".to_string(), ArgValue::Bool(entry.ok)),
                    (
                        "error".to_string(),
                        entry
                            .error
                            .clone()
                            .map(ArgValue::String)
                            .unwrap_or(ArgValue::Null),
                    ),
                    (
                        "logs".to_string(),
                        ArgValue::Array(entry.logs.iter().cloned().map(ArgValue::String).collect()),
                    ),
                    (
                        "assertions".to_string(),
                        ArgValue::Array(
                            entry
                                .assertions
                                .iter()
                                .map(|assertion| {
                                    ArgValue::Map(BTreeMap::from([
                                        ("passed".to_string(), ArgValue::Bool(assertion.passed)),
                                        (
                                            "message".to_string(),
                                            ArgValue::String(assertion.message.clone()),
                                        ),
                                    ]))
                                })
                                .collect(),
                        ),
                    ),
                    ("duration_ms".to_string(), ArgValue::UInt(entry.duration_ms)),
                ]))
            })
            .collect(),
    )
}

/// Determine whether a map matches a command's named parameters.
fn map_matches_named(spec: &CommandSpec, map: &BTreeMap<String, ArgValue>) -> bool {
    if map.is_empty() {
        return false;
    }
    let allowed = spec
        .params
        .iter()
        .filter(|param| param.kind == commands::CommandParamKind::User)
        .map(|param| commands::normalize_key(param.name))
        .collect::<HashSet<_>>();
    let mut matched = false;
    for key in map.keys() {
        let normalized = commands::normalize_key(key);
        if allowed.contains(&normalized) {
            matched = true;
        } else {
            return false;
        }
    }
    matched
}

/// Build command arguments from converted script values.
fn build_args_from_values(
    spec: &CommandSpec,
    mut values: Vec<ArgValue>,
    allow_map_named: bool,
) -> StdResult<CommandArgs, String> {
    if allow_map_named && values.len() == 1 {
        let arg = values.pop().expect("single argument checked above");
        if let ArgValue::Map(map) = arg {
            if map_matches_named(spec, &map) {
                return Ok(CommandArgs::Named(map));
            }
            return Ok(CommandArgs::Positional(vec![ArgValue::Map(map)]));
        }
        return Ok(CommandArgs::Positional(vec![arg]));
    }
    Ok(CommandArgs::Positional(values))
}

/// Dispatch a command using the active script context.
fn dispatch_command(
    scope: &Scope<'_>,
    spec: &'static CommandSpec,
    node_id: NodeId,
    values: Vec<ArgValue>,
    allow_map_named: bool,
) -> Result<ArgValue> {
    with_current_canopy(scope, |canopy, _| {
        let args = build_args_from_values(spec, values, allow_map_named).map_err(|message| {
            error::Error::from(commands::CommandError::conversion(format!(
                "command {}: {message}",
                spec.id.0
            )))
        })?;
        let invocation = CommandInvocation { id: spec.id, args };
        commands::dispatch(&mut canopy.core, node_id, &invocation).map_err(error::Error::from)
    })
}

/// Dispatch a command by id using the current focus-relative context.
fn dispatch_command_by_name(
    scope: &Scope<'_>,
    name: &str,
    node_id: Option<NodeId>,
    values: Vec<ArgValue>,
) -> Result<ArgValue> {
    let allow_map_named = values.len() == 1;
    let (anchor, spec) = with_current_canopy(scope, |canopy, anchor| {
        let spec = canopy.core.commands.get(name).ok_or_else(|| {
            error::Error::from(commands::CommandError::UnknownCommand {
                id: name.to_string(),
            })
        })?;
        Ok((anchor, spec))
    })?;
    dispatch_command(
        scope,
        spec,
        node_id.unwrap_or(anchor),
        values,
        allow_map_named,
    )
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

/// Parsed options for script-created bindings.
#[derive(Debug, Clone, Default)]
struct ScriptBindOptions {
    /// Optional mode override.
    mode: String,
    /// Optional path filter override.
    path: String,
    /// Optional human-readable description.
    desc: Option<String>,
}

/// Parse `BindOptions` from an optional script table.
fn parse_bind_options<'s>(
    scope: &Scope<'s>,
    options: Option<Table<'s>>,
) -> StdResult<ScriptBindOptions, RuntimeError> {
    let Some(options) = options else {
        return Ok(ScriptBindOptions::default());
    };
    let field = |name: &str| -> StdResult<Option<String>, RuntimeError> {
        match options.get::<_, ScopedValue>(scope, name)? {
            ScopedValue::Nil => Ok(None),
            value => scoped_value_to_string(scope, value)
                .map(Some)
                .map_err(RuntimeError::runtime),
        }
    };
    Ok(ScriptBindOptions {
        mode: field("mode")?.unwrap_or_default(),
        path: field("path")?.unwrap_or_default(),
        desc: field("desc")?,
    })
}

/// Positional argument reader over a host call's values.
struct ArgReader<'s> {
    /// Remaining argument values, in order.
    values: vec::IntoIter<ScopedValue<'s>>,
    /// One-based index of the next argument, for error messages.
    index: usize,
}

impl<'s> ArgReader<'s> {
    /// Wrap a host call's arguments.
    fn new(args: MultiValue<'s>) -> Self {
        Self {
            values: args.into_vec().into_iter(),
            index: 0,
        }
    }

    /// Take the next argument, `Nil` when exhausted.
    fn next_value(&mut self) -> ScopedValue<'s> {
        self.index += 1;
        self.values.next().unwrap_or(ScopedValue::Nil)
    }

    /// Take the remaining arguments.
    fn rest(self) -> Vec<ScopedValue<'s>> {
        self.values.collect()
    }

    /// Take a required string argument.
    fn string(&mut self, scope: &Scope<'s>) -> StdResult<String, RuntimeError> {
        let index = self.index + 1;
        scoped_value_to_string(scope, self.next_value())
            .map_err(|message| RuntimeError::runtime(format!("argument {index}: {message}")))
    }

    /// Take a required integer argument.
    fn integer(&mut self, _scope: &Scope<'s>) -> StdResult<i64, RuntimeError> {
        let index = self.index + 1;
        match self.next_value() {
            ScopedValue::Integer(value) => Ok(value),
            ScopedValue::Number(value) if value.fract() == 0.0 => Ok(value as i64),
            other => Err(RuntimeError::runtime(format!(
                "argument {index}: expected integer, got {}",
                other.type_name()
            ))),
        }
    }

    /// Take a required node id argument.
    fn node_id(&mut self, scope: &Scope<'s>) -> StdResult<NodeId, RuntimeError> {
        let node_id = node_id_from_value(scope, self.next_value())?;
        with_current_canopy(scope, |canopy, _| {
            validate_node_handle(&canopy.core, node_id).map(|()| node_id)
        })
        .map_err(|err| canopy_to_host(&err))
    }

    /// Take an optional node id argument.
    fn opt_node_id(&mut self, scope: &Scope<'s>) -> StdResult<Option<NodeId>, RuntimeError> {
        let next = self.next_value();
        if matches!(next, ScopedValue::Nil) {
            return Ok(None);
        }
        let node_id = node_id_from_value(scope, next)?;
        with_current_canopy(scope, |canopy, _| {
            validate_node_handle(&canopy.core, node_id).map(|()| Some(node_id))
        })
        .map_err(|err| canopy_to_host(&err))
    }

    /// Take a required function argument.
    fn function(&mut self, _scope: &Scope<'s>) -> StdResult<Function<'s>, RuntimeError> {
        let index = self.index + 1;
        match self.next_value() {
            ScopedValue::Function(function) => Ok(function),
            other => Err(RuntimeError::runtime(format!(
                "argument {index}: expected function, got {}",
                other.type_name()
            ))),
        }
    }

    /// Take an optional table argument.
    fn opt_table(&mut self, _scope: &Scope<'s>) -> StdResult<Option<Table<'s>>, RuntimeError> {
        let index = self.index + 1;
        match self.next_value() {
            ScopedValue::Nil => Ok(None),
            ScopedValue::Table(table) => Ok(Some(table)),
            other => Err(RuntimeError::runtime(format!(
                "argument {index}: expected table, got {}",
                other.type_name()
            ))),
        }
    }
}

/// Parsed arguments for `canopy.wait_for`.
struct WaitForArgs {
    /// Predicate closure to poll.
    predicate: StashedClosure,
    /// Optional timeout in milliseconds.
    timeout_ms: Option<u64>,
}

impl<'s> FromLuaMulti<'s> for WaitForArgs {
    fn from_lua_multi(values: MultiValue<'s>, scope: &Scope<'s>) -> StdResult<Self, RuntimeError> {
        let mut args = ArgReader::new(values);
        let predicate = scope.stash_function(args.function(scope)?)?;
        let timeout_ms = optional_timeout_ms(args.next_value())?;
        Ok(Self {
            predicate,
            timeout_ms,
        })
    }
}

/// Parsed arguments for `canopy.wait_for_node`.
struct WaitForNodeArgs {
    /// Command owner that should become available.
    owner: String,
    /// Optional timeout in milliseconds.
    timeout_ms: Option<u64>,
}

impl<'s> FromLuaMulti<'s> for WaitForNodeArgs {
    fn from_lua_multi(values: MultiValue<'s>, scope: &Scope<'s>) -> StdResult<Self, RuntimeError> {
        let mut args = ArgReader::new(values);
        let owner = args.string(scope)?;
        let timeout_ms = optional_timeout_ms(args.next_value())?;
        Ok(Self { owner, timeout_ms })
    }
}

/// Parsed arguments for `canopy.wait_for_screen_text`.
struct WaitForScreenTextArgs {
    /// Text fragment expected on screen.
    text: String,
    /// Optional timeout in milliseconds.
    timeout_ms: Option<u64>,
}

impl<'s> FromLuaMulti<'s> for WaitForScreenTextArgs {
    fn from_lua_multi(values: MultiValue<'s>, scope: &Scope<'s>) -> StdResult<Self, RuntimeError> {
        let mut args = ArgReader::new(values);
        let text = args.string(scope)?;
        let timeout_ms = optional_timeout_ms(args.next_value())?;
        Ok(Self { text, timeout_ms })
    }
}

/// Parse an optional millisecond timeout from a script argument.
fn optional_timeout_ms(value: ScopedValue<'_>) -> StdResult<Option<u64>, RuntimeError> {
    match value {
        ScopedValue::Nil => Ok(None),
        ScopedValue::Integer(value) if value >= 0 => Ok(Some(value as u64)),
        ScopedValue::Number(value) if value.fract() == 0.0 && value >= 0.0 => {
            Ok(Some(value as u64))
        }
        other => Err(RuntimeError::runtime(format!(
            "expected non-negative timeout milliseconds, got {}",
            other.type_name()
        ))),
    }
}

/// Build a timeout error for an async wait helper.
fn wait_timeout(timeout_ms: u64) -> RuntimeError {
    canopy_to_host(&error::Error::ScriptTimeout { timeout_ms })
}

/// Poll app state and a predicate until it succeeds or times out.
async fn wait_until<F>(
    ctx: AsyncHostContext,
    timeout_ms: Option<u64>,
    mut ready: F,
) -> StdResult<HostReturn, RuntimeError>
where
    F: FnMut(
        AsyncHostContext,
    ) -> Pin<Box<dyn Future<Output = StdResult<bool, RuntimeError>> + Send>>,
{
    let started = Instant::now();
    loop {
        ctx.scope(|scope| {
            let mut canopy = scope
                .context_mut::<Canopy>()
                .ok_or_else(|| RuntimeError::runtime("no active canopy context"))?;
            canopy.service_automation();
            Ok(())
        })
        .await?;
        if ready(ctx.clone()).await? {
            return Ok(host_return(true));
        }
        if let Some(timeout_ms) = timeout_ms
            && started.elapsed() >= Duration::from_millis(timeout_ms)
        {
            return Err(wait_timeout(timeout_ms));
        }
        yield_now().await;
    }
}

/// Async implementation of `canopy.wait_for`.
async fn wait_for_predicate(
    ctx: AsyncHostContext,
    args: WaitForArgs,
) -> StdResult<HostReturn, RuntimeError> {
    wait_until(ctx, args.timeout_ms, move |ctx| {
        let predicate = args.predicate.clone();
        Box::pin(async move {
            match ctx.call_protected(&predicate, ()).await? {
                Ok(values) => Ok(owned_truthy(values.values.first())),
                Err(error) => Err(RuntimeError::runtime(format!(
                    "wait predicate failed: {}",
                    owned_value_to_display(error.value())
                ))),
            }
        })
    })
    .await
}

/// Async implementation of `canopy.wait_for_node`.
async fn wait_for_node(
    ctx: AsyncHostContext,
    args: WaitForNodeArgs,
) -> StdResult<HostReturn, RuntimeError> {
    wait_until(ctx, args.timeout_ms, move |ctx| {
        let owner = args.owner.clone();
        Box::pin(async move {
            ctx.scope(move |scope| {
                let canopy = scope
                    .context_mut::<Canopy>()
                    .ok_or_else(|| RuntimeError::runtime("no active canopy context"))?;
                Ok(canopy
                    .command_availability_from_focus()
                    .iter()
                    .any(|entry| {
                        entry.resolution.is_some()
                            && matches!(
                                entry.spec.dispatch,
                                commands::CommandDispatchKind::Node { owner: entry_owner }
                                    if entry_owner == owner
                            )
                    }))
            })
            .await
        })
    })
    .await
}

/// Async implementation of `canopy.wait_for_screen_text`.
async fn wait_for_screen_text(
    ctx: AsyncHostContext,
    args: WaitForScreenTextArgs,
) -> StdResult<HostReturn, RuntimeError> {
    wait_until(ctx, args.timeout_ms, move |ctx| {
        let text = args.text.clone();
        Box::pin(async move {
            ctx.scope(move |scope| {
                let mut canopy = scope
                    .context_mut::<Canopy>()
                    .ok_or_else(|| RuntimeError::runtime("no active canopy context"))?;
                let screen = screen_text(&mut canopy)?;
                Ok(screen.contains(&text))
            })
            .await
        })
    })
    .await
}

/// Async host function for `canopy.wait_for`.
fn wait_for_host_fn() -> Box<dyn AsyncHostFunction> {
    async_host_fn(wait_for_predicate)
}

/// Async host function for `canopy.wait_for_node`.
fn wait_for_node_host_fn() -> Box<dyn AsyncHostFunction> {
    async_host_fn(wait_for_node)
}

/// Async host function for `canopy.wait_for_screen_text`.
fn wait_for_screen_text_host_fn() -> Box<dyn AsyncHostFunction> {
    async_host_fn(wait_for_screen_text)
}

/// Convert the remaining host-call values into command arguments.
fn values_to_args<'s>(
    scope: &Scope<'s>,
    values: Vec<ScopedValue<'s>>,
) -> StdResult<Vec<ArgValue>, RuntimeError> {
    values
        .into_iter()
        .map(|value| scoped_to_arg_value(scope, value).map_err(RuntimeError::runtime))
        .collect()
}

/// Run a query against the live Canopy and return its value to the script.
fn host_value<'s>(
    scope: &Scope<'s>,
    f: impl FnOnce(&mut Canopy, NodeId) -> Result<ArgValue>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let value = with_current_canopy(scope, f)?;
    ret_arg(scope, &value)
}

/// Build an empty host-call return.
fn ret_none<'s>() -> MultiValue<'s> {
    MultiValue::new()
}

/// Build a single-value host-call return.
fn ret_one(value: ScopedValue<'_>) -> MultiValue<'_> {
    MultiValue::from_values(vec![value])
}

/// Build a single-value host-call return from a command argument value.
fn ret_arg<'s>(scope: &Scope<'s>, value: &ArgValue) -> StdResult<MultiValue<'s>, RuntimeError> {
    Ok(ret_one(arg_value_to_scoped(scope, value)?))
}

/// Build a single async host return value.
fn host_return(value: impl Into<OwnedValue>) -> HostReturn {
    HostReturn {
        values: vec![value.into()],
    }
}

/// Return true for Luau-truthy owned values.
fn owned_truthy(value: Option<&OwnedValue>) -> bool {
    !matches!(
        value,
        None | Some(OwnedValue::Nil | OwnedValue::Boolean(false))
    )
}

/// Display an owned async host value in an error message.
fn owned_value_to_display(value: &OwnedValue) -> String {
    match value {
        OwnedValue::Nil => "nil".to_string(),
        OwnedValue::Boolean(value) => value.to_string(),
        OwnedValue::Integer(value) => value.to_string(),
        OwnedValue::Number(value) => value.to_string(),
        OwnedValue::Bytes(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        other => format!("{other:?}"),
    }
}

/// Convert a canopy error into a host-call error.
impl From<error::Error> for RuntimeError {
    fn from(error: error::Error) -> Self {
        canopy_to_host(&error)
    }
}

/// Convert a canopy error into a structured Ruau runtime error.
fn canopy_to_host(err: &error::Error) -> RuntimeError {
    let payload = CanopyErrorPayload::from(err);
    let mut fields = vec![ScriptErrorField::new("kind", payload.kind.as_str())];
    if let Some(command) = payload.command.clone() {
        fields.push(ScriptErrorField::new("command", command));
    }
    if let Some(owner) = payload.owner.clone() {
        fields.push(ScriptErrorField::new("owner", owner));
    }
    RuntimeError::structured(payload.message.clone(), fields).with_payload(payload)
}

/// Normalized cloneable canopy error payload carried through Ruau errors.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CanopyErrorPayload {
    /// Stable script-visible category.
    kind: error::ScriptErrorKind,
    /// Timeout duration for script timeout errors.
    timeout_ms: Option<u64>,
    /// Command id when the error came from command dispatch.
    command: Option<String>,
    /// Owner name when the error came from node-target resolution.
    owner: Option<String>,
    /// Human-readable error message.
    message: String,
}

impl From<&error::Error> for CanopyErrorPayload {
    fn from(err: &error::Error) -> Self {
        match err {
            error::Error::Command(err) => Self::from(err),
            error::Error::ScriptTimeout { timeout_ms } => {
                Self::new(error::ScriptErrorKind::Timeout, err.to_string())
                    .with_timeout_ms(*timeout_ms)
            }
            error::Error::NodeNotFound(node) => {
                Self::new(error::ScriptErrorKind::NodeNotFound, err.to_string())
                    .with_owner(format!("{node:?}"))
            }
            error::Error::NodeDetached(node) => {
                Self::new(error::ScriptErrorKind::NodeDetached, err.to_string())
                    .with_owner(format!("{node:?}"))
            }
            error::Error::TypeMismatch { .. } | error::Error::NodeTypeMismatch { .. } => {
                Self::new(error::ScriptErrorKind::TypeMismatch, err.to_string())
            }
            error::Error::NotFound(_) => {
                Self::new(error::ScriptErrorKind::NotFound, err.to_string())
            }
            error::Error::Invalid(_) | error::Error::InvalidOperation(_) => {
                Self::new(error::ScriptErrorKind::Invalid, err.to_string())
            }
            error::Error::ScriptStructured {
                kind,
                command,
                owner,
                message,
            } => Self {
                kind: *kind,
                timeout_ms: None,
                command: command.clone(),
                owner: owner.clone(),
                message: message.clone(),
            },
            _ => Self::new(error::ScriptErrorKind::Canopy, err.to_string()),
        }
    }
}

impl From<&commands::CommandError> for CanopyErrorPayload {
    fn from(err: &commands::CommandError) -> Self {
        match err {
            commands::CommandError::UnknownCommand { id } => {
                Self::new(error::ScriptErrorKind::UnknownCommand, err.to_string())
                    .with_command(id.clone())
            }
            commands::CommandError::DuplicateCommand { id } => {
                Self::new(error::ScriptErrorKind::DuplicateCommand, err.to_string())
                    .with_command(id.clone())
            }
            commands::CommandError::ConflictingCommand { id } => {
                Self::new(error::ScriptErrorKind::ConflictingCommand, err.to_string())
                    .with_command(id.clone())
            }
            commands::CommandError::InvalidCommand { id, .. } => {
                Self::new(error::ScriptErrorKind::InvalidCommand, err.to_string())
                    .with_command(id.clone())
            }
            commands::CommandError::NoTarget { id, owner } => {
                Self::new(error::ScriptErrorKind::NoTarget, err.to_string())
                    .with_command(id.clone())
                    .with_owner(owner.clone())
            }
            commands::CommandError::InvalidNode { .. } => {
                Self::new(error::ScriptErrorKind::InvalidNode, err.to_string())
            }
            commands::CommandError::ArityMismatch { .. } => {
                Self::new(error::ScriptErrorKind::ArityMismatch, err.to_string())
            }
            commands::CommandError::MissingNamedArg { .. } => Self::new(
                error::ScriptErrorKind::MissingNamedArgument,
                err.to_string(),
            ),
            commands::CommandError::UnknownNamedArg { .. } => Self::new(
                error::ScriptErrorKind::UnknownNamedArgument,
                err.to_string(),
            ),
            commands::CommandError::TypeMismatch { .. } => {
                Self::new(error::ScriptErrorKind::TypeMismatch, err.to_string())
            }
            commands::CommandError::MissingInjected { .. } => {
                Self::new(error::ScriptErrorKind::MissingInjected, err.to_string())
            }
            commands::CommandError::Conversion { .. } => {
                Self::new(error::ScriptErrorKind::Conversion, err.to_string())
            }
            commands::CommandError::TargetTypeMismatch => {
                Self::new(error::ScriptErrorKind::TargetTypeMismatch, err.to_string())
            }
            commands::CommandError::Exec(_) => {
                Self::new(error::ScriptErrorKind::CommandExecution, err.to_string())
            }
        }
    }
}

impl CanopyErrorPayload {
    /// Builds a payload without command routing context.
    fn new(kind: error::ScriptErrorKind, message: String) -> Self {
        Self {
            kind,
            timeout_ms: None,
            command: None,
            owner: None,
            message,
        }
    }

    /// Attaches a timeout duration.
    fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// Attaches a command id.
    fn with_command(mut self, command: String) -> Self {
        self.command = Some(command);
        self
    }

    /// Attaches an owner name.
    fn with_owner(mut self, owner: String) -> Self {
        self.owner = Some(owner);
        self
    }

    /// Convert this host payload into a core error while preserving traceback context.
    fn to_canopy_error(&self, label: &str, traceback: Option<&str>) -> error::Error {
        if let Some(timeout_ms) = self.timeout_ms {
            return error::Error::ScriptTimeout { timeout_ms };
        }
        let message = match traceback {
            Some(traceback) => format!("{label} failed: {}\n{traceback}", self.message),
            None => format!("{label} failed: {}", self.message),
        };
        error::Error::ScriptStructured {
            kind: self.kind,
            command: self.command.clone(),
            owner: self.owner.clone(),
            message,
        }
    }
}

/// Convert an owned async-driver result into a command argument value.
fn marshaled_to_arg_value(value: &ValueSnapshot) -> StdResult<ArgValue, String> {
    marshaled_to_arg_value_at(value, &ValuePath::root("result"))
}

/// Convert a marshaled value at one nested command-value path.
fn marshaled_to_arg_value_at(
    value: &ValueSnapshot,
    path: &ValuePath,
) -> StdResult<ArgValue, String> {
    match value {
        ValueSnapshot::Nil => Ok(ArgValue::Null),
        ValueSnapshot::Boolean(value) => Ok(ArgValue::Bool(*value)),
        ValueSnapshot::Integer(value) => Ok(ArgValue::Int(*value)),
        ValueSnapshot::Number(value) => number_to_arg_value(*value, path),
        ValueSnapshot::String(bytes) => Ok(ArgValue::String(
            String::from_utf8(bytes.clone())
                .map_err(|error| path.error(format!("invalid UTF-8 string: {error}")))?,
        )),
        ValueSnapshot::Table(pairs) => marshaled_table_to_arg_value(pairs, path),
        ValueSnapshot::Vector(_) => Err(path.error("unsupported script value type: vector")),
        ValueSnapshot::LightUserdata { .. } => {
            Err(path.error("unsupported script value type: lightuserdata"))
        }
        ValueSnapshot::Buffer(_) => Err(path.error("unsupported script value type: buffer")),
        ValueSnapshot::Opaque(kind) => {
            Err(path.error(format!("unsupported script value type: {kind}")))
        }
    }
}

/// Convert an owned marshaled table into a command argument value.
fn marshaled_table_to_arg_value(
    pairs: &[MarshaledPair],
    path: &ValuePath,
) -> StdResult<ArgValue, String> {
    let layout = classify_marshaled_table(pairs);
    reject_unsupported_layout(&layout, path)?;
    match layout {
        TableLayout::Empty => Ok(ArgValue::Map(BTreeMap::new())),
        TableLayout::Sequence { len } => {
            let mut values = vec![None; len];
            for pair in pairs {
                let index = marshaled_sequence_index(&pair.key, path)?;
                values[index - 1] =
                    Some(marshaled_to_arg_value_at(&pair.value, &path.index(index))?);
            }
            Ok(ArgValue::Array(
                values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        value.ok_or_else(|| path.error(format!("missing index {}", index + 1)))
                    })
                    .collect::<StdResult<_, _>>()?,
            ))
        }
        TableLayout::StringMap { .. } => {
            let mut values = BTreeMap::new();
            for pair in pairs {
                let key = marshaled_table_key(&pair.key, path)?;
                values.insert(
                    key.clone(),
                    marshaled_to_arg_value_at(&pair.value, &path.field(&key))?,
                );
            }
            Ok(ArgValue::Map(values))
        }
        _ => unreachable!("unsupported layouts were rejected"),
    }
}

/// Read a sequence index from a classified marshaled key.
fn marshaled_sequence_index(key: &ValueSnapshot, path: &ValuePath) -> StdResult<usize, String> {
    match key {
        ValueSnapshot::Integer(index) => usize::try_from(*index)
            .map_err(|_| path.error(format!("table index is out of range: {index}"))),
        ValueSnapshot::Number(index) => Ok(*index as usize),
        other => Err(path.error(format!(
            "expected sequence index, got {}",
            other.type_name()
        ))),
    }
}

/// Read a strict UTF-8 string key from a classified marshaled key.
fn marshaled_table_key(key: &ValueSnapshot, path: &ValuePath) -> StdResult<String, String> {
    let ValueSnapshot::String(bytes) = key else {
        return Err(path.error(format!("expected string key, got {}", key.type_name())));
    };
    String::from_utf8(bytes.clone())
        .map_err(|error| path.error(format!("invalid UTF-8 key: {error}")))
}

/// Display an owned async-driver value in an error message.
fn marshaled_value_to_display(value: &ValueSnapshot) -> String {
    match value {
        ValueSnapshot::Nil => "nil".to_string(),
        ValueSnapshot::Boolean(value) => value.to_string(),
        ValueSnapshot::Integer(value) => value.to_string(),
        ValueSnapshot::Number(value) => value.to_string(),
        ValueSnapshot::String(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        other => format!("<{}>", other.type_name()),
    }
}

/// A plain-function canopy host handler.
type HostHandler =
    for<'s> fn(&Scope<'s>, MultiValue<'s>) -> StdResult<MultiValue<'s>, RuntimeError>;

/// Run an owner's default-bindings script inside the current live scope.
fn run_default_bindings_in_scope(scope: &Scope<'_>, owner: &str) -> Result<()> {
    let run = with_current_canopy(scope, |canopy, _| {
        canopy.prepare_registered_default_bindings(owner)
    })?;
    let result = run.host.execute_in_scope(scope, run.root_id, run.script_id);
    with_current_canopy(scope, |canopy, _| {
        canopy.record_registered_default_bindings(owner, &run, &result);
        Ok(())
    })?;
    result
}

/// Store a binding closure and install the binding, releasing the closure if
/// installation fails.
fn install_function_binding<'s>(
    scope: &Scope<'s>,
    function: Function<'s>,
    input: inputmap::InputSpec,
    options: &ScriptBindOptions,
) -> StdResult<i64, RuntimeError> {
    let stashed = scope.stash_function(function)?;
    let label = Some(
        options
            .desc
            .clone()
            .unwrap_or_else(|| script_callback_label(scope)),
    );
    with_current_canopy(scope, |canopy, _| {
        let function_id = canopy.script_host.store_function(stashed, label)?;
        let result =
            canopy
                .keymap
                .replace_binding(&options.mode, input, &options.path, function_id);
        match result {
            Ok((binding_id, removed)) => {
                canopy.release_removed_bindings(removed);
                Ok(binding_id.as_u64() as i64)
            }
            Err(err) => {
                canopy.script_host.release_function(function_id);
                Err(err)
            }
        }
    })
    .map_err(|err| canopy_to_host(&err))
}

/// `canopy.cmd`: dispatch a command by fully-qualified id.
fn host_cmd<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let name = args.string(scope)?;
    let values = values_to_args(scope, args.rest())?;
    let result = dispatch_command_by_name(scope, &name, None, values)?;
    ret_arg(scope, &result)
}

/// `canopy.cmd_on`: dispatch a command against a specific node.
fn host_cmd_on<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    let name = args.string(scope)?;
    let values = values_to_args(scope, args.rest())?;
    let result = dispatch_command_by_name(scope, &name, Some(node_id), values)?;
    ret_arg(scope, &result)
}

/// `canopy.log`: append a log line to the evaluation diagnostics.
fn host_log<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let message = scoped_value_to_display(scope, args.next_value());
    tracing::info!("{message}");
    with_current_canopy(scope, |canopy, _| {
        canopy.script_host.push_log(message);
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.assert`: record an assertion and fail the script when false.
fn host_assert<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let condition = !matches!(
        args.next_value(),
        ScopedValue::Nil | ScopedValue::Boolean(false)
    );
    let message = match args.next_value() {
        ScopedValue::Nil => "assertion failed".to_string(),
        value => scoped_value_to_string(scope, value).map_err(RuntimeError::runtime)?,
    };
    with_current_canopy(scope, |canopy, _| {
        canopy
            .script_host
            .push_assertion(condition, message.clone());
        Ok(())
    })?;
    if condition {
        Ok(ret_none())
    } else {
        Err(RuntimeError::runtime(message))
    }
}

/// `canopy.root`: return the root node id.
fn host_root<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| Ok(node_id_to_arg(canopy.core.root_id())))
}

/// `canopy.focused`: return the focused node id, or nil.
fn host_focused<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| {
        Ok(canopy
            .core
            .focus_id()
            .map(node_id_to_arg)
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.node_info`: return the `NodeInfo` record for a node.
fn host_node_info<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    host_value(scope, |canopy, _| {
        node_info_to_arg(canopy, node_id).map(ArgValue::Map)
    })
}

/// `canopy.find_node`: return the first node matching a path pattern.
fn host_find_node<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let pattern = args.string(scope)?;
    host_value(scope, |canopy, _| {
        let filter = PathFilter::normalized(&pattern)?;
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(root_ctx
            .find_node_matching(&filter)
            .map(node_id_to_arg)
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.find_nodes`: return all nodes matching a path pattern.
fn host_find_nodes<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let pattern = args.string(scope)?;
    host_value(scope, |canopy, _| {
        let filter = PathFilter::normalized(&pattern)?;
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(node_list_to_arg(root_ctx.find_nodes_matching(&filter)))
    })
}

/// `canopy.parent`: return a node's parent, or nil for the root.
fn host_parent<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    host_value(scope, |canopy, _| {
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(root_ctx
            .parent_of(node_id)
            .map(node_id_to_arg)
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.children`: return a node's children.
fn host_children<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    host_value(scope, |canopy, _| {
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(node_list_to_arg(root_ctx.children_of(node_id)))
    })
}

/// `canopy.tree`: return the recursive node tree from the root.
fn host_tree<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| {
        tree_node_to_arg(canopy, canopy.core.root_id())
    })
}

/// `canopy.set_focus`: focus a node, returning whether focus moved.
fn host_set_focus<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    let focused = with_current_canopy(scope, |canopy, _| {
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        ctx.set_focus(node_id).map(ChangeOutcome::changed)
    })?;
    Ok(ret_one(ScopedValue::Boolean(focused)))
}

/// `canopy.node_at`: return the node at screen coordinates, or nil.
fn host_node_at<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let x = args.integer(scope)?;
    let y = args.integer(scope)?;
    host_value(scope, |canopy, _| {
        Ok(canopy
            .core
            .locate_node(canopy.core.root_id(), point_from_coords(x, y)?)?
            .map(node_id_to_arg)
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.focus_next`: move focus to the next focusable node.
fn host_focus_next<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    with_current_canopy(scope, |canopy, _| {
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        ctx.focus_next(FocusScope::Root)?;
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.focus_prev`: move focus to the previous focusable node.
fn host_focus_prev<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    with_current_canopy(scope, |canopy, _| {
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        ctx.focus_prev(FocusScope::Root)?;
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.focus_dir`: move focus in a direction.
fn host_focus_dir<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let dir = args.string(scope)?;
    with_current_canopy(scope, |canopy, _| {
        let dir = commands::FromArgValue::from_arg_value(&ArgValue::String(dir))
            .map_err(error::Error::from)?;
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        ctx.focus_dir(FocusScope::Root, dir)?;
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.send_key`: inject a key event.
fn host_send_key<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let key_spec = args.string(scope)?;
    with_current_canopy(scope, |canopy, _| {
        let key = key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?;
        let _reentrant = ReentrantCanopyGuard::push(canopy);
        canopy.key(Some(scope), key)
    })?;
    Ok(ret_none())
}

/// `canopy.send_click`: inject a left click at screen coordinates.
fn host_send_click<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let x = args.integer(scope)?;
    let y = args.integer(scope)?;
    with_current_canopy(scope, |canopy, _| {
        let location = point_from_coords(x, y)?;
        let _reentrant = ReentrantCanopyGuard::push(canopy);
        canopy.mouse(
            Some(scope),
            mouse::MouseEvent {
                action: mouse::Action::Down,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location,
            },
        )?;
        canopy.mouse(
            Some(scope),
            mouse::MouseEvent {
                action: mouse::Action::Up,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location,
            },
        )
    })?;
    Ok(ret_none())
}

/// `canopy.send_scroll`: inject a scroll event at screen coordinates.
fn host_send_scroll<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let dir = args.string(scope)?;
    let x = args.integer(scope)?;
    let y = args.integer(scope)?;
    with_current_canopy(scope, |canopy, _| {
        let action = if dir.eq_ignore_ascii_case("up") {
            mouse::Action::ScrollUp
        } else if dir.eq_ignore_ascii_case("down") {
            mouse::Action::ScrollDown
        } else {
            return Err(error::Error::Script(format!(
                "unknown scroll direction: {dir}"
            )));
        };
        let _reentrant = ReentrantCanopyGuard::push(canopy);
        canopy.mouse(
            Some(scope),
            mouse::MouseEvent {
                action,
                button: mouse::Button::None,
                modifiers: key::Empty,
                location: point_from_coords(x, y)?,
            },
        )
    })?;
    Ok(ret_none())
}
/// `canopy.bindings`: return the active binding table across all modes.
fn host_bindings<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| {
        Ok(ArgValue::Array(
            canopy
                .keymap
                .bindings()
                .into_iter()
                .map(|binding| binding_info_to_arg(canopy, binding.mode, &binding.info))
                .collect(),
        ))
    })
}

/// `canopy.commands`: return metadata for all registered commands.
fn host_commands<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, node_id| {
        let resolver = commands::CommandResolver::new(&canopy.core, node_id);
        let mut availability = resolver.availability();
        availability.sort_by_key(|item| item.spec.id.0);
        Ok(ArgValue::Array(
            availability
                .into_iter()
                .map(|item| command_info_to_arg(item.spec, item.resolution))
                .collect(),
        ))
    })
}

/// `canopy.resolve`: return the dispatch target for an owner.
fn host_resolve<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let owner = args.string(scope)?;
    host_value(scope, |canopy, node_id| {
        let resolver = commands::CommandResolver::new(&canopy.core, node_id);
        Ok(resolver
            .resolve_owner(&owner)
            .and_then(commands::CommandResolution::target)
            .map_or(ArgValue::Null, node_id_to_arg))
    })
}

/// `canopy.input_mode`: return the active input mode.
fn host_input_mode<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mode = with_current_canopy(scope, |canopy, _| Ok(canopy.input_mode().to_string()))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&mode)?)))
}

/// `canopy.set_mode`: switch the active input mode.
fn host_set_mode<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let mode = args.string(scope)?;
    with_current_canopy(scope, |canopy, _| {
        canopy.set_input_mode(&mode)?;
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.push_mode`: push an input mode above the current mode.
fn host_push_mode<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let mode = args.string(scope)?;
    with_current_canopy(scope, |canopy, _| {
        canopy.push_input_mode(&mode)?;
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.pop_mode`: pop the top input mode and return the active mode.
fn host_pop_mode<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mode = with_current_canopy(scope, |canopy, _| Ok(canopy.pop_input_mode().to_string()))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&mode)?)))
}

/// `canopy.bind`: bind a key spec to a Luau callback.
fn host_bind<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let key_spec = args.string(scope)?;
    let function = args.function(scope)?;
    let input =
        inputmap::InputSpec::Key(key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?);
    let id = install_function_binding(scope, function, input, &ScriptBindOptions::default())?;
    Ok(ret_one(ScopedValue::Number(id as f64)))
}

/// `canopy.bind_with`: bind a key spec with explicit options.
fn host_bind_with<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let key_spec = args.string(scope)?;
    let options = parse_bind_options(scope, args.opt_table(scope)?)?;
    let function = args.function(scope)?;
    let input =
        inputmap::InputSpec::Key(key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?);
    let id = install_function_binding(scope, function, input, &options)?;
    Ok(ret_one(ScopedValue::Number(id as f64)))
}

/// `canopy.bind_mouse`: bind a mouse spec to a Luau callback.
fn host_bind_mouse<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let mouse_spec = args.string(scope)?;
    let function = args.function(scope)?;
    let input = inputmap::InputSpec::Mouse(
        mouse::Mouse::parse_spec(&mouse_spec).map_err(error::Error::Script)?,
    );
    let id = install_function_binding(scope, function, input, &ScriptBindOptions::default())?;
    Ok(ret_one(ScopedValue::Number(id as f64)))
}

/// `canopy.bind_mouse_with`: bind a mouse spec with explicit options.
fn host_bind_mouse_with<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let mouse_spec = args.string(scope)?;
    let options = parse_bind_options(scope, args.opt_table(scope)?)?;
    let function = args.function(scope)?;
    let input = inputmap::InputSpec::Mouse(
        mouse::Mouse::parse_spec(&mouse_spec).map_err(error::Error::Script)?,
    );
    let id = install_function_binding(scope, function, input, &options)?;
    Ok(ret_one(ScopedValue::Number(id as f64)))
}

/// `canopy.unbind`: remove a binding by numeric id.
fn host_unbind<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let id = args.integer(scope)?;
    let removed = with_current_canopy(scope, |canopy, _| {
        Ok(canopy.unbind(inputmap::BindingId::from_u64(id as u64)))
    })?;
    Ok(ret_one(ScopedValue::Boolean(removed)))
}

/// `canopy.unbind_key`: remove key bindings matching a spec and options.
fn host_unbind_key<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let key_spec = args.string(scope)?;
    let options = parse_bind_options(scope, args.opt_table(scope)?)?;
    with_current_canopy(scope, |canopy, _| {
        let mode = (!options.mode.is_empty()).then_some(options.mode.as_str());
        let path = (!options.path.is_empty()).then_some(options.path.as_str());
        let key = key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?;
        let _ = canopy.unbind_input(inputmap::InputSpec::Key(key), mode, path);
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.clear_bindings`: remove every binding from every mode.
fn host_clear_bindings<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    with_current_canopy(scope, |canopy, _| {
        let _ = canopy.clear_bindings();
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.screen`: return the rendered screen as rows of cell strings.
fn host_screen<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let rows = with_current_canopy(scope, |canopy, _| screen_to_arg(canopy))?;
    ret_arg(scope, &rows)
}

/// `canopy.screen_cells`: return the rendered screen with style metadata.
fn host_screen_cells<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let rows = with_current_canopy(scope, |canopy, _| screen_cells_to_arg(canopy))?;
    ret_arg(scope, &rows)
}

/// `canopy.screen_text`: return the rendered screen as plain text.
fn host_screen_text<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let text = with_current_canopy(scope, |canopy, _| screen_text(canopy))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&text)?)))
}

/// `canopy.screen_region`: return rendered plain text inside a screen rectangle.
fn host_screen_region<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let x = args.integer(scope)?;
    let y = args.integer(scope)?;
    let w = args.integer(scope)?;
    let h = args.integer(scope)?;
    let rect = RectI32::new(
        i32::try_from(x).unwrap_or(if x < 0 { i32::MIN } else { i32::MAX }),
        i32::try_from(y).unwrap_or(if y < 0 { i32::MIN } else { i32::MAX }),
        u32::try_from(w.max(0)).unwrap_or(u32::MAX),
        u32::try_from(h.max(0)).unwrap_or(u32::MAX),
    );
    let text = with_current_canopy(scope, |canopy, _| screen_text_for_rect(canopy, rect))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&text)?)))
}

/// `canopy.node_region`: return rendered plain text inside a node's content rect.
fn host_node_region<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    let text = with_current_canopy(scope, |canopy, _| {
        canopy.refresh_snapshot()?;
        let view = canopy
            .core
            .node(node_id)
            .ok_or_else(|| error::Error::from(commands::CommandError::InvalidNode { id: node_id }))?
            .view;
        screen_text_for_rect(canopy, view.content)
    })?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&text)?)))
}

/// `canopy.route_trace`: return the most recent input route trace.
fn host_route_trace<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| Ok(route_trace_to_arg(canopy)))
}

/// `canopy.diagnostic_dump`: return a diagnostic dump for a node.
fn host_diagnostic_dump<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let requested = args.opt_node_id(scope)?;
    let dump = with_current_canopy(scope, |canopy, node_id| {
        let target = requested.unwrap_or(node_id);
        Ok(canopy.diagnostic_dump(target))
    })?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&dump)?)))
}

/// `canopy.help_snapshot`: return the current contextual help snapshot.
fn host_help_snapshot<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| Ok(help_snapshot_to_arg(canopy)))
}

/// `canopy.script_journal`: return recorded script evaluations.
fn host_script_journal<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| Ok(script_journal_to_arg(canopy)))
}

/// `canopy.api`: return the generated Luau API definition.
fn host_api<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let api = with_current_canopy(scope, |canopy, _| canopy.script_api().map(str::to_string))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&api)?)))
}

/// `canopy.on_start`: register a callback to run after the first render.
fn host_on_start<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let function = args.function(scope)?;
    let stashed = scope.stash_function(function)?;
    let label = script_callback_label(scope);
    with_current_canopy(scope, |canopy, _| {
        let function_id = canopy.script_host.store_function(stashed, Some(label))?;
        canopy
            .script_host
            .state
            .borrow_mut()
            .on_start_hooks
            .push(function_id);
        Ok(())
    })?;
    Ok(ret_none())
}

/// `fixtures`: list all registered fixtures.
fn host_fixtures<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| Ok(fixtures_to_arg(canopy)))
}

/// Build the declaration-coupled base Canopy module.
fn build_base_module() -> Result<Arc<dyn NativeModule>> {
    let mut builder = module::Builder::new("canopy");
    defs::register_framework_types(&mut builder);
    builder.host_type(
        commands::declaration::Class::new("NodeId"),
        Arc::new(node_handle_type()),
    );
    base_api::register(&mut builder);
    builder.build().map_err(|error| {
        error::Error::Script(format!("building base script module failed: {error}"))
    })
}

/// Build declaration-coupled per-owner command modules for the surface.
fn build_owner_modules(
    commands: &CommandSet,
    default_binding_owners: &BTreeSet<String>,
) -> Result<Vec<Arc<dyn NativeModule>>> {
    let mut modules = Vec::new();
    for (owner, specs) in defs::owner_command_specs(commands, default_binding_owners) {
        let global_name = luau_global_owner_name(&owner);
        let mut builder = module::Builder::new(global_name.clone());
        defs::register_owner_dependencies(&mut builder, &specs);
        for spec in specs {
            let mut binding = Binding::library(
                global_name.clone(),
                commands::declaration::Type::func(defs::command_fn_sig(spec)),
            );
            if let Some(documentation) = defs::command_doc(spec) {
                binding = binding.doc(documentation);
            }
            builder.borrowed_function(
                spec.name,
                binding,
                move |scope: &Scope<'_>, args: MultiValue<'_>| {
                    let values = values_to_args(scope, ArgReader::new(args).rest())?;
                    let allow_map_named = values.len() == 1;
                    let node_id = with_current_canopy(scope, |_, node_id| Ok(node_id))?;
                    let result = dispatch_command(scope, spec, node_id, values, allow_map_named)?;
                    ret_arg(scope, &result)
                },
            );
        }
        if default_binding_owners.contains(&owner) {
            builder.borrowed_function(
                "default_bindings",
                Binding::library(
                    global_name,
                    commands::declaration::Type::func(
                        commands::declaration::FunctionSignature::new(),
                    ),
                )
                .doc("Register this widget's default bindings."),
                move |scope: &Scope<'_>, _args: MultiValue<'_>| {
                    run_default_bindings_in_scope(scope, &owner)?;
                    Ok(ret_none())
                },
            );
        }
        modules.push(builder.build().map_err(|error| {
            error::Error::Script(format!("building owner script module failed: {error}"))
        })?);
    }
    Ok(modules)
}

/// A retained script root or stored callback resolvable inside a live VM scope.
enum CallTarget {
    /// A compiled script root owned by the retained runtime.
    Root(RootHandle),
    /// A callback pending promotion or owned by the retained runtime.
    Stored(StoredFunctionTarget),
}

impl CallTarget {
    /// Resolve the callable inside the given scope.
    fn resolve<'s>(
        &self,
        scope: &Scope<'s>,
        label: &str,
        timeout: Option<Duration>,
    ) -> Result<Function<'s>> {
        match self {
            Self::Root(root) => root
                .resolve(scope)
                .map_err(|error| retained_runtime_error_to_canopy(&error, label, timeout)),
            Self::Stored(StoredFunctionTarget::Pending(stashed)) => {
                scope.fetch_function(stashed).map_err(lua_to_canopy)
            }
            Self::Stored(StoredFunctionTarget::Retained(handle)) => handle
                .resolve(scope)
                .map_err(|error| retained_runtime_error_to_canopy(&error, label, timeout)),
        }
    }
}

/// Convert a caught script error into a canopy error.
fn script_error_to_canopy<'s>(
    scope: &Scope<'s>,
    error: &ScriptError<'s>,
    label: &str,
    timeout: Option<Duration>,
) -> error::Error {
    if let Some(timeout_error) = timeout_error(error.kind(), timeout) {
        return timeout_error;
    }
    if let Some(payload) = error.payload_ref::<CanopyErrorPayload>() {
        return payload.to_canopy_error(label, error.traceback());
    }
    let message = scoped_value_to_display(scope, error.value());
    match error.traceback() {
        Some(traceback) => error::Error::Script(format!("{label} failed: {message}\n{traceback}")),
        None => error::Error::Script(format!("{label} failed: {message}")),
    }
}

/// Convert a fatal VM error into a canopy error.
fn runtime_error_to_canopy(
    error: &RuntimeError,
    label: &str,
    timeout: Option<Duration>,
) -> error::Error {
    if let Some(timeout_error) = timeout_error(error.kind(), timeout) {
        return timeout_error;
    }
    if let Some(payload) = error.payload_ref::<CanopyErrorPayload>() {
        return payload.to_canopy_error(label, None);
    }
    error::Error::Script(format!("{label} failed: {error}"))
}

/// Convert an async owned-entry execution error into a canopy error.
fn exec_error_to_canopy(error: &ExecError, label: &str, timeout: Option<Duration>) -> error::Error {
    if let Some(timeout_error) = timeout_error(error.kind(), timeout) {
        return timeout_error;
    }
    match error {
        ExecError::Script(error) => marshaled_script_error_to_canopy(error, label, timeout),
        ExecError::Stopped(_) => timeout_error(error.kind(), timeout).unwrap_or_else(|| {
            error::Error::Script(format!("{label} failed: script evaluation was cancelled"))
        }),
        ExecError::PanicPoison => error::Error::Script(format!(
            "{label} failed: script VM is poisoned and refuses further work"
        )),
        ExecError::Entry { message } => error::Error::Script(format!("{label} failed: {message}")),
        ExecError::Marshal { message } => error::Error::Script(format!(
            "{label} failed: marshaling script result failed: {message}"
        )),
    }
}

/// Convert a retained-runtime state or execution failure into a canopy error.
fn retained_runtime_error_to_canopy(
    error: &LifecycleError,
    label: &str,
    timeout: Option<Duration>,
) -> error::Error {
    match error {
        LifecycleError::Exec(error) => exec_error_to_canopy(error, label, timeout),
        LifecycleError::Runtime(error) => runtime_error_to_canopy(error, label, timeout),
        LifecycleError::StaleHandle { .. }
        | LifecycleError::InUse { .. }
        | LifecycleError::PermanentHandle { .. }
        | LifecycleError::Load(_)
        | LifecycleError::PreparedLoad(_)
        | LifecycleError::BindEnvironment(_) => {
            error::Error::Script(format!("{label} failed: {error}"))
        }
    }
}

/// Convert an async owned script error into a canopy error.
fn marshaled_script_error_to_canopy(
    error: &MarshaledScriptError,
    label: &str,
    timeout: Option<Duration>,
) -> error::Error {
    if let Some(timeout_error) = timeout_error(error.kind(), timeout) {
        return timeout_error;
    }
    if let Some(payload) = error.payload_ref::<CanopyErrorPayload>() {
        return payload.to_canopy_error(label, error.traceback());
    }
    let message = marshaled_value_to_display(error.value());
    match error.traceback() {
        Some(traceback) => error::Error::Script(format!("{label} failed: {message}\n{traceback}")),
        None => error::Error::Script(format!("{label} failed: {message}")),
    }
}

/// Build the cooperative-timeout error for a cancelled or deadlined run.
fn timeout_error(kind: RuntimeErrorKind, timeout: Option<Duration>) -> Option<error::Error> {
    if !matches!(
        kind,
        RuntimeErrorKind::Cancelled | RuntimeErrorKind::Deadline
    ) {
        return None;
    }
    let timeout_ms = timeout
        .map(|timeout| u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    Some(error::Error::ScriptTimeout { timeout_ms })
}

/// Run a resolved callable inside a live scope and convert its result.
fn call_in_scope<'s>(
    scope: &Scope<'s>,
    function: Function<'s>,
    label: &str,
    timeout: Option<Duration>,
) -> Result<ArgValue> {
    match scope.call_protected::<_, MultiValue>(function, ()) {
        Ok(Ok(values)) => {
            let value = values
                .into_vec()
                .into_iter()
                .next()
                .unwrap_or(ScopedValue::Nil);
            scoped_to_arg_value(scope, value)
                .map_err(|message| error::Error::Script(format!("{label}: {message}")))
        }
        Ok(Err(script_error)) => Err(script_error_to_canopy(scope, &script_error, label, timeout)),
        Err(runtime_error) => Err(runtime_error_to_canopy(&runtime_error, label, timeout)),
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
    fn store_function(
        &self,
        stashed: StashedClosure,
        label: Option<String>,
    ) -> Result<LuauFunctionId> {
        self.state.borrow_mut().closures.insert(stashed, label)
    }

    /// Release a stored function reference. The underlying registry pin is
    /// released on the VM's next step.
    pub fn release_function(&self, id: LuauFunctionId) {
        self.state.borrow_mut().closures.remove(id);
    }

    /// Return the help/debug label for a stored function.
    pub fn function_label(&self, id: LuauFunctionId) -> Option<String> {
        self.state.borrow().closures.label(id)
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

#[cfg(test)]
mod tests {
    use proptest::{
        prelude::*,
        test_runner::{TestCaseError, TestCaseResult},
    };

    use super::*;
    use crate::{
        core::testing::model::trace_result,
        testing::ttree::{get_state, run_ttree},
    };

    #[derive(Clone, Debug)]
    enum ClosureOperation {
        Bind(u8),
        Unbind(u8),
        ClearBindings,
        OnStart,
        InvalidKey,
        InvalidPath,
        ExhaustClosureIdentifier,
        ExhaustBindingIdentifier,
    }

    #[derive(Debug)]
    struct ClosureModel {
        bound_keys: BTreeSet<char>,
        on_start_hooks: usize,
    }

    impl ClosureModel {
        fn new() -> Self {
            Self {
                bound_keys: BTreeSet::new(),
                on_start_hooks: 0,
            }
        }
    }

    fn closure_operation_strategy() -> impl Strategy<Value = Vec<ClosureOperation>> {
        prop::collection::vec(
            prop_oneof![
                (0_u8..3).prop_map(ClosureOperation::Bind),
                (0_u8..3).prop_map(ClosureOperation::Unbind),
                Just(ClosureOperation::ClearBindings),
                Just(ClosureOperation::OnStart),
                Just(ClosureOperation::InvalidKey),
                Just(ClosureOperation::InvalidPath),
                Just(ClosureOperation::ExhaustClosureIdentifier),
                Just(ClosureOperation::ExhaustBindingIdentifier),
            ],
            1..30,
        )
    }

    fn closure_key(index: u8) -> char {
        ['a', 'b', 'c'][usize::from(index) % 3]
    }

    fn execute_registry_script(canopy: &mut Canopy, host: &LuauHost, source: &str) -> Result<()> {
        let script = host.compile(source)?;
        host.execute(canopy, canopy.core.root_id(), script, None)
            .map(|_| ())
    }

    fn assert_closure_model(
        canopy: &Canopy,
        host: &LuauHost,
        model: &ClosureModel,
    ) -> TestCaseResult {
        let state = host.state.borrow();
        prop_assert_eq!(state.on_start_hooks.len(), model.on_start_hooks);
        prop_assert_eq!(
            state.closures.functions.len(),
            model.bound_keys.len() + model.on_start_hooks
        );
        prop_assert_eq!(canopy.keymap.bindings().len(), model.bound_keys.len());
        Ok(())
    }

    fn apply_closure_operation(
        canopy: &mut Canopy,
        host: &LuauHost,
        model: &mut ClosureModel,
        operation: &ClosureOperation,
    ) -> TestCaseResult {
        match *operation {
            ClosureOperation::Bind(index) => {
                let key = closure_key(index);
                let result = execute_registry_script(
                    canopy,
                    host,
                    &format!(r#"canopy.bind("{key}", function() end)"#),
                );
                prop_assert!(result.is_ok(), "{result:?}");
                model.bound_keys.insert(key);
            }
            ClosureOperation::Unbind(index) => {
                let key = closure_key(index);
                let result = execute_registry_script(
                    canopy,
                    host,
                    &format!(r#"canopy.unbind_key("{key}")"#),
                );
                prop_assert!(result.is_ok(), "{result:?}");
                model.bound_keys.remove(&key);
            }
            ClosureOperation::ClearBindings => {
                let result = execute_registry_script(canopy, host, "canopy.clear_bindings()");
                prop_assert!(result.is_ok(), "{result:?}");
                model.bound_keys.clear();
            }
            ClosureOperation::OnStart => {
                let result =
                    execute_registry_script(canopy, host, "canopy.on_start(function() end)");
                prop_assert!(result.is_ok(), "{result:?}");
                model.on_start_hooks += 1;
            }
            ClosureOperation::InvalidKey => {
                let result = execute_registry_script(
                    canopy,
                    host,
                    r#"canopy.bind("Ctrl+", function() end)"#,
                );
                prop_assert!(result.is_err());
            }
            ClosureOperation::InvalidPath => {
                let result = execute_registry_script(
                    canopy,
                    host,
                    r#"canopy.bind_with("a", { path = "invalid-name" }, function() end)"#,
                );
                prop_assert!(result.is_err());
            }
            ClosureOperation::ExhaustClosureIdentifier => {
                let previous = host.state.borrow().closures.next_function_id;
                host.state.borrow_mut().closures.next_function_id = u64::MAX;
                let result =
                    execute_registry_script(canopy, host, "canopy.on_start(function() end)");
                prop_assert!(result.is_err());
                host.state.borrow_mut().closures.next_function_id = previous;
            }
            ClosureOperation::ExhaustBindingIdentifier => {
                let previous = canopy.keymap.replace_next_id(u64::MAX);
                let result =
                    execute_registry_script(canopy, host, r#"canopy.bind("z", function() end)"#);
                prop_assert!(result.is_err());
                canopy.keymap.replace_next_id(previous);
            }
        }
        assert_closure_model(canopy, host, model)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn script_closure_registry_matches_model(operations in closure_operation_strategy()) {
            let mut canopy = Canopy::new();
            canopy
                .finalize_api()
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let host = canopy.script_host.clone();
            let mut model = ClosureModel::new();
            for (index, operation) in operations.iter().enumerate() {
                trace_result(
                    apply_closure_operation(&mut canopy, &host, &mut model, operation),
                    &operations,
                    index,
                )?;
            }
        }
    }

    #[test]
    fn reentrant_canopy_guard_restores_nested_stack() -> Result<()> {
        REENTRANT_CANOPY.with(|stack| assert!(stack.borrow().is_empty()));
        let mut outer = Canopy::new();
        let mut inner = Canopy::new();

        {
            let _outer_guard = ReentrantCanopyGuard::push(&mut outer);
            with_reentrant_canopy(|canopy| {
                canopy.script_context_stack.push(canopy.root_id());
                Ok(())
            })
            .expect("outer guard installed")?;

            {
                let _inner_guard = ReentrantCanopyGuard::push(&mut inner);
                with_reentrant_canopy(|canopy| {
                    canopy.script_context_stack.push(canopy.root_id());
                    Ok(())
                })
                .expect("inner guard installed")?;
            }

            with_reentrant_canopy(|canopy| {
                canopy.script_context_stack.push(canopy.root_id());
                Ok(())
            })
            .expect("outer guard restored")?;
        }

        assert_eq!(outer.script_context_stack.len(), 2);
        assert_eq!(inner.script_context_stack.len(), 1);
        assert!(with_reentrant_canopy(|_| Ok(())).is_none());
        Ok(())
    }

    /// Build one marshaled table pair for value-policy tests.
    fn pair(key: ValueSnapshot, value: ValueSnapshot) -> MarshaledPair {
        MarshaledPair { key, value }
    }

    #[test]
    fn marshaled_scalar_policy_is_explicit() {
        assert_eq!(
            marshaled_to_arg_value(&ValueSnapshot::Nil),
            Ok(ArgValue::Null)
        );
        assert_eq!(
            marshaled_to_arg_value(&ValueSnapshot::Boolean(true)),
            Ok(ArgValue::Bool(true))
        );
        assert_eq!(
            marshaled_to_arg_value(&ValueSnapshot::Integer(7)),
            Ok(ArgValue::Int(7))
        );
        assert_eq!(
            marshaled_to_arg_value(&ValueSnapshot::Number(7.0)),
            Ok(ArgValue::Int(7))
        );
        assert_eq!(
            marshaled_to_arg_value(&ValueSnapshot::Number(7.5)),
            Ok(ArgValue::Float(7.5))
        );
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                marshaled_to_arg_value(&ValueSnapshot::Number(value)),
                Err("result: non-finite numbers are not supported".to_string())
            );
        }
        assert_eq!(
            marshaled_to_arg_value(&ValueSnapshot::String(vec![0xff])),
            Err(
                "result: invalid UTF-8 string: invalid utf-8 sequence of 1 bytes from index 0"
                    .to_string()
            )
        );
    }

    #[test]
    fn marshaled_table_policy_uses_shared_layouts_and_paths() {
        assert_eq!(
            marshaled_to_arg_value(&ValueSnapshot::Table(Vec::new())),
            Ok(ArgValue::Map(BTreeMap::new()))
        );
        assert_eq!(
            marshaled_to_arg_value(&ValueSnapshot::Table(vec![
                pair(
                    ValueSnapshot::Integer(2),
                    ValueSnapshot::String(b"b".to_vec())
                ),
                pair(
                    ValueSnapshot::Integer(1),
                    ValueSnapshot::String(b"a".to_vec())
                ),
            ])),
            Ok(ArgValue::Array(vec![
                ArgValue::String("a".to_string()),
                ArgValue::String("b".to_string()),
            ]))
        );

        let external_node = ValueSnapshot::Table(vec![
            pair(
                ValueSnapshot::String(b"type".to_vec()),
                ValueSnapshot::String(b"NodeId".to_vec()),
            ),
            pair(
                ValueSnapshot::String(b"token".to_vec()),
                ValueSnapshot::String(b"node-1".to_vec()),
            ),
        ]);
        assert!(matches!(
            marshaled_to_arg_value(&external_node),
            Ok(ArgValue::Map(_))
        ));

        let nested = ValueSnapshot::Table(vec![pair(
            ValueSnapshot::String(b"actions".to_vec()),
            ValueSnapshot::Table(vec![pair(
                ValueSnapshot::Integer(1),
                ValueSnapshot::Table(vec![pair(
                    ValueSnapshot::String(b"target".to_vec()),
                    ValueSnapshot::Opaque("userdata"),
                )]),
            )]),
        )]);
        assert_eq!(
            marshaled_to_arg_value(&nested),
            Err("result.actions[1].target: unsupported script value type: userdata".to_string())
        );

        let invalid = [
            (
                ValueSnapshot::Table(vec![
                    pair(ValueSnapshot::Integer(1), ValueSnapshot::Nil),
                    pair(ValueSnapshot::Integer(3), ValueSnapshot::Nil),
                ]),
                "result: sparse table missing index 2",
            ),
            (
                ValueSnapshot::Table(vec![
                    pair(ValueSnapshot::Integer(1), ValueSnapshot::Nil),
                    pair(ValueSnapshot::String(b"name".to_vec()), ValueSnapshot::Nil),
                ]),
                "result: mixed integer and string table keys are not supported",
            ),
            (
                ValueSnapshot::Table(vec![pair(ValueSnapshot::Boolean(true), ValueSnapshot::Nil)]),
                "result: unsupported table key type: boolean",
            ),
        ];
        for (value, expected) in invalid {
            assert_eq!(marshaled_to_arg_value(&value), Err(expected.to_string()));
        }
    }

    #[test]
    fn live_and_marshaled_value_policy_agree_without_erasing_node_identity() {
        let surface = Surface::builder()
            .module(build_base_module().expect("base module builds"))
            .build()
            .expect("surface builds");
        let mut vm = surface
            .vm_builder(&VmConfig::untrusted(
                Ambient::deterministic(0),
                Limits::unlimited(),
            ))
            .build()
            .expect("VM builds");
        vm.step(|scope| {
            let ordinary = scope.create_table()?;
            ordinary.set_index(scope, 2, "two")?;
            ordinary.set_index(scope, 1, true)?;
            let path = ValuePath::root("value");
            let live = scoped_to_arg_value_at(scope, ScopedValue::Table(ordinary), &path)
                .expect("ordinary live value converts");
            let marshaled = scope.marshal(ScopedValue::Table(ordinary))?;
            let owned = marshaled_to_arg_value_at(&marshaled, &path)
                .expect("ordinary marshaled value converts");
            assert_eq!(live, owned);

            let node_id = NodeId::default();
            let nested_node = scope.create_table()?;
            nested_node.set(
                scope,
                "target",
                scope.create_userdata(NodeHandle { id: node_id })?,
            )?;
            assert_eq!(
                scoped_to_arg_value(scope, ScopedValue::Table(nested_node)),
                Ok(ArgValue::Map(BTreeMap::from([(
                    "target".to_string(),
                    ArgValue::Node(node_id),
                )])))
            );

            let sparse = scope.create_table()?;
            sparse.set_index(scope, 1, true)?;
            sparse.set_index(scope, 3, true)?;
            assert_eq!(
                scoped_to_arg_value(scope, ScopedValue::Table(sparse)),
                Err("value: sparse table missing index 2".to_string())
            );

            let mixed = scope.create_table()?;
            mixed.set_index(scope, 1, true)?;
            mixed.set(scope, "name", "mixed")?;
            assert_eq!(
                scoped_to_arg_value(scope, ScopedValue::Table(mixed)),
                Err("value: mixed integer and string table keys are not supported".to_string())
            );
            assert_eq!(
                scoped_to_arg_value(scope, ScopedValue::Number(f64::INFINITY)),
                Err("value: non-finite numbers are not supported".to_string())
            );
            let invalid_utf8 = scope.create_string([0xff])?;
            assert!(
                scoped_to_arg_value(scope, ScopedValue::String(invalid_utf8))
                    .is_err_and(|error| error.starts_with("value: invalid UTF-8 string:"))
            );
            Ok(())
        })
        .expect("live conversion scope succeeds");
    }

    #[test]
    fn tcompile_error_reports_details() {
        let host = LuauHost::new();
        let err = host.compile("local =").unwrap_err();
        let error::Error::Parse(parse) = err else {
            panic!("expected a parse error");
        };
        // The strict-mode prefix occupies line 1, so the fault is on line 2,
        // carried as a structured position from the compiler.
        assert!(
            parse.to_string().contains("(line 2"),
            "parse error should carry the source line: {parse}"
        );
    }

    #[test]
    fn tprint_output_lands_in_script_logs() -> Result<()> {
        run_ttree(|c, _, _| {
            c.finalize_api()?;
            let scr = c.script_host.compile(r#"print("plain print", 42)"#)?;
            let host = c.script_host.clone();
            host.execute(c, c.core.root_id(), scr, None)?;
            let logs = host.take_logs();
            assert!(
                logs.iter().any(|line| line.contains("plain print")),
                "print output should land in the evaluation log: {logs:?}"
            );
            Ok(())
        })
    }

    #[test]
    fn print_quota_and_sequential_diagnostics_are_per_invocation() -> Result<()> {
        run_ttree(|c, _, _| {
            c.finalize_api()?;
            let host = c.script_host.clone();
            let noisy = host.compile("for i = 1, 5000 do print(i) end")?;
            host.execute(c, c.core.root_id(), noisy, None)?;
            let logs = host.take_logs();
            let marker = String::from_utf8_lossy(SinkQuota::TRUNCATION_MARKER)
                .trim_end()
                .to_string();
            assert!(logs.iter().any(|line| line == &marker));
            assert!(logs.len() <= 4097, "quota should bound captured calls");

            let quiet = host.compile("return true")?;
            host.execute(c, c.core.root_id(), quiet, None)?;
            assert!(host.take_logs().is_empty());
            Ok(())
        })
    }

    #[test]
    fn node_handle_marshal_hook_returns_external_token_record() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.finalize_api()?;
            let host = c.script_host.clone();
            let mut runtime_cell = host.runtime.borrow_mut();
            let runtime = runtime_cell.as_mut().expect("finalized runtime");
            let mut marshaled = None;
            runtime
                .step(&CallOptions::new(), |scope| {
                    let value =
                        ScopedValue::Userdata(scope.create_userdata(NodeHandle { id: tree.a })?);
                    marshaled = Some(scope.marshal(value)?);
                    Ok(())
                })
                .map_err(|err| error::Error::Script(err.to_string()))?;

            assert_eq!(
                marshaled.expect("marshaled value"),
                node_handle_marshal(&NodeHandle { id: tree.a })
            );
            Ok(())
        })
    }

    #[test]
    fn retained_node_handle_is_rejected_after_removal() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.finalize_api()?;
            let host = c.script_host.clone();
            c.core.remove_subtree(tree.a)?;
            let anchor = c.core.root_id();
            c.script_context_stack.push(anchor);
            let mut runtime_cell = host.runtime.borrow_mut();
            let runtime = runtime_cell.as_mut().expect("finalized runtime");
            runtime
                .step_with_context(c, &CallOptions::new(), |scope| {
                    let handle = scope.create_userdata(NodeHandle { id: tree.a })?;
                    let values = MultiValue::from_values(vec![ScopedValue::Userdata(handle)]);
                    let error = ArgReader::new(values)
                        .node_id(scope)
                        .expect_err("removed node handle should be rejected");
                    assert!(error.script_fields().iter().any(|field| {
                        field.name == "kind"
                            && matches!(&field.value, OwnedValue::Bytes(value) if value == b"node_invalid")
                    }));
                    Ok(())
                })
                .map_err(|error| error::Error::Script(error.to_string()))?;
            assert_eq!(c.script_context_stack.pop(), Some(anchor));
            Ok(())
        })
    }

    #[test]
    fn binding_replacement_releases_old_and_failed_callbacks() -> Result<()> {
        run_ttree(|c, _, _| {
            c.finalize_api()?;
            let host = c.script_host.clone();
            let install = host.compile(
                r#"canopy.bind_with("a", { path = "" }, function() local value = true end)"#,
            )?;
            host.execute(c, c.core.root_id(), install, None)?;
            assert_eq!(host.state.borrow().closures.functions.len(), 1);

            let replace = host.compile(
                r#"canopy.bind_with("a", { path = "" }, function() local value = false end)"#,
            )?;
            host.execute(c, c.core.root_id(), replace, None)?;
            assert_eq!(host.state.borrow().closures.functions.len(), 1);

            let invalid = host
                .compile(r#"canopy.bind_with("a", { path = "invalid-name" }, function() end)"#)?;
            host.execute(c, c.core.root_id(), invalid, None)
                .expect_err("invalid replacement path should fail");
            assert_eq!(host.state.borrow().closures.functions.len(), 1);

            let exhausted = host.compile("canopy.on_start(function() end)")?;
            host.state.borrow_mut().closures.next_function_id = u64::MAX;
            host.execute(c, c.core.root_id(), exhausted, None)
                .expect_err("closure identifier exhaustion should fail");
            assert_eq!(host.state.borrow().closures.functions.len(), 1);
            Ok(())
        })
    }

    #[test]
    fn script_identifier_exhaustion_is_reported() -> Result<()> {
        run_ttree(|c, _, _| {
            c.script_host.state.borrow_mut().scripts.next_script_id = u64::MAX;
            let error = c
                .script_host
                .compile("return true")
                .expect_err("script identifier exhaustion should fail");
            assert!(matches!(error, error::Error::InvalidOperation(_)));
            Ok(())
        })
    }

    #[test]
    fn wait_for_returns_when_predicate_is_truthy() -> Result<()> {
        run_ttree(|c, _, _| {
            let value =
                c.eval_script_value("return canopy.wait_for(function() return true end, 10)")?;
            assert_eq!(value, ArgValue::Bool(true));
            Ok(())
        })
    }

    #[test]
    fn wait_for_timeout_surfaces_as_script_timeout() -> Result<()> {
        run_ttree(|c, _, _| {
            let error = c
                .eval_script_value("return canopy.wait_for(function() return false end, 1)")
                .expect_err("wait should time out");
            assert!(matches!(
                error,
                error::Error::ScriptTimeout { timeout_ms: 1 }
            ));
            Ok(())
        })
    }

    #[test]
    fn tscript_bindings_carry_declaration_sites() -> Result<()> {
        run_ttree(|c, _, _| {
            c.finalize_api()?;
            let scr = c
                .script_host
                .compile("canopy.bind(\"z\", function() end)")?;
            let host = c.script_host.clone();
            host.execute(c, c.core.root_id(), scr, None)?;
            let check = c.script_host.compile(
                r#"
                for _, binding in canopy.bindings() do
                    if binding.input == "z" then
                        local desc = tostring(binding.desc)
                        canopy.assert(
                            string.find(desc, "script:", 1, true) ~= nil,
                            "binding desc should carry the declaration site: " .. desc
                        )
                        return
                    end
                end
                canopy.assert(false, "binding for z not found")
                "#,
            )?;
            host.execute(c, c.core.root_id(), check, None)?;
            Ok(())
        })
    }

    #[test]
    fn texecute() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.finalize_api()?;
            let scr = c.script_host.compile(r#"bb_la.c_leaf()"#)?;
            let host = c.script_host.clone();
            host.execute(c, tree.b_a, scr, None)?;
            assert_eq!(get_state().path, ["bb_la.c_leaf()"]);
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn truntime_error_returns_script_error() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.finalize_api()?;
            let scr = c.script_host.compile(r#"canopy.assert(false, "boom")"#)?;
            let host = c.script_host.clone();
            let err = host.execute(c, tree.b_a, scr, None);
            assert!(matches!(err, Err(error::Error::Script(_))));
            Ok(())
        })
    }

    #[test]
    fn script_context_stack_pops_after_runtime_error() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.finalize_api()?;
            let scr = c.script_host.compile(r#"error("boom")"#)?;
            let host = c.script_host.clone();
            let err = host.execute(c, tree.a, scr, None);
            assert!(matches!(err, Err(error::Error::Script(_))));
            assert!(c.script_context_stack.is_empty());
            Ok(())
        })
    }

    #[test]
    fn tcheck_script_reports_type_errors() -> Result<()> {
        run_ttree(|c, _, _| {
            c.finalize_api()?;
            let result = c
                .script_host
                .check_script("tests/type-error.luau", "local value: string = 1")?;
            assert!(!result.is_ok());
            assert!(result.has_errors());
            assert!(
                result
                    .diagnostics()
                    .iter()
                    .all(|diagnostic| diagnostic.source.as_deref()
                        == Some("tests/type-error.luau"))
            );
            assert!(
                result
                    .diagnostics()
                    .iter()
                    .all(|diagnostic| { diagnostic.line > 0 && diagnostic.column > 0 })
            );
            Ok(())
        })
    }

    #[test]
    fn tcompile_rejects_type_errors_when_finalized() -> Result<()> {
        run_ttree(|c, _, _| {
            c.finalize_api()?;
            let err = c.script_host.compile("local value: string = 1");
            assert!(matches!(err, Err(error::Error::Parse(_))));
            Ok(())
        })
    }
}
