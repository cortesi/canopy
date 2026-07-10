#![deny(unsafe_code)]
//! Command-line tooling for running and automating canopy apps.

use std::{
    collections::HashMap,
    env,
    fmt::Display,
    fs,
    path::{Component, Path, PathBuf},
    process::{Stdio, exit, id},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use canopy_mcp::{
    ApplyFixtureRequest, BootstrapResponse, ScriptAssertion, ScriptEvalOutcome, ScriptEvalRequest,
};
use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tmcp::{Client, ToolError, ToolResult, mcp_server, schema::CallToolResult, tool, tool_params};
use tokio::{
    net::UnixStream,
    process::{Child, Command},
    sync::Mutex,
    time::sleep,
};

/// MCP client name reported to spawned or connected servers.
const CLIENT_NAME: &str = "canopyctl";
/// MCP client version reported to spawned or connected servers.
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Top-level CLI arguments.
#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    /// Selected subcommand.
    #[command(subcommand)]
    command: Commands,
}

/// Supported `canopyctl` subcommands.
#[derive(Subcommand)]
enum Commands {
    /// Run an interactive app process with live UDS automation enabled.
    Run(RunArgs),
    /// Expose `canopyctl` itself as an MCP server on stdio.
    Mcp,
    /// Execute a Luau smoke suite through the headless MCP server.
    Smoke(SmokeArgs),
    /// Replay a recorded script journal through a headless app instance.
    Replay(ReplayArgs),
    /// Print MCP bootstrap information from a headless app instance.
    Bootstrap(SpawnArgs),
    /// List registered fixtures from a headless app instance.
    Fixtures(SpawnArgs),
    /// Evaluate one Luau script against a headless app instance.
    #[command(alias = "script-eval")]
    Eval(EvalArgs),
    /// Print the rendered `.d.luau` API from a headless app instance.
    #[command(alias = "script-api")]
    Api(SpawnArgs),
}

/// Arguments for `canopyctl run`.
#[derive(Args)]
struct RunArgs {
    /// Optional fixture to apply after the live UDS server is ready.
    #[arg(long)]
    fixture: Option<String>,
    /// Command override passed after `--`.
    #[arg(last = true)]
    command: Vec<String>,
}

/// Shared arguments for subcommands that only need an optional command override.
#[derive(Args)]
struct SpawnArgs {
    /// Command override passed after `--`.
    #[arg(last = true)]
    command: Vec<String>,
}

/// Arguments for `canopyctl smoke`.
#[derive(Args)]
struct SmokeArgs {
    /// Optional explicit script paths relative to the suite root.
    scripts: Vec<PathBuf>,
    /// Optional suite directory override.
    #[arg(long)]
    suite: Option<PathBuf>,
    /// Stop after the first failing script.
    #[arg(long)]
    fail_fast: bool,
    /// Optional per-script timeout override in milliseconds.
    #[arg(long)]
    timeout_ms: Option<u64>,
    /// Command override passed after `--`.
    #[arg(last = true)]
    command: Vec<String>,
}

/// Arguments for `canopyctl replay`.
#[derive(Args)]
struct ReplayArgs {
    /// Path to a JSON replay journal.
    journal: PathBuf,
    /// Optional fixture to apply before each replayed entry.
    #[arg(long)]
    fixture: Option<String>,
    /// Stop after the first failing replay entry.
    #[arg(long)]
    fail_fast: bool,
    /// Replay entries that originally failed.
    #[arg(long)]
    include_failed: bool,
    /// Optional per-entry timeout override in milliseconds.
    #[arg(long)]
    timeout_ms: Option<u64>,
    /// Command override passed after `--`.
    #[arg(last = true)]
    command: Vec<String>,
}

/// Arguments for `canopyctl eval`.
#[derive(Args)]
struct EvalArgs {
    /// Inline Luau source.
    script: Option<String>,
    /// Path to a Luau source file.
    #[arg(short = 'f', long)]
    file: Option<PathBuf>,
    /// Optional fixture to apply before evaluation.
    #[arg(long)]
    fixture: Option<String>,
    /// Optional evaluation timeout override in milliseconds.
    #[arg(long)]
    timeout_ms: Option<u64>,
    /// Write a replay journal containing this evaluation.
    #[arg(long)]
    journal_out: Option<PathBuf>,
    /// Command override passed after `--`.
    #[arg(last = true)]
    command: Vec<String>,
}

