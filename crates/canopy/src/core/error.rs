use std::{error::Error as StdError, fmt, io, result::Result as StdResult, sync::mpsc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    commands::CommandError,
    core::{id::NodeId, keyroute::KeyDispatchDivergence},
    geom,
    layout::LayoutValidationError,
};

/// Result type for canopy operations.
pub type Result<T> = StdResult<T, Error>;

/// A parse failure, with its source position when known.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct ParseError {
    /// Parse error message.
    pub message: String,
    /// One-based source line, when known.
    pub line: Option<usize>,
    /// One-based source column, when known.
    pub column: Option<usize>,
}

impl ParseError {
    /// Construct a parse error from a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            column: None,
        }
    }

    /// Construct a parse error with optional line and column information.
    pub fn with_position(
        message: impl Into<String>,
        line: Option<usize>,
        column: Option<usize>,
    ) -> Self {
        Self {
            message: message.into(),
            line,
            column,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)?;
        match (self.line, self.column) {
            (Some(line), Some(column)) => write!(f, " (line {line}, column {column})"),
            (Some(line), None) => write!(f, " (line {line})"),
            (None, Some(column)) => write!(f, " (column {column})"),
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

/// Define the structured script error categories and their protocol labels.
macro_rules! script_error_kinds {
    ($( $(#[$meta:meta])* $variant:ident => $label:literal ),* $(,)?) => {
        /// Stable category for a structured script or command failure.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub enum ScriptErrorKind {
            $(
                $(#[$meta])*
                #[serde(rename = $label)]
                $variant,
            )*
        }

        impl ScriptErrorKind {
            /// Return the stable protocol label for this category.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( Self::$variant => $label, )*
                }
            }
        }
    };
}

script_error_kinds! {
    /// Cooperative execution timeout.
    Timeout => "timeout",
    /// Node lookup failed.
    NodeNotFound => "node_not_found",
    /// A node exists but is detached.
    NodeDetached => "node_detached",
    /// A value or widget type did not match.
    TypeMismatch => "type_mismatch",
    /// A requested value was not found.
    NotFound => "not_found",
    /// Invalid input or operation.
    Invalid => "invalid",
    /// Operation requires an unwound widget callback boundary.
    InvalidPhase => "invalid_phase",
    /// Unclassified Canopy failure.
    Canopy => "canopy_error",
    /// Unknown command identifier.
    UnknownCommand => "unknown_command",
    /// Conflicting command definition.
    ConflictingCommand => "conflicting_command",
    /// Invalid command definition.
    InvalidCommand => "invalid_command",
    /// No command target was found.
    NoTarget => "no_target",
    /// An exact node does not own the command.
    WrongOwner => "wrong_owner",
    /// The command is currently disabled.
    DisabledCommand => "command_disabled",
    /// A command node handle is stale.
    InvalidNode => "node_invalid",
    /// Positional argument count mismatch.
    ArityMismatch => "arity_mismatch",
    /// Required named argument is missing.
    MissingNamedArgument => "missing_named_arg",
    /// An unknown named argument was supplied.
    UnknownNamedArgument => "unknown_named_arg",
    /// Argument conversion failed.
    Conversion => "conversion",
    /// An injected value is missing.
    MissingInjected => "missing_injected",
    /// The routed target has the wrong widget type.
    TargetTypeMismatch => "target_type_mismatch",
    /// Command implementation returned an error.
    CommandExecution => "command_exec",
    /// Another top-level script evaluation is active.
    ScriptBusy => "script_busy",
    /// Script evaluation was explicitly cancelled.
    ScriptCancelled => "script_cancelled",
}

impl fmt::Display for ScriptErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Core error type.
#[derive(Error, Debug)]
pub enum Error {
    /// Evaluation explicitly cancelled by its caller.
    #[error("script evaluation cancelled")]
    ScriptCancelled,
    /// Another runtime turn or top-level script evaluation is active.
    #[error("script busy: {0}")]
    ScriptBusy(String),
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
    /// Turn driver failure: a closed channel, a failed backend, or a runtime
    /// that could not start.
    #[error("driver: {0}")]
    Driver(String),
    /// Internal failure, such as a broken core invariant.
    #[error("internal: {0}")]
    Internal(String),
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
    /// Invalid input or operation.
    #[error("invalid: {0}")]
    Invalid(String),
    /// Requested item was not found.
    #[error("not found: {0}")]
    NotFound(String),
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
    /// Operation attempted before a mutable widget callback returned.
    #[error("{operation} is not allowed during a widget mutation callback")]
    InvalidPhase {
        /// Operation requiring an unwound callback boundary.
        operation: &'static str,
    },
    /// Structural mutation attempted while a failed edit is unwinding.
    #[error("tree edit {operation} is not allowed during rollback")]
    TreeEditDuringRollback {
        /// Requested tree operation.
        operation: &'static str,
    },
    /// Command dispatch failure.
    #[error(transparent)]
    Command(#[from] CommandError),
    /// A checked key dispatch diverged from its prospective analysis.
    #[error("checked key dispatch diverged from the analyzed route")]
    KeyDispatchDivergence(Box<KeyDispatchDivergence>),

    #[error("parse error: {0}")]
    /// Parsing failure.
    Parse(#[from] ParseError),

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

impl Error {
    /// Return the stable script-visible category of this error.
    ///
    /// Luau error payloads and automation reports both classify through this
    /// one mapping.
    pub fn script_kind(&self) -> ScriptErrorKind {
        match self {
            Self::ScriptCancelled => ScriptErrorKind::ScriptCancelled,
            Self::ScriptBusy(_) => ScriptErrorKind::ScriptBusy,
            Self::ScriptTimeout { .. } => ScriptErrorKind::Timeout,
            Self::ScriptStructured { kind, .. } => *kind,
            Self::Command(error) => error.script_kind(),
            Self::NodeOperation { source, .. } => source.script_kind(),
            Self::NodeNotFound(_) => ScriptErrorKind::NodeNotFound,
            Self::NodeDetached(_) => ScriptErrorKind::NodeDetached,
            Self::NodeTypeMismatch { .. } => ScriptErrorKind::TypeMismatch,
            Self::NotFound(_) => ScriptErrorKind::NotFound,
            Self::Invalid(_) => ScriptErrorKind::Invalid,
            Self::InvalidPhase { .. } => ScriptErrorKind::InvalidPhase,
            Self::RenderWidthLimit { .. }
            | Self::RenderHeightLimit { .. }
            | Self::RenderCellCountOverflow { .. }
            | Self::RenderCellLimit { .. }
            | Self::RenderAllocation { .. }
            | Self::InvalidCellCharacter { .. }
            | Self::Geometry(_)
            | Self::InvalidLayout(_)
            | Self::TerminalIo(_)
            | Self::Driver(_)
            | Self::Internal(_)
            | Self::ReentrantWidgetBorrow(_)
            | Self::MultipleMatches
            | Self::DuplicateChildKey(_)
            | Self::DuplicateChild { .. }
            | Self::AlreadyAttached(_)
            | Self::WouldCreateCycle { .. }
            | Self::TreeEditDuringRollback { .. }
            | Self::KeyDispatchDivergence(_)
            | Self::Parse(_) => ScriptErrorKind::Canopy,
        }
    }

    /// Construct a structured script failure.
    pub(crate) fn script_structured(kind: ScriptErrorKind, message: impl Into<String>) -> Self {
        Self::ScriptStructured {
            kind,
            command: None,
            owner: None,
            message: message.into(),
        }
    }

    /// Attach an owner name to a structured script failure.
    pub(crate) fn with_owner(mut self, owner: impl Into<String>) -> Self {
        if let Self::ScriptStructured { owner: slot, .. } = &mut self {
            *slot = Some(owner.into());
        }
        self
    }

    /// Construct an unclassified structured script failure.
    pub(crate) fn script(message: impl Into<String>) -> Self {
        Self::script_structured(ScriptErrorKind::Canopy, message)
    }
}

impl From<mpsc::RecvError> for Error {
    fn from(e: mpsc::RecvError) -> Self {
        Self::Driver(e.to_string())
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
        assert_eq!(error.column, Some(17));
        assert_eq!(error.to_string(), "unexpected token (line 3, column 17)");
    }
}
