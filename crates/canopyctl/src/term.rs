//! The control protocol of an app that canopyctl hosts in a terminal.
//!
//! [`canopy_term::TermHost`] runs the app and acknowledges its input and
//! frames. A detached host serves a control socket beside the app's socket,
//! at [`control_path`], speaking one JSON request and one JSON response per
//! line.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use canopy_term::TermHost;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

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

/// Serve one request on `host`.
pub async fn serve(host: &mut TermHost, request: &TermRequest) -> Result<TermResponse> {
    host.keys(&request.keys).await?;
    let screen = match &request.until {
        Some(text) => Some(host.screen_until(text).await?),
        None if request.screen => Some(host.screen().await?),
        None => None,
    };
    Ok(TermResponse {
        screen,
        error: None,
    })
}

/// Bind the control socket beside the app's socket, so clients can connect
/// before [`serve_control`] runs.
pub fn bind_control(host: &TermHost) -> Result<UnixListener> {
    let path = control_path(host.socket());
    UnixListener::bind(&path).with_context(|| format!("bind {}", path.display()))
}

/// Serve requests on the control socket until a request stops the host or
/// the app exits.
pub async fn serve_control(host: TermHost, listener: UnixListener) -> Result<()> {
    let path = control_path(host.socket());
    let result = accept(host, &listener).await;
    drop(fs::remove_file(&path));
    result
}

/// Accept control connections until one stops the host or the app exits.
async fn accept(mut host: TermHost, listener: &UnixListener) -> Result<()> {
    loop {
        let stream = tokio::select! {
            accepted = listener.accept() => accepted?.0,
            () = host.exited() => return host.stop().await,
        };
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        while let Some(line) = lines.next_line().await? {
            let request = serde_json::from_str::<TermRequest>(&line);
            let stop = request.as_ref().is_ok_and(|request| request.stop);
            let response = match request {
                Ok(request) if request.stop => TermResponse::default(),
                Ok(request) => {
                    serve(&mut host, &request)
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
                return host.stop().await;
            }
        }
    }
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
