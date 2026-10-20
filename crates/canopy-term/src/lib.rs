//! Host an interactive Canopy app in a terminal and drive it as a person does.
//!
//! [`TermHost`] runs an app in a pseudo-terminal behind an Itty terminal
//! emulator, so the app runs exactly as it does for a person: in its
//! interactive mode, with terminal input parsing and rendering. Keys reach it
//! as the bytes a terminal sends, and the screen reads back what the terminal
//! shows. Its live automation socket serves scripts beside the terminal.
//!
//! The host never judges from elapsed or quiet time whether the app is done.
//! It starts once the app signals that its automation socket accepts
//! connections. After it sends keys, it waits on the live socket until the
//! app's input count has risen by the keys it sent. To read the screen, it
//! asks the app to write an APC marker after its latest frame, and waits for
//! the terminal to read it: terminal output is ordered, so the terminal has
//! then read that frame. Timeouts only turn a hang into an error.
//!
//! A test starts its package's binary with [`TermHost::start`], passing
//! `{socket}` where the app takes its live automation socket, then drives it
//! with [`TermHost::keys`] and reads it with [`TermHost::screen`] and
//! [`TermHost::eval`]. `examples/gyms/tests/terminal.rs` shows the shape.

mod ready;

use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use canopy_mcp::{EvalReport, READY_SOCKET_ENV, ScreenSize, ScriptEvalRequest};
use itty_headless::{
    DriverError, DriverHandle, EnvironmentPolicy, HeadlessRequest, HeadlessTerminal, Key, KeyCode,
    MatchSpec, Signal, WaitOptions,
};
pub use ready::ReadySignal;
use serde_json::Value as JsonValue;
use tempfile::TempDir;
use tmcp::{Client, schema::ToolResultMode};
use tokio::net::UnixStream;

/// Transcript retained by a hosted terminal, in bytes.
const TRANSCRIPT_BYTES: usize = 4 * 1024 * 1024;
/// How long a stopping app has to exit before the host forces it.
const STOP_GRACE: Duration = Duration::from_secs(2);
/// Longest wait for the app or the terminal to acknowledge input, a frame, or
/// expected text. The waits end on the acknowledgment, so only a hang
/// reaches this.
const ACK_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest wait [`TermHost::start`] allows the app to report ready.
const START_TIMEOUT: Duration = Duration::from_secs(60);
/// Longest wait for the app to exit, after which the host waits again.
const EXIT_WATCH: Duration = Duration::from_secs(3600);
/// Prefix of the APC payload that itty records as a marker.
const MARK_PREFIX: &str = "itty:mark:";
/// The placeholder [`TermHost::start`] replaces with the automation socket.
const SOCKET_PLACEHOLDER: &str = "{socket}";
/// MCP client name reported to the app.
const CLIENT_NAME: &str = "canopy-term";
/// MCP client version reported to the app.
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A command to run in a hosted terminal.
#[derive(Debug, Clone)]
pub struct TermCommand {
    /// The program to run.
    program: OsString,
    /// Its arguments.
    args: Vec<OsString>,
    /// Its working directory, or the host's.
    cwd: Option<PathBuf>,
    /// Variables added to the host's environment.
    env: BTreeMap<OsString, OsString>,
}

impl TermCommand {
    /// Run `program`.
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
        }
    }

    /// Add one argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add arguments.
    #[must_use]
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Run in `dir`.
    #[must_use]
    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    /// Set an environment variable.
    #[must_use]
    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
}

/// An app running in a terminal that the host owns.
pub struct TermHost {
    /// The terminal and its session worker.
    terminal: HeadlessTerminal,
    /// The app's live automation socket.
    socket: PathBuf,
    /// The live MCP client, which acknowledges input and frames, and serves
    /// the scripts of whoever hosts the app.
    client: Arc<Client<()>>,
    /// The last frame marker issued.
    marks: u64,
    /// The temporary directory that holds the sockets of a started app.
    _sockets: Option<TempDir>,
}

