use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use canopy::{
    Canopy, FixtureInfo,
    commands::{ArgValue, CommandDispatchKind, CommandResolution},
    error::Error as CanopyError,
    geom::Size,
    testing::render::NopBackend,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tmcp::{TOOL_ERROR_INTERNAL, schema::CallToolResult, tool_params};

use crate::Result;

/// Shared application factory used by the automation helpers.
pub type AppFactory = Arc<dyn Fn() -> Result<Canopy> + Send + Sync>;

/// Convert a closure into a shared app factory.
pub fn app_factory<F>(factory: F) -> AppFactory
where
    F: Fn() -> Result<Canopy> + Send + Sync + 'static,
{
    Arc::new(factory)
}

/// Default headless viewport used by the automation helpers.
const DEFAULT_VIEW_SIZE: Size = Size { w: 120, h: 40 };

/// Short operating guide returned by the bootstrap tool.
const BOOTSTRAP_GUIDE: &str = "Use script_eval for actions and assertions. Scripts run against \
the generated Luau API, can call canopy.help_snapshot(), canopy.commands(), canopy.screen_text(), \
canopy.screen_cells(), canopy.route_trace(), and canopy.script_journal(), and should prefer typed \
command calls over coordinate input when possible.";

/// Request payload for the `script_eval` tool.
#[derive(Debug, Clone, PartialEq)]
#[tool_params]
pub struct ScriptEvalRequest {
    /// Luau source code to execute.
    pub script: String,
    /// Optional named fixture applied before evaluation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixture: Option<String>,
    /// Optional evaluation timeout in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Structured typecheck diagnostic returned by `script_eval`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptDiagnostic {
    /// Diagnostic severity such as `error` or `warning`.
    pub severity: String,
    /// One-based line number, or zero when the diagnostic is not source-bound.
    pub line: usize,
    /// One-based column number, or zero when the diagnostic is not source-bound.
    pub column: usize,
    /// Human-readable diagnostic message.
    pub message: String,
}

/// Assertion outcome recorded during script execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptAssertion {
    /// Whether the assertion passed.
    pub passed: bool,
    /// Assertion message emitted by the runtime.
    pub message: String,
}

/// Timing information for a script evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptTiming {
    /// Time spent constructing and rendering the headless app.
    pub build_ms: u64,
    /// Time spent executing the script and final render.
    pub exec_ms: u64,
    /// Total wall-clock time for the request.
    pub total_ms: u64,
}

/// Evaluation task state exposed to automation callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScriptTaskState {
    /// Evaluation completed successfully.
    Completed,
    /// Evaluation failed before completion.
    Failed,
    /// Evaluation stopped at the cooperative timeout boundary.
    TimedOut,
}

impl ScriptTiming {
    /// Zeroed timing information for early errors.
    pub fn zero() -> Self {
        Self {
            build_ms: 0,
            exec_ms: 0,
            total_ms: 0,
        }
    }
}

/// Error details included in a failed script evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptErrorInfo {
    #[serde(rename = "type")]
    /// Pipeline stage that failed: `build`, `typecheck`, `timeout`, or `runtime`.
    pub error_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Stable host error category such as `no_target` or `unknown_command`,
    /// when the failure carried structured fields.
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Command id when the error came from command dispatch.
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Owner name when the error came from node-target resolution.
    pub owner: Option<String>,
    /// Human-readable error message.
    pub message: String,
}

/// Structured response for the `script_eval` tool and smoke runner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptEvalOutcome {
    /// Whether the script completed successfully.
    pub success: bool,
    /// Final task state for the evaluation.
    pub state: ScriptTaskState,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Optional JSON-serializable script return value.
    pub value: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Log lines emitted during evaluation.
    pub logs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Assertion outcomes recorded during evaluation.
    pub assertions: Vec<ScriptAssertion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Typecheck diagnostics captured before execution.
    pub diagnostics: Vec<ScriptDiagnostic>,
    /// Timing information for the request.
    pub timing: ScriptTiming,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Error payload when evaluation fails.
    pub error: Option<ScriptErrorInfo>,
}

