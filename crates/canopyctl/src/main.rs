#![deny(unsafe_code)]
//! Command-line tooling for running and automating canopy apps.

/// CLI configuration and `.canopyctl.toml` resolution.
mod config;
/// Replay journal types and their file IO.
mod replay;
/// MCP client sessions and the manager shared by the CLI and the proxy server.
mod session;

use std::{
    fmt::Display,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Stdio, exit, id},
    result,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use canopy_mcp::{
    ApplyFixtureRequest, ApplyFixtureResponse, BootstrapRequest, ScriptEvalOutcome,
    ScriptEvalRequest, SuiteConfig, Viewport, discover_scripts, fixture_for_script,
};
use clap::{Args, Parser, Subcommand};
use ruau_script_api::{ScriptApiQuery, ScriptApiResponse};
use tmcp::{ToolError, ToolResult, mcp_server, schema::CallToolResult, tool_params};
use tokio::{net::UnixStream, sync::Mutex, time::sleep};

use crate::{
    config::LoadedConfig,
    replay::{
        ReplayEnvelope, ReplayExpectation, ReplayFile, ReplayStep, load_replay, validate_viewport,
        write_replay,
    },
    session::{Session, SessionKind, SessionManager},
};

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
    /// Replay a versioned envelope against an explicit live or headless target.
    Replay(ReplayArgs),
    /// Print MCP bootstrap information from a headless app instance.
    Bootstrap(SpawnArgs),
    /// List registered fixtures from a headless app instance.
    Fixtures(SpawnArgs),
    /// Evaluate one Luau script against a headless app instance.
    Eval(EvalArgs),
    /// Discover the checked Luau API of a headless app instance.
    Api(ApiArgs),
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

/// Shared arguments for subcommands that only need an optional command
/// override.
#[derive(Args)]
struct SpawnArgs {
    /// Command override passed after `--`.
    #[arg(last = true)]
    command: Vec<String>,
}

/// Arguments for checked Luau API discovery.
#[derive(Args)]
struct ApiArgs {
    /// List canonical public paths.
    #[arg(long)]
    list: bool,
    /// Show one exact or unambiguous source or item.
    #[arg(long)]
    filter: Option<String>,
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
#[derive(Args, Default)]
struct ReplayArgs {
    /// Connect to this live socket instead of spawning a headless command.
    #[arg(long, conflicts_with = "command")]
    socket: Option<PathBuf>,
    /// Permit legacy journals with incomplete reproduction metadata.
    #[arg(long)]
    legacy: bool,
    /// Report and permit each compatibility mismatch before executing sources.
    #[arg(long)]
    allow_mismatch: bool,
    /// Explicit WIDTHxHEIGHT for legacy headless replay.
    #[arg(long, value_parser = parse_viewport)]
    viewport: Option<Viewport>,
    /// Path to a JSON replay journal.
    journal: PathBuf,
    /// Fixture for legacy replay (once for live, per eval for headless).
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
    /// Optional headless viewport as WIDTHxHEIGHT.
    #[arg(long, value_parser = parse_viewport)]
    viewport: Option<Viewport>,
    /// Write a versioned replay envelope containing this evaluation.
    #[arg(long)]
    journal_out: Option<PathBuf>,
    /// Command override passed after `--`.
    #[arg(last = true)]
    command: Vec<String>,
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
    async fn bootstrap(&self, request: BootstrapRequest) -> ToolResult<CallToolResult> {
        self.touch().await;
        let bootstrap = self.sessions.bootstrap(request).await.map_err(tool_error)?;
        to_tool_result(bootstrap)
    }

    #[tool]
    /// Apply a fixture to the active session.
    async fn apply_fixture(&self, params: ApplyFixtureRequest) -> ToolResult<CallToolResult> {
        self.touch().await;
        self.sessions
            .apply_fixture(params.name.clone())
            .await
            .map_err(tool_error)?;
        to_tool_result(ApplyFixtureResponse {
            applied: params.name,
        })
    }

