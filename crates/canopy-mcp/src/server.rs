use std::{
    fs, io,
    os::unix::fs::FileTypeExt,
    path::{Path, PathBuf},
    result,
    sync::mpsc,
    thread,
};

use canopy::AutomationHandle;
use ruau_script_api::{ScriptApiError, ScriptApiQuery, ScriptApiResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tmcp::{Server, ToolError, ToolResult, mcp_server, schema::CallToolResult};
use tokio::{net::UnixListener, runtime::Builder, sync::oneshot, task::block_in_place};

use crate::{
    AppFactory, AppMetadata, BootstrapRequest, Error, LiveContext, Result,
    script::{
        AppEvaluator, ScriptEvalRequest, bootstrap_for_canopy, evaluate_live_request,
        query_script_api, validate_live_viewport,
    },
};

/// Build an MCP tool result with structured and text JSON payloads.
pub fn json_tool_result(value: serde_json::Value) -> CallToolResult {
    let text = value.to_string();
    CallToolResult::new()
        .with_structured_content(value)
        .with_text_content(text)
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
    /// One identity and explicit fixture state shared across reconnects.
    context: LiveContext,
}

/// Construct the live MCP server for a running canopy app.
fn live_canopy_mcp_server(
    automation: AutomationHandle,
    context: LiveContext,
) -> LiveCanopyMcpServer {
    LiveCanopyMcpServer {
        automation,
        context,
    }
}

#[mcp_server]
impl CanopyMcpServer {
    #[tool]
    /// Return the operating guide, generated API, fixtures, availability, and
    /// journal summary.
    async fn bootstrap(&self, params: BootstrapRequest) -> ToolResult<CallToolResult> {
        let bootstrap = self
            .evaluator
            .bootstrap_with_request(&params)
            .map_err(|error| ToolError::internal(error.to_string()))?;
        let value = serde_json::to_value(bootstrap)
            .map_err(|error| ToolError::internal(error.to_string()))?;
        Ok(json_tool_result(value))
    }

    #[tool]
    /// Evaluate a Luau script against a fresh headless canopy app instance.
    async fn script_eval(&self, params: ScriptEvalRequest) -> ToolResult<CallToolResult> {
        Ok(self.evaluator.evaluate(&params).to_tool_result())
    }

    #[tool(read_only, output_schema = ScriptApiResponse)]
    /// Return shared discovery for the generated app API.
    async fn script_api(&self, params: ScriptApiQuery) -> ToolResult<CallToolResult> {
        let api = self
            .evaluator
            .script_api()
            .map_err(|error| ToolError::internal(error.to_string()))?;
        script_api_tool_result(query_script_api(api, &params))
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
        Ok(json_tool_result(value))
    }
}

#[mcp_server]
impl LiveCanopyMcpServer {
    #[tool]
    /// Return the operating guide, generated API, fixtures, availability, and
    /// journal summary.
    async fn bootstrap(&self, params: BootstrapRequest) -> ToolResult<CallToolResult> {
        let automation = self.automation.clone();
        let context = self.context.clone();
        let bootstrap = block_in_place(move || {
            automation.request(move |canopy| {
                canopy.finalize_api()?;
                let metadata = context.metadata(canopy)?;
                validate_live_viewport(params.viewport, &metadata)?;
                bootstrap_for_canopy(canopy, metadata)
            })
        })
        .map_err(|error| ToolError::internal(error.to_string()))?;
        let value = serde_json::to_value(bootstrap)
            .map_err(|error| ToolError::internal(error.to_string()))?;
        Ok(json_tool_result(value))
    }

    #[tool]
    /// Evaluate a Luau script against the currently running canopy app.
    async fn script_eval(&self, params: ScriptEvalRequest) -> ToolResult<CallToolResult> {
        let outcome =
            evaluate_live_request(self.automation.clone(), params, self.context.clone()).await;
        Ok(outcome.to_tool_result())
    }

    #[tool(read_only, output_schema = ScriptApiResponse)]
    /// Return shared discovery for the generated running-app API.
    async fn script_api(&self, params: ScriptApiQuery) -> ToolResult<CallToolResult> {
        let automation = self.automation.clone();
        let api = block_in_place(move || {
            automation.request(|canopy| canopy.script_api().map(str::to_string))
        })
        .map_err(|error| ToolError::internal(error.to_string()))?;
        script_api_tool_result(query_script_api(api, &params))
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
        Ok(json_tool_result(value))
    }

    #[tool]
    /// Apply a named fixture to the running app and trigger a re-render.
    async fn apply_fixture(&self, params: ApplyFixtureRequest) -> ToolResult<CallToolResult> {
        let name = params.name;
        let applied_name = name.clone();
        let automation = self.automation.clone();
        let context = self.context.clone();
        block_in_place(move || {
            automation.request(move |canopy| {
                canopy.apply_fixture(&name)?;
                context.fixture_applied();
                Ok(())
            })
        })
        .map_err(|error| ToolError::internal(error.to_string()))?;
        let value = json!({ "applied": applied_name });
        Ok(json_tool_result(value))
    }
}

/// Map shared discovery to one tmcp result envelope.
fn script_api_tool_result(
    result: result::Result<ScriptApiResponse, ScriptApiError>,
) -> ToolResult<CallToolResult> {
    match result {
        Ok(response) => {
            let structured = serde_json::to_value(&response)
                .map_err(|error| ToolError::internal(error.to_string()))?;
            Ok(CallToolResult::new()
                .with_structured_content(structured)
                .with_text_content(response.content))
        }
        Err(error) => {
            let structured = serde_json::to_value(&error)
                .map_err(|error| ToolError::internal(error.to_string()))?;
            Ok(CallToolResult::new()
                .with_is_error(true)
                .with_structured_content(structured)
                .with_text_content(error.message))
        }
    }
}

/// Serve `bootstrap`, `script_eval`, `script_api`, and `fixtures` over stdio
/// for an app factory.
/// This low-level entry point grants trusted-local access to all exposed native
/// actions. Use `launch_with_options` to enforce an application launch policy.
pub fn serve_stdio(factory: AppFactory) -> Result<()> {
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
        self.shutdown()
    }

    /// Signal the listener, join its thread, and remove the socket file.
    fn shutdown(&mut self) -> Result<()> {
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
        let _ignored = self.shutdown();
    }
}

