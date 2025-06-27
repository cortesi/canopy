use expectrl::{spawn, Eof, Session};
use std::process::Command;
use std::time::Duration;

use crate::{Error, Result};

/// A handle to a process running under a pseudo terminal.
pub struct PtyApp {
    sess: Session,
}

impl PtyApp {
    /// Spawn a command with the given arguments.
    pub fn spawn_cmd(cmd: &str, args: &[&str]) -> Result<Self> {
        let mut c = cmd.to_string();
        for a in args {
            c.push(' ');
            c.push_str(a);
        }
        let sess = spawn(c).map_err(|e| Error::Internal(e.to_string()))?;
        Ok(PtyApp { sess })
    }

    /// Expect the supplied pattern within `timeout`.
    pub fn expect(&mut self, pat: &str, timeout: Duration) -> Result<()> {
        self.sess.set_expect_timeout(Some(timeout));
        self.sess
            .expect(pat)
            .map(|_| ())
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Send raw text to the running process.
    pub fn send(&mut self, s: &str) -> Result<()> {
        self.sess
            .send(s)
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Send text followed by a newline.
    pub fn send_line(&mut self, s: &str) -> Result<()> {
        self.sess
            .send_line(s)
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Wait for the process to exit.
    pub fn wait_eof(&mut self, timeout: Duration) -> Result<()> {
        self.sess.set_expect_timeout(Some(timeout));
        self.sess
            .expect(Eof)
            .map(|_| ())
            .map_err(|e| Error::Internal(e.to_string()))
    }
}

/// Spawn a workspace binary by first building it, then running from target/debug.
pub fn spawn_workspace_bin(name: &str, args: &[&str]) -> Result<PtyApp> {
    // First, build the binary
    let output = Command::new("cargo")
        .args(["build", "-p", name, "--bin", name])
        .output()
        .map_err(|e| Error::Internal(format!("Failed to run cargo build: {e}")))?;

    if !output.status.success() {
        return Err(Error::Internal(format!(
            "cargo build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    // Now spawn the built binary directly
    let bin = format!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../target/debug/{}"),
        name
    );
    PtyApp::spawn_cmd(&bin, args)
}