/// Parsed `.canopyctl.toml` contents.
#[derive(Debug, Default, Clone, Deserialize)]
struct FileConfig {
    /// Application launch settings.
    app: Option<AppSection>,
    /// Smoke-runner settings.
    smoke: Option<SmokeSection>,
    /// `canopyctl mcp` server settings.
    mcp: Option<McpSection>,
}

/// `[app]` section from `.canopyctl.toml`.
#[derive(Debug, Default, Clone, Deserialize)]
struct AppSection {
    /// Command used to start the headless stdio MCP server.
    headless: Option<Vec<String>>,
    /// Command used to run the interactive app.
    run: Option<Vec<String>>,
    /// Extra args appended to the interactive command to inject the socket path.
    mcp_args: Option<Vec<String>>,
    /// Working directory for spawned processes.
    cwd: Option<PathBuf>,
    /// Environment variables merged into spawned processes.
    env: Option<HashMap<String, String>>,
}

/// `[smoke]` section from `.canopyctl.toml`.
#[derive(Debug, Default, Clone, Deserialize)]
struct SmokeSection {
    /// Suite directory scanned when no explicit scripts are passed.
    suite: Option<PathBuf>,
    /// Default fail-fast behavior.
    fail_fast: Option<bool>,
    /// Default per-script timeout in milliseconds.
    timeout_ms: Option<u64>,
}

/// `[mcp]` section from `.canopyctl.toml`.
#[derive(Debug, Default, Clone, Deserialize)]
struct McpSection {
    /// Idle timeout before `canopyctl mcp` exits.
    idle_shutdown_after_secs: Option<u64>,
}

/// Loaded config together with the directory it resolves paths against.
#[derive(Debug, Clone)]
struct LoadedConfig {
    /// Parsed config contents.
    file: FileConfig,
    /// Directory relative paths resolve against.
    config_dir: PathBuf,
}

impl LoadedConfig {
    /// Load `.canopyctl.toml` by walking upward from the current directory.
    fn load() -> Result<Self> {
        let cwd = env::current_dir().context("read current directory")?;
        let config_path = discover_config_path(&cwd)?;
        if let Some(config_path) = config_path {
            let contents = fs::read_to_string(&config_path)
                .with_context(|| format!("read {}", config_path.display()))?;
            let file =
                toml::from_str::<FileConfig>(&contents).with_context(|| "parse .canopyctl.toml")?;
            let config_dir = config_path
                .parent()
                .ok_or_else(|| anyhow!("config path missing parent"))?
                .to_path_buf();
            Ok(Self { file, config_dir })
        } else {
            Ok(Self {
                file: FileConfig::default(),
                config_dir: cwd,
            })
        }
    }

    /// Resolve a config-relative path into an absolute path.
    fn resolve_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.config_dir.join(path)
        }
    }

    /// Return the merged application environment.
    fn app_env(&self) -> HashMap<String, String> {
        self.file
            .app
            .as_ref()
            .and_then(|app| app.env.clone())
            .unwrap_or_default()
    }

    /// Return the working directory used for spawned app processes.
    fn app_cwd(&self) -> PathBuf {
        self.file
            .app
            .as_ref()
            .and_then(|app| app.cwd.as_ref())
            .map(|cwd| self.resolve_path(cwd))
            .unwrap_or_else(|| self.config_dir.clone())
    }

    /// Build the interactive app command, including injected MCP socket args.
    fn run_command(&self, override_command: &[String], socket: &Path) -> Result<ResolvedCommand> {
        let base = if override_command.is_empty() {
            self.file
                .app
                .as_ref()
                .and_then(|app| app.run.clone())
                .ok_or_else(|| anyhow!("missing [app].run and no -- COMMAND override"))?
        } else {
            override_command.to_vec()
        };
        let extra = self
            .file
            .app
            .as_ref()
            .and_then(|app| app.mcp_args.clone())
            .unwrap_or_else(|| vec!["--mcp={socket}".to_string()]);
        let replaced = extra
            .into_iter()
            .map(|arg| arg.replace("{socket}", &socket.display().to_string()));
        ResolvedCommand::new(
            base.into_iter().chain(replaced).collect(),
            self.app_cwd(),
            self.app_env(),
        )
    }

    /// Build the headless stdio MCP command.
    fn headless_command(&self, override_command: &[String]) -> Result<ResolvedCommand> {
        let argv = if override_command.is_empty() {
            self.file
                .app
                .as_ref()
                .and_then(|app| app.headless.clone())
                .ok_or_else(|| anyhow!("missing [app].headless and no -- COMMAND override"))?
        } else {
            override_command.to_vec()
        };
        ResolvedCommand::new(argv, self.app_cwd(), self.app_env())
    }

    /// Resolve the smoke suite directory.
    fn smoke_suite_dir(&self, override_suite: Option<&Path>) -> PathBuf {
        if let Some(override_suite) = override_suite {
            self.resolve_path(override_suite)
        } else if let Some(configured) = self
            .file
            .smoke
            .as_ref()
            .and_then(|smoke| smoke.suite.as_ref())
        {
            self.resolve_path(configured)
        } else {
            self.config_dir.join("smoke")
        }
    }

    /// Resolve the effective smoke timeout.
    fn smoke_timeout_ms(&self, override_timeout_ms: Option<u64>) -> Option<u64> {
        override_timeout_ms.or_else(|| self.file.smoke.as_ref().and_then(|smoke| smoke.timeout_ms))
    }

    /// Resolve the effective fail-fast setting.
    fn smoke_fail_fast(&self, override_fail_fast: bool) -> bool {
        override_fail_fast
            || self
                .file
                .smoke
                .as_ref()
                .and_then(|smoke| smoke.fail_fast)
                .unwrap_or(false)
    }

    /// Resolve the effective MCP idle timeout.
    fn idle_shutdown_after(&self) -> Duration {
        Duration::from_secs(
            self.file
                .mcp
                .as_ref()
                .and_then(|mcp| mcp.idle_shutdown_after_secs)
                .unwrap_or(1200),
        )
    }
}

