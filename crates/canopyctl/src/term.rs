//! Interactive apps hosted in a terminal that canopyctl owns.
//!
//! A hosted app runs in a pseudo-terminal behind an Itty terminal emulator,
//! so it runs exactly as it does for a person: in its interactive mode, with
//! terminal input parsing and rendering, while its live automation socket
//! serves scripts. Keys reach it as the bytes a terminal sends, and the screen
//! reads back what the terminal shows.
//!
//! A detached host serves a control socket beside the app's socket, at
//! [`control_path`], speaking one JSON request and one JSON response per line.
//!
//! The host never guesses when the app is done from elapsed or quiet time.
//! After it sends keys, it waits on the app's live socket until the app's
//! input count has risen by the keys it sent. To read the screen, it asks the
//! app to write an APC marker after its latest frame, and waits for the
//! terminal to read it: terminal output is ordered, so the terminal has then
//! read that frame. Timeouts only turn a hang into an error.

use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use canopy_mcp::{ScreenSize, ScriptEvalRequest};
use itty_headless::{
    DriverHandle, EnvironmentPolicy, HeadlessRequest, HeadlessTerminal, Key, KeyCode, MatchSpec,
    Signal, WaitOptions,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    time::sleep,
};

use crate::{config::ResolvedCommand, session::Session};

/// Transcript retained by a hosted terminal, in bytes.
const TRANSCRIPT_BYTES: usize = 4 * 1024 * 1024;
/// How long a stopping app has to exit before the host forces it.
const STOP_GRACE: Duration = Duration::from_secs(2);
/// Longest wait for the app or the terminal to acknowledge input, a frame, or
/// expected text. The waits end on the acknowledgment, so only a hang
/// reaches this.
const ACK_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest wait for the app to exit, after which the host checks again.
const EXIT_WATCH: Duration = Duration::from_secs(3600);
/// Prefix of the APC payload that itty records as a marker.
const MARK_PREFIX: &str = "itty:mark:";

/// Return the control socket path beside an app's automation socket.
pub fn control_path(socket: &Path) -> PathBuf {
    let mut path = socket.as_os_str().to_owned();
    path.push(".term");
    PathBuf::from(path)
}

/// Return the log path of a detached host serving the app at `socket`.
pub fn log_path(socket: &Path) -> PathBuf {
    socket.with_extension("log")
}

/// One request to a hosted terminal.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TermRequest {
    /// Input in order. Each entry is a key with optional modifiers, such as
    /// `i`, `enter`, or `ctrl+c`, or `:` and text to type as it stands, one
    /// key per character. A lone `:` presses the colon key.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    /// Return the visible screen once the app has drawn the input.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub screen: bool,
    /// Before returning the screen, wait until it shows this text, for a
    /// result that arrives after the input, such as a worker's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    /// Stop the app and the host.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stop: bool,
}

/// The response to one [`TermRequest`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TermResponse {
    /// The visible screen, one string per row, when the request asked for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<Vec<String>>,
    /// Why the request failed, if it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// An app running in a terminal that canopyctl owns.
pub struct TermHost {
    /// The terminal and its session worker.
    terminal: HeadlessTerminal,
    /// The app's live automation socket.
    socket: PathBuf,
    /// A live session with the app, which acknowledges input and frames, and
    /// serves the scripts of whoever hosts it.
    session: Session,
    /// The last frame marker issued.
    marks: u64,
}

impl TermHost {
    /// Start `command` in a terminal of `size`, serving automation at
    /// `socket`, and wait up to `ready` for the socket to accept connections.
    pub async fn launch(
        command: &ResolvedCommand,
        socket: PathBuf,
        size: ScreenSize,
        ready: Duration,
    ) -> Result<Self> {
        let request = terminal_request(command, size)?;
        let terminal = HeadlessTerminal::spawn(request).map_err(|error| anyhow!("{error}"))?;
        let session = match wait_ready(&terminal.handle(), &socket, ready).await {
            Ok(()) => Session::connect_live(&socket).await,
            Err(error) => Err(error),
        };
        match session {
            Ok(session) => Ok(Self {
                terminal,
                socket,
                session,
                marks: 0,
            }),
            Err(error) => {
                // The app may have started without serving automation.
                drop(terminal.finish(STOP_GRACE).await);
                Err(error)
            }
        }
    }

