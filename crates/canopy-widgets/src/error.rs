use std::result::Result as StdResult;

use thiserror::Error;

/// Errors emitted by canopy-widgets helpers.
#[derive(Debug, Error)]
pub enum Error {
    /// Font parsing failed.
    #[error("font loading failed: {0}")]
    FontLoad(&'static str),
}

/// Result type for canopy-widgets helpers.
pub type Result<T> = StdResult<T, Error>;
