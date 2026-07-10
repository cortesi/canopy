use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use canopy::{AutomationHandle, Canopy, error::Error as CanopyError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tmcp::{Server, ToolError, ToolResult, mcp_server, schema::CallToolResult, tool};
use tokio::{net::UnixListener, runtime::Builder, sync::oneshot, task::block_in_place};

use crate::{
    Error, Result,
    script::{
        AppEvaluator, AppFactory, ScriptEvalRequest, app_factory, bootstrap_for_canopy,
        evaluate_live,
    },
};

/// Build an MCP tool result with structured and text JSON payloads.
fn json_tool_result(value: &serde_json::Value) -> CallToolResult {
    CallToolResult::new()
        .with_structured_content(value.clone())
        .with_text_content(value.to_string())
}

/// Minimal stdio MCP server for canopy automation.
#[derive(Clone)]
struct CanopyMcpServer {
    /// Headless evaluator shared by all tool calls.
    evaluator: AppEvaluator,
}

/// Request payload for applying a named fixture to a live app.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ApplyFixtureRequest {
    /// Registered fixture name.
    pub name: String,
}

/// Construct the MCP server for an app factory.
fn canopy_mcp_server(factory: AppFactory) -> CanopyMcpServer {
    CanopyMcpServer {
        evaluator: AppEvaluator::new(factory),
    }
}

/// Live MCP server that proxies tool calls onto a running canopy UI thread.
#[derive(Clone)]
struct LiveCanopyMcpServer {
    /// Handle used to marshal work onto the runloop thread.
    automation: AutomationHandle,
}

/// Construct the live MCP server for a running canopy app.
fn live_canopy_mcp_server(automation: AutomationHandle) -> LiveCanopyMcpServer {
    LiveCanopyMcpServer { automation }
}

#[mcp_server]
impl CanopyMcpServer {
    #[tool]
    /// Return the operating guide, generated API, fixtures, availability, and journal summary.
    async fn bootstrap(&self) -> ToolResult<CallToolResult> {
        let bootstrap = self
            .evaluator
            .bootstrap()
            .map_err(|error| ToolError::internal(error.to_string()))?;
        let value = serde_json::to_value(bootstrap)
            .map_err(|error| ToolError::internal(error.to_string()))?;
        Ok(json_tool_result(&value))
    }

    #[tool]
    /// Evaluate a Luau script against a fresh headless canopy app instance.
    async fn script_eval(&self, params: ScriptEvalRequest) -> ToolResult<CallToolResult> {
        Ok(self.evaluator.evaluate(&params).to_tool_result())
    }

    #[tool]
    /// Return the rendered Luau API definition for the app.
    async fn script_api(&self) -> ToolResult<CallToolResult> {
        let api = self
            .evaluator
            .script_api()
            .map_err(|error| ToolError::internal(error.to_string()))?;
        Ok(CallToolResult::new().with_text_content(api))
    }

    #[tool]
    /// List the application's registered fixtures.
    async fn fixtures(&self) -> ToolResult<CallToolResult> {
        let fixtures = self
            .evaluator
            .fixtures()
            .map_err(|error| ToolError::internal(error.to_string()))?;
        let value = serde_json::to_value(fixtures)
            .map_err(|error| ToolError::internal(error.to_string()))?;
        Ok(json_tool_result(&value))
    }
}

#[mcp_server]
impl LiveCanopyMcpServer {
    #[tool]
    /// Return the operating guide, generated API, fixtures, availability, and journal summary.
    async fn bootstrap(&self) -> ToolResult<CallToolResult> {
        let automation = self.automation.clone();
        let bootstrap = block_in_place(move || {
            automation.request(|canopy| {
                canopy.finalize_api()?;
                bootstrap_for_canopy(canopy)
                    .map_err(|error| CanopyError::Internal(error.to_string()))
            })
        })
        .map_err(|error| ToolError::internal(error.to_string()))?;
        let value = serde_json::to_value(bootstrap)
            .map_err(|error| ToolError::internal(error.to_string()))?;
        Ok(json_tool_result(&value))
    }

    #[tool]
    /// Evaluate a Luau script against the currently running canopy app.
    async fn script_eval(&self, params: ScriptEvalRequest) -> ToolResult<CallToolResult> {
        let automation = self.automation.clone();
        let outcome = block_in_place(move || {
            automation.request(move |canopy| Ok(evaluate_live(canopy, &params)))
        })
        .map_err(|error| ToolError::internal(error.to_string()))?;
        Ok(outcome.to_tool_result())
    }

