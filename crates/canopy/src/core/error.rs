use std::{result::Result as StdResult, sync::mpsc};

use thiserror::Error;

use crate::{core::id::NodeId, geom};

/// Result type for canopy-core operations.
pub type Result<T> = StdResult<T, Error>;

/// Parse error marker type.
#[derive(PartialEq, Eq, Error, Debug, Clone)]
#[error("{message}")]
pub struct ParseError {
    /// Parse error message, optionally including location.
    message: String,
}

impl ParseError {
    /// Construct a parse error from a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Construct a parse error with optional line/offset information.
    pub fn with_position(
        message: impl Into<String>,
        line: Option<usize>,
        offset: Option<usize>,
    ) -> Self {
        let message = message.into();
        let message = match (line, offset) {
            (Some(line), Some(offset)) => format!("{message} (line {line}, offset {offset})"),
            (Some(line), None) => format!("{message} (line {line})"),
            (None, Some(offset)) => format!("{message} (offset {offset})"),
            (None, None) => message,
        };
        Self { message }
    }
}

/// Core error type.
#[derive(PartialEq, Eq, Error, Debug, Clone)]
pub enum Error {
    #[error("focus: {0}")]
    /// Focus-related failure.
    Focus(String),
    #[error("render: {0}")]
    /// Rendering failure.
    Render(String),
    #[error("geometry: {0}")]
    /// Geometry failure.
    Geometry(String),
    #[error("layout: {0}")]
    /// Layout failure.
    Layout(String),
    #[error("runloop: {0}")]
    /// Run loop failure.
    RunLoop(String),
    #[error("internal: {0}")]
    /// Internal error.
    Internal(String),
    #[error("invalid: {0}")]
    /// Invalid input error.
    Invalid(String),
    /// Requested item was not found.
    #[error("not found: {0}")]
    NotFound(String),
    /// Widget type mismatch.
    #[error("type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        /// Expected widget type name.
        expected: String,
        /// Actual widget type name.
        actual: String,
    },
    /// A query matched multiple nodes.
    #[error("multiple matches")]
    MultipleMatches,
    /// Duplicate child key under the same parent.
    #[error("duplicate child key: {0}")]
    DuplicateChildKey(String),
    /// Child is already attached to a parent.
    #[error("already attached: {0:?}")]
    AlreadyAttached(NodeId),
    /// Attaching would create a parent/child cycle.
    #[error("would create cycle: parent {parent:?}, child {child:?}")]
    WouldCreateCycle {
        /// Parent node involved in the cycle.
        parent: NodeId,
        /// Child node involved in the cycle.
        child: NodeId,
    },
    /// Invalid structural operation.
    #[error("invalid operation: {0}")]
    InvalidOperation(String),
    #[error("unknown command: {0}")]
    /// Command not found.
    UnknownCommand(String),

    #[error("parse error: {0}")]
    /// Parsing failure.
    Parse(#[source] ParseError),

    #[error("script run error: {0}")]
    /// Script execution failure.
    Script(String),

    /// No result was generated on node traversal.
    #[error("no result")]
    NoResult,

    /// Node not found in the arena.
    #[error("node not found: {0:?}")]
    NodeNotFound(NodeId),
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
