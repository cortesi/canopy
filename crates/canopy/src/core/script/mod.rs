use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt, mem,
    ptr::NonNull,
    rc::Rc,
    result::Result as StdResult,
    sync::Arc,
    time::Duration,
    vec,
};

use oxau::{
    compile::{BytecodeChunk, CompileOptions, compile_for, restrict_compile_options},
    diagnostic::{DiagnosticSeverity, TypeDiagnostic},
    embed::{
        Function, HostType, HostTypeBuilder, IntoLua, ModuleBinding, ModuleBuilder,
        ModuleBuilderExt, MultiValue, NativeModule, RuntimeError, Scope, ScopedHostFunction,
        ScopedValue, ScriptError, ScriptErrorField, StashedClosure, Table,
    },
    profile::Profile,
    session::{Ambient, Cancel, Limits, LoadedModule, RuntimeErrorKind, SinkQuota, Vm},
    source::{ModuleId, ModuleSource},
    surface::SurfaceSpec,
};
use serde::{Deserialize, Serialize};

use crate::{
    Canopy, NodeId,
    commands::{self, ArgValue, CommandArgs, CommandInvocation, CommandSet, CommandSpec},
    core::{
        context::{Context, CoreContext, CoreViewContext, ReadContext},
        help::BindingKind,
        inputmap::{self, BindingTarget},
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

pub use modules::{CanopyModuleSource, ScriptModuleRoots};

/// Script identifier.
pub type ScriptId = u64;

/// Stable handle for a stored Luau closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LuauFunctionId(u64);

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
    /// Construct an error diagnostic at a source location.
    pub fn error(line: usize, column: usize, message: impl Into<String>) -> Self {
        Self {
            severity: "error".to_string(),
            line,
            column,
            message: message.into(),
        }
    }

    /// Return true if this diagnostic should fail script evaluation.
    pub fn is_error(&self) -> bool {
        self.severity == "error"
    }
}

/// Stable result returned by Luau typechecking APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptCheckResult {
    /// Diagnostics emitted by the checker.
    diagnostics: Vec<ScriptCheckDiagnostic>,
}

impl ScriptCheckResult {
    /// Construct a successful typecheck result.
    pub fn ok() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
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
}

/// Cached compiled script: the compiled chunk plus its module once loaded into
/// the retained VM. Loading happens at `finalize()` for scripts compiled
/// earlier, and at `compile()` time afterwards.
struct Script {
    /// Compiled bytecode chunk.
    chunk: BytecodeChunk,
    /// Original source text.
    source: String,
    /// Module name used when loading the chunk into the retained VM.
    module_name: Vec<u8>,
    /// Module loaded into the retained VM, shared so executions need not hold
    /// the host state borrow while the script runs.
    module: Option<Rc<LoadedModule>>,
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
    fn insert(&mut self, chunk: BytecodeChunk, source: &str, module_name: &[u8]) -> ScriptId {
        let id = self.next_script_id;
        self.next_script_id = self.next_script_id.saturating_add(1);
        self.scripts.insert(
            id,
            Script {
                chunk,
                source: source.to_string(),
                module_name: module_name.to_vec(),
                module: None,
            },
        );
        id
    }

    /// Return the loaded module for a script, if the script exists and is loaded.
    fn module(&self, id: ScriptId) -> Option<Rc<LoadedModule>> {
        self.scripts
            .get(&id)
            .and_then(|script| script.module.clone())
    }

    /// Return a clone of the compiled chunk for a script.
    fn chunk(&self, id: ScriptId) -> Option<BytecodeChunk> {
        self.scripts.get(&id).map(|script| script.chunk.clone())
    }

    /// Return the module name used when loading a script.
    fn module_name(&self, id: ScriptId) -> Option<Vec<u8>> {
        self.scripts
            .get(&id)
            .map(|script| script.module_name.clone())
    }

    /// Record the loaded module for a script.
    fn set_module(&mut self, id: ScriptId, module: Rc<LoadedModule>) {
        if let Some(script) = self.scripts.get_mut(&id) {
            script.module = Some(module);
        }
    }

    /// Return the original source for a script.
    fn source(&self, id: ScriptId) -> Option<String> {
        self.scripts.get(&id).map(|script| script.source.clone())
    }
}

/// Stored Luau closure with a stable host-side id. The stash pins the closure
/// in the VM registry; dropping it queues the release for the VM's next step.
struct StoredFunction {
    /// Registry-rooted closure handle.
    stashed: StashedClosure,
    /// Help/debug label for the closure.
    label: Option<String>,
}

/// Stored Luau closure registry.
#[derive(Default)]
struct ClosureRegistry {
    /// Stored Luau closures keyed by stable id.
    functions: HashMap<LuauFunctionId, StoredFunction>,
    /// Next stored function identifier.
    next_function_id: u64,
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
    fn insert(&mut self, stashed: StashedClosure, label: Option<String>) -> LuauFunctionId {
        let id = LuauFunctionId(self.next_function_id);
        self.next_function_id = self.next_function_id.saturating_add(1);
        self.functions.insert(id, StoredFunction { stashed, label });
        id
    }

    /// Return a shared handle to a stored function's stash.
    fn stashed(&self, id: LuauFunctionId) -> Option<StashedClosure> {
        self.functions
            .get(&id)
            .map(|function| function.stashed.clone())
    }

    /// Return the help/debug label for a stored function.
    fn label(&self, id: LuauFunctionId) -> Option<String> {
        self.functions
            .get(&id)
            .and_then(|function| function.label.clone())
    }

    /// Remove a stored function, dropping its registry pin.
    fn remove(&mut self, id: LuauFunctionId) {
        self.functions.remove(&id);
    }
}

/// Diagnostics collected during script execution.
#[derive(Default)]
struct ScriptDiagnostics {
    /// Log messages emitted by the most recent script evaluation.
    logs: Vec<String>,
    /// Assertion results emitted by the most recent script evaluation.
    assertions: Vec<ScriptAssertion>,
}

impl ScriptDiagnostics {
    /// Clear recorded logs and assertions.
    fn clear(&mut self) {
        self.logs.clear();
        self.assertions.clear();
    }

    /// Append a log line.
    fn push_log(&mut self, message: String) {
        self.logs.push(message);
    }

    /// Append an assertion result.
    fn push_assertion(&mut self, passed: bool, message: String) {
        self.assertions.push(ScriptAssertion { passed, message });
    }

    /// Drain log lines.
    fn take_logs(&mut self) -> Vec<String> {
        mem::take(&mut self.logs)
    }

    /// Return a clone of current log lines.
    fn logs(&self) -> Vec<String> {
        self.logs.clone()
    }

    /// Drain assertion results.
    fn take_assertions(&mut self) -> Vec<ScriptAssertion> {
        mem::take(&mut self.assertions)
    }

    /// Return a clone of current assertion results.
    fn assertions(&self) -> Vec<ScriptAssertion> {
        self.assertions.clone()
    }
}

/// Shared mutable host state.
#[derive(Default)]
struct LuauState {
    /// Compiled script cache.
    scripts: ScriptCache,
    /// Stored closure registry.
    closures: ClosureRegistry,
    /// Execution diagnostics.
    diagnostics: ScriptDiagnostics,
    /// Cached rendered d.luau definitions.
    definitions: Option<String>,
    /// Audited script surface used for checks and VM construction.
    surface: Option<SurfaceSpec>,
    /// Whether the command surface has been finalized.
    finalized: bool,
    /// Deferred hooks to execute after the first live render.
    on_start_hooks: Vec<LuauFunctionId>,
}