    /// Return the live session with the app, for its host's scripts.
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Return the terminal's driver handle.
    fn handle(&self) -> DriverHandle {
        self.terminal.handle()
    }

    /// Serve one request.
    pub async fn serve(&mut self, request: &TermRequest) -> Result<TermResponse> {
        let keys = parse_input(&request.keys)?;
        // A terminal reads an escape and the key after it in one read as
        // that key with Alt, so a bare escape goes alone, and the next key
        // waits until the app has taken it.
        for batch in split_after_escapes(keys) {
            self.press(batch).await?;
        }
        if let Some(text) = &request.until {
            self.handle()
                .wait_for_match(MatchSpec::text(text), WaitOptions::new(ACK_TIMEOUT))
                .await
                .map_err(|error| anyhow!("waiting for {text:?}: {error}"))?;
        }
        let screen = if request.screen || request.until.is_some() {
            Some(self.screen().await?)
        } else {
            None
        };
        Ok(TermResponse {
            screen,
            error: None,
        })
    }

    /// Press `keys`, and wait until the app has dispatched every one.
    async fn press(&self, keys: Vec<Key>) -> Result<()> {
        if keys.is_empty() {
            return Ok(());
        }
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

    /// Return how many input events the app has dispatched.
    async fn input_count(&self) -> Result<u64> {
        let value = self.eval("return canopy.input_count()").await?;
        value
            .as_u64()
            .with_context(|| format!("the app reported an input count of {value}"))
    }

    /// Return the visible screen once the terminal has read the app's latest
    /// frame.
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

    /// Evaluate `source` on the app's live session and return its value.
    async fn eval(&self, source: &str) -> Result<JsonValue> {
        let mut request = ScriptEvalRequest::new(source);
        // The script's own waits end first, so a hang reports from there.
        request.timeout_ms = Some(u64::try_from(ACK_TIMEOUT.as_millis())? * 2);
        let report = self.session.eval(request).await?;
        if !report.success {
            let reason = report
                .error
                .map_or_else(|| format!("{:?}", report.state), |error| error.message);
            bail!("{reason}");
        }
        Ok(report.value.unwrap_or(JsonValue::Null))
    }

    /// Stop the app, forcing it after a grace period, and the terminal.
    pub async fn stop(self) -> Result<()> {
        let Self {
            terminal,
            socket,
            session,
            ..
        } = self;
        session.shutdown().await;
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

    /// Bind the control socket beside the app's socket, so clients can
    /// connect before [`Self::serve_control`] runs.
    pub fn bind_control(&self) -> Result<UnixListener> {
        let path = control_path(&self.socket);
        UnixListener::bind(&path).with_context(|| format!("bind {}", path.display()))
    }

    /// Serve requests on the control socket until a request stops the host or
    /// the app exits.
    pub async fn serve_control(self, listener: UnixListener) -> Result<()> {
        let path = control_path(&self.socket);
        let result = self.accept(&listener).await;
        drop(fs::remove_file(&path));
        result
    }

    /// Accept control connections until one stops the host or the app exits.
    async fn accept(mut self, listener: &UnixListener) -> Result<()> {
        loop {
            let handle = self.handle();
            let exited = async move {
                // The terminal reports the exit; the long timeout only
                // renews the wait.
                while handle.wait_child_exit(EXIT_WATCH).await.is_err() {
                    if handle.child_info().await.is_err() {
                        return;
                    }
                }
            };
            let stream = tokio::select! {
                accepted = listener.accept() => accepted?.0,
                () = exited => return self.stop().await,
            };
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            while let Some(line) = lines.next_line().await? {
                let request = serde_json::from_str::<TermRequest>(&line);
                let stop = request.as_ref().is_ok_and(|request| request.stop);
                let response = match request {
                    Ok(request) if request.stop => TermResponse::default(),
                    Ok(request) => {
                        self.serve(&request)
                            .await
                            .unwrap_or_else(|error| TermResponse {
                                screen: None,
                                error: Some(format!("{error:#}")),
                            })
                    }
                    Err(error) => TermResponse {
                        screen: None,
                        error: Some(format!("invalid request: {error}")),
                    },
                };
                let mut encoded = serde_json::to_vec(&response)?;
                encoded.push(b'\n');
                writer.write_all(&encoded).await?;
                if stop {
                    return self.stop().await;
                }
            }
        }
    }
}

/// Wait for the app's automation socket to accept a connection, failing early
/// when the app exits first.
///
/// Nothing announces the socket, so this retries the connection; the retry
/// interval affects only how soon readiness is seen, never what is seen.
async fn wait_ready(handle: &DriverHandle, socket: &Path, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if UnixStream::connect(socket).await.is_ok() {
            return Ok(());
        }
        let child = handle
            .child_info()
            .await
            .map_err(|error| anyhow!("{error}"))?;
        if !child.running && child.termination.is_some() {
            let rows = handle.visible_text().await.unwrap_or_default();
            let screen = rows.join("\n");
            bail!(
                "the app exited before serving automation:\n{}",
                screen.trim_end()
            );
        }
        sleep(Duration::from_millis(100)).await;
    }
    bail!("timed out waiting for {}", socket.display())
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

/// Parse request input into the keys it presses.
fn parse_input(input: &[String]) -> Result<Vec<Key>> {
    let mut keys = Vec::new();
    for entry in input {
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

/// Send one request to the host serving the app at `socket`.
pub async fn send(socket: &Path, request: &TermRequest) -> Result<TermResponse> {
    let path = control_path(socket);
    let stream = UnixStream::connect(&path).await.with_context(|| {
        format!(
            "connect to {} (is the app hosted with --pty?)",
            path.display()
        )
    })?;
    let (reader, mut writer) = stream.into_split();
    let mut encoded = serde_json::to_vec(request)?;
    encoded.push(b'\n');
    writer.write_all(&encoded).await?;
    let mut lines = BufReader::new(reader).lines();
    let line = lines
        .next_line()
        .await?
        .context("the host closed the connection")?;
    let response: TermResponse = serde_json::from_str(&line)?;
    if let Some(error) = &response.error {
        bail!("{error}");
    }
    Ok(response)
}

/// Build the terminal request for `command`. The child inherits the
/// environment without the launching package's Cargo identity, as a Cargo
/// child of canopyctl does.
fn terminal_request(command: &ResolvedCommand, size: ScreenSize) -> Result<HeadlessRequest> {
    let (program, args) = command.argv().split_first().context("command is empty")?;
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
    environment.extend(
        command
            .env()
            .iter()
            .map(|(key, value)| (OsString::from(key), OsString::from(value))),
    );
    Ok(HeadlessRequest::new(program)
        .args(args)
        .cwd(command.cwd())
        .environment_policy(EnvironmentPolicy::Clear)
        .envs(environment)
        .dimensions(size.width as usize, size.height as usize)
        .transcript_bytes(TRANSCRIPT_BYTES))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_mixes_keys_and_typed_text() {
        let input = ["l", ":/links", "enter", ":", "::x", "ctrl+c"].map(String::from);
        let expected = [
            "l", "/", "l", "i", "n", "k", "s", "enter", ":", ":", "x", "ctrl+c",
        ]
        .map(|key| key.parse::<Key>().expect("key"));
        assert_eq!(parse_input(&input).expect("input"), expected);
        assert!(parse_input(&["nokey".to_owned()]).is_err());
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
}
