/// Crossterm backend implementation.
pub mod crossterm;
use std::{fmt::Debug, sync::Arc};

use parking_lot::Mutex;

use crate::error::Result;

/// A handle for controlling our rendering back-end. The primary use is to
/// suspend and resume rendering to permit us to fork out to another process
/// that wants to control the terminal - for example, spawning an external
/// editor.
pub trait BackendControl: Debug + Send {
    /// Start the backend renderer.
    fn start(&mut self) -> Result<()>;

    /// Stop the backend renderer, releasing control of the terminal.
    fn stop(&mut self) -> Result<()>;
}

/// Guard that ensures backend start/stop are paired for a terminal session.
pub(crate) struct TerminalSession {
    /// Shared session state used by normal exit, panic cleanup, and drop.
    state: Arc<Mutex<TerminalState>>,
}

/// Mutable state behind every terminal cleanup path.
struct TerminalState {
    /// Backend controller owned for the complete session lifetime.
    backend: Box<dyn BackendControl>,
    /// Whether the session has an active backend start.
    active: bool,
}

/// Cloneable cleanup capability for a panic hook.
#[derive(Clone)]
pub(crate) struct TerminalCleanup {
    /// Shared terminal state.
    state: Arc<Mutex<TerminalState>>,
}

impl TerminalCleanup {
    /// Stop the backend if the session is active.
    pub(crate) fn stop(&self) -> Result<()> {
        let mut state = self.state.lock();
        if state.active {
            state.backend.stop()?;
            state.active = false;
        }
        Ok(())
    }
}

impl TerminalSession {
    /// Start the backend and create a new session guard.
    pub(crate) fn new(mut backend: Box<dyn BackendControl>) -> Result<Self> {
        backend.start()?;
        Ok(Self {
            state: Arc::new(Mutex::new(TerminalState {
                backend,
                active: true,
            })),
        })
    }

    /// Return a cleanup capability suitable for a panic hook.
    pub(crate) fn cleanup(&self) -> TerminalCleanup {
        TerminalCleanup {
            state: Arc::clone(&self.state),
        }
    }

    /// Stop the backend if the session is active.
    pub(crate) fn stop(&self) -> Result<()> {
        self.cleanup().stop()
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        drop(self.cleanup().stop());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;

    use super::*;
    use crate::error::Error;

    /// Recorded backend lifecycle calls.
    #[derive(Debug, Default)]
    struct Lifecycle {
        starts: usize,
        stops: usize,
    }

    /// Backend controller that records balanced ownership transitions.
    #[derive(Debug)]
    struct RecordingControl {
        lifecycle: Arc<Mutex<Lifecycle>>,
        fail_start: bool,
    }

    impl BackendControl for RecordingControl {
        fn start(&mut self) -> Result<()> {
            self.lifecycle.lock().starts += 1;
            if self.fail_start {
                return Err(Error::Render("injected start failure".into()));
            }
            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            self.lifecycle.lock().stops += 1;
            Ok(())
        }
    }

    #[test]
    fn session_drop_stops_owned_backend_once() -> Result<()> {
        let lifecycle = Arc::new(Mutex::new(Lifecycle::default()));
        let backend = Box::new(RecordingControl {
            lifecycle: Arc::clone(&lifecycle),
            fail_start: false,
        });

        drop(TerminalSession::new(backend)?);

        let lifecycle = lifecycle.lock();
        assert_eq!(lifecycle.starts, 1);
        assert_eq!(lifecycle.stops, 1);
        Ok(())
    }

    #[test]
    fn explicit_session_stop_is_idempotent_with_drop() -> Result<()> {
        let lifecycle = Arc::new(Mutex::new(Lifecycle::default()));
        let backend = Box::new(RecordingControl {
            lifecycle: Arc::clone(&lifecycle),
            fail_start: false,
        });
        let session = TerminalSession::new(backend)?;

        session.stop()?;
        drop(session);

        assert_eq!(lifecycle.lock().stops, 1);
        Ok(())
    }

    #[test]
    fn panic_cleanup_and_session_drop_share_one_stop_state() -> Result<()> {
        let lifecycle = Arc::new(Mutex::new(Lifecycle::default()));
        let backend = Box::new(RecordingControl {
            lifecycle: Arc::clone(&lifecycle),
            fail_start: false,
        });
        let session = TerminalSession::new(backend)?;

        session.cleanup().stop()?;
        drop(session);

        assert_eq!(lifecycle.lock().stops, 1);
        Ok(())
    }

    #[test]
    fn failed_session_start_never_calls_stop() {
        let lifecycle = Arc::new(Mutex::new(Lifecycle::default()));
        let backend = Box::new(RecordingControl {
            lifecycle: Arc::clone(&lifecycle),
            fail_start: true,
        });

        assert!(TerminalSession::new(backend).is_err());

        let lifecycle = lifecycle.lock();
        assert_eq!(lifecycle.starts, 1);
        assert_eq!(lifecycle.stops, 0);
    }
}
