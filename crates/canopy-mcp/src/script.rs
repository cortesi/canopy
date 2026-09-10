use std::{
    result,
    time::{Duration, Instant},
};

use canopy::{
    AutomationHandle, Canopy, EvalRequest, FixtureInfo, ScriptOrigin,
    commands::{ArgValue, CommandStatus, CommandTarget},
    error::{Error as CanopyError, Result as CanopyResult, ScriptErrorKind},
    render::NopBackend,
    script::{ScriptAssertion, ScriptCheckDiagnostic},
};
use ruau_script_api::{
    ScriptApiAvailability, ScriptApiCatalog, ScriptApiEntry, ScriptApiError, ScriptApiGuide,
    ScriptApiLoad, ScriptApiQuery, ScriptApiResolution, ScriptApiResponse, ScriptApiSource,
    ScriptApiTask,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tmcp::{TOOL_ERROR_INTERNAL, schema::CallToolResult, tool_params};
use tokio::task::spawn_blocking;

#[cfg(test)]
use crate::metadata::test_app_factory as app_factory;
use crate::{AppFactory, ExecutionMetadata, ResetPolicy, Result, Viewport, metadata::LiveContext};

/// Short operating guide returned by the bootstrap tool.
const BOOTSTRAP_GUIDE: &str = "Use script_eval for actions and assertions. Scripts run against \
the generated Luau API, can call canopy.available_bindings(), canopy.commands(), canopy.screen_text(), \
canopy.screen_cells(), canopy.route_trace(), and canopy.script_journal(), and should prefer typed \
command calls over coordinate input when possible. Top-level scripts start at root; \
use canopy.call_focus for focus-relative actions and canopy.call_exact for a stable node target.";

/// Request payload for the `script_eval` tool.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
    /// Optional headless dimensions; live requests may only confirm current
    /// dimensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
}

impl ScriptEvalRequest {
    /// Construct a request with default fixture, timeout, and viewport options.
    pub fn new(script: impl Into<String>) -> Self {
        Self {
            script: script.into(),
            fixture: None,
            timeout_ms: None,
            viewport: None,
        }
    }
}

/// Optional dimensions for bootstrap replay preflight.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[tool_params]
pub struct BootstrapRequest {
    /// Requested headless viewport, or a live viewport compatibility check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
}

/// Timing information for a script evaluation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
    /// Evaluation was explicitly cancelled.
    Cancelled,
}

/// Pipeline stage that caused a script evaluation to fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScriptErrorType {
    /// Application construction or initial rendering failed.
    Build,
    /// Luau typechecking failed.
    Typecheck,
    /// Evaluation exceeded its cooperative timeout.
    Timeout,
    /// Evaluation failed while running.
    Runtime,
    /// Evaluation was explicitly cancelled.
    Cancelled,
    /// Request metadata or options were invalid.
    Invalid,
}

impl From<ScriptErrorType> for ScriptTaskState {
    fn from(error_type: ScriptErrorType) -> Self {
        match error_type {
            ScriptErrorType::Timeout => Self::TimedOut,
            ScriptErrorType::Cancelled => Self::Cancelled,
            _ => Self::Failed,
        }
    }
}

/// Error details included in a failed script evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptErrorInfo {
    #[serde(rename = "type")]
    /// Pipeline stage that failed.
    pub error_type: ScriptErrorType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Stable host error category such as `no_target` or `unknown_command`,
    /// when the failure carried structured fields.
    pub kind: Option<ScriptErrorKind>,
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
    /// Execution identity and domain reset behavior for this request.
    pub metadata: ExecutionMetadata,
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
    pub diagnostics: Vec<ScriptCheckDiagnostic>,
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
    /// Eligibility, when an owner resolves.
    pub status: Option<String>,
    /// User-facing reason for disabled eligibility.
    pub disabled_reason: Option<String>,
    /// Missing originating event or row context, separate from eligibility.
    pub missing_requirements: Vec<String>,
}

/// Compact script journal record returned by bootstrap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BootstrapJournalEntry {
    /// Monotonic journal id.
    pub id: u64,
    /// Script origin.
    #[schemars(with = "String")]
    pub origin: ScriptOrigin,
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
    /// Execution identity and domain reset behavior for this bootstrap
    /// instance.
    pub metadata: ExecutionMetadata,
    /// Operating guide for the automation surface.
    pub guide: String,
    /// Compact discovery inventory for the generated API.
    pub api_sources: Vec<ScriptApiEntry>,
    /// Registered fixtures.
    pub fixtures: Vec<FixtureInfo>,
    /// Default top-level script target policy.
    pub default_target: String,
    /// Availability from the root used by top-level script evaluation.
    pub default_commands: Vec<BootstrapCommand>,
    /// Availability from the currently focused node.
    pub focus_commands: Vec<BootstrapCommand>,
    /// Recent script journal entries.
    pub journal: Vec<BootstrapJournalEntry>,
}

