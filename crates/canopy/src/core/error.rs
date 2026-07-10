use std::{result::Result as StdResult, sync::mpsc};

use thiserror::Error;

use crate::{commands::CommandError, core::id::NodeId, geom, layout::LayoutValidationError};

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
#[derive(Error, Debug)]
pub enum Error {
    #[error("focus: {0}")]
    /// Focus-related failure.
    Focus(String),
    #[error("render: {0}")]
    /// Rendering failure.
    Render(String),
    /// A render target exceeds its configured width limit.
    #[error("render target width {requested} exceeds limit {limit}")]
    RenderWidthLimit {
        /// Requested target width.
        requested: u32,
        /// Configured maximum width.
        limit: u32,
    },
    /// A render target exceeds its configured height limit.
    #[error("render target height {requested} exceeds limit {limit}")]
    RenderHeightLimit {
        /// Requested target height.
        requested: u32,
        /// Configured maximum height.
        limit: u32,
    },
    /// Render-target dimensions cannot be represented as a cell count.
    #[error("render target {width}x{height} cell count overflows usize")]
    RenderCellCountOverflow {
        /// Requested target width.
        width: u32,
        /// Requested target height.
        height: u32,
    },
    /// A render target exceeds its configured total-cell limit.
    #[error("render target cell count {requested} exceeds limit {limit}")]
    RenderCellLimit {
        /// Requested target cell count.
        requested: usize,
        /// Configured maximum cell count.
        limit: usize,
    },
    /// Render-target backing storage could not be reserved.
    #[error("could not allocate render target with {cells} cells")]
    RenderAllocation {
        /// Requested target cell count.
        cells: usize,
    },
    /// A single-cell drawing API received a character with an invalid width.
    #[error("single-cell drawing character {ch:?} has terminal width {width}")]
    InvalidCellCharacter {
        /// Rejected character.
        ch: char,
        /// Computed terminal width.
        width: usize,
    },
    #[error("geometry: {0}")]
    /// Geometry failure.
    Geometry(String),
    #[error("layout: {0}")]
    /// Layout failure.
    Layout(String),
    /// Invalid layout configuration.
    #[error(transparent)]
    InvalidLayout(#[from] LayoutValidationError),
    #[error("runloop: {0}")]
    /// Run loop failure.
    RunLoop(String),
    #[error("internal: {0}")]
    /// Internal error.
    Internal(String),
    /// Core invariant violation.
    #[error("invariant violation: {0}")]
    Invariant(String),
    /// Re-entrant widget borrow attempt.
    #[error("re-entrant widget borrow: {0:?}")]
    ReentrantWidgetBorrow(NodeId),
    /// Widget access failure with node and operation context.
    #[error("widget access: {0}")]
    WidgetAccess(String),
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
    /// A live node stores a different widget type than requested.
    #[error("node {node:?} does not store {expected}")]
    NodeTypeMismatch {
        /// Node whose widget type was checked.
        node: NodeId,
        /// Requested widget type.
        expected: &'static str,
    },
    /// A query matched multiple nodes.
    #[error("multiple matches")]
    MultipleMatches,
    /// Duplicate child key under the same parent.
    #[error("duplicate child key: {0}")]
    DuplicateChildKey(String),
    /// Duplicate child under the same parent.
    #[error("duplicate child {child:?} under parent {parent:?}")]
    DuplicateChild {
        /// Parent node.
        parent: NodeId,
        /// Child node.
        child: NodeId,
    },
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
    /// Structural mutation attempted while a failed edit is unwinding.
    #[error("tree edit {operation} is not allowed during rollback")]
    TreeEditDuringRollback {
        /// Requested tree operation.
        operation: &'static str,
    },
    /// Command dispatch failure.
    #[error(transparent)]
    Command(#[from] CommandError),

    #[error("parse error: {0}")]
    /// Parsing failure.
    Parse(#[source] ParseError),

    #[error("script run error: {0}")]
    /// Script execution failure.
    Script(String),

    /// Script execution failure with stable host category fields.
    #[error("script run error: {message}")]
    ScriptStructured {
        /// Stable script-visible category.
        kind: String,
        /// Command id when the error came from command dispatch.
        command: Option<String>,
        /// Owner name when the error came from node-target resolution.
        owner: Option<String>,
        /// Human-readable error message.
        message: String,
    },

    /// Script execution exceeded its cooperative timeout.
    #[error("script evaluation exceeded {timeout_ms}ms")]
    ScriptTimeout {
        /// Requested timeout in milliseconds.
        timeout_ms: u64,
    },

    /// No result was generated on node traversal.
    #[error("no result")]
    NoResult,

    /// Node not found in the arena.
    #[error("node not found: {0:?}")]
    NodeNotFound(NodeId),
    /// Node exists but is not attached to the root tree.
    #[error("node is detached: {0:?}")]
    NodeDetached(NodeId),
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
