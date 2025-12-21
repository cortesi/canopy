/// Crossterm backend implementation.
pub mod crossterm;
use std::{fmt::Debug, process};

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
