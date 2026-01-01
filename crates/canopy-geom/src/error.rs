use std::{error::Error as StdError, fmt, result::Result as StdResult};

/// Geometry error type.
#[derive(Debug, Clone)]
pub enum Error {
    /// Generic geometry error message.
    Geometry(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Geometry(s) => write!(f, "{s}"),
        }
    }
}

impl StdError for Error {}

/// Result type for geometry operations.
pub type Result<T> = StdResult<T, Error>;
