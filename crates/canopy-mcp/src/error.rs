use std::{error::Error as StdError, io, path::PathBuf, result::Result as StdResult};

use canopy::{commands::CommandError, error::Error as CanopyError};
use thiserror::Error;

/// Result type used by `canopy-mcp`.
pub type Result<T> = StdResult<T, Error>;

/// Errors returned by `canopy-mcp`.
#[derive(Debug, Error)]
pub enum Error {
    /// A canopy runtime error.
    #[error(transparent)]
    Canopy(#[from] CanopyError),
    /// A canopy command conversion error.
    #[error(transparent)]
    Command(#[from] CommandError),
    /// An I/O error.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// An MCP transport or protocol error.
    #[error(transparent)]
    Tmcp(#[from] tmcp::Error),
    /// The application factory failed to build an app instance.
    #[error("app setup failed: {0}")]
    App(#[source] Box<dyn StdError + Send + Sync>),
    /// The UDS listener thread panicked while shutting down.
    #[error("UDS listener thread panicked")]
    ListenerThreadPanicked,
    /// The UDS listener stopped before reporting startup readiness.
    #[error("UDS listener failed to report readiness")]
    ListenerReadinessClosed,
    /// A smoke suite did not resolve to any Luau scripts.
    #[error("no .luau scripts found under {0}")]
    NoScripts(PathBuf),
}

impl Error {
    /// Wrap an application-specific setup error.
    pub fn app(error: impl Into<Box<dyn StdError + Send + Sync>>) -> Self {
        Self::App(error.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_preserves_source_chain() {
        let error = Error::app(io::Error::other("setup exploded"));
        let source = StdError::source(&error).expect("app error source");

        assert_eq!(source.to_string(), "setup exploded");
    }
}
