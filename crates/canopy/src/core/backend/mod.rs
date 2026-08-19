/// Crossterm backend implementation.
pub mod crossterm;
use std::fmt::Debug;

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
pub struct TerminalSession {
    /// Backend controller owned for the complete session lifetime.
    backend: Box<dyn BackendControl>,
    /// Whether the session has an active backend start.
    active: bool,
}

impl TerminalSession {
    /// Start the backend and create a new session guard.
    pub(crate) fn new(mut backend: Box<dyn BackendControl>) -> Result<Self> {
        backend.start()?;
        Ok(Self {
            backend,
            active: true,
        })
    }

    /// Stop the backend if the session is active.
    pub(crate) fn stop(&mut self) -> Result<()> {
        if self.active {
            self.backend.stop()?;
            self.active = false;
        }
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        drop(self.stop());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

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
            self.lifecycle.lock().unwrap().starts += 1;
            if self.fail_start {
                return Err(Error::Internal("injected start failure".into()));
            }
            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            self.lifecycle.lock().unwrap().stops += 1;
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

        let lifecycle = lifecycle.lock().unwrap();
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
        let mut session = TerminalSession::new(backend)?;

        session.stop()?;
        drop(session);

        assert_eq!(lifecycle.lock().unwrap().stops, 1);
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

        let lifecycle = lifecycle.lock().unwrap();
        assert_eq!(lifecycle.starts, 1);
        assert_eq!(lifecycle.stops, 0);
    }
}
