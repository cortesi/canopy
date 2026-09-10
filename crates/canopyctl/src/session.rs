//! MCP client sessions and the manager shared by the CLI and the proxy server.

use std::path::Path;

use anyhow::{Context, Result, bail};
use canopy::FixtureInfo;
use canopy_mcp::{
    ApplyFixtureRequest, ApplyFixtureResponse, BootstrapRequest, BootstrapResponse,
    ScriptEvalOutcome, ScriptEvalRequest,
};
use ruau_script_api::ScriptApiQuery;
use tmcp::{
    Client,
    schema::{CallToolResult, ToolResultMode},
};
use tokio::{
    net::UnixStream,
    process::Child,
    sync::{MappedMutexGuard, Mutex, MutexGuard},
};

use crate::config::{LoadedConfig, ResolvedCommand};

/// MCP client name reported to spawned or connected servers.
const CLIENT_NAME: &str = "canopyctl";
/// MCP client version reported to spawned or connected servers.
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Active headless or live MCP client session.
pub struct Session {
    /// Connected MCP client.
    client: Client<()>,
    /// Managed headless child process, when applicable.
    child: Option<Child>,
    /// Session kind.
    kind: SessionKind,
    /// Default fixture applied to future headless evals.
    default_fixture: Option<String>,
}

/// Session mode tracked by `canopyctl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    /// Auto-spawned headless stdio MCP session.
    Headless,
    /// Connected live UDS MCP session.
    Live,
}

impl Session {
    /// Connect to a live UDS MCP server.
    pub async fn connect_live(socket: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket)
            .await
            .with_context(|| format!("connect to {}", socket.display()))?;
        let (reader, writer) = stream.into_split();
        let mut client = Client::new(CLIENT_NAME, CLIENT_VERSION);
        client.connect_stream(reader, writer).await?;
        Ok(Self {
            client,
            child: None,
            kind: SessionKind::Live,
            default_fixture: None,
        })
    }

    /// Spawn a headless stdio MCP server and connect to it.
    pub async fn spawn_headless(command: &ResolvedCommand) -> Result<Self> {
        let mut client = Client::new(CLIENT_NAME, CLIENT_VERSION);
        let spawned = client.connect_process(command.to_command()).await?;
        Ok(Self {
            client,
            child: Some(spawned.process),
            kind: SessionKind::Headless,
            default_fixture: None,
        })
    }

    /// Shut down the session and any managed child process.
    pub async fn shutdown(mut self) {
        if let Some(mut child) = self.child.take() {
            let _ignored = child.kill().await;
            let _ignored = child.wait().await;
        }
    }

    /// Evaluate one Luau script through the session.
    pub async fn eval(&self, mut request: ScriptEvalRequest) -> Result<ScriptEvalOutcome> {
        if self.kind == SessionKind::Headless && request.fixture.is_none() {
            request.fixture = self.default_fixture.clone();
        }
        let result = self.client.call_tool("script_eval", request).await?;
        result
            .extract_as::<ScriptEvalOutcome>(ToolResultMode::Structured)
            .context("decode structured script_eval outcome")
    }

    /// Request shared API discovery.
    pub async fn api(&self, query: &ScriptApiQuery) -> Result<CallToolResult> {
        Ok(self.client.call_tool("script_api", query).await?)
    }

    /// Return the explicitly selected transport mode.
    pub fn kind(&self) -> SessionKind {
        self.kind
    }

    /// Request bootstrap information at an optional headless viewport.
    pub async fn bootstrap(&self, request: BootstrapRequest) -> Result<BootstrapResponse> {
        Ok(self
            .client
            .call_tool_structured("bootstrap", request)
            .await?)
    }

    /// Request the fixture catalog.
    pub async fn fixtures(&self) -> Result<Vec<FixtureInfo>> {
        Ok(self.client.call_tool_structured("fixtures", ()).await?)
    }

    /// Apply or remember a fixture for the session.
    pub async fn apply_fixture(&mut self, name: String) -> Result<()> {
        match self.kind {
            SessionKind::Live => {
                let result = self
                    .client
                    .call_tool("apply_fixture", ApplyFixtureRequest { name })
                    .await?;
                if result.is_error() {
                    bail!(
                        "{}",
                        result
                            .error_message()
                            .unwrap_or_else(|| "apply_fixture failed".to_owned())
                    );
                }
                let _: ApplyFixtureResponse = result
                    .extract_as(ToolResultMode::Structured)
                    .context("decode structured apply_fixture response")?;
            }
            SessionKind::Headless => {
                self.default_fixture = Some(name);
            }
        }
        Ok(())
    }
}