/// Fully resolved process launch specification.
#[derive(Debug, Clone)]
struct ResolvedCommand {
    /// Program and arguments.
    argv: Vec<String>,
    /// Working directory.
    cwd: PathBuf,
    /// Spawned-process environment variables.
    env: HashMap<String, String>,
}

impl ResolvedCommand {
    /// Construct a validated resolved command.
    fn new(argv: Vec<String>, cwd: PathBuf, env: HashMap<String, String>) -> Result<Self> {
        if argv.is_empty() {
            bail!("command is empty");
        }
        Ok(Self { argv, cwd, env })
    }

    /// Convert the resolved command into a `tokio::process::Command`.
    fn to_command(&self) -> Command {
        let mut command = Command::new(&self.argv[0]);
        command.args(&self.argv[1..]);
        command.current_dir(&self.cwd);
        command.envs(&self.env);
        command
    }
}

/// Serializable fixture metadata returned by automation tools.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct FixtureInfo {
    /// Stable fixture name.
    name: String,
    /// Human-readable fixture description.
    description: String,
}

/// Serializable request body sent to `script_eval`.
#[derive(Debug, Clone, Serialize)]
struct EvalRequestPayload {
    /// Luau source to evaluate.
    script: String,
    /// Optional fixture applied before evaluation.
    #[serde(skip_serializing_if = "Option::is_none")]
    fixture: Option<String>,
    /// Optional evaluation timeout in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u64>,
}

impl From<ScriptEvalRequest> for EvalRequestPayload {
    fn from(value: ScriptEvalRequest) -> Self {
        Self {
            script: value.script,
            fixture: value.fixture,
            timeout_ms: value.timeout_ms,
        }
    }
}

/// JSON replay journal accepted by `canopyctl replay`.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct ReplayJournal {
    /// Recorded script evaluations.
    journal: Vec<ReplayEntry>,
}

/// One replayable script evaluation.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct ReplayEntry {
    /// Optional monotonic source journal id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<u64>,
    /// Script origin such as `eval` or `startup:app`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<String>,
    /// Evaluated Luau source text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    /// Alternate source field accepted for hand-written replay files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    script: Option<String>,
    /// Whether the original evaluation completed successfully.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ok: Option<bool>,
    /// Error message from the original evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Logs emitted by the original evaluation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    logs: Vec<String>,
    /// Assertions emitted by the original evaluation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    assertions: Vec<ScriptAssertion>,
    /// Original wall-clock duration in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
}

impl ReplayEntry {
    /// Return a stable human-readable origin.
    fn origin(&self) -> &str {
        self.origin.as_deref().unwrap_or("journal")
    }

    /// Return the script source for this replay entry.
    fn source(&self) -> Result<&str> {
        self.source
            .as_deref()
            .or(self.script.as_deref())
            .ok_or_else(|| {
                anyhow!(
                    "replay entry from {} has no source/script field",
                    self.origin()
                )
            })
    }
}