impl TermHost {
    /// Start `command` in a terminal of `size`, replacing `{socket}` in its
    /// arguments with a live automation socket in a temporary directory.
    pub async fn start(command: TermCommand, size: ScreenSize) -> Result<Self> {
        let sockets = tempfile::Builder::new().prefix("canopy-term").tempdir()?;
        let socket = sockets.path().join("app.sock");
        let placeholder = OsString::from(SOCKET_PLACEHOLDER);
        let args = command
            .args
            .iter()
            .map(|arg| match arg.to_str() {
                Some(text) if text.contains(SOCKET_PLACEHOLDER) => {
                    OsString::from(text.replace(SOCKET_PLACEHOLDER, &socket.display().to_string()))
                }
                _ if *arg == placeholder => socket.clone().into_os_string(),
                _ => arg.clone(),
            })
            .collect();
        let command = TermCommand { args, ..command };
        let mut host = Self::launch(command, socket, size, START_TIMEOUT).await?;
        host._sockets = Some(sockets);
        Ok(host)
    }

    /// Start `command`, which serves live automation at `socket`, in a
    /// terminal of `size`, and wait up to `ready` for the app to report that
    /// the socket accepts connections.
    pub async fn launch(
        command: TermCommand,
        socket: PathBuf,
        size: ScreenSize,
        ready: Duration,
    ) -> Result<Self> {
        let signal = ReadySignal::bind(socket.with_extension("ready"))?;
        let command = command.env(READY_SOCKET_ENV, signal.path());
        let terminal = HeadlessTerminal::spawn(terminal_request(&command, size))
            .map_err(|error| anyhow!("{error}"))?;
        let client = match wait_ready(&terminal.handle(), signal, ready).await {
            Ok(()) => connect(&socket).await,
            Err(error) => Err(error),
        };
        match client {
            Ok(client) => Ok(Self {
                terminal,
                socket,
                client: Arc::new(client),
                marks: 0,
                _sockets: None,
            }),
            Err(error) => {
                // The app may have started without serving automation.
                drop(terminal.finish(STOP_GRACE).await);
                Err(error)
            }
        }
    }

    /// Return the app's live automation socket.
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// Return the live MCP client, to share with other script tools.
    pub fn client(&self) -> Arc<Client<()>> {
        Arc::clone(&self.client)
    }

    /// Return the terminal's driver handle.
    fn handle(&self) -> DriverHandle {
        self.terminal.handle()
    }

    /// Press input in order, and wait until the app has taken every key.
    ///
    /// Each entry is a key with optional modifiers, such as `i`, `enter`,
    /// `esc`, `shift+tab`, or `ctrl+c`, or `:` and text to type as it stands,
    /// such as `:/needle`, one key per character. A lone `:` presses the colon
    /// key.
    pub async fn keys(&mut self, input: &[impl AsRef<str>]) -> Result<()> {
        // A terminal reads an escape and the key after it in one read as
        // that key with Alt, so a bare escape goes alone, and the next key
        // waits until the app has taken it.
        for batch in split_after_escapes(parse_input(input)?) {
            self.press(batch).await?;
        }
        Ok(())
    }

    /// Press `keys`, and wait until the app has taken every one.
    async fn press(&self, keys: Vec<Key>) -> Result<()> {
        let sent = keys.len() as u64;
        let before = self.input_count().await?;
        self.handle()
            .keys(keys)
            .await
            .map_err(|error| anyhow!("{error}"))?;
        let target = before + sent;
        self.eval(&format!(
            "canopy.wait_for(function() return canopy.input_count() >= {target} end, {})",
            ACK_TIMEOUT.as_millis()
        ))
        .await
        .context("waiting for the app to take the keys")?;
        Ok(())
    }

    /// Return how many input events the app has taken.
    async fn input_count(&self) -> Result<u64> {
        let value = self.eval("return canopy.input_count()").await?;
        value
            .as_u64()
            .with_context(|| format!("the app reported an input count of {value}"))
    }