/// Compact command availability record returned by bootstrap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BootstrapCommand {
    /// Command name relative to its owner.
    pub name: String,
    /// Widget owner name, or empty for free commands.
    pub owner: String,
    /// Whether the command currently resolves.
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Debug token for the current target node, when available.
    pub target: Option<String>,
}

/// Compact script journal record returned by bootstrap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BootstrapJournalEntry {
    /// Monotonic journal id.
    pub id: u64,
    /// Script origin.
    pub origin: String,
    /// Whether the evaluation completed successfully.
    pub ok: bool,
    /// Number of logs emitted by this evaluation.
    pub log_count: usize,
    /// Number of assertions emitted by this evaluation.
    pub assertion_count: usize,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

/// Bootstrap payload for an agent entering a Canopy app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BootstrapResponse {
    /// Operating guide for the automation surface.
    pub guide: String,
    /// Full generated Luau API definition.
    pub api: String,
    /// Stable FNV-1a digest of `api`.
    pub api_digest: String,
    /// Registered fixtures.
    pub fixtures: Vec<FixtureInfo>,
    /// Current command availability.
    pub commands: Vec<BootstrapCommand>,
    /// Recent script journal entries.
    pub journal: Vec<BootstrapJournalEntry>,
}

impl ScriptEvalOutcome {
    /// Encode the outcome as an MCP tool result.
    pub fn to_tool_result(&self) -> CallToolResult {
        match serde_json::to_value(self) {
            Ok(value) => {
                let mut result = CallToolResult::new()
                    .with_structured_content(value.clone())
                    .with_text_content(value.to_string());
                if !self.success {
                    result = result.with_is_error(true);
                }
                result
            }
            Err(error) => CallToolResult::error(
                TOOL_ERROR_INTERNAL,
                format!("failed to serialize script result: {error}"),
            ),
        }
    }

    /// Build a failure payload with no result value.
    pub fn error_only(
        error_type: impl Into<String>,
        message: impl Into<String>,
        diagnostics: Vec<ScriptDiagnostic>,
        timing: ScriptTiming,
    ) -> Self {
        let error_type = error_type.into();
        Self {
            success: false,
            state: script_task_state(&error_type),
            value: None,
            logs: Vec::new(),
            assertions: Vec::new(),
            diagnostics,
            timing,
            error: Some(ScriptErrorInfo {
                error_type,
                kind: None,
                command: None,
                owner: None,
                message: message.into(),
            }),
        }
    }
}

/// Headless evaluator that creates a fresh canopy app instance for each request.
#[derive(Clone)]
pub struct AppEvaluator {
    /// Factory that builds a fresh canopy app for each request.
    factory: AppFactory,
    /// Headless viewport used during rendering and event simulation.
    view_size: Size,
}

impl AppEvaluator {
    /// Construct an evaluator with a default headless viewport size.
    pub fn new(factory: AppFactory) -> Self {
        Self {
            factory,
            view_size: DEFAULT_VIEW_SIZE,
        }
    }

    /// Override the headless viewport size used for evaluations.
    pub fn with_view_size(mut self, width: u32, height: u32) -> Self {
        self.view_size = Size::new(width, height);
        self
    }

    /// Render and return the app's Luau API definition.
    pub fn script_api(&self) -> Result<String> {
        let mut canopy = (self.factory)()?;
        canopy.finalize_api()?;
        Ok(canopy.script_api().to_string())
    }

    /// Return the evaluator's registered fixture catalog.
    pub fn fixtures(&self) -> Result<Vec<FixtureInfo>> {
        let canopy = (self.factory)()?;
        Ok(canopy.fixture_infos())
    }

    /// Return bootstrap information for a fresh headless app instance.
    pub fn bootstrap(&self) -> Result<BootstrapResponse> {
        let mut session = HeadlessSession::new(&self.factory, self.view_size, None)?;
        Ok(bootstrap_for_canopy(&mut session.canopy))
    }

