use crate::Result;

/// A handle for controlling our rendering back-end. The primary use is to
/// suspend and resume rendering to permit us to fork out to another process
/// that wants to control the terminal - for example, spawning an external
/// editor.
pub trait BackendControl {
    /// Enter the backend control state.
    fn enter(&mut self) -> Result<()>;
    /// Exit the backend control state.
    fn exit(&mut self) -> Result<()>;
}