impl ScriptEvalOutcome {
    /// Attach boundary-owned metadata to any success or failure outcome.
    fn with_metadata(mut self, metadata: ExecutionMetadata) -> Self {
        self.metadata = metadata;
        self
    }
    /// Encode the outcome as an MCP tool result.
    pub fn to_tool_result(&self) -> CallToolResult {
        match serde_json::to_value(self) {
            Ok(value) => {
                let text = value.to_string();
                let mut result = CallToolResult::new()
                    .with_structured_content(value)
                    .with_text_content(text);
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
    pub(crate) fn error_only(
        error_type: ScriptErrorType,
        message: impl Into<String>,
        diagnostics: Vec<ScriptCheckDiagnostic>,
        timing: ScriptTiming,
    ) -> Self {
        Self {
            metadata: ExecutionMetadata::default(),
            success: false,
            state: error_type.into(),
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

impl AppFactory {
    /// Render and return the app's Luau API definition.
    pub fn script_api(&self) -> Result<String> {
        let canopy = self.build()?;
        Ok(canopy.script_api()?.to_string())
    }

    /// Return the evaluator's registered fixture catalog.
    pub fn fixtures(&self) -> Result<Vec<FixtureInfo>> {
        let canopy = self.build()?;
        Ok(canopy.fixture_infos())
    }

    /// Bootstrap at requested dimensions, validated before factory invocation.
    pub fn bootstrap(&self, request: &BootstrapRequest) -> Result<BootstrapResponse> {
        let viewport = request.viewport.unwrap_or_default();
        viewport.validate()?;
        let mut metadata = ExecutionMetadata::fresh(self.metadata(), viewport);
        let canopy = build_headless(self, viewport, None, &mut metadata)?;
        Ok(bootstrap_for_canopy(&canopy, metadata)?)
    }

    /// Evaluate a Luau script against a fresh headless app.
    pub fn evaluate(&self, request: &ScriptEvalRequest) -> ScriptEvalOutcome {
        let viewport = request.viewport.unwrap_or_default();
        let mut metadata = ExecutionMetadata::fresh(self.metadata(), viewport);
        let outcome = (|| {
            if let Err(error) = viewport.validate() {
                return ScriptEvalOutcome::error_only(
                    ScriptErrorType::Invalid,
                    error.to_string(),
                    Vec::new(),
                    ScriptTiming::default(),
                );
            }
            let total_start = Instant::now();
            let mut canopy =
                match build_headless(self, viewport, request.fixture.as_deref(), &mut metadata) {
                    Ok(canopy) => canopy,
                    Err(error) => {
                        return ScriptEvalOutcome::error_only(
                            ScriptErrorType::Build,
                            error.to_string(),
                            Vec::new(),
                            ScriptTiming::default(),
                        );
                    }
                };
            let build_ms = elapsed_ms(total_start);
            evaluate_in(&mut canopy, request, build_ms, total_start, true)
        })();
        outcome.with_metadata(metadata)
    }
}

/// Build a bootstrap payload from a finalized app.
pub fn bootstrap_for_canopy(
    canopy: &Canopy,
    mut metadata: ExecutionMetadata,
) -> CanopyResult<BootstrapResponse> {
    let api = canopy.script_api()?.to_string();
    metadata.api_digest = Some(stable_digest(&api));
    let catalog =
        script_api_catalog(api).map_err(|error| CanopyError::Invalid(error.to_string()))?;
    let ScriptApiResolution::Response(overview) = catalog
        .query(&ScriptApiQuery::default())
        .map_err(|error| CanopyError::Invalid(error.to_string()))?
    else {
        unreachable!("Canopy declarations are ready")
    };
    let focus_commands = bootstrap_commands(canopy, CommandTarget::Focus)?;
    let default_commands = bootstrap_commands(canopy, CommandTarget::From(canopy.root_id()))?;
    Ok(BootstrapResponse {
        metadata,
        guide: format!("{BOOTSTRAP_GUIDE} Call script_api for declarations."),
        api_sources: overview.entries,
        fixtures: canopy.fixture_infos(),
        default_target: "root".to_string(),
        default_commands,
        focus_commands,
        journal: bootstrap_journal(canopy),
    })
}

/// Build one request-scoped catalog from a finalized app declaration.
pub fn script_api_catalog(api: String) -> result::Result<ScriptApiCatalog, ScriptApiError> {
    ScriptApiCatalog::new(
        ScriptApiGuide {
            introduction: "Discover the generated Canopy API before you automate an app. `canopy` and app-owned command tables are globals; do not call require().".to_owned(),
            tasks: vec![
                ScriptApiTask {
                    name: "Inspect the app".to_owned(),
                    instruction: "Request canopy.screen_text, canopy.screen_cells, or canopy.route_trace.".to_owned(),
                },
                ScriptApiTask {
                    name: "Run commands".to_owned(),
                    instruction: "Request canopy.commands or one app-owned command path.".to_owned(),
                },
                ScriptApiTask {
                    name: "Use fixtures".to_owned(),
                    instruction: "Request canopy.fixtures and apply a named fixture before evaluation.".to_owned(),
                },
            ],
        },
        vec![ScriptApiSource {
            id: "canopy".to_owned(),
            description: "Generated globals for the active Canopy application.".to_owned(),
            load: ScriptApiLoad::Global {
                roots: vec!["canopy".to_owned()],
            },
            availability: ScriptApiAvailability::Ready,
            declaration: Some(api),
            example: Some("return canopy.screen_text()".to_owned()),
        }],
    )
}

/// Resolve one query against a generated declaration.
pub fn query_script_api(
    api: String,
    query: &ScriptApiQuery,
) -> result::Result<ScriptApiResponse, ScriptApiError> {
    match script_api_catalog(api)?.query(query)? {
        ScriptApiResolution::Response(response) => Ok(response),
        ScriptApiResolution::SourceRequired(_) => unreachable!("Canopy declarations are ready"),
    }
}

/// Return command availability records.
fn bootstrap_commands(
    canopy: &Canopy,
    target: CommandTarget,
) -> CanopyResult<Vec<BootstrapCommand>> {
    Ok(canopy
        .command_availability(target)?
        .into_iter()
        .map(|availability| {
            let owner = availability.spec.dispatch.owner().unwrap_or("");
            let (status, disabled_reason) = match &availability.status {
                Some(status) => {
                    let reason = match status {
                        CommandStatus::Disabled(reason) => Some(reason.clone()),
                        CommandStatus::Enabled => None,
                    };
                    (Some(status.label().to_string()), reason)
                }
                None => (None, None),
            };
            BootstrapCommand {
                name: availability.spec.name.to_string(),
                owner: owner.to_string(),
                available: availability.resolution.is_some(),
                status,
                disabled_reason,
                missing_requirements: availability
                    .missing_requirements
                    .into_iter()
                    .map(|requirement| requirement.as_str().to_string())
                    .collect(),
            }
        })
        .collect())
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
pub fn stable_digest(text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Evaluate a Luau script against an existing live canopy app.
#[cfg(test)]
pub fn evaluate_live(
    canopy: &mut Canopy,
    request: &ScriptEvalRequest,
    context: &LiveContext,
) -> ScriptEvalOutcome {
    let metadata = match context.metadata(canopy) {
        Ok(metadata) => metadata,
        Err(error) => {
            return ScriptEvalOutcome::error_only(
                ScriptErrorType::Invalid,
                error.to_string(),
                Vec::new(),
                ScriptTiming::default(),
            )
            .with_metadata(context.unavailable_metadata());
        }
    };
    let outcome = match validate_live_request(request, &metadata) {
        Ok(()) => evaluate_in(canopy, request, 0, Instant::now(), false),
        Err(error) => ScriptEvalOutcome::error_only(
            ScriptErrorType::Invalid,
            error.to_string(),
            Vec::new(),
            ScriptTiming::default(),
        ),
    };
    outcome.with_metadata(metadata)
}

/// Check a live request without changing fixtures, viewport, or application
/// state.
fn validate_live_request(
    request: &ScriptEvalRequest,
    metadata: &ExecutionMetadata,
) -> CanopyResult<()> {
    if request.fixture.is_some() {
        return Err(CanopyError::Invalid(
            "live sessions do not support eval(fixture=...); use apply_fixture instead".into(),
        ));
    }
    validate_live_viewport(request.viewport, metadata)
}

/// Validate live dimensions as a compatibility check; never resize the live
/// app.
pub fn validate_live_viewport(
    viewport: Option<Viewport>,
    metadata: &ExecutionMetadata,
) -> CanopyResult<()> {
    if let Some(viewport) = viewport {
        viewport
            .validate()
            .map_err(|error| CanopyError::Invalid(error.to_string()))?;
        if Some(viewport) != metadata.viewport {
            return Err(CanopyError::Invalid(
                "requested viewport differs from live viewport".into(),
            ));
        }
    }
    Ok(())
}

/// Evaluate on the live driver, awaiting completion without borrowing the UI.
pub async fn evaluate_live_request(
    automation: AutomationHandle,
    request: ScriptEvalRequest,
    context: LiveContext,
) -> ScriptEvalOutcome {
    let metadata_handle = automation.clone();
    let observed_context = context.clone();
    let metadata = match spawn_blocking(move || {
        metadata_handle.request(move |canopy| observed_context.metadata(canopy))
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result.map_err(|error| error.to_string()))
    {
        Ok(metadata) => metadata,
        Err(message) => {
            return ScriptEvalOutcome::error_only(
                ScriptErrorType::Runtime,
                message,
                Vec::new(),
                ScriptTiming::default(),
            )
            .with_metadata(context.unavailable_metadata());
        }
    };
    if let Err(error) = validate_live_request(&request, &metadata) {
        return ScriptEvalOutcome::error_only(
            ScriptErrorType::Invalid,
            error.to_string(),
            Vec::new(),
            ScriptTiming::default(),
        )
        .with_metadata(metadata);
    }
    evaluate_live_request_inner(automation, request)
        .await
        .with_metadata(metadata)
}

/// Submit and await a request whose live execution contract has been validated.
async fn evaluate_live_request_inner(
    automation: AutomationHandle,
    request: ScriptEvalRequest,
) -> ScriptEvalOutcome {
    let total_start = Instant::now();
    let preflight_handle = automation.clone();
    let source = request.script.clone();
    let preflight = spawn_blocking(move || {
        preflight_handle.request(move |canopy| {
            let gate = typecheck_for_eval(canopy, &source, live_timing(total_start, total_start));
            Ok((canopy.root_id(), gate))
        })
    })
    .await;
    let (anchor, gate) = match preflight {
        Ok(Ok(preflight)) => preflight,
        Ok(Err(error)) => {
            return failed_info(
                canopy_error_info(&error),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                live_timing(total_start, total_start),
            );
        }
        Err(error) => {
            return ScriptEvalOutcome::error_only(
                ScriptErrorType::Runtime,
                error.to_string(),
                Vec::new(),
                live_timing(total_start, total_start),
            );
        }
    };
    let diagnostics = match gate {
        TypecheckGate::Ready(diagnostics) => diagnostics,
        TypecheckGate::Failed(outcome) => return *outcome,
    };
    let exec_start = Instant::now();
    let ticket = match automation.submit_eval(EvalRequest {
        source: request.script,
        timeout: request
            .timeout_ms
            .filter(|timeout| *timeout > 0)
            .map(Duration::from_millis),
        anchor,
    }) {
        Ok(ticket) => ticket,
        Err(error) => {
            return failed_info(
                canopy_error_info(&error),
                Vec::new(),
                Vec::new(),
                diagnostics,
                live_timing(total_start, exec_start),
            );
        }
    };
    let completion = match ticket.completion.await {
        Ok(completion) => completion,
        Err(error) => {
            return ScriptEvalOutcome::error_only(
                ScriptErrorType::Runtime,
                format!("live evaluation completion channel closed: {error}"),
                diagnostics,
                live_timing(total_start, exec_start),
            );
        }
    };
    let timing = live_timing(total_start, exec_start);
    match completion.result.as_ref() {
        Ok(value) => match value.to_external_json_value() {
            Ok(value) => ScriptEvalOutcome {
                metadata: ExecutionMetadata::default(),
                success: true,
                state: ScriptTaskState::Completed,
                value: Some(value),
                logs: completion.logs,
                assertions: completion.assertions,
                diagnostics,
                timing,
                error: None,
            },
            Err(error) => failure_with_logs(
                &crate::Error::from(error),
                completion.logs,
                completion.assertions,
                diagnostics,
                timing,
            ),
        },
        Err(error) => failed_info(
            canopy_error_info(error),
            completion.logs,
            completion.assertions,
            diagnostics,
            timing,
        ),
    }
}

/// Elapsed wall time for transport timing metadata.
fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Timing for live evaluation, where construction belongs to the running app.
fn live_timing(total_start: Instant, exec_start: Instant) -> ScriptTiming {
    ScriptTiming {
        build_ms: 0,
        exec_ms: elapsed_ms(exec_start),
        total_ms: elapsed_ms(total_start),
    }
}

/// Typecheck, evaluate, and report one script against an already-built canopy
/// app.
///
/// `render` is supplied for headless evaluation, where nothing else drives the
/// screen after the script runs; a live app renders on its own event loop.
fn evaluate_in(
    canopy: &mut Canopy,
    request: &ScriptEvalRequest,
    build_ms: u64,
    total_start: Instant,
    render: bool,
) -> ScriptEvalOutcome {
    let diagnostics = match typecheck_for_eval(
        canopy,
        &request.script,
        ScriptTiming {
            build_ms,
            exec_ms: 0,
            total_ms: elapsed_ms(total_start),
        },
    ) {
        TypecheckGate::Ready(diagnostics) => diagnostics,
        TypecheckGate::Failed(outcome) => return *outcome,
    };

    let exec_start = Instant::now();
    let eval_result = eval_script(canopy, &request.script, request.timeout_ms).and_then(|value| {
        if render {
            canopy.render(&mut NopBackend::new())?;
        }
        Ok(value.to_external_json_value()?)
    });
    let exec_ms = elapsed_ms(exec_start);
    let timing = ScriptTiming {
        build_ms,
        exec_ms,
        total_ms: elapsed_ms(total_start),
    };
    let logs = canopy.take_script_logs();
    let assertions = canopy.take_script_assertions();

    match eval_result {
        Ok(value) => ScriptEvalOutcome {
            metadata: ExecutionMetadata::default(),
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

/// Build and initially render a fresh headless app for one script request.
fn build_headless(
    factory: &AppFactory,
    viewport: Viewport,
    fixture: Option<&str>,
    metadata: &mut ExecutionMetadata,
) -> Result<Canopy> {
    let mut canopy = factory.build()?;
    metadata.api_digest = Some(stable_digest(canopy.script_api()?));
    canopy.set_root_size(viewport.into())?;
    if let Some(fixture) = fixture {
        canopy.apply_fixture(fixture)?;
        if metadata.reset != ResetPolicy::Isolated {
            metadata.reset = ResetPolicy::Fixture;
        }
    }
    canopy.render(&mut NopBackend::new())?;
    Ok(canopy)
}

/// Evaluate a script with an optional cooperative timeout.
fn eval_script(canopy: &mut Canopy, script: &str, timeout_ms: Option<u64>) -> Result<ArgValue> {
    let outcome = canopy.eval(EvalRequest {
        source: script.to_string(),
        timeout: timeout_ms
            .filter(|timeout| *timeout > 0)
            .map(Duration::from_millis),
        anchor: canopy.root_id(),
    })?;
    outcome.into_result().map_err(Into::into)
}

/// Result of the shared typecheck gate used by headless and live evaluation.
enum TypecheckGate {
    /// Typechecking succeeded and evaluation may continue.
    Ready(Vec<ScriptCheckDiagnostic>),
    /// Typechecking failed and evaluation should stop.
    Failed(Box<ScriptEvalOutcome>),
}

/// Run Luau typechecking and return a failure outcome when evaluation should
/// stop.
fn typecheck_for_eval(canopy: &mut Canopy, script: &str, timing: ScriptTiming) -> TypecheckGate {
    let result = match canopy.check_script("canopy/mcp-eval", script) {
        Ok(result) => result,
        Err(error) => {
            return TypecheckGate::Failed(Box::new(ScriptEvalOutcome::error_only(
                ScriptErrorType::Typecheck,
                error.to_string(),
                Vec::new(),
                timing,
            )));
        }
    };
    let has_errors = result.has_errors();
    let diagnostics = result.into_diagnostics();
    if has_errors {
        return TypecheckGate::Failed(Box::new(ScriptEvalOutcome::error_only(
            ScriptErrorType::Typecheck,
            "script failed Luau type checking",
            diagnostics,
            timing,
        )));
    }
    TypecheckGate::Ready(diagnostics)
}

/// Build a failed outcome while preserving logs, assertions, and diagnostics.
fn failure_with_logs(
    error: &crate::Error,
    logs: Vec<String>,
    assertions: Vec<ScriptAssertion>,
    diagnostics: Vec<ScriptCheckDiagnostic>,
    timing: ScriptTiming,
) -> ScriptEvalOutcome {
    failed_info(
        script_error_info(error),
        logs,
        assertions,
        diagnostics,
        timing,
    )
}

/// Build a failed outcome from owned structured error fields.
fn failed_info(
    info: ScriptErrorInfo,
    logs: Vec<String>,
    assertions: Vec<ScriptAssertion>,
    diagnostics: Vec<ScriptCheckDiagnostic>,
    timing: ScriptTiming,
) -> ScriptEvalOutcome {
    ScriptEvalOutcome {
        metadata: ExecutionMetadata::default(),
        success: false,
        state: info.error_type.into(),
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
    if let crate::Error::Canopy(error) = error {
        return canopy_error_info(error);
    }
    ScriptErrorInfo {
        error_type: ScriptErrorType::Runtime,
        kind: None,
        command: None,
        owner: None,
        message: error.to_string(),
    }
}

/// Preserve structured fields from shared driver errors without cloning the
/// error.
fn canopy_error_info(error: &CanopyError) -> ScriptErrorInfo {
    if let CanopyError::ScriptStructured {
        kind,
        command,
        owner,
        message,
    } = error
    {
        return ScriptErrorInfo {
            error_type: if *kind == ScriptErrorKind::Timeout {
                ScriptErrorType::Timeout
            } else {
                ScriptErrorType::Runtime
            },
            kind: Some(*kind),
            command: command.clone(),
            owner: owner.clone(),
            message: message.clone(),
        };
    }
    let error_type = match error {
        CanopyError::ScriptTimeout { .. } => ScriptErrorType::Timeout,
        CanopyError::ScriptCancelled => ScriptErrorType::Cancelled,
        _ => ScriptErrorType::Runtime,
    };
    ScriptErrorInfo {
        error_type,
        kind: matches!(error, CanopyError::ScriptCancelled)
            .then_some(ScriptErrorKind::ScriptCancelled),
        command: None,
        owner: None,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        CanopyBuilder, Fixture, FocusDirection, derive_commands, error::Result as CanopyResult,
        prelude::*, testing::contracts,
    };

    use super::*;

    #[test]
    fn stable_digest_matches_fnv1a_vectors() {
        assert_eq!(stable_digest(""), "cbf29ce484222325");
        assert_eq!(stable_digest("a"), "af63dc4c8601ec8c");
        assert_eq!(stable_digest("foobar"), "85944171f73967e8");
    }

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
        fn name(&self) -> NodeName {
            NodeName::convert("script_target")
        }
    }

    impl Loader for ScriptTarget {
        fn load(cnpy: &mut Canopy) -> CanopyResult<()> {
            cnpy.add_commands::<Self>()
        }
    }

    fn test_app() -> crate::Result<Canopy> {
        CanopyBuilder::new()
            .configure(|canopy| {
                ScriptTarget::load(canopy)?;
                canopy.register_default_bindings("script_target", r#"canopy.log("defaults")"#)?;
                canopy.register_fixture(Fixture::new(
                    "seeded",
                    "Set script_target to a known value",
                    |canopy| canopy.eval_script("script_target.set(31)").map(|_| ()),
                ))
            })
            .assemble(|canopy| {
                canopy.replace_root(ScriptTarget::new())?;
                Ok(())
            })
            .build()
            .map_err(Into::into)
    }

    fn test_factory_with(metadata: crate::AppMetadata) -> AppFactory {
        AppFactory::new(metadata, test_app)
    }

    fn test_factory() -> AppFactory {
        test_factory_with(crate::AppMetadata::test())
    }

    #[test]
    fn headless_metadata_preserves_reset_contract_and_independent_sessions() {
        let factory = test_factory_with(crate::AppMetadata {
            app: "persistent-test".into(),
            reset: ResetPolicy::External,
        });
        let evaluator = factory;
        let mut request = ScriptEvalRequest {
            viewport: Some(Viewport {
                width: 24,
                height: 5,
            }),
            ..ScriptEvalRequest::new("return script_target.get()")
        };
        let first = evaluator.evaluate(&request);
        let second = evaluator.evaluate(&request);
        assert!(first.success && second.success);
        assert_ne!(first.metadata.session_id, second.metadata.session_id);
        assert_eq!(first.metadata.app, "persistent-test");
        assert_eq!(first.metadata.reset, ResetPolicy::External);
        assert_eq!(first.metadata.viewport, request.viewport);
        assert!(first.metadata.api_digest.is_some());
        request.fixture = Some("seeded".into());
        assert_eq!(
            evaluator.evaluate(&request).metadata.reset,
            ResetPolicy::Fixture
        );
        let isolated = test_factory_with(crate::AppMetadata {
            app: "isolated-test".into(),
            reset: ResetPolicy::Isolated,
        });
        assert_eq!(
            isolated.evaluate(&request).metadata.reset,
            ResetPolicy::Isolated
        );
    }

    #[test]
    fn invalid_viewport_never_constructs_the_factory() {
        let evaluator = app_factory(|| panic!("invalid viewport constructed app"));
        for viewport in [
            Viewport {
                width: 0,
                height: 1,
            },
            Viewport {
                width: 2049,
                height: 1,
            },
            Viewport {
                width: 2048,
                height: 2048,
            },
        ] {
            assert!(
                evaluator
                    .bootstrap(&BootstrapRequest {
                        viewport: Some(viewport)
                    })
                    .is_err()
            );
            let outcome = evaluator.evaluate(&ScriptEvalRequest {
                viewport: Some(viewport),
                ..ScriptEvalRequest::new("return 1")
            });
            assert!(!outcome.success);
            assert_eq!(outcome.metadata.viewport, Some(viewport));
            assert_eq!(outcome.metadata.api_digest, None);
        }
    }

    #[test]
    fn app_viewport_limits_are_checked_before_fixture_mutation() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let changes = Arc::new(AtomicUsize::new(0));
        let factory_changes = changes.clone();
        let evaluator = app_factory(move || {
            let changes = factory_changes.clone();
            CanopyBuilder::new()
                .configure(move |canopy| {
                    canopy.set_render_limits(canopy::RenderLimits::new(10, 10, 100))?;
                    canopy.register_fixture(Fixture::new(
                        "mutate",
                        "Count fixture changes",
                        move |_| {
                            changes.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        },
                    ))
                })
                .build()
                .map_err(Into::into)
        });
        let outcome = evaluator.evaluate(&ScriptEvalRequest {
            fixture: Some("mutate".into()),
            viewport: Some(Viewport {
                width: 20,
                height: 5,
            }),
            ..ScriptEvalRequest::new("return 1")
        });
        assert!(!outcome.success);
        assert_eq!(changes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn direct_live_context_retains_identity_and_rejects_resize() -> crate::Result<()> {
        let mut canopy = test_factory().build()?;
        canopy.set_root_size(Size::new(20, 5))?;
        canopy.render(&mut NopBackend::new())?;
        let context = LiveContext::new(crate::AppMetadata {
            app: "live-test".into(),
            reset: ResetPolicy::Isolated,
        });
        let mut request = ScriptEvalRequest::new("return script_target.get()");
        let first = evaluate_live(&mut canopy, &request, &context);
        let second = evaluate_live(&mut canopy, &request, &context.clone());
        assert!(first.success && second.success);
        assert_eq!(first.metadata.session_id, second.metadata.session_id);
        assert_eq!(first.metadata.execution, crate::ExecutionMode::LiveSession);
        assert_eq!(first.metadata.reset, ResetPolicy::External);
        request.viewport = Some(Viewport {
            width: 30,
            height: 5,
        });
        request.script = "script_target.set(99)".into();
        assert!(!evaluate_live(&mut canopy, &request, &context).success);
        assert_eq!(canopy.snapshot().unwrap().viewport, Size::new(20, 5));
        request.viewport = None;
        request.script = "return script_target.get()".into();
        assert_eq!(
            evaluate_live(&mut canopy, &request, &context).value,
            first.value
        );
        Ok(())
    }

    #[test]
    fn shared_trace_through_headless_evaluator() -> crate::Result<()> {
        let evaluator = AppFactory::new(
            crate::AppMetadata {
                app: "contract".into(),
                reset: ResetPolicy::Isolated,
            },
            || Ok(contracts::app()?),
        );
        let result = evaluator.evaluate(&ScriptEvalRequest {
            viewport: Some(Viewport {
                width: 12,
                height: 3,
            }),
            ..ScriptEvalRequest::new(contracts::SCRIPT)
        });
        assert!(result.success, "{result:?}");
        assert_eq!(
            result.value,
            Some(contracts::expected().to_external_json_value()?)
        );
        Ok(())
    }

    #[test]
    fn bootstrap_declares_script_default() -> crate::Result<()> {
        let evaluator = test_factory();
        let bootstrap = evaluator.bootstrap(&BootstrapRequest::default())?;
        assert_eq!(bootstrap.default_target, "root");
        assert!(
            bootstrap
                .default_commands
                .iter()
                .any(|command| command.name == "get" && command.available)
        );
        let json = serde_json::to_value(&bootstrap).unwrap();
        let decoded: BootstrapResponse = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, bootstrap);
        Ok(())
    }

    #[test]
    fn script_api_lists_commands() -> crate::Result<()> {
        let evaluator = test_factory();
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
        let evaluator = test_factory();
        let api = evaluator.script_api()?;
        let marker = "export type FocusDirection";
        let (_, tail) = api
            .split_once(marker)
            .expect("script API should contain the generated owner module");
        let actual = format!("{marker}{tail}");
        let expected = r#"export type FocusDirection = "Next" | "Prev" | "Up" | "Down" | "Left" | "Right"

declare script_target: {
    choose: (direction: FocusDirection, count: number?) -> number,
    --- Register this widget's default bindings.
    default_bindings: () -> (),
    get: () -> number,
    set: (value: number) -> (),
}

-- ===== Fixtures =====
-- seeded: Set script_target to a known value"#;

        assert_eq!(actual.trim_end(), expected);
        Ok(())
    }

    #[test]
    fn evaluate_returns_value_and_logs() {
        let evaluator = test_factory();
        let outcome = evaluator.evaluate(&ScriptEvalRequest::new(
            r#"
                canopy.log("hello")
                script_target.set(7)
                return script_target.get()
            "#
            .to_string(),
        ));
        assert!(outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Completed);
        assert_eq!(outcome.logs, vec!["hello"]);
        assert_eq!(outcome.value, Some(JsonValue::from(7)));
    }

    #[test]
    fn evaluate_returns_node_handles_as_external_tokens() {
        let evaluator = test_factory();
        let outcome = evaluator.evaluate(&ScriptEvalRequest::new(
            "local root = canopy.root()\nprint(root)\nreturn root".to_string(),
        ));

        assert!(outcome.success);
        let value = outcome.value.expect("node token");
        assert_eq!(value["type"], JsonValue::String("NodeId".to_string()));
        assert!(value["token"].is_string());
        assert_eq!(outcome.logs, vec![value["token"].as_str().unwrap()]);
    }

    #[test]
    fn evaluate_applies_fixtures_and_named_optional_args() {
        let evaluator = test_factory();
        let outcome = evaluator.evaluate(&ScriptEvalRequest {
            fixture: Some("seeded".to_string()),
            ..ScriptEvalRequest::new(
                r#"
                canopy.assert(script_target.get() == 31, "fixture should run before eval")
                return canopy.cmd("script_target::choose", { direction = "Right" })
            "#
                .to_string(),
            )
        });

        assert!(outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Completed);
        assert_eq!(outcome.value, Some(JsonValue::from(6)));
    }

    #[test]
    fn evaluate_reports_cooperative_timeout() {
        let evaluator = test_factory();
        let outcome = evaluator.evaluate(&ScriptEvalRequest {
            timeout_ms: Some(1),
            ..ScriptEvalRequest::new("while true do end")
        });

        assert!(!outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::TimedOut);
        assert_eq!(
            outcome.error.as_ref().map(|error| error.error_type),
            Some(ScriptErrorType::Timeout)
        );
    }

    #[test]
    fn evaluate_reports_typecheck_errors() {
        let evaluator = test_factory();
        let outcome = evaluator.evaluate(&ScriptEvalRequest::new(
            r#"script_target.set("bad")"#.to_string(),
        ));
        assert!(!outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Failed);
        assert_eq!(
            outcome.error.as_ref().map(|error| error.error_type),
            Some(ScriptErrorType::Typecheck)
        );
        assert!(!outcome.diagnostics.is_empty());
    }

    #[test]
    fn evaluate_reports_structured_command_errors() {
        let evaluator = test_factory();
        let outcome = evaluator.evaluate(&ScriptEvalRequest::new(
            r#"canopy.cmd("missing::command")"#.to_string(),
        ));

        assert!(!outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Failed);
        let error = outcome.error.as_ref().expect("structured error");
        assert_eq!(error.error_type, ScriptErrorType::Runtime);
        assert_eq!(error.kind, Some(ScriptErrorKind::UnknownCommand));
        assert_eq!(error.command.as_deref(), Some("missing::command"));
        assert_eq!(error.owner, None);
    }

    #[test]
    fn evaluate_live_reports_json_conversion_errors() -> crate::Result<()> {
        let mut canopy = test_factory().build()?;
        let outcome = evaluate_live(
            &mut canopy,
            &ScriptEvalRequest::new("return function() end".to_string()),
            &LiveContext::new(crate::AppMetadata::test()),
        );

        assert!(!outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Failed);
        assert_eq!(outcome.value, None);
        assert_eq!(
            outcome.error.as_ref().map(|error| error.error_type),
            Some(ScriptErrorType::Runtime)
        );
        Ok(())
    }

    #[test]
    fn evaluate_live_rejects_fixture_parameter() -> crate::Result<()> {
        let mut canopy = test_factory().build()?;
        let outcome = evaluate_live(
            &mut canopy,
            &ScriptEvalRequest {
                fixture: Some("seeded".to_string()),
                ..ScriptEvalRequest::new("return script_target.get()")
            },
            &LiveContext::new(crate::AppMetadata::test()),
        );

        assert!(!outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Failed);
        assert_eq!(
            outcome.error.as_ref().map(|error| error.error_type),
            Some(ScriptErrorType::Invalid)
        );
        Ok(())
    }

    #[test]
    fn evaluate_live_observes_applied_fixture() -> crate::Result<()> {
        let mut canopy = test_factory().build()?;
        canopy.apply_fixture("seeded")?;
        let outcome = evaluate_live(
            &mut canopy,
            &ScriptEvalRequest::new("return script_target.get()".to_string()),
            &LiveContext::new(crate::AppMetadata::test()),
        );

        assert!(outcome.success);
        assert_eq!(outcome.state, ScriptTaskState::Completed);
        assert_eq!(outcome.value, Some(JsonValue::from(31)));
        Ok(())
    }
    #[test]
    fn cancellation_has_a_distinct_task_state() {
        let outcome = failure_with_logs(
            &crate::Error::Canopy(CanopyError::ScriptCancelled),
            vec!["before cancellation".to_string()],
            Vec::new(),
            Vec::new(),
            ScriptTiming::default(),
        );
        assert_eq!(outcome.state, ScriptTaskState::Cancelled);
        assert_eq!(outcome.logs, vec!["before cancellation"]);
        assert_eq!(
            outcome.error.as_ref().unwrap().kind,
            Some(ScriptErrorKind::ScriptCancelled)
        );
    }
}
