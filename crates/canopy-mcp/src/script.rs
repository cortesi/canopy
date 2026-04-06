use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use canopy::{Canopy, FixtureInfo, geom::Size, testing::render::NopBackend};
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
    /// One-based line number.
    pub line: usize,
    /// One-based column number.
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
    /// Error category such as `build`, `typecheck`, `timeout`, or `runtime`.
    pub error_type: String,
    /// Human-readable error message.
    pub message: String,
}

/// Structured response for the `script_eval` tool and smoke runner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptEvalOutcome {
    /// Whether the script completed successfully.
    pub success: bool,
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

impl ScriptEvalOutcome {
    /// Encode the outcome as an MCP tool result.
    pub fn to_tool_result(&self) -> CallToolResult {
        match serde_json::to_value(self) {
            Ok(value) => {
                let mut result = CallToolResult::new()
                    .with_structured_content(value.clone())
                    .with_text_content(value.to_string());
                if !self.success {
                    result = result.mark_as_error();
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
        Self {
            success: false,
            value: None,
            logs: Vec::new(),
            assertions: Vec::new(),
            diagnostics,
            timing,
            error: Some(ScriptErrorInfo {
                error_type: error_type.into(),
                message: message.into(),
            }),
        }
    }
}

impl From<ScriptEvalOutcome> for CallToolResult {
    fn from(outcome: ScriptEvalOutcome) -> Self {
        outcome.to_tool_result()
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

    /// Evaluate a Luau script, enforcing the requested timeout when present.
    pub fn evaluate_with_timeout(&self, request: ScriptEvalRequest) -> ScriptEvalOutcome {
        let Some(timeout_ms) = request.timeout_ms.filter(|timeout| *timeout > 0) else {
            return self.evaluate(&request);
        };

        let (tx, rx) = mpsc::channel();
        let evaluator = self.clone();
        let mut request = request;
        request.timeout_ms = None;
        thread::spawn(move || {
            drop(tx.send(evaluator.evaluate(&request)));
        });

        match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
            Ok(outcome) => outcome,
            Err(mpsc::RecvTimeoutError::Timeout) => ScriptEvalOutcome::error_only(
                "timeout",
                format!("script evaluation exceeded {timeout_ms}ms"),
                Vec::new(),
                ScriptTiming::zero(),
            ),
            Err(mpsc::RecvTimeoutError::Disconnected) => ScriptEvalOutcome::error_only(
                "runtime",
                "script evaluation thread terminated unexpectedly",
                Vec::new(),
                ScriptTiming::zero(),
            ),
        }
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

        let diagnostics = match session.typecheck(&request.script) {
            Ok(diagnostics) => diagnostics,
            Err(error) => {
                return ScriptEvalOutcome::error_only(
                    "typecheck",
                    error.to_string(),
                    Vec::new(),
                    ScriptTiming {
                        build_ms,
                        exec_ms: 0,
                        total_ms: total_start.elapsed().as_millis() as u64,
                    },
                );
            }
        };
        if !diagnostics.is_empty() {
            return ScriptEvalOutcome::error_only(
                "typecheck",
                "script failed Luau type checking",
                diagnostics,
                ScriptTiming {
                    build_ms,
                    exec_ms: 0,
                    total_ms: total_start.elapsed().as_millis() as u64,
                },
            );
        }

        let exec_start = Instant::now();
        let eval_result = session.evaluate(&request.script);
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
                value: Some(value),
                logs,
                assertions,
                diagnostics,
                timing,
                error: None,
            },
            Err(error) => ScriptEvalOutcome {
                success: false,
                value: None,
                logs,
                assertions,
                diagnostics,
                timing,
                error: Some(ScriptErrorInfo {
                    error_type: "runtime".to_string(),
                    message: error.to_string(),
                }),
            },
        }
    }
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
    let diagnostics = match canopy.check_script(&request.script) {
        Ok(result) => result
            .errors()
            .into_iter()
            .map(|diagnostic| ScriptDiagnostic {
                severity: "error".to_string(),
                line: diagnostic.line as usize + 1,
                column: diagnostic.col as usize + 1,
                message: diagnostic.message.clone(),
            })
            .collect::<Vec<_>>(),
        Err(error) => {
            return ScriptEvalOutcome::error_only(
                "typecheck",
                error.to_string(),
                Vec::new(),
                ScriptTiming::zero(),
            );
        }
    };

    if !diagnostics.is_empty() {
        return ScriptEvalOutcome::error_only(
            "typecheck",
            "script failed Luau type checking",
            diagnostics,
            ScriptTiming {
                build_ms: 0,
                exec_ms: 0,
                total_ms: total_start.elapsed().as_millis() as u64,
            },
        );
    }

    let exec_start = Instant::now();
    let eval_result = canopy.eval_script_value(&request.script);
    let exec_ms = exec_start.elapsed().as_millis() as u64;
    let timing = ScriptTiming {
        build_ms: 0,
        exec_ms,
        total_ms: total_start.elapsed().as_millis() as u64,
    };
    let logs = canopy.take_script_logs();
    let assertions = canopy
        .take_script_assertions()
        .into_iter()
        .map(|assertion| ScriptAssertion {
            passed: assertion.passed,
            message: assertion.message,
        })
        .collect();

    match eval_result {
        Ok(value) => ScriptEvalOutcome {
            success: true,
            value: Some(value.to_json_value().unwrap_or(JsonValue::Null)),
            logs,
            assertions,
            diagnostics,
            timing,
            error: None,
        },
        Err(error) => ScriptEvalOutcome {
            success: false,
            value: None,
            logs,
            assertions,
            diagnostics,
            timing,
            error: Some(ScriptErrorInfo {
                error_type: "runtime".to_string(),
                message: error.to_string(),
            }),
        },
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

    /// Type-check a script against the app's rendered Luau API.
    fn typecheck(&mut self, script: &str) -> Result<Vec<ScriptDiagnostic>> {
        let result = self.canopy.check_script(script)?;
        Ok(result
            .errors()
            .into_iter()
            .map(|diagnostic| ScriptDiagnostic {
                severity: "error".to_string(),
                line: diagnostic.line as usize + 1,
                column: diagnostic.col as usize + 1,
                message: diagnostic.message.clone(),
            })
            .collect())
    }

    /// Execute a script and return its JSON-serializable result value.
    fn evaluate(&mut self, script: &str) -> Result<JsonValue> {
        let value = self.canopy.eval_script_value(script)?;
        self.canopy.render(&mut self.backend)?;
        Ok(value.to_json_value()?)
    }

    /// Drain the script log buffer.
    fn take_logs(&self) -> Vec<String> {
        self.canopy.take_script_logs()
    }

    /// Drain recorded assertion results.
    fn take_assertions(&self) -> Vec<ScriptAssertion> {
        self.canopy
            .take_script_assertions()
            .into_iter()
            .map(|assertion| ScriptAssertion {
                passed: assertion.passed,
                message: assertion.message,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        ReadContext, command, derive_commands, error::Result as CanopyResult, prelude::*,
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
            canopy.finalize_api()?;
            canopy
                .core
                .replace_subtree(canopy.core.root_id(), ScriptTarget::new())?;
            Ok(canopy)
        })
    }

    #[test]
    fn script_api_lists_commands() -> crate::Result<()> {
        let evaluator = AppEvaluator::new(test_factory());
        let api = evaluator.script_api()?;
        assert!(api.contains("declare script_target"));
        assert!(api.contains("set: (value: number) -> ()"));
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
        assert_eq!(outcome.logs, vec!["hello"]);
        assert_eq!(outcome.value, Some(JsonValue::from(7)));
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
        assert_eq!(
            outcome
                .error
                .as_ref()
                .map(|error| error.error_type.as_str()),
            Some("typecheck")
        );
        assert!(!outcome.diagnostics.is_empty());
        assert!(
            outcome
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("number"))
        );
    }
}