    #[tool(read_only, output_schema = ScriptApiResponse)]
    /// Return shared API discovery for the active session.
    async fn script_api(&self, params: ScriptApiQuery) -> ToolResult<CallToolResult> {
        self.touch().await;
        self.sessions.api(&params).await.map_err(tool_error)
    }

    #[tool]
    /// Return the fixture catalog for the active session.
    async fn fixtures(&self) -> ToolResult<CallToolResult> {
        self.touch().await;
        let fixtures = self.sessions.fixtures().await.map_err(tool_error)?;
        to_tool_result(fixtures)
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
    let mut suite = SuiteConfig::new(&suite_dir);
    suite.scripts = args.scripts;
    suite.timeout_ms = config.smoke_timeout_ms(args.timeout_ms);
    suite.fail_fast = config.smoke_fail_fast(args.fail_fast);

    smoke_scripts(&session, &suite).await
}

/// Run resolved smoke scripts through one connected session.
async fn smoke_scripts(session: &Session, suite: &SuiteConfig) -> Result<()> {
    let mut failed = 0usize;
    for path in discover_scripts(suite)? {
        let source =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let script_fixture = fixture_for_script(&suite.suite_dir, &path);
        let started = Instant::now();
        let outcome = session
            .eval(ScriptEvalRequest {
                fixture: script_fixture.clone(),
                timeout_ms: suite.timeout_ms,
                ..ScriptEvalRequest::new(source)
            })
            .await?;
        let elapsed = started.elapsed().as_millis();
        let fixture = script_fixture.as_deref().unwrap_or("-");
        let test_name = smoke_test_name(&suite.suite_dir, &path, script_fixture.as_deref());
        if outcome.success {
            println!("PASS fixture={fixture} test={test_name} ({elapsed}ms)");
        } else {
            failed += 1;
            println!("FAIL fixture={fixture} test={test_name} ({elapsed}ms)");
            if let Some(error) = outcome.error {
                println!("  {}", error.message);
            }
            if suite.fail_fast {
                break;
            }
        }
    }

    if failed > 0 {
        bail!("{failed} smoke script(s) failed");
    }
    Ok(())
}

/// Parse and validate one CLI viewport before an app is spawned.
fn parse_viewport(text: &str) -> result::Result<Viewport, String> {
    let (width, height) = text
        .split_once('x')
        .ok_or_else(|| "viewport must be WIDTHxHEIGHT".to_owned())?;
    let viewport = Viewport {
        width: width.parse().map_err(|_| "invalid viewport width")?,
        height: height.parse().map_err(|_| "invalid viewport height")?,
    };
    validate_viewport(viewport).map_err(|error| error.to_string())?;
    Ok(viewport)
}

/// Execute `canopyctl replay` after parsing all sources and assumptions.
async fn replay_command(config: LoadedConfig, args: ReplayArgs) -> Result<()> {
    let input = load_replay(&args.journal, args.legacy)?;
    validate_replay_options(&input, &args)?;
    let mut session = match &args.socket {
        Some(socket) => Session::connect_live(socket).await?,
        None => Session::spawn_headless(&config.headless_command(&args.command)?).await?,
    };
    let result = replay_file(&mut session, input, &args).await;
    session.shutdown().await;
    result
}

/// Validate compatibility before applying fixtures or running any replay step.
async fn replay_file(session: &mut Session, input: ReplayFile, args: &ReplayArgs) -> Result<()> {
    validate_replay_options(&input, args)?;
    let (steps, fixture, viewport) = match input {
        ReplayFile::Versioned(envelope) => {
            if args.fixture.is_some() || args.viewport.is_some() {
                bail!("fixture and viewport overrides are only accepted for legacy journals");
            }
            envelope.validate()?;
            let requested = (session.kind() == SessionKind::Headless).then_some(envelope.viewport);
            let target = session
                .bootstrap(BootstrapRequest {
                    viewport: requested,
                })
                .await?;
            validate_viewport(
                target
                    .metadata
                    .viewport
                    .context("target bootstrap has no viewport")?,
            )?;
            if let Some(name) = &envelope.fixture
                && !target.fixtures.iter().any(|fixture| fixture.name == *name)
            {
                bail!("replay fixture {name:?} is not implemented by the target");
            }
            let differences = envelope.mismatches(&target.metadata)?;
            if !differences.is_empty() && !args.allow_mismatch {
                bail!("replay compatibility mismatch:\n{}", differences.join("\n"));
            }
            for difference in differences {
                eprintln!("OVERRIDE {difference}");
            }
            (envelope.steps, envelope.fixture, requested)
        }
        ReplayFile::Legacy(entries) => {
            eprintln!(
                "LEGACY replay: metadata is incomplete; complete reproduction is not claimed"
            );
            if let Some(viewport) = args.viewport {
                validate_viewport(viewport)?;
            }
            if session.kind() == SessionKind::Live && args.viewport.is_some() {
                bail!("legacy viewport selection is only supported by headless sessions");
            }
            validate_fixture(session, args.fixture.as_deref()).await?;
            let steps = entries
                .into_iter()
                .map(|entry| {
                    Ok(ReplayStep {
                        source: entry.source()?.to_owned(),
                        expect: ReplayExpectation {
                            success: !entry.originally_failed(),
                        },
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            (steps, args.fixture.clone(), args.viewport)
        }
    };
    if !steps
        .iter()
        .any(|step| step.expect.success || args.include_failed)
    {
        return Ok(());
    }
    if session.kind() == SessionKind::Live
        && let Some(name) = &fixture
    {
        session.apply_fixture(name.clone()).await?;
    }
    let inline_fixture = if session.kind() == SessionKind::Headless {
        fixture
    } else {
        None
    };
    replay_entries(session, steps, inline_fixture, viewport, args).await
}

/// Reject conflicting or incomplete target options before connecting.
fn validate_replay_options(input: &ReplayFile, args: &ReplayArgs) -> Result<()> {
    match input {
        ReplayFile::Versioned(_) if args.fixture.is_some() || args.viewport.is_some() => {
            bail!("fixture and viewport overrides are only accepted for legacy journals");
        }
        ReplayFile::Legacy(_) if args.socket.is_none() && args.viewport.is_none() => {
            bail!("legacy headless replay requires an explicit --viewport WIDTHxHEIGHT");
        }
        ReplayFile::Legacy(_) if args.socket.is_some() && args.viewport.is_some() => {
            bail!("legacy viewport selection is only supported by headless sessions");
        }
        _ => Ok(()),
    }
}

/// Reject a missing fixture implementation even when compatibility is
/// overridden.
async fn validate_fixture(session: &Session, fixture: Option<&str>) -> Result<()> {
    if let Some(name) = fixture
        && !session
            .fixtures()
            .await?
            .iter()
            .any(|fixture| fixture.name == name)
    {
        bail!("replay fixture {name:?} is not implemented by the target");
    }
    Ok(())
}

/// Execute selected steps and compare both successful and failed expectations.
async fn replay_entries(
    session: &Session,
    steps: Vec<ReplayStep>,
    fixture: Option<String>,
    viewport: Option<Viewport>,
    args: &ReplayArgs,
) -> Result<()> {
    let mut failed = 0usize;
    let mut skipped = 0usize;
    for (index, step) in steps.into_iter().enumerate() {
        let replay_id = index + 1;
        if !step.expect.success && !args.include_failed {
            skipped += 1;
            println!(
                "SKIP replay#{replay_id} (recorded failure; select --include-failed to execute)"
            );
            continue;
        }
        let outcome = session
            .eval(ScriptEvalRequest {
                fixture: fixture.clone(),
                timeout_ms: args.timeout_ms,
                viewport,
                ..ScriptEvalRequest::new(step.source)
            })
            .await?;
        if outcome.success == step.expect.success {
            println!(
                "PASS replay#{replay_id} expected_success={}",
                step.expect.success
            );
        } else {
            failed += 1;
            println!(
                "FAIL replay#{replay_id} expected_success={} actual_success={}",
                step.expect.success, outcome.success
            );
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
    let bootstrap = session.bootstrap(BootstrapRequest::default()).await?;
    println!("{}", serde_json::to_string_pretty(&bootstrap)?);
    Ok(())
}

/// Format a smoke script path relative to the suite root and its fixture
/// directory.
fn smoke_test_name(suite_dir: &Path, script_path: &Path, fixture: Option<&str>) -> String {
    let relative = script_path
        .strip_prefix(suite_dir)
        .unwrap_or(script_path)
        .to_path_buf();

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
    let script = read_eval_script(args.file.as_deref(), args.script.as_deref())?;
    let command = config.headless_command(&args.command)?;
    let session = Session::spawn_headless(&command).await?;
    let outcome = session
        .eval(ScriptEvalRequest {
            fixture: args.fixture.clone(),
            timeout_ms: args.timeout_ms,
            viewport: args.viewport,
            ..ScriptEvalRequest::new(script.clone())
        })
        .await?;
    write_eval_output(
        &outcome,
        script,
        args.journal_out.as_deref(),
        args.fixture,
        &mut io::stdout().lock(),
    )?;
    if !outcome.success {
        exit(1);
    }
    Ok(())
}

/// Write evaluation output and its optional journal before exit status
/// handling.
fn write_eval_output(
    outcome: &ScriptEvalOutcome,
    script: String,
    journal_out: Option<&Path>,
    fixture: Option<String>,
    output: &mut impl Write,
) -> Result<()> {
    serde_json::to_writer_pretty(&mut *output, outcome)?;
    writeln!(output)?;
    if let Some(path) = journal_out {
        write_replay(path, &ReplayEnvelope::record(script, fixture, outcome)?)?;
    }
    Ok(())
}

/// Execute `canopyctl api`.
async fn api_command(config: LoadedConfig, args: ApiArgs) -> Result<()> {
    let command = config.headless_command(&args.command)?;
    let session = Session::spawn_headless(&command).await?;
    let result = session
        .api(&ScriptApiQuery {
            list: args.list,
            filter: args.filter,
        })
        .await?;
    if result.is_error() {
        bail!(
            "{}",
            result
                .error_message()
                .unwrap_or_else(|| "script_api failed".to_owned())
        );
    }
    print!(
        "{}",
        result
            .text()
            .ok_or_else(|| anyhow::anyhow!("script_api returned no text"))?
    );
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

/// Convert an arbitrary error into a tmcp tool error.
fn tool_error(error: impl Display) -> ToolError {
    ToolError::internal(error.to_string())
}

/// Encode a typed MCP response.
fn to_tool_result(value: impl serde::Serialize) -> ToolResult<CallToolResult> {
    CallToolResult::structured(value).map_err(tool_error)
}

#[cfg(test)]
mod tests {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        thread,
    };

    use canopy::{Canopy, Fixture, Work, geom::Size, testing::contracts};
    use canopy_mcp::{AppMetadata, ExecutionMode, ResetPolicy, serve_uds};
    use futures::{StreamExt, executor};
    use tmcp::schema::ToolResultMode;
    use tokio::runtime::Builder;

    use super::*;
    use crate::{
        replay::ReplayInput,
        session::tests::{evaluator_session, manager_with_session, peer_session, request},
    };

    #[test]
    fn live_replay_applies_fixture_once_and_preserves_state_between_steps() -> Result<()> {
        let mut canopy = Canopy::new();
        let applications = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&applications);
        canopy.register_fixture(Fixture::new("seed", "Seed live state", move |canopy| {
            observed.fetch_add(1, Ordering::Relaxed);
            canopy.set_input_mode("seed");
            Ok(())
        }))?;
        canopy.finalize_api()?;
        canopy.set_root_size(Size::new(20, 5))?;
        canopy.turn(Work::Prepare)?;
        let automation = canopy.automation_handle();
        let mut events = canopy.take_event_receiver().expect("test owns events");
        let directory = tempfile::tempdir()?;
        let socket = directory.path().join("replay.sock");
        let listener = serve_uds(
            &socket,
            automation.clone(),
            AppMetadata {
                app: "canopyctl-test".into(),
                reset: ResetPolicy::External,
            },
        )?;
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| -> Result<()> {
                Builder::new_current_thread().enable_all().build()?.block_on(async {
                    let mut session = Session::connect_live(&socket).await?;
                    assert_eq!(session.kind(), SessionKind::Live);
                    let bootstrap = session.bootstrap(BootstrapRequest::default()).await?;
                    let envelope = ReplayEnvelope {
                        schema: "canopy.replay/1".into(), app: bootstrap.metadata.app,
                        api_digest: bootstrap.metadata.api_digest.expect("live digest"),
                        execution: ExecutionMode::LiveSession,
                        viewport: bootstrap.metadata.viewport.context("live viewport")?,
                        fixture: Some("seed".into()), reset: ResetPolicy::Fixture,
                        steps: [
                            "canopy.assert(canopy.input_mode() == \"seed\"); canopy.set_mode(\"first\")",
                            "canopy.assert(canopy.input_mode() == \"first\"); canopy.set_mode(\"second\")",
                        ].into_iter().map(|source| ReplayStep { source: source.into(), expect: ReplayExpectation { success: true } }).collect(),
                    };
                    let args = ReplayArgs { socket: Some(socket), ..ReplayArgs::default() };
                    replay_file(&mut session, ReplayFile::Versioned(envelope), &args).await
                })
            }));
            done_tx.send(result).expect("test observer connected");
            automation
                .submit(Box::new(|_| {}))
                .expect("wake test observer");
        });
        let result = loop {
            if let Ok(result) = done_rx.try_recv() {
                break result;
            }
            executor::block_on(events.next()).expect("live MCP wakes the UI");
            canopy.turn(Work::Wake)?;
        };
        worker.join().expect("live replay worker exits");
        listener.stop()?;
        result.expect("live replay client did not panic")?;
        assert_eq!(applications.load(Ordering::Relaxed), 1);
        assert_eq!(canopy.input_mode(), "second");
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shared_contract_crosses_real_evaluator_proxy_and_versioned_replay() -> Result<()> {
        let (session, _, peer) = evaluator_session().await?;
        let proxy = CanopyctlMcpServer {
            sessions: manager_with_session(session).await?,
            last_activity: Arc::new(Mutex::new(Instant::now())),
        };
        let mut request = request(contracts::SCRIPT);
        request.viewport = Some(Viewport {
            width: 12,
            height: 3,
        });
        let response = proxy.script_eval(request).await?;
        let outcome = response.extract_as::<ScriptEvalOutcome>(ToolResultMode::Structured)?;
        assert!(outcome.success, "{:?}", outcome.error);
        assert_eq!(
            outcome.value,
            Some(contracts::expected().to_external_json_value()?)
        );
        let envelope = ReplayEnvelope::record(contracts::SCRIPT.to_owned(), None, &outcome)?;
        let encoded = serde_json::to_string(&envelope)?;
        let replay = replay::parse_replay(&encoded, false)?;
        let (mut session, calls, replay_peer) = evaluator_session().await?;
        replay_file(&mut session, replay, &ReplayArgs::default()).await?;
        assert_eq!(calls.lock().await.as_slice(), [contracts::SCRIPT]);
        peer.abort();
        replay_peer.abort();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn strict_preflight_blocks_execution_and_overrides_never_hide_missing_fixtures()
    -> Result<()> {
        let (mut session, calls, peer) = evaluator_session().await?;
        let target = session
            .bootstrap(BootstrapRequest {
                viewport: Some(Viewport {
                    width: 12,
                    height: 3,
                }),
            })
            .await?;
        let mut envelope = ReplayEnvelope {
            schema: "canopy.replay/1".into(),
            app: "wrong app".into(),
            api_digest: "wrong digest".into(),
            execution: ExecutionMode::LiveSession,
            viewport: target.metadata.viewport.context("target viewport")?,
            fixture: None,
            reset: ResetPolicy::External,
            steps: vec![ReplayStep {
                source: contracts::SCRIPT.into(),
                expect: ReplayExpectation { success: true },
            }],
        };
        let error = replay_file(
            &mut session,
            ReplayFile::Versioned(envelope.clone()),
            &ReplayArgs::default(),
        )
        .await
        .unwrap_err();
        for field in ["app:", "api_digest:", "execution:", "reset:"] {
            assert!(error.to_string().contains(field), "{error:#}");
        }
        assert!(calls.lock().await.is_empty());
        let args = ReplayArgs {
            allow_mismatch: true,
            ..ReplayArgs::default()
        };
        replay_file(&mut session, ReplayFile::Versioned(envelope.clone()), &args).await?;
        assert_eq!(calls.lock().await.len(), 1);
        calls.lock().await.clear();
        envelope.fixture = Some("missing fixture".into());
        envelope.reset = ResetPolicy::Fixture;
        let error = replay_file(&mut session, ReplayFile::Versioned(envelope), &args)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("not implemented"));
        assert!(calls.lock().await.is_empty());
        peer.abort();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn selected_recorded_failures_are_executed_and_compared() -> Result<()> {
        let (session, calls, peer) = evaluator_session().await?;
        let steps = vec![ReplayStep {
            source: "error(\"expected failure\")".into(),
            expect: ReplayExpectation { success: false },
        }];
        replay_entries(&session, steps.clone(), None, None, &ReplayArgs::default()).await?;
        assert!(calls.lock().await.is_empty());
        let args = ReplayArgs {
            include_failed: true,
            ..ReplayArgs::default()
        };
        replay_entries(&session, steps, None, None, &args).await?;
        assert_eq!(calls.lock().await.len(), 1);
        let error = replay_entries(
            &session,
            vec![ReplayStep {
                source: "return true".into(),
                expect: ReplayExpectation { success: false },
            }],
            None,
            None,
            &args,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("1 replay entries failed"));
        peer.abort();
        Ok(())
    }

    #[test]
    fn replay_cli_selects_live_or_headless_and_requires_legacy_viewport() -> Result<()> {
        let cli = Cli::try_parse_from([
            "canopyctl",
            "replay",
            "trace.json",
            "--socket",
            "/tmp/app.sock",
        ])?;
        let Commands::Replay(args) = cli.command else {
            panic!("replay command");
        };
        assert_eq!(args.socket, Some(PathBuf::from("/tmp/app.sock")));
        assert!(
            Cli::try_parse_from([
                "canopyctl",
                "replay",
                "trace.json",
                "--socket",
                "/tmp/app.sock",
                "--",
                "todo",
                "mcp"
            ])
            .is_err()
        );
        let legacy = replay::parse_replay("[{\"script\":\"return 1\"}]", true)?;
        assert!(validate_replay_options(&legacy, &ReplayArgs::default()).is_err());
        assert!(parse_viewport("0x3").is_err());
        assert!(parse_viewport("12x0").is_err());
        assert!(parse_viewport("12x3").is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn smoke_failure_accounting_preserves_fail_fast() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let scripts = vec![
            directory.path().join("first.luau"),
            directory.path().join("second.luau"),
        ];
        fs::write(&scripts[0], "failure")?;
        fs::write(&scripts[1], "success")?;
        for fail_fast in [false, true] {
            let (session, calls, peer) = peer_session().await?;
            let mut suite = SuiteConfig::new(directory.path());
            suite.scripts = scripts.clone();
            suite.fail_fast = fail_fast;
            let error = smoke_scripts(&session, &suite).await.unwrap_err();
            assert_eq!(error.to_string(), "1 smoke script(s) failed");
            assert_eq!(
                *calls.lock().await,
                if fail_fast {
                    vec!["failure"]
                } else {
                    vec!["failure", "success"]
                }
            );
            peer.abort();
        }
        Ok(())
    }

    #[tokio::test]
    async fn replay_failure_accounting_preserves_fail_fast() -> Result<()> {
        for fail_fast in [false, true] {
            let (session, calls, peer) = peer_session().await?;
            let journal = serde_json::from_str::<ReplayInput>(
                r#"[{"script":"failure"},{"script":"success"}]"#,
            )?
            .into_entries()
            .into_iter()
            .map(|entry| {
                Ok(ReplayStep {
                    source: entry.source()?.to_owned(),
                    expect: ReplayExpectation { success: true },
                })
            })
            .collect::<Result<Vec<_>>>()?;
            let args = ReplayArgs {
                fail_fast,
                ..ReplayArgs::default()
            };
            let error = replay_entries(&session, journal, None, None, &args)
                .await
                .unwrap_err();
            assert_eq!(error.to_string(), "1 replay entries failed; 0 skipped");
            assert_eq!(
                *calls.lock().await,
                if fail_fast {
                    vec!["failure"]
                } else {
                    vec!["failure", "success"]
                }
            );
            peer.abort();
        }
        Ok(())
    }

    #[tokio::test]
    async fn failed_eval_writes_json_and_replay_journal() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let journal = directory.path().join("journal.json");
        let (session, _, peer) = peer_session().await?;
        let outcome = session.eval(request("failure")).await?;
        let mut output = Vec::new();
        write_eval_output(
            &outcome,
            "failure".to_owned(),
            Some(&journal),
            None,
            &mut output,
        )?;
        let decoded: ScriptEvalOutcome = serde_json::from_slice(&output)?;
        assert!(!decoded.success);
        assert_eq!(decoded, outcome);
        let ReplayFile::Versioned(envelope) = load_replay(&journal, false)? else {
            panic!("versioned recording");
        };
        assert_eq!(envelope.steps.len(), 1);
        assert!(!envelope.steps[0].expect.success);
        assert_eq!(envelope.steps[0].source, "failure");
        peer.abort();
        Ok(())
    }

    #[tokio::test]
    async fn proxy_does_not_acknowledge_failed_fixture() -> Result<()> {
        let (session, _, peer) = peer_session().await?;
        let server = CanopyctlMcpServer {
            sessions: manager_with_session(session).await?,
            last_activity: Arc::new(Mutex::new(Instant::now())),
        };
        assert!(
            server
                .apply_fixture(ApplyFixtureRequest {
                    name: "bad".to_owned()
                })
                .await
                .is_err()
        );
        let success = server
            .apply_fixture(ApplyFixtureRequest {
                name: "good".to_owned(),
            })
            .await?;
        assert_eq!(
            success.structured_content,
            Some(serde_json::json!({"applied":"good"}))
        );
        peer.abort();
        Ok(())
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

    #[test]
    fn api_command_accepts_shared_query_forms() {
        let cli = Cli::try_parse_from([
            "canopyctl",
            "api",
            "--filter",
            "canopy.screen_text",
            "--",
            "todo",
            "mcp",
        ])
        .expect("API query");
        let Commands::Api(args) = cli.command else {
            panic!("API command");
        };
        assert_eq!(args.filter.as_deref(), Some("canopy.screen_text"));
        assert!(!args.list);
        assert_eq!(args.command, ["todo", "mcp"]);
    }
}