/// Serve live MCP automation for a running canopy app over a Unix-domain
/// socket.
/// This low-level entry point grants trusted-local access. The application must
/// choose a private socket directory and enforce suitable host filesystem
/// permissions.
pub fn serve_uds(
    socket_path: impl AsRef<Path>,
    automation: AutomationHandle,
) -> Result<UdsServerHandle> {
    serve_uds_with_context(
        socket_path,
        automation,
        LiveContext::new(AppMetadata::default()),
    )
}

/// Serve a live application with an explicit identity retained across
/// reconnects.
pub fn serve_uds_with_context(
    socket_path: impl AsRef<Path>,
    automation: AutomationHandle,
    context: LiveContext,
) -> Result<UdsServerHandle> {
    let socket_path = socket_path.as_ref().to_path_buf();
    match fs::symlink_metadata(&socket_path) {
        Ok(metadata) if metadata.file_type().is_socket() => fs::remove_file(&socket_path)?,
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("MCP socket path is not a socket: {}", socket_path.display()),
            )
            .into());
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
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
                        let context = context.clone();
                        tokio::spawn(async move {
                            let (reader, writer) = stream.into_split();
                            let _ignored = Server::new(move || live_canopy_mcp_server(automation.clone(), context.clone()))
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
    use std::{
        os::unix::{fs::symlink, net::UnixListener as StdUnixListener},
        panic::catch_unwind,
        path::Path,
    };

    use canopy::{
        Fixture, command, derive_commands, error::Result as CanopyResult, geom::Size, prelude::*,
        testing::contracts,
    };
    use tokio::net::UnixStream;

    use super::*;
    use crate::app_factory;

    #[test]
    fn uds_preserves_non_socket_paths() -> crate::Result<()> {
        let directory = tempfile::tempdir()?;
        let file = directory.path().join("data");
        fs::write(&file, "preserve me")?;
        let link = directory.path().join("link");
        symlink(&file, &link)?;
        let dangling = directory.path().join("dangling");
        symlink(directory.path().join("absent"), &dangling)?;
        let folder = directory.path().join("folder");
        fs::create_dir(&folder)?;
        for path in [&file, &link, &dangling, &folder] {
            let error = serve_uds(path, Canopy::new().automation_handle())
                .err()
                .expect("reject non-socket");
            assert!(
                matches!(error, crate::Error::Io(error) if error.kind() == io::ErrorKind::AlreadyExists)
            );
        }
        assert_eq!(fs::read_to_string(&file)?, "preserve me");
        assert_eq!(fs::read_link(&link)?, file);
        assert_eq!(fs::read_link(&dangling)?, directory.path().join("absent"));
        Ok(())
    }

    #[test]
    fn uds_starts_new_and_replaces_stale_socket() -> crate::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("app.sock");
        for stale in [false, true] {
            if stale {
                drop(StdUnixListener::bind(&path)?);
            }
            let server = serve_uds(&path, Canopy::new().automation_handle())?;
            assert!(fs::symlink_metadata(&path)?.file_type().is_socket());
            server.stop()?;
            assert!(!path.exists());
        }
        Ok(())
    }

    struct EchoNode {
        value: i32,
        started: Option<mpsc::Sender<()>>,
    }

    #[derive_commands]
    impl EchoNode {
        fn new() -> Self {
            Self {
                value: 0,
                started: None,
            }
        }

        #[command]
        fn signal_started(&self) {
            if let Some(started) = &self.started {
                started.send(()).expect("test observer remains connected");
            }
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
    async fn script_api_returns_shared_dynamic_discovery() {
        let server = server();
        let overview = server
            .script_api(ScriptApiQuery::default())
            .await
            .expect("overview");
        assert_eq!(overview.structured_content.unwrap()["mode"], "overview");

        let listed = server
            .script_api(ScriptApiQuery {
                list: true,
                filter: None,
            })
            .await
            .expect("list");
        assert!(listed.text().unwrap().contains("echo_node.ping"));

        let detail = server
            .script_api(ScriptApiQuery {
                list: false,
                filter: Some("echo_node.ping".to_owned()),
            })
            .await
            .expect("detail");
        assert_eq!(detail.structured_content.unwrap()["mode"], "detail");

        let missing = server
            .script_api(ScriptApiQuery {
                list: false,
                filter: Some("missing-path".to_owned()),
            })
            .await
            .expect("missing result");
        assert!(missing.is_error());
        assert_eq!(missing.structured_content.unwrap()["kind"], "not_found");
    }

    #[tokio::test]
    async fn bootstrap_returns_digest_inventory_and_availability() {
        let result = server()
            .bootstrap(BootstrapRequest::default())
            .await
            .expect("bootstrap");
        let payload = result.structured_content.expect("structured content");
        assert!(payload.get("api").is_none());
        assert!(!payload["api_digest"].as_str().expect("digest").is_empty());
        assert_eq!(payload["api_sources"][0]["source"], "canopy");
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
    async fn shared_trace_through_direct_mcp_handler() -> crate::Result<()> {
        let server = canopy_mcp_server(app_factory(|| Ok(contracts::app()?)).with_metadata(
            AppMetadata {
                app: "contract".into(),
                reset: crate::ResetPolicy::Isolated,
            },
        ));
        let response = server
            .script_eval(ScriptEvalRequest {
                script: contracts::SCRIPT.into(),
                fixture: None,
                timeout_ms: None,
                viewport: Some(crate::Viewport {
                    width: 12,
                    height: 3,
                }),
            })
            .await
            .expect("direct MCP evaluation");
        let outcome: crate::ScriptEvalOutcome =
            serde_json::from_value(response.structured_content.expect("structured result"))
                .expect("typed eval outcome");
        assert!(outcome.success, "{outcome:?}");
        assert_eq!(
            outcome.value,
            Some(contracts::expected().to_external_json_value()?)
        );
        assert_eq!(outcome.metadata.app, "contract");
        Ok(())
    }

    /// Connect one real MCP client to the test listener.
    async fn live_client(path: &Path) -> crate::Result<tmcp::Client<()>> {
        let stream = UnixStream::connect(path).await?;
        let (reader, writer) = stream.into_split();
        let mut client = tmcp::Client::new("metadata-test", "1");
        client.connect_stream(reader, writer).await?;
        Ok(client)
    }

    /// Exercise successful and failed fixtures, then reconnect to the same
    /// listener.
    async fn reconnect_metadata_trace(path: &Path) -> crate::Result<()> {
        let client = live_client(path).await?;
        let before: crate::BootstrapResponse = client
            .call_tool_structured("bootstrap", BootstrapRequest::default())
            .await?;
        assert_eq!(before.metadata.reset, crate::ResetPolicy::External);
        let missing = client
            .call_tool(
                "apply_fixture",
                ApplyFixtureRequest {
                    name: "missing".into(),
                },
            )
            .await?;
        assert!(missing.is_error());
        let unchanged: crate::BootstrapResponse = client
            .call_tool_structured("bootstrap", BootstrapRequest::default())
            .await?;
        assert_eq!(unchanged.metadata, before.metadata);
        let recursive = client
            .call_tool(
                "apply_fixture",
                ApplyFixtureRequest {
                    name: "scripted".into(),
                },
            )
            .await?;
        assert!(recursive.is_error());
        assert!(
            recursive
                .error_message()
                .unwrap_or_default()
                .contains("another runtime turn or evaluation is active")
        );
        let still_external: crate::BootstrapResponse = client
            .call_tool_structured("bootstrap", BootstrapRequest::default())
            .await?;
        assert_eq!(still_external.metadata, before.metadata);
        let applied = client
            .call_tool(
                "apply_fixture",
                ApplyFixtureRequest {
                    name: "seeded".into(),
                },
            )
            .await?;
        assert!(
            !applied.is_error(),
            "fixture application failed: {applied:?}"
        );
        let outcome: crate::ScriptEvalOutcome = client
            .call_tool_structured(
                "script_eval",
                ScriptEvalRequest {
                    script: "return echo_node.get()".into(),
                    fixture: None,
                    timeout_ms: None,
                    viewport: None,
                },
            )
            .await?;
        assert!(outcome.success);
        assert_eq!(outcome.value, Some(serde_json::Value::from(7)));
        assert_eq!(outcome.metadata.reset, crate::ResetPolicy::Fixture);
        assert_eq!(outcome.metadata.session_id, before.metadata.session_id);
        drop(client);
        let reconnected = live_client(path).await?;
        let after: crate::BootstrapResponse = reconnected
            .call_tool_structured("bootstrap", BootstrapRequest::default())
            .await?;
        assert_eq!(after.metadata.session_id, before.metadata.session_id);
        assert_eq!(after.metadata.reset, crate::ResetPolicy::Fixture);
        assert_eq!(after.metadata.execution, crate::ExecutionMode::LiveSession);
        Ok(())
    }

    #[test]
    fn live_metadata_survives_real_socket_reconnects() -> crate::Result<()> {
        use futures::{StreamExt, executor};
        let mut canopy = Canopy::new();
        EchoNode::load(&mut canopy)?;
        canopy.register_fixture(Fixture::new(
            "seeded",
            "Set persistent live state",
            |canopy| {
                canopy.with_root_context(|ctx| {
                    ctx.dispatch_exact(ctx.node_id(), &EchoNode::call_set(7).invocation())?;
                    Ok(())
                })
            },
        ))?;
        canopy.register_fixture(Fixture::new(
            "scripted",
            "Attempt recursive top-level evaluation",
            |canopy| canopy.eval_script("echo_node.set(99)"),
        ))?;
        canopy.finalize_api()?;
        canopy.replace_root(EchoNode::new())?;
        canopy.set_root_size(Size::new(20, 5))?;
        canopy.turn(canopy::Work::Prepare)?;
        let automation = canopy.automation_handle();
        let mut events = canopy.take_event_receiver().expect("test owns events");
        let directory = tempfile::tempdir()?;
        let socket = directory.path().join("metadata.sock");
        let server = serve_uds_with_context(
            &socket,
            automation.clone(),
            LiveContext::new(AppMetadata {
                app: "live-test".into(),
                reset: crate::ResetPolicy::External,
            }),
        )?;
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result = catch_unwind(|| {
                Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(crate::Error::from)
                    .and_then(|runtime| runtime.block_on(reconnect_metadata_trace(&socket)))
            });
            done_tx.send(result).expect("test observer connected");
            automation
                .submit(Box::new(|_| {}))
                .expect("wake test observer");
        });
        let result = loop {
            if let Ok(result) = done_rx.try_recv() {
                break result;
            }
            executor::block_on(events.next()).expect("MCP work wakes UI");
            canopy.turn(canopy::Work::Wake)?;
        };
        worker.join().expect("MCP client worker");
        server.stop()?;
        result.expect("live transport metadata assertions")
    }

    #[tokio::test]
    async fn script_eval_returns_json_payload() {
        let result = server()
            .script_eval(ScriptEvalRequest {
                script: "return echo_node.ping()".to_string(),
                fixture: None,
                timeout_ms: None,
                viewport: None,
            })
            .await
            .expect("script_eval");
        let payload = result.structured_content.expect("structured content");
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
                viewport: None,
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
                viewport: None,
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
    #[test]
    fn live_pending_eval_allows_native_progress_and_reports_busy() -> crate::Result<()> {
        use canopy::{
            EvalRequest, Work,
            commands::ArgValue,
            error::{Error as CanopyError, ScriptErrorKind},
            geom::Size,
        };
        use futures::{StreamExt, executor};

        let mut canopy = Canopy::new();
        EchoNode::load(&mut canopy)?;
        canopy.finalize_api()?;
        let (started_tx, started_rx) = mpsc::channel();
        canopy.replace_root(EchoNode {
            value: 0,
            started: Some(started_tx),
        })?;
        canopy.set_root_size(Size::new(20, 5))?;
        canopy.turn(Work::Prepare)?;
        let automation = canopy.automation_handle();
        let mut events = canopy
            .take_event_receiver()
            .expect("test owns event receiver");
        let server =
            live_canopy_mcp_server(automation.clone(), LiveContext::new(AppMetadata::default()));
        let worker = thread::spawn(move || {
            let runtime = Builder::new_current_thread().enable_all().build().unwrap();
            runtime.block_on(server.script_eval(ScriptEvalRequest {
                script: "echo_node.signal_started(); canopy.wait_for(function() return echo_node.get() == 7 end); return echo_node.get()".to_string(),
                fixture: None, timeout_ms: None, viewport: None,
            })).expect("live eval transport")
        });
        while started_rx.try_recv().is_err() {
            executor::block_on(events.next()).expect("queued work wakes the UI");
            let outcome = canopy.turn(Work::Wake)?;
            assert!(
                outcome.completed.is_empty(),
                "wait must remain pending before mutation"
            );
        }
        assert!(
            !worker.is_finished(),
            "MCP must await the pending runtime ticket"
        );
        let busy = automation.submit_eval(EvalRequest {
            source: "return 99".to_string(),
            timeout: None,
            anchor: canopy.root_id(),
        })?;
        let (mutation_tx, mutation_rx) = mpsc::channel();
        automation.submit(Box::new(move |canopy| {
            let result = canopy.with_root_context(|ctx| {
                ctx.dispatch_exact(ctx.node_id(), &EchoNode::call_set(7).invocation())?;
                Ok(())
            });
            mutation_tx
                .send(result)
                .expect("mutation observer connected");
        }))?;
        executor::block_on(events.next()).expect("native mutation wakes the UI");
        let mut outcome = canopy.turn(Work::Wake)?;
        mutation_rx.recv().expect("native callback ran")?;
        let busy_result = executor::block_on(busy.completion).expect("busy request completed");
        assert!(matches!(
            busy_result.result.as_ref(),
            Err(CanopyError::ScriptStructured {
                kind: ScriptErrorKind::ScriptBusy,
                ..
            })
        ));
        while outcome.completed.is_empty() {
            executor::block_on(events.next()).expect("publication wakes the waiting script");
            outcome = canopy.turn(Work::Wake)?;
        }
        assert!(matches!(
            outcome.completed[0].result.as_ref(),
            Ok(ArgValue::Int(7))
        ));
        let response = worker.join().expect("MCP worker exits");
        let payload = response
            .structured_content
            .expect("structured evaluation result");
        assert_eq!(payload["success"], true);
        assert_eq!(payload["value"], 7);
        Ok(())
    }
}