impl LuauState {
    /// Construct empty script host state.
    fn new() -> Self {
        Self {
            scripts: ScriptCache::new(),
            closures: ClosureRegistry::new(),
            ..Self::default()
        }
    }

    /// Mark the script API as finalized and cache its definitions.
    fn finalize(&mut self, definitions: String, surface: SurfaceSpec) {
        self.definitions = Some(definitions);
        self.surface = Some(surface);
        self.finalized = true;
    }

    /// Drain deferred `on_start` hooks in registration order.
    fn drain_on_start_hooks(&mut self) -> Vec<LuauFunctionId> {
        mem::take(&mut self.on_start_hooks)
    }
}

/// Active script execution context.
#[derive(Clone, Copy)]
struct ScriptExecutionContext {
    /// Current canopy instance.
    canopy: NonNull<Canopy>,
    /// Node used as the command dispatch anchor.
    node_id: NodeId,
    /// Type-erased pointer to the innermost live VM scope, when a host call or
    /// scope step is active. Nested script execution re-enters the VM through
    /// this scope instead of the (already borrowed) `Vm`.
    scope: Option<NonNull<()>>,
}

impl ScriptExecutionContext {
    /// Execute a closure with the active canopy instance.
    fn with_canopy<R>(self, f: impl FnOnce(&mut Canopy, NodeId) -> Result<R>) -> Result<R> {
        // SAFETY: contexts are pushed only by `ScriptContextGuard` while executing a script
        // callback on the current thread. The guard is stack-scoped and pops this context on drop,
        // so the pointer is used only while the original `&mut Canopy` is live.
        let canopy = unsafe { &mut *self.canopy.as_ptr() };
        f(canopy, self.node_id)
    }
}

/// Stack guard for the thread-local script execution context.
struct ScriptContextGuard;

impl ScriptContextGuard {
    /// Push a script execution context for the current thread. The innermost
    /// live scope (if any) is inherited: a nested script run still executes
    /// inside the same VM borrow.
    fn push(canopy: &mut Canopy, node_id: NodeId) -> Self {
        SCRIPT_GLOBAL.with(|stack| {
            let mut stack = stack.borrow_mut();
            let scope = stack.last().and_then(|context| context.scope);
            stack.push(ScriptExecutionContext {
                canopy: NonNull::from(canopy),
                node_id,
                scope,
            });
        });
        Self
    }

    /// Push a script execution context carrying an explicit live scope.
    fn push_with_scope(canopy: &mut Canopy, node_id: NodeId, scope: &Scope<'_>) -> Self {
        SCRIPT_GLOBAL.with(|stack| {
            stack.borrow_mut().push(ScriptExecutionContext {
                canopy: NonNull::from(canopy),
                node_id,
                scope: Some(NonNull::from(scope).cast()),
            });
        });
        Self
    }
}

impl Drop for ScriptContextGuard {
    fn drop(&mut self) {
        SCRIPT_GLOBAL.with(|stack| {
            let _ = stack.borrow_mut().pop();
        });
    }
}

/// Stack guard installing the active host-call scope on top of the current
/// script context, so nested execution paths re-enter the VM through it.
struct ScopeContextGuard {
    /// Whether a context was pushed (no-op when no script context is active).
    pushed: bool,
}

impl ScopeContextGuard {
    /// Push a copy of the current context carrying the host call's scope.
    fn push(scope: &Scope<'_>) -> Self {
        SCRIPT_GLOBAL.with(|stack| {
            let mut stack = stack.borrow_mut();
            let Some(top) = stack.last().copied() else {
                return Self { pushed: false };
            };
            stack.push(ScriptExecutionContext {
                scope: Some(NonNull::from(scope).cast()),
                ..top
            });
            Self { pushed: true }
        })
    }
}

impl Drop for ScopeContextGuard {
    fn drop(&mut self) {
        if self.pushed {
            SCRIPT_GLOBAL.with(|stack| {
                let _ = stack.borrow_mut().pop();
            });
        }
    }
}

thread_local! {
    static SCRIPT_GLOBAL: RefCell<Vec<ScriptExecutionContext>> = const { RefCell::new(Vec::new()) };
}

/// Return the innermost live scope pointer from the script context stack.
fn current_scope_ptr() -> Option<NonNull<()>> {
    SCRIPT_GLOBAL.with(|stack| stack.borrow().last().and_then(|context| context.scope))
}

/// Return true when a live script scope is active on this thread.
///
/// True means the current Rust code was reached from inside a running script,
/// so any script execution started now is nested within that evaluation.
pub(crate) fn in_live_scope() -> bool {
    current_scope_ptr().is_some()
}

/// Reconstruct a scope reference from the type-erased context pointer.
///
/// # Safety
/// The pointer must come from `current_scope_ptr` while the host call (or scope
/// step) that pushed it is still on the Rust stack — guaranteed because
/// contexts are popped by their stack guards before the scope dies. The
/// fabricated lifetime is bounded by the caller's borrow, so no handle minted
/// from the returned scope can outlive the live scope.
unsafe fn scope_from_ptr<'a>(ptr: NonNull<()>) -> &'a Scope<'a> {
    unsafe { ptr.cast::<Scope<'a>>().as_ref() }
}

/// Luau host state shared by the canopy runtime.
#[derive(Clone)]
pub(crate) struct LuauHost {
    /// Retained oxau VM, built by `finalize()`.
    vm: Rc<RefCell<Option<Vm>>>,
    /// Shared mutable host state.
    state: Rc<RefCell<LuauState>>,
}

/// Backwards-compatible type alias used throughout the current codebase.
pub(crate) type ScriptHost = LuauHost;

impl fmt::Debug for LuauHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LuauHost").finish_non_exhaustive()
    }
}

