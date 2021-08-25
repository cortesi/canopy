use crate::Result;

pub trait ControlBackend {
    /// Enter the backend control state.
    fn enter(&mut self) -> Result<()>;
    /// Exit the backend control state.
    fn exit(&mut self) -> Result<()>;
}