    /// Evaluate a Luau script against a fresh headless app.
    pub fn evaluate(&self, request: &ScriptEvalRequest) -> ScriptEvalOutcome {
        let total_start = Instant::now();
        let build_start = Instant::now();
        let mut session =
            match HeadlessSession::new(&self.factory, self.view_size, request.fixture.as_deref()) {
                Ok(session) => session,
                Err(error) => {
                    return ScriptEvalOutcome::error_only(
                        "build",
                        error.to_string(),
                        Vec::new(),
                        ScriptTiming::zero(),
                    );
                }
            };
        let build_ms = build_start.elapsed().as_millis() as u64;

        let diagnostics = match typecheck_for_eval(
            &mut session.canopy,
            &request.script,
            ScriptTiming {
                build_ms,
                exec_ms: 0,
                total_ms: total_start.elapsed().as_millis() as u64,
            },
        ) {
            TypecheckGate::Ready(diagnostics) => diagnostics,
            TypecheckGate::Failed(outcome) => return *outcome,
        };

        let exec_start = Instant::now();
        let eval_result = session.evaluate(&request.script, request.timeout_ms);
        let exec_ms = exec_start.elapsed().as_millis() as u64;
        let timing = ScriptTiming {
            build_ms,
            exec_ms,
            total_ms: total_start.elapsed().as_millis() as u64,
        };
        let logs = session.take_logs();
        let assertions = session.take_assertions();

        match eval_result {
            Ok(value) => ScriptEvalOutcome {
                success: true,
                state: ScriptTaskState::Completed,
                value: Some(value),
                logs,
                assertions,
                diagnostics,
                timing,
                error: None,
            },
            Err(error) => failure_with_logs(&error, logs, assertions, diagnostics, timing),
        }
    }
}

/// Build a bootstrap payload from a finalized app.
pub fn bootstrap_for_canopy(canopy: &mut Canopy) -> BootstrapResponse {
    let api = canopy.script_api().to_string();
    BootstrapResponse {
        guide: BOOTSTRAP_GUIDE.to_string(),
        api_digest: stable_digest(&api),
        api,
        fixtures: canopy.fixture_infos(),
        commands: bootstrap_commands(canopy),
        journal: bootstrap_journal(canopy),
    }
}

/// Return command availability records.
fn bootstrap_commands(canopy: &Canopy) -> Vec<BootstrapCommand> {
    canopy
        .command_availability_from_focus()
        .into_iter()
        .map(|availability| {
            let owner = match availability.spec.dispatch {
                CommandDispatchKind::Node { owner } => owner,
                CommandDispatchKind::Free => "",
            };
            BootstrapCommand {
                name: availability.spec.name.to_string(),
                owner: owner.to_string(),
                available: availability.resolution.is_some(),
                target: availability
                    .resolution
                    .and_then(CommandResolution::target)
                    .map(|target| format!("{target:?}")),
            }
        })
        .collect()
}

/// Return compact journal records.
fn bootstrap_journal(canopy: &Canopy) -> Vec<BootstrapJournalEntry> {
    canopy
        .script_journal()
        .iter()
        .rev()
        .take(20)
        .rev()
        .map(|entry| BootstrapJournalEntry {
            id: entry.id,
            origin: entry.origin.clone(),
            ok: entry.ok,
            log_count: entry.logs.len(),
            assertion_count: entry.assertions.len(),
            duration_ms: entry.duration_ms,
        })
        .collect()
}

