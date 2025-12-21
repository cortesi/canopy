use std::{
    fmt::{self, Display},
    result::Result as StdResult,
    sync::mpsc,
};

use thiserror::Error;

use crate::geom;

/// Result type for canopy-core operations.
pub type Result<T> = StdResult<T, Error>;

/// Parse error marker type.
#[derive(PartialEq, Eq, Error, Debug, Clone)]
pub struct ParseError {}

impl Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Core error type.
#[derive(PartialEq, Eq, Error, Debug, Clone)]
pub enum Error {
    #[error("focus")]
    /// Focus-related failure.
    Focus(String),
    #[error("render")]
    /// Rendering failure.
    Render(String),
    #[error("geometry")]
    /// Geometry failure.
    Geometry(String),
    #[error("layout")]
    /// Layout failure.
    Layout(String),
    #[error("runloop")]
    /// Run loop failure.
    RunLoop(String),
    #[error("internal")]
    /// Internal error.
    Internal(String),
    #[error("invalid")]
    /// Invalid input error.
    Invalid(String),
    #[error("unknown command")]
    /// Command not found.
    UnknownCommand(String),

    #[error("parse error")]
    /// Parsing failure.
    Parse(ParseError),

    #[error("script run error")]
    /// Script execution failure.
    Script(String),

    /// No result was generated on node traversal
    #[error("no result")]
    NoResult,
}

impl From<mpsc::RecvError> for Error {
    fn from(e: mpsc::RecvError) -> Self {
        Self::RunLoop(e.to_string())
    }
}

impl From<geom::Error> for Error {
    fn from(e: geom::Error) -> Self {
        Self::Geometry(e.to_string())
    }
}
