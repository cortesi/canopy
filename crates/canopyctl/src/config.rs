//! CLI configuration and `.canopyctl.toml` resolution.

use std::{
    collections::HashMap,
    env, fmt, fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use canopy_term::{TermCommand, is_cargo_package_var};
use serde::Deserialize;
use tokio::process::Command;

use crate::screenshot::ScreenshotOptions;

/// Parsed `.canopyctl.toml` contents.
#[derive(Debug, Default, Clone, Deserialize)]
struct FileConfig {
    /// Application launch settings.
    app: Option<AppSection>,
    /// Smoke-runner settings.
    smoke: Option<SmokeSection>,
    /// Screenshot gallery settings.
    gallery: Option<GallerySection>,
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
    /// Extra args appended to the interactive command to inject the socket
    /// path.
    mcp_args: Option<Vec<String>>,
    /// Working directory for spawned processes.
    cwd: Option<PathBuf>,
    /// Environment variables merged into spawned processes.
    env: Option<HashMap<String, String>>,
    /// Directory of the sockets of a run.
    socket_dir: Option<PathBuf>,
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

/// `[gallery]` section from `.canopyctl.toml`.
#[derive(Debug, Default, Clone, Deserialize)]
struct GallerySection {
    /// Directory of gallery scripts.
    suite: Option<PathBuf>,
    /// Directory that receives the images and the viewer.
    out: Option<PathBuf>,
    /// Screen sizes as `WIDTHxHEIGHT`.
    sizes: Option<Vec<String>>,
    /// Per-script timeout in milliseconds.
    timeout_ms: Option<u64>,
    /// Font size in pixels, before the scale.
    font_size: Option<f32>,
    /// Image pixels for each pixel of the font size.
    scale: Option<f32>,
    /// Title of the viewer.
    title: Option<String>,
    /// Command that builds the fixture of a run in the directory given as
    /// its last argument, before the app starts.
    setup: Option<Vec<String>>,
    /// Command that serves the gallery, in place of `[app].headless`.
    /// `{fixture}` in an argument stands for the fixture directory.
    headless: Option<Vec<String>>,
}

/// Gallery settings after defaults and overrides.
#[derive(Debug, Clone)]
pub struct GallerySettings {
    /// Directory of gallery scripts.
    pub suite: PathBuf,
    /// Directory that receives the images and the viewer.
    pub out: PathBuf,
    /// Screen sizes as `WIDTHxHEIGHT`.
    pub sizes: Vec<String>,
    /// Per-script timeout in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Font size in pixels, before the scale.
    pub font_size: f32,
    /// Image pixels for each pixel of the font size.
    pub scale: f32,
    /// Title of the viewer.
    pub title: String,
}

/// `[mcp]` section from `.canopyctl.toml`.
#[derive(Debug, Default, Clone, Deserialize)]
struct McpSection {
    /// Idle time after which `canopyctl mcp` shuts down its app.
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
        if let Some(config_path) = discover_config_path(&cwd) {
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

    /// Return a config of `toml`, resolved against `dir`, for a test.
    #[cfg(test)]
    pub(crate) fn from_toml(toml: &str, dir: &Path) -> Self {
        Self {
            file: toml::from_str(toml).expect("a test config parses"),
            config_dir: dir.to_path_buf(),
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

    /// Return `argv` as a command that runs as the app does: in `[app].cwd`,
    /// else the config directory, with `[app].env` merged into its
    /// environment.
    fn app_command(&self, argv: Vec<String>) -> Result<ResolvedCommand> {
        let app = self.file.app.as_ref();
        let cwd = app
            .and_then(|app| app.cwd.as_ref())
            .map_or_else(|| self.config_dir.clone(), |cwd| self.resolve_path(cwd));
        let env = app.and_then(|app| app.env.clone()).unwrap_or_default();
        ResolvedCommand::new(argv, cwd, env)
    }

    /// Return the directory of the sockets of a run: `[app].socket_dir`, else
    /// `tmp/` beside the config. The path drops each `.` and `..` lexically,
    /// because a socket path has a small bound.
    pub fn socket_dir(&self) -> PathBuf {
        let dir = self
            .file
            .app
            .as_ref()
            .and_then(|app| app.socket_dir.as_ref())
            .map_or_else(|| self.config_dir.join("tmp"), |dir| self.resolve_path(dir));
        normalize_lexically(&dir)
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
        self.app_command(base.into_iter().chain(replaced).collect())
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
        self.app_command(argv)
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

    /// Resolve the gallery settings. Paths resolve against the config
    /// directory.
    pub fn gallery(&self) -> GallerySettings {
        let section = self.file.gallery.clone().unwrap_or_default();
        let title = section.title.unwrap_or_else(|| {
            self.config_dir.file_name().map_or_else(
                || "gallery".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            )
        });
        let shot = ScreenshotOptions::default();
        GallerySettings {
            suite: self.resolve_path(&section.suite.unwrap_or_else(|| "gallery".into())),
            out: self.resolve_path(&section.out.unwrap_or_else(|| "tmp/gallery".into())),
            sizes: section.sizes.unwrap_or_else(|| vec!["100x30".to_owned()]),
            timeout_ms: section.timeout_ms,
            font_size: section.font_size.unwrap_or(shot.font_size),
            scale: section.scale.unwrap_or(shot.scale),
            title,
        }
    }

    /// Return the command that builds the gallery's fixture in `fixture`, or
    /// none when `[gallery].setup` names none. It runs as the app does, with
    /// the directory as its last argument.
    pub fn gallery_setup(&self, fixture: &Path) -> Option<Result<ResolvedCommand>> {
        let setup = self.file.gallery.as_ref()?.setup.clone()?;
        let argv = setup
            .into_iter()
            .chain([fixture.display().to_string()])
            .collect();
        Some(self.app_command(argv))
    }

    /// Build the command that serves the gallery: the override, else
    /// `[gallery].headless`, else `[app].headless`. With a `fixture`, each
    /// `{fixture}` in an argument becomes its path.
    pub fn gallery_command(
        &self,
        override_command: &[String],
        fixture: Option<&Path>,
    ) -> Result<ResolvedCommand> {
        let configured = self
            .file
            .gallery
            .as_ref()
            .and_then(|gallery| gallery.headless.clone());
        let Some(argv) = (!override_command.is_empty())
            .then(|| override_command.to_vec())
            .or(configured)
        else {
            return self.headless_command(&[]);
        };
        let argv = argv
            .into_iter()
            .map(|arg| match fixture {
                Some(fixture) => arg.replace("{fixture}", &fixture.display().to_string()),
                None => arg,
            })
            .collect();
        self.app_command(argv)
    }

    /// Resolve the idle time after which `canopyctl mcp` shuts down its app.
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

impl fmt::Display for ResolvedCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.argv.join(" "))
    }
}

impl ResolvedCommand {
    /// Construct a validated resolved command.
    pub fn new(argv: Vec<String>, cwd: PathBuf, env: HashMap<String, String>) -> Result<Self> {
        if argv.is_empty() {
            bail!("command is empty");
        }
        Ok(Self { argv, cwd, env })
    }

    /// Convert the resolved command into one for a hosted terminal.
    pub fn to_term_command(&self) -> TermCommand {
        let mut command = TermCommand::new(&self.argv[0])
            .args(&self.argv[1..])
            .cwd(&self.cwd);
        for (key, value) in &self.env {
            command = command.env(key, value);
        }
        command
    }

    /// Convert the resolved command into a `tokio::process::Command`. The
    /// child inherits the environment without the launching package's Cargo
    /// identity, as a hosted terminal's does, so a Cargo command under any
    /// wrapper keeps its own.
    pub fn to_command(&self) -> Command {
        let mut command = Command::new(&self.argv[0]);
        for (name, _) in env::vars_os() {
            if name.to_str().is_some_and(is_cargo_package_var) {
                command.env_remove(name);
            }
        }
        command.args(&self.argv[1..]);
        command.current_dir(&self.cwd);
        command.envs(&self.env);
        command
    }
}

/// Discover `.canopyctl.toml` by walking upward until the repository root.
fn discover_config_path(start: &Path) -> Option<PathBuf> {
    let repo_root = find_repo_root(start);
    for ancestor in start.ancestors() {
        let candidate = ancestor.join(".canopyctl.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        if Some(ancestor) == repo_root.as_deref() {
            break;
        }
    }
    None
}

/// Find the nearest repository root containing a `.git` directory.
fn find_repo_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Drop each `.` of `path`, and each `..` with the name before it, without a
/// look at the file system. A `..` above the start of a relative path stays.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normal.components().next_back() {
                Some(Component::Normal(_)) => {
                    normal.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                Some(Component::ParentDir | Component::CurDir) | None => {
                    normal.push(component);
                }
            },
            _ => normal.push(component),
        }
    }
    normal
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    /// Return a config of `toml`, resolved against `dir`.
    fn config(toml: &str, dir: &Path) -> LoadedConfig {
        LoadedConfig::from_toml(toml, dir)
    }

    #[test]
    fn a_gallery_command_names_its_fixture() {
        let dir = PathBuf::from("/project");
        let fixture = Path::new("/tmp/fixture");
        let both = config(
            r#"
            [app]
            headless = ["app", "smoke"]
            [gallery]
            setup = ["build"]
            headless = ["app", "mcp", "{fixture}/tree"]
            "#,
            &dir,
        );
        let setup = both.gallery_setup(fixture).unwrap().unwrap();
        assert_eq!(setup.to_string(), "build /tmp/fixture");
        assert_eq!(
            both.gallery_command(&[], Some(fixture))
                .unwrap()
                .to_string(),
            "app mcp /tmp/fixture/tree"
        );
        assert_eq!(
            both.gallery_command(&["other".to_owned()], Some(fixture))
                .unwrap()
                .to_string(),
            "other",
            "the command after -- wins"
        );

        let plain = config("[app]\nheadless = [\"app\", \"smoke\"]\n", &dir);
        assert!(plain.gallery_setup(fixture).is_none());
        assert_eq!(
            plain.gallery_command(&[], None).unwrap().to_string(),
            "app smoke",
            "with no gallery command, the app's serves"
        );
    }

    #[test]
    fn the_sockets_of_a_run_go_to_the_configured_directory() {
        let dir = PathBuf::from("/repo/project");
        assert_eq!(
            config("", &dir).socket_dir(),
            Path::new("/repo/project/tmp")
        );
        assert_eq!(
            config("[app]\nsocket_dir = \"../tmp/./project\"\n", &dir).socket_dir(),
            Path::new("/repo/tmp/project"),
            "the socket directory drops `.` and `..`, so that each socket path stays short"
        );
        for (path, normal) in [
            ("/a/../../b", "/b"),
            ("../a/./b/..", "../a"),
            ("a/../../b", "../b"),
        ] {
            assert_eq!(
                normalize_lexically(Path::new(path)),
                Path::new(normal),
                "{path}"
            );
        }
        assert_eq!(
            config("[app]\nsocket_dir = \"/run/sockets\"\n", &dir).socket_dir(),
            Path::new("/run/sockets")
        );
    }

    #[test]
    fn a_command_of_any_program_drops_the_launching_package_identity() {
        // Cargo and Nextest set the package variables for a test process.
        assert!(env::var_os("CARGO_PKG_NAME").is_some());
        let command = ResolvedCommand::new(
            vec!["sh".into(), "-c".into(), "true".into()],
            PathBuf::from("."),
            HashMap::new(),
        )
        .unwrap()
        .to_command();
        let removed: Vec<&OsStr> = command
            .as_std()
            .get_envs()
            .filter_map(|(name, value)| value.is_none().then_some(name))
            .collect();
        assert!(
            removed.contains(&OsStr::new("CARGO_PKG_NAME")),
            "{removed:?}"
        );
        assert!(
            removed.contains(&OsStr::new("CARGO_MANIFEST_DIR")),
            "{removed:?}"
        );
    }
}