    /// Return the visible screen, one string per row without trailing
    /// blanks, once the terminal has read the app's latest frame.
    pub async fn screen(&mut self) -> Result<Vec<String>> {
        self.marks += 1;
        let token = self.marks.to_string();
        let queued = self
            .eval(&format!(
                "return canopy.terminal_apc(\"{MARK_PREFIX}{token}\")"
            ))
            .await?;
        if queued != JsonValue::Bool(true) {
            bail!("the app draws on no terminal that can mark its frames");
        }
        self.handle()
            .wait_for_marker(&token, ACK_TIMEOUT)
            .await
            .map_err(|error| anyhow!("waiting for the terminal to read a frame: {error}"))?;
        self.visible_text().await
    }

    /// Wait until the screen shows `text`, for a result that arrives after
    /// the input, such as a worker's, and return the screen.
    pub async fn screen_until(&mut self, text: &str) -> Result<Vec<String>> {
        self.handle()
            .wait_for_match(MatchSpec::text(text), WaitOptions::new(ACK_TIMEOUT))
            .await
            .map_err(|error| anyhow!("waiting for {text:?}: {error}"))?;
        self.screen().await
    }

    /// Return the terminal's visible rows, without trailing blanks.
    async fn visible_text(&self) -> Result<Vec<String>> {
        let rows = self
            .handle()
            .visible_text()
            .await
            .map_err(|error| anyhow!("{error}"))?;
        Ok(rows
            .into_iter()
            .map(|row| row.trim_end().to_owned())
            .collect())
    }

    /// Evaluate a Luau script on the app's live session and return its value.
    pub async fn eval(&self, source: &str) -> Result<JsonValue> {
        let mut request = ScriptEvalRequest::new(source);
        // The script's own waits end first, so a hang reports from there.
        request.timeout_ms = Some(u64::try_from(ACK_TIMEOUT.as_millis())? * 2);
        let report = self
            .client
            .call_tool("script_eval", request)
            .await?
            .extract_as::<EvalReport>(ToolResultMode::Structured)
            .context("decode structured script_eval outcome")?;
        if !report.success {
            let reason = report
                .error
                .map_or_else(|| format!("{:?}", report.state), |error| error.message);
            bail!("{reason}");
        }
        Ok(report.value.unwrap_or(JsonValue::Null))
    }

    /// Wait until the app exits.
    pub async fn exited(&self) {
        let handle = self.handle();
        // The terminal reports the exit; the long timeout only renews the
        // wait.
        while handle.wait_child_exit(EXIT_WATCH).await.is_err() {
            if handle.child_info().await.is_err() {
                return;
            }
        }
    }

    /// Stop the app, forcing it after a grace period, and the terminal.
    pub async fn stop(self) -> Result<()> {
        let Self {
            terminal, socket, ..
        } = self;
        // Closing the terminal hangs up the app, as closing a window does.
        terminal
            .handle()
            .kill_child(Signal::Hup)
            .await
            .map_err(|error| anyhow!("{error}"))?;
        terminal
            .finish(STOP_GRACE)
            .await
            .map_err(|error| anyhow!("{error}"))?;
        // An app stopped by a signal leaves its socket behind.
        drop(fs::remove_file(&socket));
        Ok(())
    }
}

/// Connect an MCP client to the live automation socket.
async fn connect(socket: &Path) -> Result<Client<()>> {
    let stream = UnixStream::connect(socket)
        .await
        .with_context(|| format!("connect to {}", socket.display()))?;
    let (reader, writer) = stream.into_split();
    let mut client = Client::new(CLIENT_NAME, CLIENT_VERSION);
    client.connect_stream(reader, writer).await?;
    Ok(client)
}

/// Wait up to `limit` for the app's ready signal, failing as soon as the app
/// exits.
async fn wait_ready(handle: &DriverHandle, signal: ReadySignal, limit: Duration) -> Result<()> {
    tokio::select! {
        ready = signal.wait(limit) => ready,
        exited = handle.wait_child_exit(limit) => match exited {
            Err(DriverError::Timeout(_)) => bail!("the app did not report ready within {limit:?}"),
            _ => {
                let rows = handle.visible_text().await.unwrap_or_default();
                bail!(
                    "the app exited before serving automation:\n{}",
                    rows.join("\n").trim_end()
                );
            }
        },
    }
}