/// The VM profile every canopy app runs under: the full safe standard library
/// without runtime compilation (`loadstring`), and no `require` source.
fn canopy_profile() -> Profile {
    Profile::full().without_runtime_compilation()
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

/// Prefix scripts with strict mode unless they already declare a mode.
fn strict_source(source: &str) -> String {
    let trimmed = source.trim_start();
    if trimmed.starts_with("--!") {
        source.to_string()
    } else {
        format!("--!strict\n{source}")
    }
}

/// Compile Luau source under the canopy profile.
fn compile_chunk(source: &str) -> Result<BytecodeChunk> {
    let profile = canopy_profile();
    let mut options = CompileOptions::for_vm_execution();
    restrict_compile_options(&profile, &mut options);
    compile_for(&profile, source.as_bytes(), &options).map_err(|err| {
        let begin = err.location().map(|location| location.begin);
        error::Error::Parse(error::ParseError::with_position(
            err.message(),
            begin.map(|position| position.line as usize + 1),
            begin.map(|position| position.column as usize + 1),
        ))
    })
}

/// Format Luau typecheck diagnostics for display.
fn format_typecheck_diagnostics(result: &ScriptCheckResult) -> String {
    result
        .errors()
        .map(|diagnostic| {
            format!(
                "{}:{}: {}",
                diagnostic.line, diagnostic.column, diagnostic.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert an Oxau type diagnostic to Canopy's stable script diagnostic shape.
pub(crate) fn type_diagnostic_to_script(diagnostic: &TypeDiagnostic) -> ScriptCheckDiagnostic {
    let begin = diagnostic.primary_location.begin;
    ScriptCheckDiagnostic {
        severity: match diagnostic.severity {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning | DiagnosticSeverity::Info => "warning",
        }
        .to_string(),
        line: begin.line as usize + 1,
        column: begin.column as usize + 1,
        message: format!(
            "{}: {}",
            diagnostic.category,
            diagnostic
                .context
                .as_deref()
                .unwrap_or("type checker diagnostic")
        ),
    }
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
fn with_current_canopy<R>(f: impl FnOnce(&mut Canopy, NodeId) -> Result<R>) -> Result<R> {
    SCRIPT_GLOBAL.with(|stack| {
        let current = *stack
            .borrow()
            .last()
            .ok_or_else(|| error::Error::Script("no active script context".into()))?;
        current.with_canopy(f)
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
        .class(commands::decl::Class::new("NodeId"))
        .eq_by(|left, right| left.id == right.id)
        .build()
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
            format!("expected NodeId, got {}", scoped_type_name(&other)),
            [
                ScriptErrorField::new("kind", "type_mismatch"),
                ScriptErrorField::new("expected", "NodeId"),
                ScriptErrorField::new("got", scoped_type_name(&other)),
            ],
        )),
    }
}

/// Return a display name for a scoped value's type.
fn scoped_type_name(value: &ScopedValue<'_>) -> &'static str {
    value.type_name()
}

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
        other => Err(format!("expected string, got {}", scoped_type_name(&other))),
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
        other => format!("<{}>", scoped_type_name(&other)),
    }
}

/// Convert a scoped value into a dynamic command argument.
fn scoped_to_arg_value<'s>(
    scope: &Scope<'s>,
    value: ScopedValue<'s>,
) -> StdResult<ArgValue, String> {
    match value {
        ScopedValue::Nil => Ok(ArgValue::Null),
        ScopedValue::Boolean(value) => Ok(ArgValue::Bool(value)),
        ScopedValue::Integer(value) => Ok(ArgValue::Int(value)),
        // Whole numbers surface as integers, matching the previous VM's
        // integer representation for integral Luau values.
        ScopedValue::Number(value)
            if value.fract() == 0.0 && (i64::MIN as f64..=i64::MAX as f64).contains(&value) =>
        {
            Ok(ArgValue::Int(value as i64))
        }
        ScopedValue::Number(value) => Ok(ArgValue::Float(value)),
        ScopedValue::String(_) => Ok(ArgValue::String(scoped_value_to_string(scope, value)?)),
        ScopedValue::Table(table) => table_to_arg_value(scope, table),
        ScopedValue::Userdata(userdata) => Ok(ArgValue::Node(
            userdata
                .borrow::<NodeHandle>(scope)
                .map_err(|err| err.message().to_string())?
                .id,
        )),
        other => Err(format!(
            "unsupported script value type: {}",
            scoped_type_name(&other)
        )),
    }
}

/// Convert a scoped table into an `ArgValue`.
fn table_to_arg_value<'s>(scope: &Scope<'s>, table: Table<'s>) -> StdResult<ArgValue, String> {
    let mut indexed = BTreeMap::new();
    let mut named = BTreeMap::new();

    for (key, value) in table.pairs(scope).map_err(|err| err.to_string())? {
        match key {
            ScopedValue::Integer(index) if index > 0 => {
                indexed.insert(index as usize, scoped_to_arg_value(scope, value)?);
            }
            ScopedValue::Number(index) if index.fract() == 0.0 && index >= 1.0 => {
                indexed.insert(index as usize, scoped_to_arg_value(scope, value)?);
            }
            ScopedValue::String(_) => {
                let key = scoped_value_to_string(scope, key)?;
                named.insert(key, scoped_to_arg_value(scope, value)?);
            }
            other => {
                return Err(format!(
                    "unsupported table key type for command args: {}",
                    scoped_type_name(&other)
                ));
            }
        }
    }

    if named.is_empty() && !indexed.is_empty() {
        let mut values = Vec::with_capacity(indexed.len());
        for expected in 1..=indexed.len() {
            let value = indexed
                .remove(&expected)
                .ok_or_else(|| "sparse arrays are not supported in command args".to_string())?;
            values.push(value);
        }
        return Ok(ArgValue::Array(values));
    }

    if indexed.is_empty() {
        return Ok(ArgValue::Map(named));
    }

    Err("mixed array/map tables are not supported in command args".into())
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
        (
            "name".to_string(),
            ArgValue::String(node.name().to_string()),
        ),
        (
            "focused".to_string(),
            ArgValue::Bool(root_ctx.node_is_focused(node_id)),
        ),
        (
            "on_focus_path".to_string(),
            ArgValue::Bool(root_ctx.node_is_on_focus_path(node_id)),
        ),
        ("hidden".to_string(), ArgValue::Bool(node.hidden())),
        ("visible".to_string(), ArgValue::Bool(!node.hidden())),
        (
            "children".to_string(),
            node_list_to_arg(node.children().iter().copied()),
        ),
        ("rect".to_string(), rect),
        ("content_rect".to_string(), content_rect),
        ("canvas".to_string(), size_to_arg(node.canvas())),
        ("scroll".to_string(), point_to_arg(node.scroll())),
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
        .children()
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

/// Render a command invocation into a human-readable target string.
fn invocation_target(invocation: &CommandInvocation) -> String {
    let (owner, name) = invocation
        .id
        .0
        .split_once("::")
        .unwrap_or(("", invocation.id.0));
    let callee = if owner.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", luau_global_owner_name(owner), name)
    };
    match &invocation.args {
        CommandArgs::Positional(values) if values.is_empty() => format!("{callee}()"),
        CommandArgs::Named(values) if values.is_empty() => format!("{callee}()"),
        _ => format!("{callee}(...)"),
    }
}

/// Convert a binding target into a discoverable summary string.
fn binding_target_summary(target: &BindingTarget) -> String {
    match target {
        BindingTarget::Script(_) => "script".to_string(),
        BindingTarget::Command(invocation) => invocation_target(invocation),
        BindingTarget::CommandSequence(commands) => {
            format!("[sequence: {} commands]", commands.len())
        }
        BindingTarget::SetInputMode(mode) if mode.is_empty() => "canopy.set_mode(\"\")".to_string(),
        BindingTarget::SetInputMode(mode) => format!("canopy.set_mode({mode:?})"),
        BindingTarget::LuauFunction(_) => "luau".to_string(),
    }
}

/// Extract an optional human-readable binding description.
fn binding_desc(canopy: &Canopy, target: &BindingTarget) -> Option<String> {
    match target {
        BindingTarget::LuauFunction(id) => canopy.script_host.function_label(*id),
        _ => None,
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
        (
            "target".to_string(),
            ArgValue::String(binding_target_summary(binding.target)),
        ),
    ]);
    if let Some(desc) = binding_desc(canopy, binding.target) {
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
    let Color::Rgb { r, g, b } = color.to_rgb() else {
        unreachable!("to_rgb always returns Color::Rgb")
    };
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
                (
                    "target".to_string(),
                    ArgValue::String(binding_target_summary(binding.target)),
                ),
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
    spec: &'static CommandSpec,
    node_id: NodeId,
    values: Vec<ArgValue>,
    allow_map_named: bool,
) -> Result<ArgValue> {
    with_current_canopy(|canopy, _| {
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
fn dispatch_command_by_name(name: &str, values: Vec<ArgValue>) -> Result<ArgValue> {
    let allow_map_named = values.len() == 1;
    with_current_canopy(|canopy, node_id| {
        let spec = canopy.core.commands.get(name).ok_or_else(|| {
            error::Error::from(commands::CommandError::UnknownCommand {
                id: name.to_string(),
            })
        })?;
        dispatch_command(spec, node_id, values, allow_map_named)
    })
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
                scoped_type_name(&other)
            ))),
        }
    }

    /// Take a required node id argument.
    fn node_id(&mut self, scope: &Scope<'s>) -> StdResult<NodeId, RuntimeError> {
        let node_id = node_id_from_value(scope, self.next_value())?;
        with_current_canopy(|canopy, _| {
            if canopy.core.nodes.contains_key(node_id) {
                Ok(node_id)
            } else {
                Err(error::Error::from(commands::CommandError::InvalidNode {
                    id: node_id,
                }))
            }
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
        with_current_canopy(|canopy, _| {
            if canopy.core.nodes.contains_key(node_id) {
                Ok(Some(node_id))
            } else {
                Err(error::Error::from(commands::CommandError::InvalidNode {
                    id: node_id,
                }))
            }
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
                scoped_type_name(&other)
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
                scoped_type_name(&other)
            ))),
        }
    }
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

/// Convert a canopy error into a host-call error.
fn canopy_to_host(err: &error::Error) -> RuntimeError {
    let payload = CanopyErrorPayload::from(err);
    let mut fields = vec![ScriptErrorField::new("kind", payload.kind.clone())];
    if let Some(command) = payload.command.clone() {
        fields.push(ScriptErrorField::new("command", command));
    }
    if let Some(owner) = payload.owner.clone() {
        fields.push(ScriptErrorField::new("owner", owner));
    }
    RuntimeError::structured(payload.message.clone(), fields).with_payload(payload)
}

/// Normalized cloneable canopy error payload carried through Oxau errors.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CanopyErrorPayload {
    /// Stable script-visible category.
    kind: String,
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
            error::Error::ScriptTimeout { .. } => Self::new("timeout", err.to_string()),
            error::Error::NodeNotFound(node) => {
                Self::new("node_not_found", err.to_string()).with_owner(format!("{node:?}"))
            }
            error::Error::TypeMismatch { .. } => Self::new("type_mismatch", err.to_string()),
            error::Error::NotFound(_) => Self::new("not_found", err.to_string()),
            error::Error::Invalid(_) | error::Error::InvalidOperation(_) => {
                Self::new("invalid", err.to_string())
            }
            error::Error::ScriptStructured {
                kind,
                command,
                owner,
                message,
            } => Self {
                kind: kind.clone(),
                command: command.clone(),
                owner: owner.clone(),
                message: message.clone(),
            },
            _ => Self::new("canopy_error", err.to_string()),
        }
    }
}