/// Stable FNV-1a digest for short API identity tokens.
fn stable_digest(text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Evaluate a Luau script against an existing live canopy app.
pub fn evaluate_live(canopy: &mut Canopy, request: &ScriptEvalRequest) -> ScriptEvalOutcome {
    if request.fixture.is_some() {
        return ScriptEvalOutcome::error_only(
            "invalid",
            "live sessions do not support eval(fixture=...); use apply_fixture instead",
            Vec::new(),
            ScriptTiming::zero(),
        );
    }

    let total_start = Instant::now();
    let diagnostics = match typecheck_for_eval(
        canopy,
        &request.script,
        ScriptTiming {
            build_ms: 0,
            exec_ms: 0,
            total_ms: total_start.elapsed().as_millis() as u64,
        },
    ) {
        TypecheckGate::Ready(diagnostics) => diagnostics,
        TypecheckGate::Failed(outcome) => return *outcome,
    };

    let exec_start = Instant::now();
    let eval_result = eval_script_value(canopy, &request.script, request.timeout_ms);
    let exec_ms = exec_start.elapsed().as_millis() as u64;
    let timing = ScriptTiming {
        build_ms: 0,
        exec_ms,
        total_ms: total_start.elapsed().as_millis() as u64,
    };
    let logs = canopy.take_script_logs();
    let assertions = script_assertions(canopy);

    match eval_result {
        Ok(value) => match value.to_external_json_value() {
            Ok(value) => ScriptEvalOutcome {
                success: true,
                state: ScriptTaskState::Completed,
                value: Some(value),
                logs,
                assertions,
                diagnostics,
                timing,
                error: None,
            },
            Err(error) => ScriptEvalOutcome {
                success: false,
                state: ScriptTaskState::Failed,
                value: None,
                logs,
                assertions,
                diagnostics,
                timing,
                error: Some(ScriptErrorInfo {
                    error_type: "runtime".to_string(),
                    kind: None,
                    command: None,
                    owner: None,
                    message: error.to_string(),
                }),
            },
        },
        Err(error) => failure_with_logs(&error, logs, assertions, diagnostics, timing),
    }
}

/// Headless canopy session used while evaluating one script request.
struct HeadlessSession {
    /// The app instance under test.
    canopy: Canopy,
    /// No-op renderer used to drive layout and event dispatch.
    backend: NopBackend,
}

impl HeadlessSession {
    /// Build and render a fresh headless canopy session.
    fn new(factory: &AppFactory, view_size: Size, fixture: Option<&str>) -> Result<Self> {
        let mut canopy = factory()?;
        canopy.finalize_api()?;
        if let Some(fixture) = fixture {
            canopy.apply_fixture(fixture)?;
        }
        canopy.set_root_size(view_size)?;
        let mut backend = NopBackend::new();
        canopy.render(&mut backend)?;
        Ok(Self { canopy, backend })
    }

    /// Execute a script and return its JSON-serializable result value.
    fn evaluate(&mut self, script: &str, timeout_ms: Option<u64>) -> Result<JsonValue> {
        let value = eval_script_value(&mut self.canopy, script, timeout_ms)?;
        self.canopy.render(&mut self.backend)?;
        Ok(value.to_external_json_value()?)
    }

    /// Drain the script log buffer.
    fn take_logs(&self) -> Vec<String> {
        self.canopy.take_script_logs()
    }

    /// Drain recorded assertion results.
    fn take_assertions(&self) -> Vec<ScriptAssertion> {
        script_assertions(&self.canopy)
    }
}

/// Evaluate a script with an optional cooperative timeout.
fn eval_script_value(
    canopy: &mut Canopy,
    script: &str,
    timeout_ms: Option<u64>,
) -> Result<ArgValue> {
    if let Some(timeout_ms) = timeout_ms.filter(|timeout| *timeout > 0) {
        canopy
            .eval_script_value_with_timeout(script, Duration::from_millis(timeout_ms))
            .map_err(Into::into)
    } else {
        canopy.eval_script_value(script).map_err(Into::into)
    }
}

/// Result of the shared typecheck gate used by headless and live evaluation.
enum TypecheckGate {
    /// Typechecking succeeded and evaluation may continue.
    Ready(Vec<ScriptDiagnostic>),
    /// Typechecking failed and evaluation should stop.
    Failed(Box<ScriptEvalOutcome>),
}

/// Run Luau typechecking and return a failure outcome when evaluation should stop.
fn typecheck_for_eval(canopy: &mut Canopy, script: &str, timing: ScriptTiming) -> TypecheckGate {
    let diagnostics = match typecheck_diagnostics(canopy, script) {
        Ok(diagnostics) => diagnostics,
        Err(error) => {
            return TypecheckGate::Failed(Box::new(ScriptEvalOutcome::error_only(
                "typecheck",
                error.to_string(),
                Vec::new(),
                timing,
            )));
        }
    };
    if diagnostics_have_errors(&diagnostics) {
        return TypecheckGate::Failed(Box::new(ScriptEvalOutcome::error_only(
            "typecheck",
            "script failed Luau type checking",
            diagnostics,
            timing,
        )));
    }
    TypecheckGate::Ready(diagnostics)
}

/// Collect assertion outcomes from the active canopy session.
fn script_assertions(canopy: &Canopy) -> Vec<ScriptAssertion> {
    canopy
        .take_script_assertions()
        .into_iter()
        .map(|assertion| ScriptAssertion {
            passed: assertion.passed,
            message: assertion.message,
        })
        .collect()
}

/// Map an error category to the corresponding task state.
fn script_task_state(error_type: &str) -> ScriptTaskState {
    if error_type == "timeout" {
        ScriptTaskState::TimedOut
    } else {
        ScriptTaskState::Failed
    }
}

/// Return true if typecheck diagnostics should fail evaluation.
fn diagnostics_have_errors(diagnostics: &[ScriptDiagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == "error")
}

/// Return the evaluation error category for a runtime error.
fn evaluation_error_type(error: &crate::Error) -> &'static str {
    if is_script_timeout(error) {
        "timeout"
    } else {
        "runtime"
    }
}