/// Accepted top-level JSON shapes for replay journals.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ReplayInput {
    /// Object form emitted by `canopyctl eval --journal-out`.
    Object(ReplayJournal),
    /// Bare array accepted for simple hand-authored replays.
    Entries(Vec<ReplayEntry>),
}

impl ReplayInput {
    /// Convert into replay entries.
    fn into_entries(self) -> Vec<ReplayEntry> {
        match self {
            Self::Object(journal) => journal.journal,
            Self::Entries(entries) => entries,
        }
    }
}

/// One discovered smoke script together with its derived fixture.
#[derive(Debug)]
struct SmokeScript {
    /// Absolute script path.
    path: PathBuf,
    /// Optional derived fixture name.
    fixture: Option<String>,
    /// Script source text.
    source: String,
}

/// Active headless or live MCP client session.
struct Session {
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
enum SessionKind {
    /// Auto-spawned headless stdio MCP session.
    Headless,
    /// Connected live UDS MCP session.
    Live,
}

impl Session {
    /// Connect to a live UDS MCP server.
    async fn connect_live(socket: &Path) -> Result<Self> {
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
    async fn spawn_headless(command: &ResolvedCommand) -> Result<Self> {
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
    async fn shutdown(mut self) {
        if let Some(mut child) = self.child.take() {
            let _ignored = child.kill().await;
            let _ignored = child.wait().await;
        }
    }

    /// Evaluate one Luau script through the session.
    async fn eval(&self, mut request: ScriptEvalRequest) -> Result<ScriptEvalOutcome> {
        if self.kind == SessionKind::Headless && request.fixture.is_none() {
            request.fixture = self.default_fixture.clone();
        }
        Ok(self
            .client
            .call_tool_structured("script_eval", EvalRequestPayload::from(request))
            .await?)
    }

    /// Request the rendered `.d.luau` API text.
    async fn api(&self) -> Result<String> {
        let result = self.client.call_tool("script_api", ()).await?;
        result
            .text()
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow!("script_api returned no text"))
    }

    /// Request bootstrap information.
    async fn bootstrap(&self) -> Result<BootstrapResponse> {
        Ok(self.client.call_tool_structured("bootstrap", ()).await?)
    }

    /// Request the fixture catalog.
    async fn fixtures(&self) -> Result<Vec<FixtureInfo>> {
        Ok(self.client.call_tool_structured("fixtures", ()).await?)
    }

    /// Apply or remember a fixture for the session.
    async fn apply_fixture(&mut self, name: String) -> Result<()> {
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
struct SessionManager {
    /// Loaded CLI configuration.
    config: LoadedConfig,
    /// Current session, if any.
    state: Mutex<Option<Session>>,
}

impl SessionManager {
    /// Construct a new session manager from loaded config.
    fn new(config: LoadedConfig) -> Self {
        Self {
            config,
            state: Mutex::new(None),
        }
    }

    /// Connect to a live UDS session, replacing any existing session.
    async fn connect_live(&self, socket: &Path) -> Result<()> {
        let previous = self.take_session().await;
        if let Some(previous) = previous {
            previous.shutdown().await;
        }
        let session = Session::connect_live(socket).await?;
        *self.state.lock().await = Some(session);
        Ok(())
    }

    /// Drop and shut down the current session, if any.
    async fn disconnect(&self) -> Result<()> {
        if let Some(session) = self.take_session().await {
            session.shutdown().await;
        }
        Ok(())
    }

    /// Evaluate a script on the active session, auto-spawning headless if needed.
    async fn eval(&self, request: ScriptEvalRequest) -> Result<ScriptEvalOutcome> {
        self.ensure_headless_if_missing().await?;
        let mut state = self.state.lock().await;
        let session = state
            .as_mut()
            .ok_or_else(|| anyhow!("session missing after initialization"))?;
        session.eval(request).await
    }

    /// Request the API text on the active session, auto-spawning headless if needed.
    async fn api(&self) -> Result<String> {
        self.ensure_headless_if_missing().await?;
        let mut state = self.state.lock().await;
        let session = state
            .as_mut()
            .ok_or_else(|| anyhow!("session missing after initialization"))?;
        session.api().await
    }

    /// Request bootstrap information on the active session, auto-spawning headless if needed.
    async fn bootstrap(&self) -> Result<BootstrapResponse> {
        self.ensure_headless_if_missing().await?;
        let mut state = self.state.lock().await;
        let session = state
            .as_mut()
            .ok_or_else(|| anyhow!("session missing after initialization"))?;
        session.bootstrap().await
    }

    /// Request the fixture catalog on the active session, auto-spawning headless if needed.
    async fn fixtures(&self) -> Result<Vec<FixtureInfo>> {
        self.ensure_headless_if_missing().await?;
        let mut state = self.state.lock().await;
        let session = state
            .as_mut()
            .ok_or_else(|| anyhow!("session missing after initialization"))?;
        session.fixtures().await
    }

    /// Apply a fixture on the active session, auto-spawning headless if needed.
    async fn apply_fixture(&self, name: String) -> Result<()> {
        self.ensure_headless_if_missing().await?;
        let mut state = self.state.lock().await;
        let session = state
            .as_mut()
            .ok_or_else(|| anyhow!("session missing after initialization"))?;
        session.apply_fixture(name).await
    }

    /// Ensure there is at least a headless session available.
    async fn ensure_headless_if_missing(&self) -> Result<()> {
        if self.state.lock().await.is_some() {
            return Ok(());
        }
        let command = self.config.headless_command(&[])?;
        let session = Session::spawn_headless(&command).await?;
        *self.state.lock().await = Some(session);
        Ok(())
    }

    /// Remove and return the current session.
    async fn take_session(&self) -> Option<Session> {
        self.state.lock().await.take()
    }
}

/// MCP proxy server implementation for `canopyctl mcp`.
#[derive(Clone)]
struct CanopyctlMcpServer {
    /// Shared session manager for tool calls.
    sessions: Arc<SessionManager>,
    /// Last observed tool activity time.
    last_activity: Arc<Mutex<Instant>>,
}

/// Tool params for the `connect` tool.
#[derive(Debug, Clone, PartialEq)]
#[tool_params]
struct ConnectRequest {
    /// Unix-domain socket path to connect to.
    socket: String,
}

#[mcp_server]
impl CanopyctlMcpServer {
    /// Record activity so the idle watchdog does not terminate the server.
    async fn touch(&self) {
        *self.last_activity.lock().await = Instant::now();
    }

    #[tool]
    /// Connect to a live canopy UDS socket.
    async fn connect(&self, params: ConnectRequest) -> ToolResult<CallToolResult> {
        self.touch().await;
        self.sessions
            .connect_live(Path::new(&params.socket))
            .await
            .map_err(tool_error)?;
        Ok(CallToolResult::new().with_text_content("connected"))
    }

    #[tool]
    /// Disconnect the current session and shut down any managed child process.
    async fn disconnect(&self) -> ToolResult<CallToolResult> {
        self.touch().await;
        self.sessions.disconnect().await.map_err(tool_error)?;
        Ok(CallToolResult::new().with_text_content("disconnected"))
    }

    #[tool]
    /// Evaluate a script on the active session.
    async fn script_eval(&self, params: ScriptEvalRequest) -> ToolResult<CallToolResult> {
        self.touch().await;
        let outcome = self.sessions.eval(params).await.map_err(tool_error)?;
        Ok(outcome.to_tool_result())
    }

    #[tool]
    /// Return bootstrap information for the active session.
    async fn bootstrap(&self) -> ToolResult<CallToolResult> {
        self.touch().await;
        let bootstrap = self.sessions.bootstrap().await.map_err(tool_error)?;
        let value = serde_json::to_value(bootstrap).map_err(tool_error)?;
        Ok(CallToolResult::new()
            .with_structured_content(value.clone())
            .with_text_content(value.to_string()))
    }

    #[tool]
    /// Apply a fixture to the active session.
    async fn apply_fixture(&self, params: ApplyFixtureRequest) -> ToolResult<CallToolResult> {
        self.touch().await;
        self.sessions
            .apply_fixture(params.name.clone())
            .await
            .map_err(tool_error)?;
        let value = serde_json::json!({ "applied": params.name });
        Ok(CallToolResult::new()
            .with_structured_content(value.clone())
            .with_text_content(value.to_string()))
    }

    #[tool]
    /// Return the rendered `.d.luau` API for the active session.
    async fn script_api(&self) -> ToolResult<CallToolResult> {
        self.touch().await;
        let api = self.sessions.api().await.map_err(tool_error)?;
        Ok(CallToolResult::new().with_text_content(api))
    }

    #[tool]
    /// Return the fixture catalog for the active session.
    async fn fixtures(&self) -> ToolResult<CallToolResult> {
        self.touch().await;
        let fixtures = self.sessions.fixtures().await.map_err(tool_error)?;
        let value = serde_json::to_value(fixtures).map_err(tool_error)?;
        Ok(CallToolResult::new()
            .with_structured_content(value.clone())
            .with_text_content(value.to_string()))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = LoadedConfig::load()?;

    match cli.command {
        Commands::Run(args) => run_command(config, args).await,
        Commands::Mcp => mcp_command(config).await,
        Commands::Smoke(args) => smoke_command(config, args).await,
        Commands::Replay(args) => replay_command(config, args).await,
        Commands::Bootstrap(args) => bootstrap_command(config, args).await,
        Commands::Fixtures(args) => fixtures_command(config, args).await,
        Commands::Eval(args) => eval_command(config, args).await,
        Commands::Api(args) => api_command(config, args).await,
    }
}

/// Execute `canopyctl run`.
async fn run_command(config: LoadedConfig, args: RunArgs) -> Result<()> {
    let socket_path = temp_socket_path(&config.config_dir)?;
    let mut command = config
        .run_command(&args.command, &socket_path)?
        .to_command();
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());

    eprintln!("{}", socket_path.display());
    let mut child = command.spawn().context("spawn interactive app")?;

    if let Some(fixture) = args.fixture {
        wait_for_socket(&socket_path, Duration::from_secs(5)).await?;
        let mut session = Session::connect_live(&socket_path).await?;
        session.apply_fixture(fixture).await?;
    }

    let status = child.wait().await.context("wait for interactive app")?;
    if let Some(code) = status.code() {
        exit(code);
    }
    Ok(())
}

/// Execute `canopyctl smoke`.
async fn smoke_command(config: LoadedConfig, args: SmokeArgs) -> Result<()> {
    let command = config.headless_command(&args.command)?;
    let session = Session::spawn_headless(&command).await?;
    let suite_dir = config.smoke_suite_dir(args.suite.as_deref());
    let scripts = collect_smoke_scripts(&suite_dir, &args.scripts)?;
    let timeout_ms = config.smoke_timeout_ms(args.timeout_ms);
    let fail_fast = config.smoke_fail_fast(args.fail_fast);

    let mut failed = 0usize;
    for script in scripts {
        let started = Instant::now();
        let outcome = session
            .eval(ScriptEvalRequest {
                script: script.source,
                fixture: script.fixture.clone(),
                timeout_ms,
            })
            .await?;
        let elapsed = started.elapsed().as_millis();
        let fixture = script.fixture.as_deref().unwrap_or("-");
        let test_name = smoke_test_name(&suite_dir, &script.path);
        if outcome.success {
            println!("PASS fixture={fixture} test={test_name} ({elapsed}ms)");
        } else {
            failed += 1;
            println!("FAIL fixture={fixture} test={test_name} ({elapsed}ms)");
            if let Some(error) = outcome.error {
                println!("  {}", error.message);
            }
            if fail_fast {
                break;
            }
        }
    }

    if failed > 0 {
        bail!("{failed} smoke script(s) failed");
    }
    Ok(())
}

/// Execute `canopyctl replay`.
async fn replay_command(config: LoadedConfig, args: ReplayArgs) -> Result<()> {
    let command = config.headless_command(&args.command)?;
    let session = Session::spawn_headless(&command).await?;
    let journal = load_replay_journal(&args.journal)?;
    let mut failed = 0usize;
    let mut skipped = 0usize;

    for (index, entry) in journal.into_iter().enumerate() {
        let replay_id = index + 1;
        if entry.ok == Some(false) && !args.include_failed {
            skipped += 1;
            println!(
                "SKIP replay#{replay_id} origin={} (originally failed)",
                entry.origin()
            );
            continue;
        }
        let script = entry.source()?;
        let outcome = session
            .eval(ScriptEvalRequest {
                script: script.to_string(),
                fixture: args.fixture.clone(),
                timeout_ms: args.timeout_ms,
            })
            .await?;
        if outcome.success {
            println!("PASS replay#{replay_id} origin={}", entry.origin());
        } else {
            failed += 1;
            println!("FAIL replay#{replay_id} origin={}", entry.origin());
            if let Some(error) = outcome.error {
                println!("  {}", error.message);
            }
            if args.fail_fast {
                break;
            }
        }
    }

    if failed > 0 {
        bail!("{failed} replay entries failed; {skipped} skipped");
    }
    Ok(())
}

/// Execute `canopyctl bootstrap`.
async fn bootstrap_command(config: LoadedConfig, args: SpawnArgs) -> Result<()> {
    let command = config.headless_command(&args.command)?;
    let session = Session::spawn_headless(&command).await?;
    let bootstrap = session.bootstrap().await?;
    println!("{}", serde_json::to_string_pretty(&bootstrap)?);
    Ok(())
}

/// Format a smoke script path relative to the suite root and fixture.
fn smoke_test_name(suite_dir: &Path, script_path: &Path) -> String {
    let relative = script_path
        .strip_prefix(suite_dir)
        .unwrap_or(script_path)
        .to_path_buf();
    let fixture = fixture_for_script(suite_dir, script_path);

    if let Some(fixture) = fixture {
        let fixture_path = Path::new(&fixture);
        if let Ok(without_fixture) = relative.strip_prefix(fixture_path) {
            return without_fixture.display().to_string();
        }
    }

    relative.display().to_string()
}

/// Execute `canopyctl fixtures`.
async fn fixtures_command(config: LoadedConfig, args: SpawnArgs) -> Result<()> {
    let command = config.headless_command(&args.command)?;
    let session = Session::spawn_headless(&command).await?;
    for fixture in session.fixtures().await? {
        println!("{}\t{}", fixture.name, fixture.description);
    }
    Ok(())
}

/// Execute `canopyctl eval`.
async fn eval_command(config: LoadedConfig, args: EvalArgs) -> Result<()> {
    if args.file.is_some() == args.script.is_some() {
        bail!("pass exactly one of -f/--file or an inline SCRIPT");
    }
    let command = config.headless_command(&args.command)?;
    let session = Session::spawn_headless(&command).await?;
    let script = read_eval_script(args.file.as_deref(), args.script.as_deref())?;
    let outcome = session
        .eval(ScriptEvalRequest {
            script: script.clone(),
            fixture: args.fixture,
            timeout_ms: args.timeout_ms,
        })
        .await?;
    println!("{}", serde_json::to_string_pretty(&outcome)?);
    if let Some(path) = args.journal_out {
        write_replay_journal(&path, replay_entry_from_eval(script, &outcome))?;
    }
    if !outcome.success {
        exit(1);
    }
    Ok(())
}

/// Execute `canopyctl api`.
async fn api_command(config: LoadedConfig, args: SpawnArgs) -> Result<()> {
    let command = config.headless_command(&args.command)?;
    let session = Session::spawn_headless(&command).await?;
    print!("{}", session.api().await?);
    Ok(())
}

/// Execute `canopyctl mcp`.
async fn mcp_command(config: LoadedConfig) -> Result<()> {
    let sessions = Arc::new(SessionManager::new(config.clone()));
    let last_activity = Arc::new(Mutex::new(Instant::now()));
    let idle_timeout = config.idle_shutdown_after();
    let watchdog_activity = last_activity.clone();
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(1)).await;
            if watchdog_activity.lock().await.elapsed() >= idle_timeout {
                exit(0);
            }
        }
    });

