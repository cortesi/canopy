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

use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use canopy_mcp::ScreenSize;
use itty_headless::{
    DriverHandle, EnvironmentPolicy, HeadlessRequest, HeadlessTerminal, Key, KeyCode, Signal,
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    time::sleep,
};

use crate::config::ResolvedCommand;

/// Transcript retained by a hosted terminal, in bytes.
const TRANSCRIPT_BYTES: usize = 4 * 1024 * 1024;
/// How long a stopping app has to exit before the host forces it.
const STOP_GRACE: Duration = Duration::from_secs(2);
/// Quiet time after input before the screen is read, so the app has drawn.
const SETTLE: Duration = Duration::from_millis(100);
/// Longest wait for the screen to settle after input.
const SETTLE_LIMIT: Duration = Duration::from_secs(2);
/// Quiet time after a bare escape, so the app reads it alone.
const ESCAPE_SETTLE: Duration = Duration::from_millis(50);

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
    /// Return the visible screen after the input settles.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub screen: bool,
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
        let host = Self { terminal, socket };
        host.wait_ready(ready).await?;
        Ok(host)
    }

    /// Wait for the app's socket, failing early when the app exits first.
    async fn wait_ready(&self, timeout: Duration) -> Result<()> {
        let started = Instant::now();
        while started.elapsed() < timeout {
            if self.socket.exists() && UnixStream::connect(&self.socket).await.is_ok() {
                return Ok(());
            }
            let child = self
                .handle()
                .child_info()
                .await
                .map_err(|error| anyhow!("{error}"))?;
            if !child.running && child.termination.is_some() {
                let screen = self.screen().await.unwrap_or_default().join("\n");
                bail!("the app exited before serving automation:\n{screen}");
            }
            sleep(Duration::from_millis(100)).await;
        }
        bail!("timed out waiting for {}", self.socket.display())
    }

    /// Return the terminal's driver handle.
    fn handle(&self) -> DriverHandle {
        self.terminal.handle()
    }

    /// Serve one request.
    pub async fn serve(&self, request: &TermRequest) -> Result<TermResponse> {
        let keys = parse_input(&request.keys)?;
        let handle = self.handle();
        // A terminal reads an escape and the key after it in one read as
        // that key with Alt, so a bare escape goes alone, and the app reads
        // it before the next key arrives.
        for batch in split_after_escapes(keys) {
            let escape = batch.last().is_some_and(is_bare_escape);
            handle
                .keys(batch)
                .await
                .map_err(|error| anyhow!("{error}"))?;
            if escape {
                // An app that draws nothing in reply still had the time.
                let _settled = handle.wait_idle(ESCAPE_SETTLE, SETTLE_LIMIT).await;
            }
        }
        let screen = if request.screen {
            Some(self.screen().await?)
        } else {
            None
        };
        Ok(TermResponse {
            screen,
            error: None,
        })
    }

    /// Return the visible screen once the app has drawn.
    pub async fn screen(&self) -> Result<Vec<String>> {
        let handle = self.handle();
        handle
            .host_tick()
            .await
            .map_err(|error| anyhow!("{error}"))?;
        // An app that never goes quiet still shows what it has drawn.
        let _settled = handle.wait_idle(SETTLE, SETTLE_LIMIT).await;
        let rows = handle
            .visible_text()
            .await
            .map_err(|error| anyhow!("{error}"))?;
        Ok(rows
            .into_iter()
            .map(|row| row.trim_end().to_owned())
            .collect())
    }

    /// Stop the app, forcing it after a grace period, and the terminal.
    pub async fn stop(self) -> Result<()> {
        let handle = self.handle();
        // Closing the terminal hangs up the app, as closing a window does.
        handle
            .kill_child(Signal::Hup)
            .await
            .map_err(|error| anyhow!("{error}"))?;
        self.terminal
            .finish(STOP_GRACE)
            .await
            .map_err(|error| anyhow!("{error}"))?;
        // An app stopped by a signal leaves its socket behind.
        drop(fs::remove_file(&self.socket));
        Ok(())
    }

    /// Serve requests on the control socket until a request stops the host or
    /// the app exits.
    pub async fn serve_control(self) -> Result<()> {
        let path = control_path(&self.socket);
        let listener =
            UnixListener::bind(&path).with_context(|| format!("bind {}", path.display()))?;
        let result = self.accept(&listener).await;
        drop(fs::remove_file(&path));
        result
    }

    /// Accept control connections until one stops the host or the app exits.
    async fn accept(self, listener: &UnixListener) -> Result<()> {
        loop {
            let exited = async {
                loop {
                    let child = self.handle().child_info().await;
                    if child.map_or(true, |child| !child.running && child.termination.is_some()) {
                        return;
                    }
                    sleep(Duration::from_millis(250)).await;
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
