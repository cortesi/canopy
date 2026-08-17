//! CLI configuration and `.canopyctl.toml` resolution.

use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use tokio::process::Command;

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
pub struct LoadedConfig {
    /// Parsed config contents.
    file: FileConfig,
    /// Directory relative paths resolve against.
    pub config_dir: PathBuf,
}

impl LoadedConfig {
    /// Load `.canopyctl.toml` by walking upward from the current directory.
    pub fn load() -> Result<Self> {
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
    pub fn resolve_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.config_dir.join(path)
        }
    }

    /// Return the merged application environment.
    pub fn app_env(&self) -> HashMap<String, String> {
        self.file
            .app
            .as_ref()
            .and_then(|app| app.env.clone())
            .unwrap_or_default()
    }

    /// Return the working directory used for spawned app processes.
    pub fn app_cwd(&self) -> PathBuf {
        self.file
            .app
            .as_ref()
            .and_then(|app| app.cwd.as_ref())
            .map(|cwd| self.resolve_path(cwd))
            .unwrap_or_else(|| self.config_dir.clone())
    }

    /// Build the interactive app command, including injected MCP socket args.
    pub fn run_command(
        &self,
        override_command: &[String],
        socket: &Path,
    ) -> Result<ResolvedCommand> {
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
    pub fn headless_command(&self, override_command: &[String]) -> Result<ResolvedCommand> {
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
    pub fn smoke_suite_dir(&self, override_suite: Option<&Path>) -> PathBuf {
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
    pub fn smoke_timeout_ms(&self, override_timeout_ms: Option<u64>) -> Option<u64> {
        override_timeout_ms.or_else(|| self.file.smoke.as_ref().and_then(|smoke| smoke.timeout_ms))
    }

    /// Resolve the effective fail-fast setting.
    pub fn smoke_fail_fast(&self, override_fail_fast: bool) -> bool {
        override_fail_fast
            || self
                .file
                .smoke
                .as_ref()
                .and_then(|smoke| smoke.fail_fast)
                .unwrap_or(false)
    }

    /// Resolve the effective MCP idle timeout.
    pub fn idle_shutdown_after(&self) -> Duration {
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
pub struct ResolvedCommand {
    /// Program and arguments.
    argv: Vec<String>,
    /// Working directory.
    cwd: PathBuf,
    /// Spawned-process environment variables.
    env: HashMap<String, String>,
}

impl ResolvedCommand {
    /// Construct a validated resolved command.
    pub fn new(argv: Vec<String>, cwd: PathBuf, env: HashMap<String, String>) -> Result<Self> {
        if argv.is_empty() {
            bail!("command is empty");
        }
        Ok(Self { argv, cwd, env })
    }

    /// Convert the resolved command into a `tokio::process::Command`.
    pub fn to_command(&self) -> Command {
        let mut command = Command::new(&self.argv[0]);
        command.args(&self.argv[1..]);
        command.current_dir(&self.cwd);
        command.envs(&self.env);
        command
    }
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
