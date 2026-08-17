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
    path::{Path, PathBuf},
    process::{Stdio, exit, id},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use canopy_mcp::{
    ApplyFixtureRequest, ScriptEvalRequest, SuiteConfig, discover_scripts, fixture_for_script,
    json_tool_result,
};
use clap::{Args, Parser, Subcommand};
use tmcp::{ToolError, ToolResult, mcp_server, schema::CallToolResult, tool_params};
use tokio::{net::UnixStream, sync::Mutex, time::sleep};

use crate::{
    config::LoadedConfig,
    replay::{load_replay_journal, replay_entry_from_eval, write_replay_journal},
    session::{Session, SessionManager},
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
        Ok(json_tool_result(&value))
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
        Ok(json_tool_result(&value))
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
        Ok(json_tool_result(&value))
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
    let scripts = discover_scripts(&suite)?;
    let timeout_ms = config.smoke_timeout_ms(args.timeout_ms);
    let fail_fast = config.smoke_fail_fast(args.fail_fast);

    let mut failed = 0usize;
    for path in scripts {
        let source =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let script_fixture = fixture_for_script(&suite_dir, &path);
        let started = Instant::now();
        let outcome = session
            .eval(ScriptEvalRequest {
                script: source,
                fixture: script_fixture.clone(),
                timeout_ms,
            })
            .await?;
        let elapsed = started.elapsed().as_millis();
        let fixture = script_fixture.as_deref().unwrap_or("-");
        let test_name = smoke_test_name(&suite_dir, &path);
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
        if entry.originally_failed() && !args.include_failed {
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

#[cfg(test)]
mod tests {
    use crate::replay::ReplayInput;

    use super::*;

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