/// Shared session manager used by the CLI and proxy MCP server.
pub struct SessionManager {
    /// Loaded CLI configuration.
    config: LoadedConfig,
    /// Current session, if any.
    state: Mutex<Option<Session>>,
}

impl SessionManager {
    /// Construct a new session manager from loaded config.
    pub fn new(config: LoadedConfig) -> Self {
        Self {
            config,
            state: Mutex::new(None),
        }
    }

    /// Connect to a live UDS session, replacing any existing session.
    pub async fn connect_live(&self, socket: &Path) -> Result<()> {
        let mut state = self.state.lock().await;
        let previous = state.take();
        if let Some(previous) = previous {
            previous.shutdown().await;
        }
        let session = Session::connect_live(socket).await?;
        *state = Some(session);
        Ok(())
    }

    /// Drop and shut down the current session, if any.
    pub async fn disconnect(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        if let Some(session) = state.take() {
            session.shutdown().await;
        }
        Ok(())
    }

    /// Evaluate a script on the active session.
    pub async fn eval(&self, request: ScriptEvalRequest) -> Result<ScriptEvalOutcome> {
        self.session().await?.eval(request).await
    }

    /// Request API discovery on the active session.
    pub async fn api(&self, query: &ScriptApiQuery) -> Result<CallToolResult> {
        self.session().await?.api(query).await
    }

    /// Request bootstrap information on the active session.
    pub async fn bootstrap(&self, request: BootstrapRequest) -> Result<BootstrapResponse> {
        self.session().await?.bootstrap(request).await
    }

    /// Request the fixture catalog on the active session.
    pub async fn fixtures(&self) -> Result<Vec<FixtureInfo>> {
        self.session().await?.fixtures().await
    }

    /// Apply a fixture on the active session.
    pub async fn apply_fixture(&self, name: String) -> Result<()> {
        self.session().await?.apply_fixture(name).await
    }

    /// Lock the active session, spawning a headless one when none is connected.
    async fn session(&self) -> Result<MappedMutexGuard<'_, Session>> {
        let mut state = self.state.lock().await;
        if state.is_none() {
            let command = self.config.headless_command(&[])?;
            *state = Some(Session::spawn_headless(&command).await?);
        }
        Ok(MutexGuard::map(state, |state| {
            state.as_mut().expect("session installed above")
        }))
    }
}

