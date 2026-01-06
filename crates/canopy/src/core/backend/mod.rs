/// Crossterm backend implementation.
pub mod crossterm;
use std::{fmt::Debug, process, ptr::NonNull};

use crate::error::Result;

/// A handle for controlling our rendering back-end. The primary use is to
/// suspend and resume rendering to permit us to fork out to another process
/// that wants to control the terminal - for example, spawning an external
/// editor.
pub trait BackendControl: Debug {
    /// Start the backend renderer.
    fn start(&mut self) -> Result<()>;

    /// Stop the backend renderer, releasing control of the terminal.
    fn stop(&mut self) -> Result<()>;

    /// Stop the render backend and exit the process.
    fn exit(&mut self, code: i32) -> ! {
        let _ = self.stop().ok();
        process::exit(code)
    }
}

/// Guard that ensures backend start/stop are paired for a terminal session.
pub(crate) struct TerminalSession {
    /// Backend controller stored in the core state.
    backend: NonNull<Box<dyn BackendControl>>,
    /// Whether the session has an active backend start.
    active: bool,
}

impl TerminalSession {
    /// Start the backend and create a new session guard.
    pub(crate) fn new(backend: &mut Box<dyn BackendControl>) -> Result<Self> {
        backend.start()?;
        Ok(Self {
            backend: NonNull::from(backend),
            active: true,
        })
    }

    /// Stop the backend if the session is active.
    pub(crate) fn stop(&mut self) -> Result<()> {
        if self.active {
            // SAFETY: backend is owned by Core for the duration of the session.
            unsafe {
                self.backend.as_mut().as_mut().stop()?;
            }
            self.active = false;
        }
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if self.active {
            // SAFETY: backend is owned by Core for the duration of the session.
            drop(unsafe { self.backend.as_mut().as_mut().stop() });
            self.active = false;
        }
    }
}
