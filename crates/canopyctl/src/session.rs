//! MCP client sessions and the manager shared by the CLI and the proxy server.

use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard, PoisonError},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use canopy_mcp::{
    ApplyFixtureRequest, ApplyFixtureResponse, BootstrapRequest, BootstrapResponse, EvalReport,
    ScreenSize, ScriptEvalRequest,
};
use canopy_term::TermHost;
use ruau_script_api::ScriptApiQuery;
use tmcp::{
    Client,
    schema::{CallToolResult, ToolResultMode},
};
use tokio::{
    net::UnixStream,
    process::Child,
    sync::{MappedMutexGuard, Mutex, MutexGuard, Notify},
    time::{Instant, sleep_until},
};

use crate::{
    config::{LoadedConfig, ResolvedCommand},
    term::{self, Read},
};

/// MCP client name reported to spawned or connected servers.
const CLIENT_NAME: &str = "canopyctl";
/// MCP client version reported to spawned or connected servers.
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Active headless or live MCP client session.
pub struct Session {
    /// Connected MCP client, which a hosted app's terminal also uses.
    client: Arc<Client<()>>,
    /// What the client talks to, and the data that only it has.
    transport: Transport,
}

/// What a session's client talks to.
enum Transport {
    /// A headless stdio MCP server, which builds a fresh app for each eval.
    Headless {
        /// The server's process; `None` for an in-process test peer that
        /// speaks the headless protocol without a process.
        child: Option<Child>,
        /// Default fixture applied to future evals.
        default_fixture: Option<String>,
    },
    /// A live app's UDS MCP server.
    Live,
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
        Ok(Self::share_live(Arc::new(client)))
    }

    /// Use a live client that another owner connected, such as the terminal
    /// hosting an app.
    pub fn share_live(client: Arc<Client<()>>) -> Self {
        Self {
            client,
            transport: Transport::Live,
        }
    }

    /// Spawn a headless stdio MCP server and connect to it.
    pub async fn spawn_headless(command: &ResolvedCommand) -> Result<Self> {
        let mut client = Client::new(CLIENT_NAME, CLIENT_VERSION);
        let mut process = command.to_command();
        // A session that drops without shutdown() still ends its server.
        process.kill_on_drop(true);
        let spawned = client.connect_process(process).await?;
        Ok(Self {
            client: Arc::new(client),
            transport: Transport::Headless {
                child: Some(spawned.process),
                default_fixture: None,
            },
        })
    }

    /// Shut down the session and any managed child process.
    pub async fn shutdown(self) {
        if let Transport::Headless {
            child: Some(mut child),
            ..
        } = self.transport
        {
            let _ignored = child.kill().await;
            let _ignored = child.wait().await;
        }
    }

    /// Evaluate one Luau script through the session.
    pub async fn eval(&self, mut request: ScriptEvalRequest) -> Result<EvalReport> {
        if let Transport::Headless {
            default_fixture, ..
        } = &self.transport
            && request.fixture.is_none()
        {
            request.fixture.clone_from(default_fixture);
        }
        let result = self.client.call_tool("script_eval", request).await?;
        result
            .extract_as::<EvalReport>(ToolResultMode::Structured)
            .context("decode structured script_eval outcome")
    }

    /// Request shared API discovery.
    pub async fn api(&self, query: &ScriptApiQuery) -> Result<CallToolResult> {
        Ok(self.client.call_tool("script_api", query).await?)
    }

    /// Return the explicitly selected transport mode.
    pub fn kind(&self) -> SessionKind {
        match self.transport {
            Transport::Headless { .. } => SessionKind::Headless,
            Transport::Live => SessionKind::Live,
        }
    }

    /// Request bootstrap information at an optional headless screen.
    pub async fn bootstrap(&self, request: BootstrapRequest) -> Result<BootstrapResponse> {
        Ok(self
            .client
            .call_tool_structured("bootstrap", request)
            .await?)
    }

    /// Apply or remember a fixture for the session.
    pub async fn apply_fixture(&mut self, name: String) -> Result<()> {
        match &mut self.transport {
            Transport::Live => {
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
            Transport::Headless {
                default_fixture, ..
            } => {
                *default_fixture = Some(name);
            }
        }
        Ok(())
    }
}

