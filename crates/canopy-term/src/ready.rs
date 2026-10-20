//! The signal an app sends once its live automation socket accepts
//! connections.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::UnixListener,
    time::timeout,
};

/// A socket that an app launched through `canopy_mcp::launch` connects to
/// once its live automation socket accepts connections.
///
/// Pass [`ReadySignal::path`] to the app in
/// [`canopy_mcp::READY_SOCKET_ENV`], start the app, and wait. The wait ends
/// on the app's signal, so nothing retries the automation socket.
pub struct ReadySignal {
    /// The listening socket.
    listener: UnixListener,
    /// Where the socket lives, removed once the wait ends.
    path: PathBuf,
}

impl ReadySignal {
    /// Listen at `path`, replacing a stale socket there.
    pub fn bind(path: PathBuf) -> Result<Self> {
        drop(fs::remove_file(&path));
        let listener =
            UnixListener::bind(&path).with_context(|| format!("bind {}", path.display()))?;
        Ok(Self { listener, path })
    }

    /// Return the socket path to pass to the app.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Wait up to `limit` for the app's signal. The limit only turns a hang
    /// into an error.
    pub async fn wait(self, limit: Duration) -> Result<()> {
        let signal = async {
            let (stream, _) = self.listener.accept().await?;
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).await?;
            anyhow::Ok(line)
        };
        let line = timeout(limit, signal).await;
        drop(fs::remove_file(&self.path));
        let Ok(line) = line else {
            bail!(
                "the app did not report ready within {limit:?}; does it launch through \
                 canopy_mcp::launch with a live socket?"
            );
        };
        if line?.trim() != "ready" {
            bail!("the app sent no ready line");
        }
        Ok(())
    }
}