/// Return true when a canopy script error represents cooperative timeout.
fn is_script_timeout(error: &crate::Error) -> bool {
    matches!(
        error,
        crate::Error::Canopy(CanopyError::ScriptTimeout { .. })
    )
}

/// Build a failed outcome while preserving logs, assertions, and diagnostics.
fn failure_with_logs(
    error: &crate::Error,
    logs: Vec<String>,
    assertions: Vec<ScriptAssertion>,
    diagnostics: Vec<ScriptDiagnostic>,
    timing: ScriptTiming,
) -> ScriptEvalOutcome {
    let info = script_error_info(error);
    let error_type = info.error_type.clone();
    let state = script_task_state(&error_type);
    ScriptEvalOutcome {
        success: false,
        state,
        value: None,
        logs,
        assertions,
        diagnostics,
        timing,
        error: Some(info),
    }
}

/// Build structured script error information from a canopy or automation error.
fn script_error_info(error: &crate::Error) -> ScriptErrorInfo {
    if let crate::Error::Canopy(CanopyError::ScriptStructured {
        kind,
        command,
        owner,
        message,
    }) = error
    {
        // `error_type` stays on the pipeline-stage axis; the host category
        // travels in `kind`.
        let error_type = if kind == "timeout" {
            "timeout"
        } else {
            "runtime"
        };
        return ScriptErrorInfo {
            error_type: error_type.to_string(),
            kind: Some(kind.clone()),
            command: command.clone(),
            owner: owner.clone(),
            message: message.clone(),
        };
    }
    ScriptErrorInfo {
        error_type: evaluation_error_type(error).to_string(),
        kind: None,
        command: None,
        owner: None,
        message: error.to_string(),
    }
}