impl From<&commands::CommandError> for CanopyErrorPayload {
    fn from(err: &commands::CommandError) -> Self {
        match err {
            commands::CommandError::UnknownCommand { id } => {
                Self::new("unknown_command", err.to_string()).with_command(id.clone())
            }
            commands::CommandError::DuplicateCommand { id } => {
                Self::new("duplicate_command", err.to_string()).with_command(id.clone())
            }
            commands::CommandError::NoTarget { id, owner } => {
                Self::new("no_target", err.to_string())
                    .with_command(id.clone())
                    .with_owner(owner.clone())
            }
            commands::CommandError::InvalidNode { .. } => {
                Self::new("node_invalid", err.to_string())
            }
            commands::CommandError::ArityMismatch { .. } => {
                Self::new("arity_mismatch", err.to_string())
            }
            commands::CommandError::MissingNamedArg { .. } => {
                Self::new("missing_named_arg", err.to_string())
            }
            commands::CommandError::UnknownNamedArg { .. } => {
                Self::new("unknown_named_arg", err.to_string())
            }
            commands::CommandError::TypeMismatch { .. } => {
                Self::new("type_mismatch", err.to_string())
            }
            commands::CommandError::MissingInjected { .. } => {
                Self::new("missing_injected", err.to_string())
            }
            commands::CommandError::Conversion { .. } => Self::new("conversion", err.to_string()),
            commands::CommandError::Exec(_) => Self::new("command_exec", err.to_string()),
        }
    }
}

impl CanopyErrorPayload {
    /// Builds a payload without command routing context.
    fn new(kind: impl Into<String>, message: String) -> Self {
        Self {
            kind: kind.into(),
            command: None,
            owner: None,
            message,
        }
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
        let message = match traceback {
            Some(traceback) => format!("{label} failed: {}\n{traceback}", self.message),
            None => format!("{label} failed: {}", self.message),
        };
        error::Error::ScriptStructured {
            kind: self.kind.clone(),
            command: self.command.clone(),
            owner: self.owner.clone(),
            message,
        }
    }
}

/// Host function adapter: installs the live scope on the script context stack
/// so nested execution paths can re-enter the VM, then runs the handler.
struct CanopyHostFn<F>(F);

impl<F> ScopedHostFunction for CanopyHostFn<F>
where
    F: for<'s> Fn(&Scope<'s>, MultiValue<'s>) -> StdResult<MultiValue<'s>, RuntimeError>
        + Send
        + Sync,
{
    fn call<'s>(
        &self,
        scope: &Scope<'s>,
        args: MultiValue<'s>,
    ) -> StdResult<MultiValue<'s>, RuntimeError> {
        let _guard = ScopeContextGuard::push(scope);
        (self.0)(scope, args)
    }
}

/// A plain-function canopy host handler.
type HostHandler =
    for<'s> fn(&Scope<'s>, MultiValue<'s>) -> StdResult<MultiValue<'s>, RuntimeError>;

/// Box a canopy host handler.
fn canopy_host_fn<F>(f: F) -> Box<dyn ScopedHostFunction>
where
    F: for<'s> Fn(&Scope<'s>, MultiValue<'s>) -> StdResult<MultiValue<'s>, RuntimeError>
        + Send
        + Sync
        + 'static,
{
    Box::new(CanopyHostFn(f))
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
    with_current_canopy(|canopy, _| {
        let function_id = canopy.script_host.store_function(stashed, label);
        let result = canopy.keymap.replace_binding(
            &options.mode,
            input,
            &options.path,
            BindingTarget::LuauFunction(function_id),
        );
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
    let result = dispatch_command_by_name(&name, values).map_err(|err| canopy_to_host(&err))?;
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
    let allow_map_named = values.len() == 1;
    let result = with_current_canopy(|canopy, _| {
        let spec = canopy
            .core
            .commands
            .get(&name)
            .ok_or_else(|| error::Error::Script(format!("unknown command: {name}")))?;
        dispatch_command(spec, node_id, values, allow_map_named)
            .map_err(|err| error::Error::Script(format!("command {name} failed: {err}")))
    })
    .map_err(|err| canopy_to_host(&err))?;
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
    with_current_canopy(|canopy, _| {
        canopy.script_host.push_log(message);
        Ok(())
    })
    .map_err(|err| canopy_to_host(&err))?;
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
    with_current_canopy(|canopy, _| {
        canopy
            .script_host
            .push_assertion(condition, message.clone());
        Ok(())
    })
    .map_err(|err| canopy_to_host(&err))?;
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
    let root = with_current_canopy(|canopy, _| Ok(node_id_to_arg(canopy.core.root_id())))
        .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &root)
}

/// `canopy.focused`: return the focused node id, or nil.
fn host_focused<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let focused = with_current_canopy(|canopy, _| {
        Ok(canopy
            .core
            .focus_id()
            .map(node_id_to_arg)
            .unwrap_or(ArgValue::Null))
    })
    .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &focused)
}

/// `canopy.node_info`: return the `NodeInfo` record for a node.
fn host_node_info<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    let info =
        with_current_canopy(|canopy, _| node_info_to_arg(canopy, node_id).map(ArgValue::Map))
            .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &info)
}

/// `canopy.find_node`: return the first node matching a path pattern.
fn host_find_node<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let pattern = args.string(scope)?;
    let result = with_current_canopy(|canopy, _| {
        let filter = PathFilter::normalized(&pattern)?;
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(root_ctx
            .find_node_matching(&filter)
            .map(node_id_to_arg)
            .unwrap_or(ArgValue::Null))
    })
    .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &result)
}

