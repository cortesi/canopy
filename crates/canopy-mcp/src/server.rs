use std::{
    fmt::Display,
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
use tmcp::{Server, ToolError, ToolResult, mcp_server, schema::CallToolResult, tool_params};
use tokio::{net::UnixListener, runtime::Builder, sync::oneshot, task::block_in_place};

use crate::{
    AppFactory, AppMetadata, BootstrapRequest, Error, Result,
    metadata::LiveContext,
    script::{
        ScriptEvalRequest, bootstrap_for_canopy, evaluate_live_request, query_script_api,
        validate_live_viewport,
    },
};

/// Convert an arbitrary error into a tmcp tool error.
fn tool_error(error: impl Display) -> ToolError {
    ToolError::internal(error.to_string())
}

/// Encode a typed payload as an MCP structured result.
fn to_tool_result(value: impl Serialize) -> ToolResult<CallToolResult> {
    CallToolResult::structured(value).map_err(tool_error)
}

/// Minimal stdio MCP server for canopy automation.
#[derive(Clone)]
struct CanopyMcpServer {
    /// Headless evaluator shared by all tool calls.
    evaluator: AppFactory,
}

/// Request payload for applying a named fixture to a live app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[tool_params]
pub struct ApplyFixtureRequest {
    /// Registered fixture name.
    pub name: String,
}

/// Response returned after a fixture is applied to a live app.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ApplyFixtureResponse {
    /// Name of the applied fixture.
    pub applied: String,
}

/// Live MCP server that proxies tool calls onto a running canopy UI thread.
#[derive(Clone)]
struct LiveCanopyMcpServer {
    /// Handle used to marshal work onto the runloop thread.
    automation: AutomationHandle,
    /// One identity and explicit fixture state shared across reconnects.
    context: LiveContext,
}

#[mcp_server]
impl CanopyMcpServer {
    #[tool]
    /// Return the operating guide, generated API, fixtures, availability, and
    /// journal summary.
    async fn bootstrap(&self, params: BootstrapRequest) -> ToolResult<CallToolResult> {
        let bootstrap = self.evaluator.bootstrap(&params).map_err(tool_error)?;
        to_tool_result(bootstrap)
    }

    #[tool]
    /// Evaluate a Luau script against a fresh headless canopy app instance.
    async fn script_eval(&self, params: ScriptEvalRequest) -> ToolResult<CallToolResult> {
        Ok(self.evaluator.evaluate(&params).to_tool_result())
    }

    #[tool(read_only, output_schema = ScriptApiResponse)]
    /// Return shared discovery for the generated app API.
    async fn script_api(&self, params: ScriptApiQuery) -> ToolResult<CallToolResult> {
        let api = self.evaluator.script_api().map_err(tool_error)?;
        script_api_tool_result(query_script_api(api, &params))
    }

    #[tool]
    /// List the application's registered fixtures.
    async fn fixtures(&self) -> ToolResult<CallToolResult> {
        let fixtures = self.evaluator.fixtures().map_err(tool_error)?;
        to_tool_result(fixtures)
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
                let metadata = context.metadata(canopy)?;
                validate_live_viewport(params.viewport, &metadata)?;
                bootstrap_for_canopy(canopy, metadata)
            })
        })
        .map_err(tool_error)?;
        to_tool_result(bootstrap)
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
        .map_err(tool_error)?;
        script_api_tool_result(query_script_api(api, &params))
    }

    #[tool]
    /// List the running app's registered fixtures.
    async fn fixtures(&self) -> ToolResult<CallToolResult> {
        let automation = self.automation.clone();
        let fixtures =
            block_in_place(move || automation.request(|canopy| Ok(canopy.fixture_infos())))
                .map_err(tool_error)?;
        to_tool_result(fixtures)
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
        .map_err(tool_error)?;
        to_tool_result(ApplyFixtureResponse {
            applied: applied_name,
        })
    }
}