/// The connection a session manager holds, with where it came from.
enum Current {
    /// A headless app that the manager started.
    Headless(Session),
    /// A live app that canopyctl did not start.
    Live {
        /// The session on the app's socket.
        session: Session,
        /// The socket it connected to.
        socket: PathBuf,
    },
    /// An app canopyctl hosts in a terminal, and a session that shares the
    /// host's live client.
    Hosted {
        /// The terminal hosting the app.
        host: TermHost,
        /// The session that shares the host's client.
        session: Session,
        /// The command that started the app.
        command: ResolvedCommand,
    },
    /// The manager released its connection after it sat idle.
    Released {
        /// What it released.
        was: Released,
    },
}

/// What an idle release ended, so the next call can follow it.
enum Released {
    /// A headless app: the next call starts another.
    Headless,
    /// A live app's session: the next call connects to its socket again.
    Live(PathBuf),
    /// An app hosted in a terminal, stopped with its host: the next call
    /// fails until `launch` starts it again.
    Hosted {
        /// The command that started the app.
        command: ResolvedCommand,
    },
}

impl Released {
    /// The error of a call that needs a hosted app that an idle release
    /// stopped.
    fn hosted_error(command: &ResolvedCommand) -> anyhow::Error {
        anyhow::anyhow!(
            "the app `{command}` that `launch` started was stopped after it sat idle; \
             call `launch` to start it again, or `disconnect` to use a headless app"
        )
    }
}

/// How busy the tools are, for the idle watchdog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Activity {
    /// This many tool calls are running.
    Busy(NonZeroUsize),
    /// No tool call has run since this instant.
    Idle(Instant),
}

/// The tools' activity, and a signal for the watchdog when they go idle.
struct ActivityState {
    /// The current activity.
    activity: StdMutex<Activity>,
    /// Notified when the last running call ends.
    idle: Notify,
}

impl ActivityState {
    /// Activity that is idle from now.
    fn new() -> Self {
        Self {
            activity: StdMutex::new(Activity::Idle(Instant::now())),
            idle: Notify::new(),
        }
    }

    /// Lock the activity. Its transitions cannot panic, so a poisoned lock
    /// still holds a valid value.
    fn lock(&self) -> StdMutexGuard<'_, Activity> {
        self.activity.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Return the current activity.
    fn get(&self) -> Activity {
        *self.lock()
    }

    /// Return whether no call has run for `limit`.
    fn idle_for(&self, limit: Duration) -> bool {
        matches!(self.get(), Activity::Idle(since) if since.elapsed() >= limit)
    }

    /// Record that a call started.
    fn start(&self) {
        let mut activity = self.lock();
        *activity = Activity::Busy(match *activity {
            Activity::Busy(running) => running.saturating_add(1),
            Activity::Idle(_) => NonZeroUsize::MIN,
        });
    }

    /// Record that a call ended, and signal the watchdog if it was the last.
    fn end(&self) {
        let mut activity = self.lock();
        let Activity::Busy(running) = *activity else {
            return;
        };
        match NonZeroUsize::new(running.get() - 1) {
            Some(running) => *activity = Activity::Busy(running),
            None => {
                *activity = Activity::Idle(Instant::now());
                drop(activity);
                // One watchdog waits, so a stored permit is never lost.
                self.idle.notify_one();
            }
        }
    }
}

/// Marks a tool call as running while it is held, from the start of the
/// call to its result. Dropping it, as when the call is cancelled, ends the
/// call.
pub struct BusyGuard<'a> {
    /// The activity that the call counts in.
    activity: &'a ActivityState,
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.activity.end();
    }
}

/// Shared session manager used by the CLI and proxy MCP server.
pub struct SessionManager {
    /// Loaded CLI configuration.
    config: LoadedConfig,
    /// Current connection, if any.
    state: Mutex<Option<Current>>,
    /// Whether tool calls are running, for the idle watchdog.
    activity: ActivityState,
}

impl SessionManager {
    /// Construct a new session manager from loaded config.
    pub fn new(config: LoadedConfig) -> Self {
        Self {
            config,
            state: Mutex::new(None),
            activity: ActivityState::new(),
        }
    }

    /// Return the loaded configuration.
    pub fn config(&self) -> &LoadedConfig {
        &self.config
    }

