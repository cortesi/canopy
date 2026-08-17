//! MCP client sessions and the manager shared by the CLI and the proxy server.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use canopy::FixtureInfo;
use canopy_mcp::{ApplyFixtureRequest, BootstrapResponse, ScriptEvalOutcome, ScriptEvalRequest};
use tmcp::Client;
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
        Ok(self
            .client
            .call_tool_structured("script_eval", request)
            .await?)
    }

    /// Request the rendered `.d.luau` API text.
    pub async fn api(&self) -> Result<String> {
        let result = self.client.call_tool("script_api", ()).await?;
        result
            .text()
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow!("script_api returned no text"))
    }

    /// Request bootstrap information.
    pub async fn bootstrap(&self) -> Result<BootstrapResponse> {
        Ok(self.client.call_tool_structured("bootstrap", ()).await?)
    }

    /// Request the fixture catalog.
    pub async fn fixtures(&self) -> Result<Vec<FixtureInfo>> {
        Ok(self.client.call_tool_structured("fixtures", ()).await?)
    }

    /// Apply or remember a fixture for the session.
    pub async fn apply_fixture(&mut self, name: String) -> Result<()> {
        match self.kind {
            SessionKind::Live => {
                let _result = self
                    .client
                    .call_tool("apply_fixture", ApplyFixtureRequest { name })
                    .await?;
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
        let previous = self.take_session().await;
        if let Some(previous) = previous {
            previous.shutdown().await;
        }
        let session = Session::connect_live(socket).await?;
        *self.state.lock().await = Some(session);
        Ok(())
    }

    /// Drop and shut down the current session, if any.
    pub async fn disconnect(&self) -> Result<()> {
        if let Some(session) = self.take_session().await {
            session.shutdown().await;
        }
        Ok(())
    }

    /// Evaluate a script on the active session.
    pub async fn eval(&self, request: ScriptEvalRequest) -> Result<ScriptEvalOutcome> {
        self.session().await?.eval(request).await
    }

    /// Request the API text on the active session.
    pub async fn api(&self) -> Result<String> {
        self.session().await?.api().await
    }

    /// Request bootstrap information on the active session.
    pub async fn bootstrap(&self) -> Result<BootstrapResponse> {
        self.session().await?.bootstrap().await
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

    /// Remove and return the current session.
    pub async fn take_session(&self) -> Option<Session> {
        self.state.lock().await.take()
    }
}