    tmcp::Server::new(move || CanopyctlMcpServer {
        sessions: sessions.clone(),
        last_activity: last_activity.clone(),
    })
    .serve_stdio()
    .await?;
    Ok(())
}

/// Discover `.canopyctl.toml` by walking upward until the repository root.
fn discover_config_path(start: &Path) -> Result<Option<PathBuf>> {
    let repo_root = find_repo_root(start);
    for ancestor in start.ancestors() {
        let candidate = ancestor.join(".canopyctl.toml");
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
        if Some(ancestor) == repo_root.as_deref() {
            break;
        }
    }
    Ok(None)
}

/// Find the nearest repository root containing a `.git` directory.
fn find_repo_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Build a unique temporary Unix-domain socket path under `./tmp`.
fn temp_socket_path(base_dir: &Path) -> Result<PathBuf> {
    let tmp_dir = base_dir.join("tmp");
    fs::create_dir_all(&tmp_dir).with_context(|| format!("create {}", tmp_dir.display()))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?
        .as_millis();
    Ok(tmp_dir.join(format!("cctl-{}-{stamp}.sock", id())))
}

/// Wait until a Unix-domain socket is ready to accept connections.
async fn wait_for_socket(socket_path: &Path, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if socket_path.exists() && UnixStream::connect(socket_path).await.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(50)).await;
    }
    bail!("timed out waiting for {}", socket_path.display());
}

