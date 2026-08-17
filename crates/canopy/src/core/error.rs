use std::{error::Error as StdError, fmt, io, result::Result as StdResult, sync::mpsc};

use thiserror::Error;

use crate::{commands::CommandError, core::id::NodeId, geom, layout::LayoutValidationError};

/// Result type for canopy-core operations.
pub type Result<T> = StdResult<T, Error>;

/// Parse error marker type.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct ParseError {
    /// Parse error message.
    pub message: String,
    /// One-based source line, when known.
    pub line: Option<usize>,
    /// Source byte offset, when known.
    pub offset: Option<usize>,
}

impl ParseError {
    /// Construct a parse error from a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            offset: None,
        }
    }

    /// Construct a parse error with optional line/offset information.
    pub fn with_position(
        message: impl Into<String>,
        line: Option<usize>,
        offset: Option<usize>,
    ) -> Self {
        Self {
            message: message.into(),
            line,
            offset,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)?;
        match (self.line, self.offset) {
            (Some(line), Some(offset)) => write!(f, " (line {line}, offset {offset})"),
            (Some(line), None) => write!(f, " (line {line})"),
            (None, Some(offset)) => write!(f, " (offset {offset})"),
            (None, None) => Ok(()),
        }
    }
}

impl StdError for ParseError {}

/// Phase in which a node-bound widget operation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeOperationKind {
    /// Widget access or lifecycle callback.
    Access,
    /// Widget measurement or layout.
    Layout,
    /// Widget rendering.
    Render,
}

impl fmt::Display for NodeOperationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Access => "widget access",
            Self::Layout => "layout",
            Self::Render => "render",
        })
    }
}

/// Stable category for a structured script or command failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptErrorKind {
    /// Cooperative execution timeout.
    Timeout,
    /// Node lookup failed.
    NodeNotFound,
    /// A node exists but is detached.
    NodeDetached,
    /// A value or widget type did not match.
    TypeMismatch,
    /// A requested value was not found.
    NotFound,
    /// Invalid input or operation.
    Invalid,
    /// Unclassified Canopy failure.
    Canopy,
    /// Unknown command identifier.
    UnknownCommand,
    /// Duplicate command identifier.
    /// Conflicting command definition.
    ConflictingCommand,
    /// Invalid command definition.
    InvalidCommand,
    /// No command target was found.
    NoTarget,
    /// A command node handle is stale.
    InvalidNode,
    /// Positional argument count mismatch.
    ArityMismatch,
    /// Required named argument is missing.
    MissingNamedArgument,
    /// An unknown named argument was supplied.
    UnknownNamedArgument,
    /// Argument conversion failed.
    Conversion,
    /// An injected value is missing.
    MissingInjected,
    /// The routed target has the wrong widget type.
    TargetTypeMismatch,
    /// Command implementation returned an error.
    CommandExecution,
    /// Another top-level script evaluation is active.
    ScriptBusy,
}

impl ScriptErrorKind {
    /// Return the stable protocol label for this category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::NodeNotFound => "node_not_found",
            Self::NodeDetached => "node_detached",
            Self::TypeMismatch => "type_mismatch",
            Self::NotFound => "not_found",
            Self::Invalid => "invalid",
            Self::Canopy => "canopy_error",
            Self::UnknownCommand => "unknown_command",
            Self::ConflictingCommand => "conflicting_command",
            Self::InvalidCommand => "invalid_command",
            Self::NoTarget => "no_target",
            Self::InvalidNode => "node_invalid",
            Self::ArityMismatch => "arity_mismatch",
            Self::MissingNamedArgument => "missing_named_arg",
            Self::UnknownNamedArgument => "unknown_named_arg",
            Self::Conversion => "conversion",
            Self::MissingInjected => "missing_injected",
            Self::TargetTypeMismatch => "target_type_mismatch",
            Self::CommandExecution => "command_exec",
            Self::ScriptBusy => "script_busy",
        }
    }
}

impl fmt::Display for ScriptErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Core error type.
#[derive(Error, Debug)]
pub enum Error {
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
    /// Geometry failure.
    #[error(transparent)]
    Geometry(#[from] geom::Error),
    /// Invalid layout configuration.
    #[error(transparent)]
    InvalidLayout(#[from] LayoutValidationError),
    /// Terminal I/O failure.
    #[error("terminal I/O failed: {0}")]
    TerminalIo(#[source] io::Error),
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
    /// Node-bound widget operation failure with its original source.
    #[error("{kind} {operation} for node {node:?} at {path}: {source}")]
    NodeOperation {
        /// Operation phase.
        kind: NodeOperationKind,
        /// Stable operation name.
        operation: &'static str,
        /// Node being operated on.
        node: NodeId,
        /// Node path at the time of failure.
        path: String,
        /// Original typed failure.
        #[source]
        source: Box<Self>,
    },
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
        kind: ScriptErrorKind,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_preserves_source_position() {
        let error = ParseError::with_position("unexpected token", Some(3), Some(17));

        assert_eq!(error.message, "unexpected token");
        assert_eq!(error.line, Some(3));
        assert_eq!(error.offset, Some(17));
        assert_eq!(error.to_string(), "unexpected token (line 3, offset 17)");
    }
}
