use std::{fmt::Display, io, path::PathBuf, result::Result as StdResult};

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
    /// A JSON encoding or decoding error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// An MCP transport or protocol error.
    #[error(transparent)]
    Tmcp(#[from] tmcp::Error),
    /// The application factory failed to build an app instance.
    #[error("app setup failed: {0}")]
    App(String),
    /// A smoke suite did not resolve to any Luau scripts.
    #[error("no .luau scripts found under {0}")]
    NoScripts(PathBuf),
}

impl Error {
    /// Wrap an application-specific setup error.
    pub fn app(error: impl Display) -> Self {
        Self::App(error.to_string())
    }
}