    #[tool]
    /// Return the rendered Luau API definition for the running app.
    async fn script_api(&self) -> ToolResult<CallToolResult> {
        let automation = self.automation.clone();
        let api = block_in_place(move || {
            automation.request(|canopy| canopy.script_api().map(str::to_string))
        })
        .map_err(|error| ToolError::internal(error.to_string()))?;
        Ok(CallToolResult::new().with_text_content(api))
    }

    #[tool]
    /// List the running app's registered fixtures.
    async fn fixtures(&self) -> ToolResult<CallToolResult> {
        let automation = self.automation.clone();
        let fixtures =
            block_in_place(move || automation.request(|canopy| Ok(canopy.fixture_infos())))
                .map_err(|error| ToolError::internal(error.to_string()))?;
        let value = serde_json::to_value(fixtures)
            .map_err(|error| ToolError::internal(error.to_string()))?;
        Ok(json_tool_result(&value))
    }

    #[tool]
    /// Apply a named fixture to the running app and trigger a re-render.
    async fn apply_fixture(&self, params: ApplyFixtureRequest) -> ToolResult<CallToolResult> {
        let name = params.name;
        let applied_name = name.clone();
        let automation = self.automation.clone();
        block_in_place(move || {
            automation.request(move |canopy| {
                canopy.apply_fixture(&name)?;
                Ok(())
            })
        })
        .map_err(|error| ToolError::internal(error.to_string()))?;
        let value = json!({ "applied": applied_name });
        Ok(json_tool_result(&value))
    }
}

/// Serve `script_eval` and `script_api` over stdio for an app factory.
pub fn serve_stdio(factory: impl Fn() -> Result<Canopy> + Send + Sync + 'static) -> Result<()> {
    let factory = app_factory(factory);
    Server::new(move || canopy_mcp_server(factory.clone()))
        .serve_stdio_blocking()
        .map_err(Error::from)
}

/// Handle for a running live UDS MCP listener.
pub struct UdsServerHandle {
    /// Socket file path served by the listener.
    socket_path: PathBuf,
    /// Shutdown signal for the listener thread.
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Background listener thread.
    thread: Option<thread::JoinHandle<Result<()>>>,
}

impl UdsServerHandle {
    /// Stop the listener and remove the socket path.
    pub fn stop(mut self) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ignored = shutdown_tx.send(());
        }
        let join_result = if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| Error::ListenerThreadPanicked)?
        } else {
            Ok(())
        };
        let _ignored = fs::remove_file(&self.socket_path);
        join_result
    }
}

impl Drop for UdsServerHandle {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ignored = shutdown_tx.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ignored = thread.join();
        }
        let _ignored = fs::remove_file(&self.socket_path);
    }
}

/// Serve live MCP automation for a running canopy app over a Unix-domain socket.
pub fn serve_uds(
    socket_path: impl AsRef<Path>,
    automation: AutomationHandle,
) -> Result<UdsServerHandle> {
    let socket_path = socket_path.as_ref().to_path_buf();
    if socket_path.exists() {
        fs::remove_file(&socket_path)?;
    }

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let listener_path = socket_path.clone();
    let thread = thread::spawn(move || {
        let runtime = Builder::new_multi_thread().enable_all().build()?;
        runtime.block_on(async move {
            let listener = match UnixListener::bind(&listener_path) {
                Ok(listener) => {
                    let _ignored = ready_tx.send(Ok(()));
                    listener
                }
                Err(error) => {
                    let error = Error::from(error);
                    let _ignored = ready_tx.send(Err(error));
                    return Ok(());
                }
            };

            let mut shutdown_rx = shutdown_rx;
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accept_result = listener.accept() => {
                        let (stream, _) = accept_result?;
                        let automation = automation.clone();
                        tokio::spawn(async move {
                            let (reader, writer) = stream.into_split();
                            let _ignored = Server::new(move || live_canopy_mcp_server(automation.clone()))
                                .serve_stream(reader, writer)
                                .await;
                        });
                    }
                }
            }

            let _ignored = fs::remove_file(&listener_path);
            Ok(())
        })
    });

    ready_rx
        .recv()
        .map_err(|_| Error::ListenerReadinessClosed)??;

    Ok(UdsServerHandle {
        socket_path,
        shutdown_tx: Some(shutdown_tx),
        thread: Some(thread),
    })
}