/// Discover smoke scripts and associate each one with its fixture.
fn collect_smoke_scripts(suite_dir: &Path, requested: &[PathBuf]) -> Result<Vec<SmokeScript>> {
    if requested.is_empty() {
        let mut files = Vec::new();
        collect_suite_entries(suite_dir, &mut files)?;
        if files.is_empty() {
            bail!("no .luau scripts found under {}", suite_dir.display());
        }
        files.sort();
        return files
            .into_iter()
            .map(|path| load_smoke_script(suite_dir, path))
            .collect();
    }

    requested
        .iter()
        .map(|path| {
            let resolved = if path.is_absolute() {
                path.clone()
            } else {
                suite_dir.join(path)
            };
            load_smoke_script(suite_dir, resolved)
        })
        .collect()
}

/// Recursively collect `.luau` files under a suite directory.
fn collect_suite_entries(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_suite_entries(&path, out)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("luau") {
            out.push(path);
        }
    }
    Ok(())
}

/// Load one smoke script from disk together with its derived fixture.
fn load_smoke_script(suite_dir: &Path, path: PathBuf) -> Result<SmokeScript> {
    let source = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(SmokeScript {
        fixture: fixture_for_script(suite_dir, &path),
        path,
        source,
    })
}

/// Derive a fixture name from the first path component under the suite root.
fn fixture_for_script(suite_dir: &Path, script_path: &Path) -> Option<String> {
    let relative = script_path.strip_prefix(suite_dir).ok()?;
    let mut components = relative.components();
    let first = components.next()?;
    components.next()?;
    match first {
        Component::Normal(name) => Some(name.to_string_lossy().to_string()),
        _ => None,
    }
}