/// Return Luau typecheck diagnostics for a script.
fn typecheck_diagnostics(canopy: &mut Canopy, script: &str) -> Result<Vec<ScriptDiagnostic>> {
    let result = canopy.check_script(script)?;
    Ok(result
        .diagnostics()
        .iter()
        .map(|diagnostic| ScriptDiagnostic {
            severity: diagnostic.severity.clone(),
            line: diagnostic.line,
            column: diagnostic.column,
            message: diagnostic.message.clone(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use canopy::{
        Fixture, ReadContext, command, commands::FocusDirection, derive_commands,
        error::Result as CanopyResult, prelude::*,
    };

    use super::*;

    struct ScriptTarget {
        value: i32,
    }

    #[derive_commands]
    impl ScriptTarget {
        fn new() -> Self {
            Self { value: 0 }
        }

        #[command]
        fn set(&mut self, value: i32) {
            self.value = value;
        }

        #[command]
        fn get(&self) -> i32 {
            self.value
        }

        #[command]
        fn choose(&mut self, direction: FocusDirection, count: Option<i32>) -> i32 {
            let direction_value = match direction {
                FocusDirection::Next => 1,
                FocusDirection::Prev => 2,
                FocusDirection::Up => 3,
                FocusDirection::Down => 4,
                FocusDirection::Left => 5,
                FocusDirection::Right => 6,
            };
            self.value = direction_value + count.unwrap_or_default();
            self.value
        }
    }

    impl Widget for ScriptTarget {
        fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ReadContext) -> CanopyResult<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("script_target")
        }
    }

    impl Loader for ScriptTarget {
        fn load(cnpy: &mut Canopy) -> CanopyResult<()> {
            cnpy.add_commands::<Self>()
        }
    }

    fn test_factory() -> AppFactory {
        app_factory(|| {
            let mut canopy = Canopy::new();
            ScriptTarget::load(&mut canopy)?;
            canopy.register_default_bindings("script_target", r#"canopy.log("defaults")"#)?;
            canopy.register_fixture(Fixture::new(
                "seeded",
                "Set script_target to a known value",
                |canopy| canopy.eval_script("script_target.set(31)"),
            ))?;
            canopy.finalize_api()?;
            let root_id = canopy.root_id();
            canopy
                .core_mut()
                .replace_subtree(root_id, ScriptTarget::new())?;
            Ok(canopy)
        })
    }

    #[test]
    fn script_api_lists_commands() -> crate::Result<()> {
        let evaluator = AppEvaluator::new(test_factory());
        let api = evaluator.script_api()?;
        assert!(api.contains("declare script_target"));
        assert!(api.contains("set: (value: number) -> ()"));
        assert!(api.contains("-- seeded: Set script_target to a known value"));
        assert!(api.contains("default_bindings: () -> ()"));
        assert!(api.contains("api: () -> string"));
        assert!(
            api.contains("choose: (direction: FocusDirection, count: number?) -> number"),
            "{api}"
        );
        Ok(())
    }

    #[test]
    fn script_api_generated_tail_matches_snapshot() -> crate::Result<()> {
        let evaluator = AppEvaluator::new(test_factory());
        let api = evaluator.script_api()?;
        let marker = "-- ===== Fixtures =====";
        let (_, tail) = api
            .split_once(marker)
            .expect("script API should contain generated fixture section");
        let actual = format!("{marker}{tail}");
        let expected = r#"-- ===== Fixtures =====
-- seeded: Set script_target to a known value

-- ===== Application Commands =====

export type FocusDirection = "Next" | "Prev" | "Up" | "Down" | "Left" | "Right"

-- ===== Commands for widget "script_target" =====

declare script_target: {
    choose: (direction: FocusDirection, count: number?) -> number,
    get: () -> number,
    set: (value: number) -> (),
    --- Register this widget's default bindings.
    default_bindings: () -> (),
}"#;

        assert_eq!(actual.trim_end(), expected);
        Ok(())
    }

    #[test]
    fn evaluate_returns_value_and_logs() {
        let evaluator = AppEvaluator::new(test_factory());
        let outcome = evaluator.evaluate(&ScriptEvalRequest {
            script: r#"
                canopy.log("hello")
                script_target.set(7)
                return script_target.get()
            "#
            .to_string(),
            fixture: None,
            timeout_ms: None,
        });
        assert!(outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Completed);
        assert_eq!(outcome.logs, vec!["hello"]);
        assert_eq!(outcome.value, Some(JsonValue::from(7)));
    }

    #[test]
    fn evaluate_returns_node_handles_as_external_tokens() {
        let evaluator = AppEvaluator::new(test_factory());
        let outcome = evaluator.evaluate(&ScriptEvalRequest {
            script: "local root = canopy.root()\nprint(root)\nreturn root".to_string(),
            fixture: None,
            timeout_ms: None,
        });

        assert!(outcome.success);
        let value = outcome.value.expect("node token");
        assert_eq!(value["type"], JsonValue::String("NodeId".to_string()));
        assert!(value["token"].is_string());
        assert_eq!(outcome.logs, vec![value["token"].as_str().unwrap()]);
    }

    #[test]
    fn evaluate_applies_fixtures_and_named_optional_args() {
        let evaluator = AppEvaluator::new(test_factory());
        let outcome = evaluator.evaluate(&ScriptEvalRequest {
            script: r#"
                canopy.assert(script_target.get() == 31, "fixture should run before eval")
                return canopy.cmd("script_target::choose", { direction = "Right" })
            "#
            .to_string(),
            fixture: Some("seeded".to_string()),
            timeout_ms: None,
        });

        assert!(outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Completed);
        assert_eq!(outcome.value, Some(JsonValue::from(6)));
    }

    #[test]
    fn evaluate_reports_cooperative_timeout() {
        let evaluator = AppEvaluator::new(test_factory());
        let outcome = evaluator.evaluate(&ScriptEvalRequest {
            script: "while true do end".to_string(),
            fixture: None,
            timeout_ms: Some(1),
        });

        assert!(!outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::TimedOut);
        assert_eq!(
            outcome
                .error
                .as_ref()
                .map(|error| error.error_type.as_str()),
            Some("timeout")
        );
    }

    #[test]
    fn evaluate_reports_typecheck_errors() {
        let evaluator = AppEvaluator::new(test_factory());
        let outcome = evaluator.evaluate(&ScriptEvalRequest {
            script: r#"script_target.set("bad")"#.to_string(),
            fixture: None,
            timeout_ms: None,
        });
        assert!(!outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Failed);
        assert_eq!(
            outcome
                .error
                .as_ref()
                .map(|error| error.error_type.as_str()),
            Some("typecheck")
        );
        assert!(!outcome.diagnostics.is_empty());
    }

    #[test]
    fn evaluate_reports_structured_command_errors() {
        let evaluator = AppEvaluator::new(test_factory());
        let outcome = evaluator.evaluate(&ScriptEvalRequest {
            script: r#"canopy.cmd("missing::command")"#.to_string(),
            fixture: None,
            timeout_ms: None,
        });

        assert!(!outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Failed);
        let error = outcome.error.as_ref().expect("structured error");
        assert_eq!(error.error_type, "runtime");
        assert_eq!(error.kind.as_deref(), Some("unknown_command"));
        assert_eq!(error.command.as_deref(), Some("missing::command"));
        assert_eq!(error.owner, None);
    }

    #[test]
    fn evaluate_live_reports_json_conversion_errors() -> crate::Result<()> {
        let mut canopy = (test_factory().as_ref())()?;
        let outcome = evaluate_live(
            &mut canopy,
            &ScriptEvalRequest {
                script: "return function() end".to_string(),
                fixture: None,
                timeout_ms: None,
            },
        );

        assert!(!outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Failed);
        assert_eq!(outcome.value, None);
        assert_eq!(
            outcome
                .error
                .as_ref()
                .map(|error| error.error_type.as_str()),
            Some("runtime")
        );
        Ok(())
    }

    #[test]
    fn evaluate_live_rejects_fixture_parameter() -> crate::Result<()> {
        let mut canopy = (test_factory().as_ref())()?;
        let outcome = evaluate_live(
            &mut canopy,
            &ScriptEvalRequest {
                script: "return script_target.get()".to_string(),
                fixture: Some("seeded".to_string()),
                timeout_ms: None,
            },
        );

        assert!(!outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Failed);
        assert_eq!(
            outcome
                .error
                .as_ref()
                .map(|error| error.error_type.as_str()),
            Some("invalid")
        );
        Ok(())
    }

    #[test]
    fn evaluate_live_observes_applied_fixture() -> crate::Result<()> {
        let mut canopy = (test_factory().as_ref())()?;
        canopy.apply_fixture("seeded")?;
        let outcome = evaluate_live(
            &mut canopy,
            &ScriptEvalRequest {
                script: "return script_target.get()".to_string(),
                fixture: None,
                timeout_ms: None,
            },
        );

        assert!(outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Completed);
        assert_eq!(outcome.value, Some(JsonValue::from(31)));
        Ok(())
    }
}