#[cfg(test)]
mod tests {
    use canopy::{
        Fixture, ViewContext, command, derive_commands, error::Result as CanopyResult, prelude::*,
    };

    use super::*;

    struct EchoNode {
        value: i32,
    }

    #[derive_commands]
    impl EchoNode {
        fn new() -> Self {
            Self { value: 0 }
        }

        #[command]
        fn ping(&self) -> &'static str {
            "pong"
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

    impl Widget for EchoNode {
        fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ViewContext) -> CanopyResult<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("echo_node")
        }
    }

    impl Loader for EchoNode {
        fn load(cnpy: &mut Canopy) -> CanopyResult<()> {
            cnpy.add_commands::<Self>()
        }
    }

    fn server() -> CanopyMcpServer {
        canopy_mcp_server(app_factory(|| {
            let mut canopy = Canopy::new();
            EchoNode::load(&mut canopy)?;
            canopy.register_fixture(Fixture::new(
                "seeded",
                "Set echo_node to a known value",
                |canopy| canopy.eval_script("echo_node.set(41)"),
            ))?;
            canopy.finalize_api()?;
            canopy.replace_root(EchoNode::new())?;
            Ok(canopy)
        }))
    }

    #[tokio::test]
    async fn script_api_returns_definition_text() {
        let result = server().script_api().await.expect("script_api");
        let text = result.text().expect("text response");
        assert!(text.contains("declare echo_node"));
    }

    #[tokio::test]
    async fn bootstrap_returns_api_and_availability() {
        let result = server().bootstrap().await.expect("bootstrap");
        let payload = result.structured_content.expect("structured content");
        assert!(
            payload["api"]
                .as_str()
                .expect("api")
                .contains("declare echo_node")
        );
        assert!(!payload["api_digest"].as_str().expect("digest").is_empty());
        assert!(
            payload["commands"]
                .as_array()
                .expect("commands")
                .iter()
                .any(|command| command["owner"] == "echo_node" && command["name"] == "ping")
        );
        assert_eq!(payload["fixtures"][0]["name"], "seeded");
    }

    #[tokio::test]
    async fn script_eval_returns_json_payload() {
        let result = server()
            .script_eval(ScriptEvalRequest {
                script: "return echo_node.ping()".to_string(),
                fixture: None,
                timeout_ms: None,
            })
            .await
            .expect("script_eval");
        let payload = serde_json::from_str::<serde_json::Value>(
            result
                .structured_content
                .as_ref()
                .expect("structured content")
                .to_string()
                .as_str(),
        )
        .expect("json payload");
        assert_eq!(payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            payload["value"],
            serde_json::Value::String("pong".to_string())
        );
    }

    #[tokio::test]
    async fn script_eval_applies_headless_fixture() {
        let result = server()
            .script_eval(ScriptEvalRequest {
                script: "return echo_node.get()".to_string(),
                fixture: Some("seeded".to_string()),
                timeout_ms: None,
            })
            .await
            .expect("script_eval");
        let payload = result.structured_content.expect("structured content");
        assert_eq!(payload["success"], serde_json::Value::Bool(true));
        assert_eq!(payload["value"], serde_json::Value::from(41));
    }

    #[tokio::test]
    async fn fixtures_returns_registered_fixture_metadata() {
        let result = server().fixtures().await.expect("fixtures");
        let payload = result.structured_content.expect("structured content");
        assert_eq!(
            payload[0]["name"],
            serde_json::Value::String("seeded".into())
        );
        assert_eq!(
            payload[0]["description"],
            serde_json::Value::String("Set echo_node to a known value".into())
        );
    }

    #[tokio::test]
    async fn script_eval_reports_typecheck_errors() {
        let result = server()
            .script_eval(ScriptEvalRequest {
                script: "echo_node.ping(1)".to_string(),
                fixture: None,
                timeout_ms: None,
            })
            .await
            .expect("script_eval");
        let payload = result.structured_content.expect("structured content");
        assert_eq!(payload["success"], serde_json::Value::Bool(false));
        assert_eq!(
            payload["state"],
            serde_json::Value::String("failed".to_string())
        );
        assert_eq!(
            payload["error"]["type"],
            serde_json::Value::String("typecheck".to_string())
        );
        assert!(
            payload["diagnostics"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
    }
}