    /// Mark a tool call as running until the guard drops. Each MCP tool
    /// holds one for its whole length, so the watchdog does not release an
    /// app that a call still uses.
    pub fn busy(&self) -> BusyGuard<'_> {
        self.activity.start();
        BusyGuard {
            activity: &self.activity,
        }
    }

    /// Connect to a live UDS session, replacing any existing session.
    pub async fn connect_live(&self, socket: &Path) -> Result<()> {
        let mut state = self.state.lock().await;
        self.release(&mut state).await?;
        let session = Session::connect_live(socket).await?;
        *state = Some(Current::Live {
            session,
            socket: socket.to_owned(),
        });
        Ok(())
    }

    /// Launch an app in a terminal that canopyctl owns, serving automation at
    /// `socket`, and make it the current session, replacing any existing one.
    pub async fn launch(
        &self,
        command: &ResolvedCommand,
        socket: PathBuf,
        size: ScreenSize,
        ready: Duration,
    ) -> Result<()> {
        let mut state = self.state.lock().await;
        self.release(&mut state).await?;
        let host = TermHost::launch(command.to_term_command(), socket, size, ready).await?;
        let session = Session::share_live(host.client());
        *state = Some(Current::Hosted {
            host,
            session,
            command: command.clone(),
        });
        Ok(())
    }

    /// Press `keys` in the terminal hosting the current app, then return the
    /// screen if `read` asks for it.
    pub async fn terminal(&self, keys: &[String], read: &Read) -> Result<Option<Vec<String>>> {
        // The host acknowledges input through the app's session, which the
        // state lock also guards from script tools.
        let mut state = self.state.lock().await;
        match state.as_mut() {
            Some(Current::Hosted { host, .. }) => term::drive(host, keys, read).await,
            Some(Current::Released {
                was: Released::Hosted { command },
            }) => Err(Released::hosted_error(command)),
            _ => bail!("no app is hosted in a terminal; launch one first"),
        }
    }

    /// Drop and shut down the current session and any app canopyctl hosts.
    /// The next call starts a headless app.
    pub async fn disconnect(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        self.release(&mut state).await
    }

    /// Release the app whenever no tool has run for `limit`, and keep
    /// watching. The server keeps serving its client.
    pub async fn watch_idle(self: Arc<Self>, limit: Duration) {
        loop {
            match self.activity.get() {
                Activity::Busy(_) => self.activity.idle.notified().await,
                Activity::Idle(since) => {
                    sleep_until(since + limit).await;
                    if self.release_if_idle(limit).await {
                        // Nothing changes until a call ends.
                        self.activity.idle.notified().await;
                    }
                }
            }
        }
    }

    /// Release the connection, remembering what it was, if no tool has run
    /// for `limit`. Return whether the tools were idle that long.
    async fn release_if_idle(&self, limit: Duration) -> bool {
        if !self.activity.idle_for(limit) {
            return false;
        }
        let mut state = self.state.lock().await;
        // A call that started while this waited for the lock keeps its
        // session.
        if !self.activity.idle_for(limit) {
            return false;
        }
        let was = match state.take() {
            None => None,
            Some(Current::Released { was }) => Some(was),
            Some(Current::Headless(session)) => {
                session.shutdown().await;
                Some(Released::Headless)
            }
            Some(Current::Live { session, socket }) => {
                session.shutdown().await;
                Some(Released::Live(socket))
            }
            Some(Current::Hosted { host, command, .. }) => {
                if let Err(error) = host.stop().await {
                    eprintln!("stop the idle app `{command}`: {error:#}");
                }
                Some(Released::Hosted { command })
            }
        };
        *state = was.map(|was| Current::Released { was });
        true
    }

    /// Shut down the connection in `state`, and the app if canopyctl hosts
    /// it, and leave nothing to follow. The caller holds the state lock, so
    /// no operation sees a half-released connection.
    async fn release(&self, state: &mut Option<Current>) -> Result<()> {
        match state.take() {
            Some(Current::Headless(session) | Current::Live { session, .. }) => {
                session.shutdown().await;
            }
            Some(Current::Hosted { host, .. }) => host.stop().await?,
            Some(Current::Released { .. }) | None => {}
        }
        Ok(())
    }

    /// Lock the active session. With none connected, start a headless one,
    /// or follow what an idle release ended: connect to a released live app
    /// again, or fail for a released hosted app.
    pub async fn session(&self) -> Result<MappedMutexGuard<'_, Session>> {
        let mut state = self.state.lock().await;
        match state.as_ref() {
            None
            | Some(Current::Released {
                was: Released::Headless,
            }) => {
                let command = self.config.headless_command(&[])?;
                *state = Some(Current::Headless(Session::spawn_headless(&command).await?));
            }
            Some(Current::Released {
                was: Released::Live(socket),
            }) => {
                let session = Session::connect_live(socket).await.with_context(|| {
                    format!(
                        "reconnect to the live app at {}, released after it sat idle",
                        socket.display()
                    )
                })?;
                let socket = socket.clone();
                *state = Some(Current::Live { session, socket });
            }
            Some(Current::Released {
                was: Released::Hosted { command },
            }) => return Err(Released::hosted_error(command)),
            Some(Current::Headless(_) | Current::Live { .. } | Current::Hosted { .. }) => {}
        }
        MutexGuard::try_map(state, |state| match state.as_mut() {
            Some(
                Current::Headless(session)
                | Current::Live { session, .. }
                | Current::Hosted { session, .. },
            ) => Some(session),
            Some(Current::Released { .. }) | None => None,
        })
        .map_err(|_| anyhow::anyhow!("no session is connected"))
    }
}