/// `canopy.find_nodes`: return all nodes matching a path pattern.
fn host_find_nodes<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let pattern = args.string(scope)?;
    let result = with_current_canopy(|canopy, _| {
        let filter = PathFilter::normalized(&pattern)?;
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(node_list_to_arg(root_ctx.find_nodes_matching(&filter)))
    })
    .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &result)
}

/// `canopy.parent`: return a node's parent, or nil for the root.
fn host_parent<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    let result = with_current_canopy(|canopy, _| {
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(root_ctx
            .parent_of(node_id)
            .map(node_id_to_arg)
            .unwrap_or(ArgValue::Null))
    })
    .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &result)
}

/// `canopy.children`: return a node's children.
fn host_children<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    let result = with_current_canopy(|canopy, _| {
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(node_list_to_arg(root_ctx.children_of(node_id)))
    })
    .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &result)
}

/// `canopy.tree`: return the recursive node tree from the root.
fn host_tree<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let tree = with_current_canopy(|canopy, _| tree_node_to_arg(canopy, canopy.core.root_id()))
        .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &tree)
}

/// `canopy.set_focus`: focus a node, returning whether focus moved.
fn host_set_focus<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    let focused = with_current_canopy(|canopy, _| {
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        Ok(ctx.set_focus(node_id))
    })
    .map_err(|err| canopy_to_host(&err))?;
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
    let result = with_current_canopy(|canopy, _| {
        Ok(canopy
            .core
            .locate_node(canopy.core.root_id(), point_from_coords(x, y)?)?
            .map(node_id_to_arg)
            .unwrap_or(ArgValue::Null))
    })
    .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &result)
}

/// `canopy.focus_next`: move focus to the next focusable node.
fn host_focus_next<'s>(
    _scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    with_current_canopy(|canopy, _| {
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        ctx.focus_next_global();
        Ok(())
    })
    .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_none())
}

/// `canopy.focus_prev`: move focus to the previous focusable node.
fn host_focus_prev<'s>(
    _scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    with_current_canopy(|canopy, _| {
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        ctx.focus_prev_global();
        Ok(())
    })
    .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_none())
}

/// `canopy.focus_dir`: move focus in a direction.
fn host_focus_dir<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let dir = args.string(scope)?;
    with_current_canopy(|canopy, _| {
        let dir = commands::FromArgValue::from_arg_value(&ArgValue::String(dir))
            .map_err(error::Error::from)?;
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        ctx.focus_dir_global(dir);
        Ok(())
    })
    .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_none())
}

/// `canopy.send_key`: inject a key event.
fn host_send_key<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let key_spec = args.string(scope)?;
    with_current_canopy(|canopy, _| {
        let key = key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?;
        canopy.key(key)
    })
    .map_err(|err| canopy_to_host(&err))?;
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
    with_current_canopy(|canopy, _| {
        let location = point_from_coords(x, y)?;
        canopy.mouse(mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location,
        })?;
        canopy.mouse(mouse::MouseEvent {
            action: mouse::Action::Up,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location,
        })
    })
    .map_err(|err| canopy_to_host(&err))?;
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
    with_current_canopy(|canopy, _| {
        let action = if dir.eq_ignore_ascii_case("up") {
            mouse::Action::ScrollUp
        } else if dir.eq_ignore_ascii_case("down") {
            mouse::Action::ScrollDown
        } else {
            return Err(error::Error::Script(format!(
                "unknown scroll direction: {dir}"
            )));
        };
        canopy.mouse(mouse::MouseEvent {
            action,
            button: mouse::Button::None,
            modifiers: key::Empty,
            location: point_from_coords(x, y)?,
        })
    })
    .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_none())
}

/// `canopy.bindings`: return the active binding table across all modes.
fn host_bindings<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let bindings = with_current_canopy(|canopy, _| {
        Ok(ArgValue::Array(
            canopy
                .keymap
                .bindings()
                .into_iter()
                .map(|binding| binding_info_to_arg(canopy, binding.mode, &binding.info))
                .collect(),
        ))
    })
    .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &bindings)
}

/// `canopy.commands`: return metadata for all registered commands.
fn host_commands<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let commands = with_current_canopy(|canopy, node_id| {
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
    .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &commands)
}

/// `canopy.resolve`: return the dispatch target for an owner.
fn host_resolve<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let owner = args.string(scope)?;
    let node = with_current_canopy(|canopy, node_id| {
        let resolver = commands::CommandResolver::new(&canopy.core, node_id);
        Ok(resolver
            .resolve_owner(&owner)
            .and_then(commands::CommandResolution::target)
            .map_or(ArgValue::Null, node_id_to_arg))
    })
    .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &node)
}

/// `canopy.input_mode`: return the active input mode.
fn host_input_mode<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mode = with_current_canopy(|canopy, _| Ok(canopy.input_mode().to_string()))
        .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&mode)?)))
}

/// `canopy.set_mode`: switch the active input mode.
fn host_set_mode<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let mode = args.string(scope)?;
    with_current_canopy(|canopy, _| {
        canopy.set_input_mode(&mode)?;
        Ok(())
    })
    .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_none())
}

/// `canopy.push_mode`: push an input mode above the current mode.
fn host_push_mode<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let mode = args.string(scope)?;
    with_current_canopy(|canopy, _| {
        canopy.push_input_mode(&mode)?;
        Ok(())
    })
    .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_none())
}

/// `canopy.pop_mode`: pop the top input mode and return the active mode.
fn host_pop_mode<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mode = with_current_canopy(|canopy, _| Ok(canopy.pop_input_mode().to_string()))
        .map_err(|err| canopy_to_host(&err))?;
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
    let input = inputmap::InputSpec::Key(
        key::Key::parse_spec(&key_spec)
            .map_err(error::Error::Script)
            .map_err(|err| canopy_to_host(&err))?,
    );
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
    let input = inputmap::InputSpec::Key(
        key::Key::parse_spec(&key_spec)
            .map_err(error::Error::Script)
            .map_err(|err| canopy_to_host(&err))?,
    );
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
        mouse::Mouse::parse_spec(&mouse_spec)
            .map_err(error::Error::Script)
            .map_err(|err| canopy_to_host(&err))?,
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
        mouse::Mouse::parse_spec(&mouse_spec)
            .map_err(error::Error::Script)
            .map_err(|err| canopy_to_host(&err))?,
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
    let removed =
        with_current_canopy(
            |canopy, _| Ok(canopy.unbind(inputmap::BindingId::from_u64(id as u64))),
        )
        .map_err(|err| canopy_to_host(&err))?;
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
    with_current_canopy(|canopy, _| {
        let mode = (!options.mode.is_empty()).then_some(options.mode.as_str());
        let path = (!options.path.is_empty()).then_some(options.path.as_str());
        let key = key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?;
        let _ = canopy.unbind_key_input(key, mode, path);
        Ok(())
    })
    .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_none())
}

/// `canopy.clear_bindings`: remove every binding from every mode.
fn host_clear_bindings<'s>(
    _scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    with_current_canopy(|canopy, _| {
        let _ = canopy.clear_bindings();
        Ok(())
    })
    .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_none())
}

/// `canopy.screen`: return the rendered screen as rows of cell strings.
fn host_screen<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let rows = with_current_canopy(|canopy, _| screen_to_arg(canopy))
        .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &rows)
}

/// `canopy.screen_cells`: return the rendered screen with style metadata.
fn host_screen_cells<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let rows = with_current_canopy(|canopy, _| screen_cells_to_arg(canopy))
        .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &rows)
}

