use expectrl::{spawn, Eof, Session};
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

    /// Ensure that `pat` does not appear within `timeout`.
    pub fn expect_absent(&mut self, pat: &str, timeout: Duration) -> Result<()> {
        use expectrl::Error as PtyError;
        self.sess.set_expect_timeout(Some(timeout));
        match self.sess.expect(pat) {
            Ok(_) => Err(Error::Internal(format!("unexpected pattern found: {pat}"))),
            Err(e) => match e {
                PtyError::ExpectTimeout => Ok(()),
                other => Err(Error::Internal(other.to_string())),
            },
        }
    }

    /// Drain any pending output.
    pub fn flush(&mut self) {
        let _ = self.expect_absent("__CANOPY_FLUSH__", Duration::from_millis(50));
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

/// Spawn a workspace binary from `target/debug` with the provided arguments.
pub fn spawn_workspace_bin(name: &str, args: &[&str]) -> Result<PtyApp> {
    let bin = format!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../target/debug/{}"),
        name
    );
    PtyApp::spawn_cmd(&bin, args)
}