/// Map shared discovery to one tmcp result envelope.
fn script_api_tool_result(
    result: result::Result<ScriptApiResponse, ScriptApiError>,
) -> ToolResult<CallToolResult> {
    match result {
        Ok(response) => {
            let structured = serde_json::to_value(&response).map_err(tool_error)?;
            Ok(CallToolResult::new()
                .with_structured_content(structured)
                .with_text_content(response.content))
        }
        Err(error) => {
            let structured = serde_json::to_value(&error).map_err(tool_error)?;
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
/// actions. Calling this function is the application's automation opt-in.
pub fn serve_stdio(factory: AppFactory) -> Result<()> {
    Server::new(move || CanopyMcpServer {
        evaluator: factory.clone(),
    })
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
    metadata: AppMetadata,
) -> Result<UdsServerHandle> {
    let context = LiveContext::new(metadata);
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
                            let _ignored = Server::new(move || LiveCanopyMcpServer {
                                automation: automation.clone(),
                                context: context.clone(),
                            })
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
        Canopy, CanopyBuilder, ContextExt, Fixture, Loader, NodeName, Widget, derive_commands,
        error::Result as CanopyResult, geom::Size, testing::contracts,
    };
    use tokio::net::UnixStream;

    use super::*;
    use crate::metadata::test_app_factory as app_factory;

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
            let error = serve_uds(path, Canopy::new().automation_handle(), AppMetadata::test())
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
            let server = serve_uds(
                &path,
                Canopy::new().automation_handle(),
                AppMetadata::test(),
            )?;
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
        CanopyMcpServer {
            evaluator: app_factory(|| {
                CanopyBuilder::new()
                    .configure(|canopy| {
                        EchoNode::load(canopy)?;
                        canopy.register_fixture(Fixture::new(
                            "seeded",
                            "Set echo_node to a known value",
                            |canopy| canopy.eval_script("echo_node.set(41)").map(|_| ()),
                        ))
                    })
                    .assemble(|canopy| {
                        canopy.replace_root(EchoNode::new())?;
                        Ok(())
                    })
                    .build()
                    .map_err(Into::into)
            }),
        }
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
        assert!(
            !payload["metadata"]["api_digest"]
                .as_str()
                .expect("digest")
                .is_empty()
        );
        assert_eq!(payload["api_sources"][0]["source"], "canopy");
        assert!(
            payload["focus_commands"]
                .as_array()
                .expect("commands")
                .iter()
                .any(|command| command["owner"] == "echo_node" && command["name"] == "ping")
        );
        assert_eq!(payload["fixtures"][0]["name"], "seeded");
    }

    #[tokio::test]
    async fn shared_trace_through_direct_mcp_handler() -> crate::Result<()> {
        let server = CanopyMcpServer {
            evaluator: AppFactory::new(
                AppMetadata {
                    app: "contract".into(),
                    reset: crate::ResetPolicy::Isolated,
                },
                || Ok(contracts::app()?),
            ),
        };
        let response = server
            .script_eval(ScriptEvalRequest {
                viewport: Some(crate::Viewport {
                    width: 12,
                    height: 3,
                }),
                ..ScriptEvalRequest::new(contracts::SCRIPT)
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
                ScriptEvalRequest::new("return echo_node.get()"),
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
            |canopy| canopy.eval_script("echo_node.set(99)").map(|_| ()),
        ))?;
        canopy.finalize_api()?;
        canopy.replace_root(EchoNode::new())?;
        canopy.set_root_size(Size::new(20, 5))?;
        canopy.turn(canopy::Work::Prepare)?;
        let automation = canopy.automation_handle();
        let mut events = canopy.take_event_receiver().expect("test owns events");
        let directory = tempfile::tempdir()?;
        let socket = directory.path().join("metadata.sock");
        let server = serve_uds(
            &socket,
            automation.clone(),
            AppMetadata {
                app: "live-test".into(),
                reset: crate::ResetPolicy::External,
            },
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
            .script_eval(ScriptEvalRequest::new(
                "return echo_node.ping()".to_string(),
            ))
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
                fixture: Some("seeded".to_string()),
                ..ScriptEvalRequest::new("return echo_node.get()")
            })
            .await
            .expect("script_eval");
        let payload = result.structured_content.expect("structured content");
        assert_eq!(payload["success"], serde_json::Value::Bool(true));
        assert_eq!(payload["value"], serde_json::Value::from(41));
    }

    #[tokio::test]
    async fn script_eval_with_many_waits_survives_tokio_coop_budget() {
        // Cross Tokio's cooperative budget with ready host roundtrips while
        // the synchronous evaluator is nested inside a Tokio task.
        let script =
            "for _ = 1, 300 do canopy.wait_for(function() return true end, 50) end return echo_node.ping()"
                .to_string();
        let result = server()
            .script_eval(ScriptEvalRequest {
                timeout_ms: Some(20_000),
                ..ScriptEvalRequest::new(script)
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn script_eval_under_tokio_preserves_execution_timeouts() {
        // Exercise both a parked publication waiter and a busy VM. Neither
        // execution limit may depend on Tokio's cooperative budget.
        for script in [
            "canopy.wait_for(function() return false end, 10000)",
            "while true do end",
        ] {
            let result = server()
                .script_eval(ScriptEvalRequest {
                    timeout_ms: Some(50),
                    ..ScriptEvalRequest::new(script)
                })
                .await
                .expect("script_eval");
            let payload = result.structured_content.expect("structured content");
            assert_eq!(payload["success"], serde_json::Value::Bool(false));
            assert_eq!(payload["state"], "timed_out");
            assert_eq!(payload["error"]["type"], "timeout");
        }
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
            .script_eval(ScriptEvalRequest::new("echo_node.ping(1)".to_string()))
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
        let server = LiveCanopyMcpServer {
            automation: automation.clone(),
            context: LiveContext::new(AppMetadata::test()),
        };
        let worker = thread::spawn(move || {
            let runtime = Builder::new_current_thread().enable_all().build().unwrap();
            runtime.block_on(server.script_eval(ScriptEvalRequest::new("echo_node.signal_started(); canopy.wait_for(function() return echo_node.get() == 7 end); return echo_node.get()".to_string()))).expect("live eval transport")
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
        canopy.turn(Work::Wake)?;
        mutation_rx.recv().expect("native callback ran")?;
        let busy_result = executor::block_on(busy.completion).expect("busy request completed");
        assert!(matches!(
            busy_result.result.as_ref(),
            Err(CanopyError::ScriptStructured {
                kind: ScriptErrorKind::ScriptBusy,
                ..
            })
        ));
        while !worker.is_finished() {
            canopy.turn(Work::Wake)?;
        }
        let response = worker.join().expect("MCP worker exits");
        let payload = response
            .structured_content
            .expect("structured evaluation result");
        assert_eq!(payload["success"], true);
        assert_eq!(payload["value"], 7);
        Ok(())
    }
}