/// `canopy.screen_text`: return the rendered screen as plain text.
fn host_screen_text<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let text = with_current_canopy(|canopy, _| {
        canopy.refresh_snapshot()?;
        let Some(buffer) = canopy.buf() else {
            return Err(error::Error::Script(
                "screen unavailable before render".into(),
            ));
        };
        Ok(buffer.screen_text())
    })
    .map_err(|err| canopy_to_host(&err))?;
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
    let text = with_current_canopy(|canopy, _| screen_text_for_rect(canopy, rect))
        .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&text)?)))
}

/// `canopy.node_region`: return rendered plain text inside a node's content rect.
fn host_node_region<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    let text = with_current_canopy(|canopy, _| {
        canopy.refresh_snapshot()?;
        let view = canopy
            .core
            .node(node_id)
            .ok_or_else(|| error::Error::from(commands::CommandError::InvalidNode { id: node_id }))?
            .view();
        screen_text_for_rect(canopy, view.content)
    })
    .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&text)?)))
}

/// `canopy.route_trace`: return the most recent input route trace.
fn host_route_trace<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let trace = with_current_canopy(|canopy, _| Ok(route_trace_to_arg(canopy)))
        .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &trace)
}

/// `canopy.diagnostic_dump`: return a diagnostic dump for a node.
fn host_diagnostic_dump<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let requested = args.opt_node_id(scope)?;
    let dump = with_current_canopy(|canopy, node_id| {
        let target = requested.unwrap_or(node_id);
        Ok(canopy.diagnostic_dump(target))
    })
    .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&dump)?)))
}

/// `canopy.help_snapshot`: return the current contextual help snapshot.
fn host_help_snapshot<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let snapshot = with_current_canopy(|canopy, _| Ok(help_snapshot_to_arg(canopy)))
        .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &snapshot)
}

/// `canopy.script_journal`: return recorded script evaluations.
fn host_script_journal<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let journal = with_current_canopy(|canopy, _| Ok(script_journal_to_arg(canopy)))
        .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &journal)
}

/// `canopy.api`: return the generated Luau API definition.
fn host_api<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let api = with_current_canopy(|canopy, _| Ok(canopy.script_api().to_string()))
        .map_err(|err| canopy_to_host(&err))?;
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
    with_current_canopy(|canopy, _| {
        let function_id = canopy.script_host.store_function(stashed, Some(label));
        canopy
            .script_host
            .state
            .borrow_mut()
            .on_start_hooks
            .push(function_id);
        Ok(())
    })
    .map_err(|err| canopy_to_host(&err))?;
    Ok(ret_none())
}

/// `fixtures`: list all registered fixtures.
fn host_fixtures<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let fixtures = with_current_canopy(|canopy, _| Ok(fixtures_to_arg(canopy)))
        .map_err(|err| canopy_to_host(&err))?;
    ret_arg(scope, &fixtures)
}

/// The base canopy native module: the `canopy` library plus global helpers.
struct CanopyBaseModule {
    /// Rendered base API declaration used by the surface audit.
    declaration: String,
}

impl CanopyBaseModule {
    /// Build a base module with declarations generated from the native table.
    fn new() -> Self {
        Self {
            declaration: defs::preamble(),
        }
    }
}

impl NativeModule for CanopyBaseModule {
    fn name(&self) -> &str {
        "canopy"
    }

    fn declaration(&self) -> oxau::decl::DeclSource<'_> {
        oxau::decl::DeclSource::Text(&self.declaration)
    }

    fn build(&self, builder: &mut dyn ModuleBuilder) {
        ModuleBuilderExt::host_type(builder, node_handle_type());
        base_api::install(builder);
    }
}

/// A per-owner native module exposing that owner's registered commands (and
/// optionally its `default_bindings` trigger) as a library table.
struct OwnerCommandsModule {
    /// Original command owner name.
    owner: String,
    /// Luau-safe global table name.
    global_name: String,
    /// Rendered `declare` block for the audit.
    declaration: String,
    /// Sorted command specs registered under this owner.
    specs: Vec<&'static CommandSpec>,
    /// Whether the owner registered default bindings.
    default_bindings: bool,
}

impl NativeModule for OwnerCommandsModule {
    fn name(&self) -> &str {
        &self.global_name
    }

    fn declaration(&self) -> oxau::decl::DeclSource<'_> {
        oxau::decl::DeclSource::Text(&self.declaration)
    }

    fn build(&self, builder: &mut dyn ModuleBuilder) {
        for spec in &self.specs {
            let spec: &'static CommandSpec = spec;
            builder.scoped_function(
                spec.name,
                ModuleBinding::library(self.global_name.clone()),
                canopy_host_fn(move |scope: &Scope<'_>, args: MultiValue<'_>| {
                    let values = values_to_args(scope, ArgReader::new(args).rest())?;
                    let allow_map_named = values.len() == 1;
                    let result = with_current_canopy(|_, node_id| {
                        dispatch_command(spec, node_id, values, allow_map_named)
                    })
                    .map_err(|err| canopy_to_host(&err))?;
                    ret_arg(scope, &result)
                }),
            );
        }
        if self.default_bindings {
            let owner = self.owner.clone();
            builder.scoped_function(
                "default_bindings",
                ModuleBinding::library(self.global_name.clone()),
                canopy_host_fn(move |_scope: &Scope<'_>, _args: MultiValue<'_>| {
                    with_current_canopy(|canopy, _| {
                        canopy.run_registered_default_bindings(&owner)?;
                        Ok(())
                    })
                    .map_err(|err| canopy_to_host(&err))?;
                    Ok(ret_none())
                }),
            );
        }
    }
}

/// Build the per-owner command modules for the surface.
fn build_owner_modules(
    commands: &CommandSet,
    default_binding_owners: &BTreeSet<String>,
) -> Vec<OwnerCommandsModule> {
    defs::owner_command_specs(commands, default_binding_owners)
        .into_iter()
        .map(|(owner, specs)| {
            let has_defaults = default_binding_owners.contains(&owner);
            OwnerCommandsModule {
                global_name: luau_global_owner_name(&owner),
                declaration: defs::render_owner_declaration(&owner, &specs, has_defaults),
                specs,
                owner,
                default_bindings: has_defaults,
            }
        })
        .collect()
}

/// A script callable resolvable inside a VM scope: a loaded module's main
/// closure, or a stored (stashed) Luau closure.
enum CallTarget {
    /// A compiled script's loaded module.
    Module(Rc<LoadedModule>),
    /// A stored Luau closure.
    Stored(StashedClosure),
}