/// Read an eval script from either a file or an inline string.
fn read_eval_script(file: Option<&Path>, inline: Option<&str>) -> Result<String> {
    match (file, inline) {
        (Some(file), None) => {
            fs::read_to_string(file).with_context(|| format!("read {}", file.display()))
        }
        (None, Some(inline)) => Ok(inline.to_string()),
        _ => bail!("pass exactly one of -f/--file or an inline SCRIPT"),
    }
}

/// Load replay entries from a journal file.
fn load_replay_journal(path: &Path) -> Result<Vec<ReplayEntry>> {
    let contents = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_str::<ReplayInput>(&contents)
        .with_context(|| format!("parse {}", path.display()))?
        .into_entries())
}

/// Write a single-entry replay journal.
fn write_replay_journal(path: &Path, entry: ReplayEntry) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let journal = ReplayJournal {
        journal: vec![entry],
    };
    let encoded = serde_json::to_string_pretty(&journal)?;
    fs::write(path, encoded).with_context(|| format!("write {}", path.display()))
}

/// Build a replay entry from an eval outcome and original source.
fn replay_entry_from_eval(source: String, outcome: &ScriptEvalOutcome) -> ReplayEntry {
    ReplayEntry {
        id: None,
        origin: Some("canopyctl eval".to_string()),
        source: Some(source),
        script: None,
        ok: Some(outcome.success),
        error: outcome.error.as_ref().map(|error| error.message.clone()),
        logs: outcome.logs.clone(),
        assertions: outcome.assertions.clone(),
        duration_ms: Some(outcome.timing.total_ms),
    }
}