#[cfg(test)]
pub mod tests {
    use std::{
        fs, future,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    use canopy::testing::contracts;
    use canopy_mcp::{
        AppFactory, AppMetadata, ExecutionMetadata, ExecutionMode, ResetPolicy, ScreenSize,
        ScriptErrorInfo, ScriptErrorType, ScriptTaskState, ScriptTiming,
    };
    use serde_json::{Value, json};
    use tmcp::{Server, ToolError, ToolResult, mcp_server};
    use tokio::{
        io::{duplex, split},
        net::UnixListener,
        sync::oneshot,
        task::{self, JoinHandle, block_in_place},
        time::sleep,
    };

    use super::*;

    /// Real factory exposed over MCP for cross-adapter contract tests.
    #[derive(Clone)]
    struct FactoryPeer {
        /// The same public factory used by headless MCP applications.
        factory: AppFactory,
        /// Sources observed at the transport boundary.
        calls: Arc<Mutex<Vec<String>>>,
    }

    #[mcp_server]
    impl FactoryPeer {
        #[tool]
        async fn script_eval(&self, request: ScriptEvalRequest) -> ToolResult<CallToolResult> {
            self.calls.lock().await.push(request.source.clone());
            Ok(block_in_place(|| self.factory.evaluate(&request)).to_tool_result())
        }

        #[tool]
        async fn bootstrap(&self, request: BootstrapRequest) -> ToolResult<CallToolResult> {
            let response = block_in_place(|| self.factory.bootstrap(&request))
                .map_err(|error| ToolError::internal(error.to_string()))?;
            CallToolResult::structured(response)
                .map_err(|error| ToolError::internal(error.to_string()))
        }
    }

    /// Connect a real fresh-app factory through an in-memory MCP transport.
    pub async fn factory_session() -> Result<(Session, Arc<Mutex<Vec<String>>>, JoinHandle<()>)> {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let peer = FactoryPeer {
            factory: AppFactory::new(
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
                .expect("factory transport");
        });
        let (reader, writer) = split(client_stream);
        let mut client = Client::new(CLIENT_NAME, CLIENT_VERSION);
        client.connect_stream(reader, writer).await?;
        Ok((
            Session {
                client: Arc::new(client),
                transport: Transport::Headless {
                    child: None,
                    default_fixture: None,
                },
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
            self.calls.lock().await.push(request.source.clone());
            match request.source.as_str() {
                "missing" => return Ok(CallToolResult::new()),
                "malformed" => {
                    return Ok(
                        CallToolResult::new().with_structured_content(json!({"success": "wrong"}))
                    );
                }
                _ => {}
            }
            let state = match request.source.as_str() {
                "failure" => ScriptTaskState::Failed,
                "timeout" => ScriptTaskState::TimedOut,
                _ => ScriptTaskState::Completed,
            };
            let success = state == ScriptTaskState::Completed;
            let outcome = EvalReport {
                metadata: ExecutionMetadata {
                    app: "test".into(),
                    execution: ExecutionMode::LiveSession,
                    instance_id: "test-session".into(),
                    screen: Some(ScreenSize::default()),
                    reset: ResetPolicy::External,
                    fixture: None,
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
        Ok((Session::share_live(Arc::new(client)), calls, task))
    }

    pub fn request(script: &str) -> ScriptEvalRequest {
        ScriptEvalRequest::new(script.to_owned())
    }

    /// Return whether the manager holds a live session.
    async fn is_live(manager: &SessionManager) -> bool {
        matches!(*manager.state.lock().await, Some(Current::Live { .. }))
    }

    /// Return whether the manager released a live session at `at`.
    async fn released_live(manager: &SessionManager, at: &Path) -> bool {
        matches!(
            &*manager.state.lock().await,
            Some(Current::Released { was: Released::Live(socket) }) if socket == at
        )
    }

    #[tokio::test(start_paused = true)]
    async fn regression_a_call_longer_than_the_idle_limit_keeps_its_session() -> Result<()> {
        let limit = Duration::from_secs(10);
        let (session, _, peer) = peer_session().await?;
        let manager = manager_with_session(session).await?;
        let watchdog = tokio::spawn(manager.clone().watch_idle(limit));
        {
            let _busy = manager.busy();
            let session = manager.session().await?;
            sleep(limit * 3).await;
            drop(session);
        }
        // The next call follows at once.
        task::yield_now().await;
        assert!(is_live(&manager).await);
        // The release comes one limit after the call ends.
        sleep(limit - Duration::from_secs(1)).await;
        assert!(is_live(&manager).await);
        sleep(Duration::from_secs(2)).await;
        assert!(matches!(
            *manager.state.lock().await,
            Some(Current::Released { .. })
        ));
        watchdog.abort();
        peer.abort();
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn a_call_that_starts_while_the_watchdog_waits_for_the_lock_keeps_its_session()
    -> Result<()> {
        let limit = Duration::from_secs(10);
        let (session, _, peer) = peer_session().await?;
        let manager = manager_with_session(session).await?;
        let watchdog = tokio::spawn(manager.clone().watch_idle(limit));
        // Holding the lock stops the watchdog between its check and its lock.
        let state = manager.state.lock().await;
        sleep(limit * 2).await;
        let busy = manager.busy();
        drop(state);
        task::yield_now().await;
        assert!(is_live(&manager).await);
        drop(busy);
        sleep(limit + Duration::from_secs(1)).await;
        assert!(!is_live(&manager).await);
        watchdog.abort();
        peer.abort();
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancelled_call_ends_its_busy_guard() -> Result<()> {
        let manager = Arc::new(SessionManager::new(LoadedConfig::load()?));
        let calling = manager.clone();
        let (started_tx, started_rx) = oneshot::channel();
        let call = tokio::spawn(async move {
            let _busy = calling.busy();
            started_tx.send(()).expect("the test waits");
            future::pending::<()>().await;
        });
        started_rx.await?;
        assert!(matches!(manager.activity.get(), Activity::Busy(n) if n.get() == 1));
        call.abort();
        assert!(call.await.unwrap_err().is_cancelled());
        assert!(matches!(manager.activity.get(), Activity::Idle(_)));
        Ok(())
    }

    #[test]
    fn activity_counts_overlapping_calls() {
        let activity = ActivityState::new();
        activity.start();
        activity.start();
        activity.end();
        assert!(matches!(activity.get(), Activity::Busy(n) if n.get() == 1));
        activity.end();
        assert!(matches!(activity.get(), Activity::Idle(_)));
        // An unmatched end leaves the tools idle.
        activity.end();
        assert!(matches!(activity.get(), Activity::Idle(_)));
    }

    /// Serve a stateful peer on `socket`, one connection after another.
    fn serve_peer(listener: UnixListener, peer: Peer) -> JoinHandle<()> {
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let peer = peer.clone();
                tokio::spawn(async move {
                    let (reader, writer) = stream.into_split();
                    drop(
                        Server::new(move || peer.clone())
                            .serve_stream(reader, writer)
                            .await,
                    );
                });
            }
        })
    }

    #[tokio::test]
    async fn regression_an_idle_release_of_a_live_app_reconnects_to_it() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let socket = directory.path().join("live.sock");
        let peer = Peer::default();
        let calls = peer.calls.clone();
        let server = serve_peer(UnixListener::bind(&socket)?, peer);
        let manager = Arc::new(SessionManager::new(LoadedConfig::load()?));
        manager.connect_live(&socket).await?;
        manager.session().await?.eval(request("first")).await?;
        assert!(manager.release_if_idle(Duration::ZERO).await);
        assert!(released_live(&manager, &socket).await);
        let session = manager.session().await?;
        assert_eq!(session.kind(), SessionKind::Live);
        session.eval(request("second")).await?;
        drop(session);
        assert_eq!(*calls.lock().await, ["first", "second"]);
        assert!(is_live(&manager).await);
        manager.disconnect().await?;
        server.abort();
        Ok(())
    }

    #[tokio::test]
    async fn a_released_live_app_that_is_gone_fails_and_keeps_its_socket() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let socket = directory.path().join("live.sock");
        let peer = Peer::default();
        let calls = peer.calls.clone();
        let server = serve_peer(UnixListener::bind(&socket)?, peer.clone());
        let manager = Arc::new(SessionManager::new(LoadedConfig::load()?));
        manager.connect_live(&socket).await?;
        assert!(manager.release_if_idle(Duration::ZERO).await);
        server.abort();
        drop(server.await);
        fs::remove_file(&socket)?;
        let error = manager.session().await.err().context("a gone app fails")?;
        assert!(format!("{error:#}").contains("released after it sat idle"));
        assert!(released_live(&manager, &socket).await);
        // The app comes back at the same socket, and a retry reaches it.
        let server = serve_peer(UnixListener::bind(&socket)?, peer);
        manager.session().await?.eval(request("again")).await?;
        assert_eq!(*calls.lock().await, ["again"]);
        manager.disconnect().await?;
        server.abort();
        Ok(())
    }

    #[tokio::test]
    async fn a_released_hosted_app_fails_until_it_is_launched_again() -> Result<()> {
        let manager = Arc::new(SessionManager::new(LoadedConfig::load()?));
        let command = ResolvedCommand::new(
            vec!["todo".into(), "--live".into()],
            PathBuf::from("."),
            Default::default(),
        )?;
        *manager.state.lock().await = Some(Current::Released {
            was: Released::Hosted { command },
        });
        for error in [
            manager.session().await.err(),
            manager.terminal(&[], &Read::Now).await.err(),
        ] {
            let error = error.context("a released hosted app fails")?.to_string();
            assert!(error.contains("`todo --live`"), "{error}");
            assert!(error.contains("`launch`"), "{error}");
        }
        assert!(matches!(
            *manager.state.lock().await,
            Some(Current::Released {
                was: Released::Hosted { .. }
            })
        ));
        // An explicit disconnect leaves nothing, so the next call starts
        // headless.
        manager.disconnect().await?;
        assert!(manager.state.lock().await.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn an_explicit_disconnect_forgets_a_live_app() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let socket = directory.path().join("live.sock");
        let server = serve_peer(UnixListener::bind(&socket)?, Peer::default());
        let manager = Arc::new(SessionManager::new(LoadedConfig::load()?));
        manager.connect_live(&socket).await?;
        manager.disconnect().await?;
        assert!(manager.state.lock().await.is_none());
        manager.connect_live(&socket).await?;
        assert!(manager.release_if_idle(Duration::ZERO).await);
        manager.disconnect().await?;
        assert!(manager.state.lock().await.is_none());
        server.abort();
        Ok(())
    }

    pub async fn manager_with_session(session: Session) -> Result<Arc<SessionManager>> {
        let manager = Arc::new(SessionManager::new(LoadedConfig::load()?));
        let current = match session.kind() {
            SessionKind::Headless => Current::Headless(session),
            // A test peer has no socket to reconnect to.
            SessionKind::Live => Current::Live {
                session,
                socket: PathBuf::from("/nonexistent/peer.sock"),
            },
        };
        *manager.state.lock().await = Some(current);
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
        session.transport = Transport::Headless {
            child: None,
            default_fixture: None,
        };
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
                    assert!(
                        operation_manager
                            .session()
                            .await?
                            .eval(request("success"))
                            .await?
                            .success
                    );
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