/// Return whether `key` is escape with no modifier.
fn is_bare_escape(key: &Key) -> bool {
    key.code == KeyCode::Escape && key.modifiers.is_empty()
}

/// Split `keys` into batches that each end at a bare escape, or at the end.
fn split_after_escapes(keys: Vec<Key>) -> Vec<Vec<Key>> {
    let mut batches = vec![Vec::new()];
    for key in keys {
        let escape = is_bare_escape(&key);
        batches.last_mut().expect("one batch").push(key);
        if escape {
            batches.push(Vec::new());
        }
    }
    batches.retain(|batch| !batch.is_empty());
    batches
}

/// Parse input entries into the keys they press.
fn parse_input(input: &[impl AsRef<str>]) -> Result<Vec<Key>> {
    let mut keys = Vec::new();
    for entry in input {
        let entry = entry.as_ref();
        match entry.strip_prefix(':').filter(|text| !text.is_empty()) {
            Some(text) => keys.extend(text.chars().map(Key::from)),
            None => keys.push(
                entry
                    .parse::<Key>()
                    .map_err(|error| anyhow!("{entry}: {error}"))?,
            ),
        }
    }
    Ok(keys)
}

/// Build the terminal request for `command`. The child inherits the
/// environment without the launching package's Cargo identity, so a Cargo
/// command keeps its own.
fn terminal_request(command: &TermCommand, size: ScreenSize) -> HeadlessRequest {
    let mut environment: BTreeMap<OsString, OsString> = env::vars_os()
        .filter(|(name, _)| {
            name.to_str().is_none_or(|key| {
                !key.starts_with("CARGO_PKG_")
                    && !matches!(
                        key,
                        "CARGO_MANIFEST_DIR" | "CARGO_MANIFEST_PATH" | "CARGO_MANIFEST_LINKS"
                    )
            })
        })
        .collect();
    environment.extend(command.env.clone());
    let mut request = HeadlessRequest::new(&command.program)
        .args(&command.args)
        .environment_policy(EnvironmentPolicy::Clear)
        .envs(environment)
        .dimensions(size.width as usize, size.height as usize)
        .transcript_bytes(TRANSCRIPT_BYTES);
    if let Some(cwd) = &command.cwd {
        request = request.cwd(cwd);
    }
    request
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_mixes_keys_and_typed_text() {
        let input = ["l", ":/links", "enter", ":", "::x", "ctrl+c"];
        let expected = [
            "l", "/", "l", "i", "n", "k", "s", "enter", ":", ":", "x", "ctrl+c",
        ]
        .map(|key| key.parse::<Key>().expect("key"));
        assert_eq!(parse_input(&input).expect("input"), expected);
        assert!(parse_input(&["nokey"]).is_err());
    }

    #[test]
    fn a_bare_escape_ends_its_batch() {
        let keys =
            ["esc", "tab", "alt+esc", "j", "esc"].map(|key| key.parse::<Key>().expect("key"));
        let batches = split_after_escapes(keys.to_vec());
        assert_eq!(
            batches,
            [keys[..1].to_vec(), keys[1..].to_vec()],
            "alt+esc stays with its batch, and a final escape ends one"
        );
    }

    #[tokio::test]
    async fn a_ready_signal_ends_on_the_ready_line() -> Result<()> {
        use tokio::io::AsyncWriteExt as _;
        let dir = tempfile::tempdir()?;
        let signal = ReadySignal::bind(dir.path().join("app.ready"))?;
        let path = signal.path().to_path_buf();
        let wait = tokio::spawn(signal.wait(Duration::from_secs(10)));
        UnixStream::connect(&path)
            .await?
            .write_all(b"ready\n")
            .await?;
        wait.await??;
        assert!(!path.exists(), "the wait removes its socket");
        Ok(())
    }
}