impl CallTarget {
    /// Resolve the callable inside the given scope.
    fn resolve<'s>(&self, scope: &Scope<'s>) -> Result<Function<'s>> {
        match self {
            Self::Module(module) => Ok(scope.module_function(module)),
            Self::Stored(stashed) => scope.fetch_function(stashed).map_err(lua_to_canopy),
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

/// Load a compiled chunk into the retained VM with an isolated chunk
/// environment, matching the per-chunk global isolation scripts ran under
/// previously.
fn load_into_vm(
    vm: &mut Vm,
    chunk: &BytecodeChunk,
    module_name: impl AsRef<[u8]>,
) -> Result<Rc<LoadedModule>> {
    let module_name = module_name.as_ref();
    let module = if module_name.starts_with(b"@") {
        vm.load_module(chunk, ModuleId::new(module_name.to_vec()))
    } else {
        vm.load_named(chunk, module_name)
    }
    .map_err(|err| error::Error::Script(format!("loading script failed: {err}")))?;
    vm.bind_chunk_environment(&module)
        .map_err(|err| error::Error::Script(format!("binding script environment failed: {err}")))?;
    Ok(Rc::new(module))
}

impl LuauHost {
    /// Construct a new Luau host. The VM itself is built by `finalize()`, once
    /// the full command surface is known.
    pub fn new() -> Self {
        Self {
            vm: Rc::new(RefCell::new(None)),
            state: Rc::new(RefCell::new(LuauState::new())),
        }
    }

    /// Return true if the API has been finalized.
    pub fn is_finalized(&self) -> bool {
        self.state.borrow().finalized
    }

    /// Return the finalized script surface.
    pub(crate) fn surface(&self) -> Option<SurfaceSpec> {
        self.state.borrow().surface.clone()
    }

    /// Type-check a Luau source string against the finalized canopy API.
    pub fn check_script(&self, source: &str) -> Result<ScriptCheckResult> {
        let surface = self.state.borrow().surface.clone().ok_or_else(|| {
            error::Error::InvalidOperation(
                "cannot type-check scripts before finalize_api()".to_string(),
            )
        })?;
        self.check_script_with_surface(source, &surface)
    }

    /// Type-check Luau source against the audited surface.
    fn check_script_with_surface(
        &self,
        source: &str,
        surface: &SurfaceSpec,
    ) -> Result<ScriptCheckResult> {
        let mut checker = surface.new_checker();
        let checked = checker.check_source(&strict_source(source));
        let diagnostics = checked
            .diagnostics()
            .iter()
            .map(type_diagnostic_to_script)
            .collect();
        Ok(ScriptCheckResult { diagnostics })
    }

    /// Enforce Luau type checking for finalized APIs in debug builds.
    fn maybe_typecheck(&self, source: &str) -> Result<()> {
        if !cfg!(debug_assertions) || !self.is_finalized() {
            return Ok(());
        }
        let result = self.check_script(source)?;
        if result.is_ok() {
            Ok(())
        } else {
            Err(error::Error::Parse(error::ParseError::new(
                format_typecheck_diagnostics(&result),
            )))
        }
    }

    /// Clear recorded logs and assertions for the next script evaluation.
    fn clear_diagnostics(&self) {
        self.state.borrow_mut().diagnostics.clear();
    }

    /// Append a log line to the current evaluation state.
    fn push_log(&self, message: String) {
        self.state.borrow_mut().diagnostics.push_log(message);
    }

    /// Append an assertion result to the current evaluation state.
    fn push_assertion(&self, passed: bool, message: String) {
        self.state
            .borrow_mut()
            .diagnostics
            .push_assertion(passed, message);
    }

    /// Drain deferred `on_start` hooks in registration order.
    pub fn drain_on_start_hooks(&self) -> Vec<LuauFunctionId> {
        self.state.borrow_mut().drain_on_start_hooks()
    }

    /// Return true when deferred `on_start` hooks are pending.
    pub fn has_on_start_hooks(&self) -> bool {
        !self.state.borrow().on_start_hooks.is_empty()
    }

    /// Take the logs collected during the most recent evaluation.
    pub fn take_logs(&self) -> Vec<String> {
        self.state.borrow_mut().diagnostics.take_logs()
    }

    /// Return log lines collected during the most recent evaluation.
    pub fn logs(&self) -> Vec<String> {
        self.state.borrow().diagnostics.logs()
    }

    /// Take the assertions collected during the most recent evaluation.
    pub fn take_assertions(&self) -> Vec<ScriptAssertion> {
        self.state.borrow_mut().diagnostics.take_assertions()
    }

    /// Return assertions collected during the most recent evaluation.
    pub fn assertions(&self) -> Vec<ScriptAssertion> {
        self.state.borrow().diagnostics.assertions()
    }

    /// Return current (log, assertion) counts for journal baselines.
    pub(crate) fn diagnostics_counts(&self) -> (usize, usize) {
        let state = self.state.borrow();
        (
            state.diagnostics.logs.len(),
            state.diagnostics.assertions.len(),
        )
    }

    /// Finalize the command surface: audit and build the script surface, then
    /// construct the retained sandboxed VM and load any scripts compiled
    /// before finalization.
    pub fn finalize(
        &self,
        commands: &CommandSet,
        default_binding_owners: &BTreeSet<String>,
        extra_modules: &[Arc<dyn NativeModule>],
        module_source: Option<Arc<dyn ModuleSource>>,
        definitions: String,
    ) -> Result<()> {
        if self.is_finalized() {
            return Err(error::Error::InvalidOperation(
                "Luau API already finalized".into(),
            ));
        }
        let mut builder =
            SurfaceSpec::builder(canopy_profile()).module(Arc::new(CanopyBaseModule::new()));
        if let Some(source) = module_source {
            builder = builder.module_source(source);
        }
        for module in extra_modules {
            builder = builder.module(Arc::clone(module));
        }
        for module in build_owner_modules(commands, default_binding_owners) {
            builder = builder.module(Arc::new(module));
        }
        let surface = builder.build().map_err(|err| {
            error::Error::Script(format!("building script surface failed: {err}"))
        })?;
        let mut vm = surface
            .vm_builder(Ambient::production(0), default_vm_limits())
            .build_sandboxed()
            .map_err(|err| error::Error::Script(format!("building script VM failed: {err}")))?;

        {
            let mut state = self.state.borrow_mut();
            let ids = state.scripts.scripts.keys().copied().collect::<Vec<_>>();
            for id in ids {
                let chunk = state
                    .scripts
                    .chunk(id)
                    .expect("script id enumerated from the cache");
                let module_name = state
                    .scripts
                    .module_name(id)
                    .expect("script id enumerated from the cache");
                let module = load_into_vm(&mut vm, &chunk, module_name)?;
                state.scripts.set_module(id, module);
            }
        }

        *self.vm.borrow_mut() = Some(vm);
        self.state.borrow_mut().finalize(definitions, surface);
        Ok(())
    }

    /// Compile a script and return its id.
    pub fn compile(&self, source: &str) -> Result<ScriptId> {
        self.compile_named(source, b"canopy")
    }

    /// Compile a script with an explicit VM module name.
    pub fn compile_named(&self, source: &str, module_name: impl AsRef<[u8]>) -> Result<ScriptId> {
        self.maybe_typecheck(source)?;
        let chunk = compile_chunk(&strict_source(source))?;
        let sid = self
            .state
            .borrow_mut()
            .scripts
            .insert(chunk, source, module_name.as_ref());
        if self.is_finalized() {
            self.load_script(sid)?;
        }
        Ok(sid)
    }

    /// Load a compiled script into the retained VM.
    fn load_script(&self, sid: ScriptId) -> Result<Rc<LoadedModule>> {
        let chunk = self
            .state
            .borrow()
            .scripts
            .chunk(sid)
            .ok_or_else(|| error::Error::Script(format!("script {sid} not found")))?;
        let mut vm_cell = self.vm.try_borrow_mut().map_err(|_| {
            error::Error::Script("cannot load a script while the script VM is executing".into())
        })?;
        let vm = vm_cell.as_mut().ok_or_else(|| {
            error::Error::InvalidOperation("cannot load scripts before finalize_api()".to_string())
        })?;
        let module_name = self
            .state
            .borrow()
            .scripts
            .module_name(sid)
            .ok_or_else(|| error::Error::Script(format!("script {sid} not found")))?;
        let module = load_into_vm(vm, &chunk, module_name)?;
        self.state
            .borrow_mut()
            .scripts
            .set_module(sid, module.clone());
        Ok(module)
    }

    /// Return the loaded module for a script, loading it if necessary.
    fn loaded_module(&self, sid: ScriptId) -> Result<Rc<LoadedModule>> {
        if let Some(module) = self.state.borrow().scripts.module(sid) {
            return Ok(module);
        }
        if self.state.borrow().scripts.chunk(sid).is_none() {
            return Err(error::Error::Script(format!("script {sid} not found")));
        }
        self.load_script(sid)
    }

    /// Execute a compiled script.
    pub fn execute(
        &self,
        canopy: &mut Canopy,
        node_id: impl Into<NodeId>,
        sid: ScriptId,
    ) -> Result<()> {
        self.execute_value(canopy, node_id, sid).map(|_| ())
    }

    /// Execute a compiled script and return its value.
    pub fn execute_value(
        &self,
        canopy: &mut Canopy,
        node_id: impl Into<NodeId>,
        sid: ScriptId,
    ) -> Result<ArgValue> {
        self.execute_value_inner(canopy, node_id.into(), sid, None)
    }

    /// Execute a compiled script with a cooperative timeout.
    pub fn execute_value_with_timeout(
        &self,
        canopy: &mut Canopy,
        node_id: impl Into<NodeId>,
        sid: ScriptId,
        timeout: Duration,
    ) -> Result<ArgValue> {
        self.execute_value_inner(canopy, node_id.into(), sid, Some(timeout))
    }

    /// Execute a compiled script and return its value.
    fn execute_value_inner(
        &self,
        canopy: &mut Canopy,
        node_id: NodeId,
        sid: ScriptId,
        timeout: Option<Duration>,
    ) -> Result<ArgValue> {
        let module = self.loaded_module(sid)?;
        // Diagnostics accumulate per top-level evaluation: a nested run
        // triggered from inside a live script must not erase the logs and
        // assertions the outer evaluation has already collected.
        if !in_live_scope() {
            self.clear_diagnostics();
        }
        let label = format!("script {sid} on node {node_id:?}");
        self.run_target(
            canopy,
            node_id,
            &CallTarget::Module(module),
            &label,
            timeout,
        )
    }

    /// Run a script callable, re-entering the VM through the innermost live
    /// scope when one is active, or through a fresh limited scope step at the
    /// top level.
    fn run_target(
        &self,
        canopy: &mut Canopy,
        node_id: NodeId,
        target: &CallTarget,
        label: &str,
        timeout: Option<Duration>,
    ) -> Result<ArgValue> {
        if let Some(ptr) = current_scope_ptr() {
            // SAFETY: the scope pointer was pushed by a live host call (or scope
            // step) further down the Rust stack; it stays valid for the whole
            // nested run.
            let scope = unsafe { scope_from_ptr(ptr) };
            let _guard = ScriptContextGuard::push(canopy, node_id);
            let function = target.resolve(scope)?;
            return call_in_scope(scope, function, label, timeout);
        }

        let vm = self.vm.clone();
        let mut vm_cell = vm.try_borrow_mut().map_err(|_| {
            error::Error::Script("script VM re-entered without a live scope".into())
        })?;
        let vm = vm_cell.as_mut().ok_or_else(|| {
            error::Error::InvalidOperation(
                "cannot execute scripts before finalize_api()".to_string(),
            )
        })?;
        // A fresh print sink per invocation: `print` output lands in the
        // evaluation log alongside `canopy.log`, bounded by a per-run quota so
        // a print loop cannot grow the log without limit.
        vm.set_print_sink_with_quota(
            Box::new(|bytes: &[u8]| {
                let line = String::from_utf8_lossy(bytes).trim_end().to_string();
                tracing::info!("{line}");
                // Print can only fire while a script context is active; a
                // missing context (no canopy run in flight) drops the line to
                // the tracing log above.
                let _logged = with_current_canopy(|canopy, _| {
                    canopy.script_host.push_log(line.clone());
                    Ok(())
                });
            }),
            PRINT_QUOTA,
        );
        let mut outcome: Option<Result<ArgValue>> = None;
        let step = vm.step_with(
            oxau::session::CallOptions::new().limits(invocation_limits(timeout)),
            |scope| {
                let _guard = ScriptContextGuard::push_with_scope(canopy, node_id, scope);
                let result = match target.resolve(scope) {
                    Ok(function) => call_in_scope(scope, function, label, timeout),
                    Err(err) => Err(err),
                };
                outcome = Some(result);
                Ok(())
            },
        );
        match step {
            Ok(()) => outcome.unwrap_or_else(|| {
                Err(error::Error::Script(format!("{label} produced no result")))
            }),
            Err(error) => Err(runtime_error_to_canopy(&error, label, timeout)),
        }
    }

    /// Return the source for a cached script.
    pub fn script_source(&self, sid: ScriptId) -> Option<String> {
        self.state.borrow().scripts.source(sid)
    }

    /// Store a stashed Luau closure and return a stable host-side id.
    fn store_function(&self, stashed: StashedClosure, label: Option<String>) -> LuauFunctionId {
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
        let stashed = self
            .state
            .borrow()
            .closures
            .stashed(id)
            .ok_or_else(|| error::Error::Script(format!("Luau function {id:?} not found")))?;
        let label = format!("Luau binding on node {node_id:?}");
        self.run_target(canopy, node_id, &CallTarget::Stored(stashed), &label, None)
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;
    use crate::testing::ttree::{get_state, run_ttree};

    /// Execute a closure with the current script context.
    fn with_script_context<R>(
        canopy: &mut Canopy,
        node_id: NodeId,
        f: impl FnOnce() -> Result<R>,
    ) -> Result<R> {
        let _guard = ScriptContextGuard::push(canopy, node_id);
        f()
    }

    #[test]
    fn tcompile_error_reports_details() {
        let host = ScriptHost::new();
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
            host.execute(c, c.core.root_id(), scr)?;
            let logs = host.take_logs();
            assert!(
                logs.iter().any(|line| line.contains("plain print")),
                "print output should land in the evaluation log: {logs:?}"
            );
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
            host.execute(c, c.core.root_id(), scr)?;
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
            host.execute(c, c.core.root_id(), check)?;
            Ok(())
        })
    }

    #[test]
    fn texecute() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.finalize_api()?;
            let scr = c.script_host.compile(r#"bb_la.c_leaf()"#)?;
            let host = c.script_host.clone();
            host.execute(c, tree.b_a, scr)?;
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
            let err = host.execute(c, tree.b_a, scr);
            assert!(matches!(err, Err(error::Error::Script(_))));
            Ok(())
        })
    }

    #[test]
    fn script_context_restores_nested_contexts() -> Result<()> {
        run_ttree(|c, _, tree| {
            with_script_context(c, tree.a, || {
                with_current_canopy(|canopy, node| {
                    assert_eq!(node, tree.a);
                    with_script_context(canopy, tree.b, || {
                        let inner = with_current_canopy(|_, node| Ok(node))?;
                        assert_eq!(inner, tree.b);
                        Ok(())
                    })
                })?;

                let restored = with_current_canopy(|_, node| Ok(node))?;
                assert_eq!(restored, tree.a);
                Ok(())
            })?;

            let error = with_current_canopy(|_, _| Ok(())).unwrap_err();
            assert!(matches!(
                error,
                error::Error::Script(message) if message == "no active script context"
            ));
            Ok(())
        })
    }

    #[test]
    fn script_context_pops_after_panic() -> Result<()> {
        run_ttree(|c, _, tree| {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let _ignored: Result<()> = with_script_context(c, tree.a, || -> Result<()> {
                    panic!("script callback panic");
                });
            }));

            assert!(result.is_err());
            let error = with_current_canopy(|_, _| Ok(())).unwrap_err();
            assert!(matches!(
                error,
                error::Error::Script(message) if message == "no active script context"
            ));
            Ok(())
        })
    }

    #[test]
    fn tcheck_script_reports_type_errors() -> Result<()> {
        run_ttree(|c, _, _| {
            c.finalize_api()?;
            let result = c.script_host.check_script("local value: string = 1")?;
            assert!(!result.is_ok());
            assert!(result.has_errors());
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