/// Convert an arbitrary error into a tmcp tool error.
fn tool_error(error: impl Display) -> ToolError {
    ToolError::internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_is_first_path_component_under_suite_root() {
        let suite = Path::new("/tmp/smoke");
        let script = Path::new("/tmp/smoke/with_items/navigation.luau");
        assert_eq!(
            fixture_for_script(suite, script),
            Some("with_items".to_string())
        );
    }

    #[test]
    fn root_level_scripts_have_no_fixture() {
        let suite = Path::new("/tmp/smoke");
        let script = Path::new("/tmp/smoke/bootstrap.luau");
        assert_eq!(fixture_for_script(suite, script), None);
    }

    #[test]
    fn replay_input_accepts_object_journal() -> Result<()> {
        let parsed = serde_json::from_str::<ReplayInput>(
            r#"{"journal":[{"origin":"eval","source":"return true","ok":true}]}"#,
        )?;
        let entries = parsed.into_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source()?, "return true");
        Ok(())
    }

    #[test]
    fn replay_input_accepts_bare_script_array() -> Result<()> {
        let parsed = serde_json::from_str::<ReplayInput>(
            r#"[{"origin":"manual","script":"canopy.assert(true, \"ok\")"}]"#,
        )?;
        let entries = parsed.into_entries();
        assert_eq!(entries[0].origin(), "manual");
        assert_eq!(entries[0].source()?, "canopy.assert(true, \"ok\")");
        Ok(())
    }
}