#[cfg(test)]
pub mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use canopy::testing::contracts;
    use canopy_mcp::{
        AppFactory, AppMetadata, ExecutionMetadata, ExecutionMode, ResetPolicy, ScriptErrorInfo,
        ScriptErrorType, ScriptTaskState, ScriptTiming, Viewport,
    };
    use serde_json::{Value, json};
    use tmcp::{Server, ToolError, ToolResult, mcp_server};
    use tokio::{
        io::{duplex, split},
        net::UnixListener,
        sync::oneshot,
        task::{JoinHandle, block_in_place},
    };

    use super::*;

    /// Real evaluator exposed over MCP for cross-adapter contract tests.
    #[derive(Clone)]
    struct EvaluatorPeer {
        /// The same public evaluator used by headless MCP applications.
        evaluator: AppFactory,
        /// Sources observed at the transport boundary.
        calls: Arc<Mutex<Vec<String>>>,
    }

    #[mcp_server]
    impl EvaluatorPeer {
        #[tool]
        async fn script_eval(&self, request: ScriptEvalRequest) -> ToolResult<CallToolResult> {
            self.calls.lock().await.push(request.script.clone());
            Ok(block_in_place(|| self.evaluator.evaluate(&request)).to_tool_result())
        }

        #[tool]
        async fn bootstrap(&self, request: BootstrapRequest) -> ToolResult<CallToolResult> {
            let response = block_in_place(|| self.evaluator.bootstrap_with_request(&request))
                .map_err(|error| ToolError::internal(error.to_string()))?;
            CallToolResult::structured(response)
                .map_err(|error| ToolError::internal(error.to_string()))
        }

        #[tool]
        async fn fixtures(&self) -> ToolResult<CallToolResult> {
            let fixtures = self
                .evaluator
                .fixtures()
                .map_err(|error| ToolError::internal(error.to_string()))?;
            CallToolResult::structured(fixtures)
                .map_err(|error| ToolError::internal(error.to_string()))
        }
    }

    /// Connect a real fresh-app evaluator through an in-memory MCP transport.
    pub async fn evaluator_session() -> Result<(Session, Arc<Mutex<Vec<String>>>, JoinHandle<()>)> {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let peer = EvaluatorPeer {
            evaluator: AppFactory::new(
                AppMetadata {
                    app: "canopyctl-test".into(),
                    reset: ResetPolicy::Isolated,
                },
                || contracts::app().map_err(Into::into),
            ),
            calls: Arc::clone(&calls),
        };
        let (client_stream, server_stream) = duplex(65536);
        let task = tokio::spawn(async move {
            let (reader, writer) = split(server_stream);
            Server::new(move || peer.clone())
                .serve_stream(reader, writer)
                .await
                .expect("evaluator transport");
        });
        let (reader, writer) = split(client_stream);
        let mut client = Client::new(CLIENT_NAME, CLIENT_VERSION);
        client.connect_stream(reader, writer).await?;
        Ok((
            Session {
                client,
                child: None,
                kind: SessionKind::Headless,
                default_fixture: None,
            },
            calls,
            task,
        ))
    }

    #[derive(Clone, Default)]
    struct Peer {
        calls: Arc<Mutex<Vec<String>>>,
    }

    #[mcp_server]
    impl Peer {
        #[tool]
        async fn script_eval(&self, request: ScriptEvalRequest) -> ToolResult<CallToolResult> {
            self.calls.lock().await.push(request.script.clone());
            match request.script.as_str() {
                "missing" => return Ok(CallToolResult::new()),
                "malformed" => {
                    return Ok(
                        CallToolResult::new().with_structured_content(json!({"success": "wrong"}))
                    );
                }
                _ => {}
            }
            let state = match request.script.as_str() {
                "failure" => ScriptTaskState::Failed,
                "timeout" => ScriptTaskState::TimedOut,
                _ => ScriptTaskState::Completed,
            };
            let success = state == ScriptTaskState::Completed;
            let outcome = ScriptEvalOutcome {
                metadata: ExecutionMetadata {
                    app: "test".into(),
                    execution: ExecutionMode::LiveSession,
                    session_id: "test-session".into(),
                    viewport: Some(Viewport::default()),
                    reset: ResetPolicy::External,
                    api_digest: Some("test-digest".into()),
                },
                success,
                state,
                value: Some(request.fixture.map_or(Value::Null, Value::String)),
                logs: Vec::new(),
                assertions: Vec::new(),
                diagnostics: Vec::new(),
                timing: ScriptTiming::default(),
                error: (!success).then(|| ScriptErrorInfo {
                    error_type: if state == ScriptTaskState::TimedOut {
                        ScriptErrorType::Timeout
                    } else {
                        ScriptErrorType::Runtime
                    },
                    kind: None,
                    command: None,
                    owner: None,
                    message: "script rejected".to_owned(),
                }),
            };
            Ok(outcome.to_tool_result())
        }

        #[tool]
        async fn apply_fixture(&self, request: ApplyFixtureRequest) -> ToolResult<CallToolResult> {
            Ok(if request.name == "bad" {
                CallToolResult::new()
                    .with_is_error(true)
                    .with_text_content("fixture rejected")
            } else {
                CallToolResult::structured(ApplyFixtureResponse {
                    applied: request.name,
                })
                .map_err(|error| ToolError::internal(error.to_string()))?
            })
        }
    }

    pub async fn peer_session() -> Result<(Session, Arc<Mutex<Vec<String>>>, JoinHandle<()>)> {
        let peer = Peer::default();
        let calls = peer.calls.clone();
        let (client_stream, server_stream) = duplex(8192);
        let task = tokio::spawn(async move {
            let (reader, writer) = split(server_stream);
            Server::new(move || peer.clone())
                .serve_stream(reader, writer)
                .await
                .expect("peer transport");
        });
        let (reader, writer) = split(client_stream);
        let mut client = Client::new(CLIENT_NAME, CLIENT_VERSION);
        client.connect_stream(reader, writer).await?;
        Ok((
            Session {
                client,
                child: None,
                kind: SessionKind::Live,
                default_fixture: None,
            },
            calls,
            task,
        ))
    }

    pub fn request(script: &str) -> ScriptEvalRequest {
        ScriptEvalRequest::new(script.to_owned())
    }

    pub async fn manager_with_session(session: Session) -> Result<Arc<SessionManager>> {
        let manager = Arc::new(SessionManager::new(LoadedConfig::load()?));
        *manager.state.lock().await = Some(session);
        Ok(manager)
    }

    #[tokio::test]
    async fn eval_decodes_outcomes_including_error_envelopes() -> Result<()> {
        let (session, _, peer) = peer_session().await?;
        for (script, state) in [
            ("success", ScriptTaskState::Completed),
            ("failure", ScriptTaskState::Failed),
            ("timeout", ScriptTaskState::TimedOut),
        ] {
            let outcome = session.eval(request(script)).await?;
            assert_eq!(outcome.state, state);
            assert_eq!(outcome.success, state == ScriptTaskState::Completed);
        }
        for script in ["missing", "malformed"] {
            let error = session.eval(request(script)).await.unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("decode structured script_eval outcome")
            );
        }
        peer.abort();
        Ok(())
    }

    #[tokio::test]
    async fn fixture_acknowledgments_and_headless_defaults() -> Result<()> {
        let (mut session, _, peer) = peer_session().await?;
        session.apply_fixture("good".to_owned()).await?;
        assert!(
            session
                .apply_fixture("bad".to_owned())
                .await
                .unwrap_err()
                .to_string()
                .contains("fixture rejected")
        );
        session.kind = SessionKind::Headless;
        session.apply_fixture("default".to_owned()).await?;
        assert_eq!(
            session.eval(request("success")).await?.value,
            Some(json!("default"))
        );
        let mut explicit = request("success");
        explicit.fixture = Some("explicit".to_owned());
        assert_eq!(session.eval(explicit).await?.value, Some(json!("explicit")));
        peer.abort();
        Ok(())
    }

    #[tokio::test]
    async fn failed_connection_leaves_the_manager_empty() -> Result<()> {
        let (session, _, peer) = peer_session().await?;
        let manager = manager_with_session(session).await?;
        let directory = tempfile::tempdir()?;
        assert!(
            manager
                .connect_live(&directory.path().join("missing.sock"))
                .await
                .is_err()
        );
        assert!(manager.state.lock().await.is_none());
        peer.abort();
        Ok(())
    }

    #[tokio::test]
    async fn connect_serializes_eval_and_disconnect_until_handshake_finishes() -> Result<()> {
        for disconnect in [false, true] {
            let directory = tempfile::tempdir()?;
            let socket = directory.path().join("peer.sock");
            let listener = UnixListener::bind(&socket)?;
            let (accepted_tx, accepted_rx) = oneshot::channel();
            let (release_tx, release_rx) = oneshot::channel();
            let peer = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.expect("accept");
                accepted_tx.send(()).expect("accepted signal");
                release_rx.await.expect("release handshake");
                let (reader, writer) = stream.into_split();
                Server::new(Peer::default)
                    .serve_stream(reader, writer)
                    .await
                    .expect("serve");
            });
            let manager = Arc::new(SessionManager::new(LoadedConfig::load()?));
            let connecting_manager = manager.clone();
            let connecting =
                tokio::spawn(async move { connecting_manager.connect_live(&socket).await });
            accepted_rx.await?;
            assert!(
                manager.state.try_lock().is_err(),
                "connection must own the transition lock"
            );
            let finished = Arc::new(AtomicBool::new(false));
            let operation_manager = manager.clone();
            let operation_finished = finished.clone();
            let (started_tx, started_rx) = oneshot::channel();
            let operation = tokio::spawn(async move {
                started_tx.send(()).expect("operation started");
                if disconnect {
                    operation_manager.disconnect().await?;
                } else {
                    assert!(operation_manager.eval(request("success")).await?.success);
                }
                operation_finished.store(true, Ordering::SeqCst);
                Ok::<_, anyhow::Error>(())
            });
            started_rx.await?;
            assert!(!finished.load(Ordering::SeqCst));
            release_tx.send(()).expect("release");
            connecting.await??;
            operation.await??;
            assert_eq!(manager.state.lock().await.is_none(), disconnect);
            manager.disconnect().await?;
            peer.abort();
        }
        Ok(())
    }
}
