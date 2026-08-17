//! Conversions between Luau host errors and canopy errors.

use std::{fmt, time::Duration};

use ruau::{
    bytecode::CompileError,
    session::LifecycleError,
    surface::PrepareGraphError,
    vm::{
        ExecError, MarshaledScriptError, RuntimeError, RuntimeErrorKind, Scope, ScriptError,
        ScriptErrorField,
    },
};

use super::{
    ScriptCheckResult, commands, error, marshaled_value_to_display, module_diagnostic_to_script,
    scoped_value_to_display,
};

/// Convert an Ruau compile error to Canopy's parse error shape.
pub(super) fn compile_error_to_canopy(err: &CompileError) -> error::Error {
    let begin = err.location().map(|location| location.begin);
    error::Error::Parse(error::ParseError::with_position(
        err.message(),
        begin.map(|position| position.line as usize + 1),
        begin.map(|position| position.column as usize + 1),
    ))
}

/// Convert preparation failures into Canopy's existing public error categories.
pub(super) fn prepare_graph_error_to_canopy(error: &PrepareGraphError) -> error::Error {
    if let Some(diagnostics) = error.diagnostics()
        && diagnostics.has_errors()
    {
        let result = ScriptCheckResult {
            diagnostics: diagnostics
                .records()
                .map(module_diagnostic_to_script)
                .collect(),
        };
        return error::Error::Parse(error::ParseError::new(result.format_diagnostics()));
    }
    if let Some(error) = error.compile_error() {
        return compile_error_to_canopy(error);
    }
    error::Error::Script(format!("preparing script graph failed: {error}"))
}

/// Convert a displayable error into a canopy script error.
pub(super) fn lua_to_canopy(err: impl fmt::Display) -> error::Error {
    error::Error::Script(err.to_string())
}

/// Convert a canopy error into a host-call error.
impl From<error::Error> for RuntimeError {
    fn from(error: error::Error) -> Self {
        canopy_to_host(&error)
    }
}

/// Convert a canopy error into a structured Ruau runtime error.
pub(super) fn canopy_to_host(err: &error::Error) -> RuntimeError {
    let payload = CanopyErrorPayload::from(err);
    let mut fields = vec![ScriptErrorField::new("kind", payload.kind.as_str())];
    if let Some(command) = payload.command.clone() {
        fields.push(ScriptErrorField::new("command", command));
    }
    if let Some(owner) = payload.owner.clone() {
        fields.push(ScriptErrorField::new("owner", owner));
    }
    RuntimeError::structured(payload.message.clone(), fields).with_payload(payload)
}

/// Normalized cloneable canopy error payload carried through Ruau errors.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CanopyErrorPayload {
    /// Stable script-visible category.
    kind: error::ScriptErrorKind,
    /// Timeout duration for script timeout errors.
    timeout_ms: Option<u64>,
    /// Command id when the error came from command dispatch.
    command: Option<String>,
    /// Owner name when the error came from node-target resolution.
    owner: Option<String>,
    /// Human-readable error message.
    message: String,
}

impl From<&error::Error> for CanopyErrorPayload {
    fn from(err: &error::Error) -> Self {
        match err {
            error::Error::Command(err) => Self::from(err),
            error::Error::ScriptTimeout { timeout_ms } => {
                Self::new(error::ScriptErrorKind::Timeout, err.to_string())
                    .with_timeout_ms(*timeout_ms)
            }
            error::Error::NodeNotFound(node) => {
                Self::new(error::ScriptErrorKind::NodeNotFound, err.to_string())
                    .with_owner(format!("{node:?}"))
            }
            error::Error::NodeDetached(node) => {
                Self::new(error::ScriptErrorKind::NodeDetached, err.to_string())
                    .with_owner(format!("{node:?}"))
            }
            error::Error::TypeMismatch { .. } | error::Error::NodeTypeMismatch { .. } => {
                Self::new(error::ScriptErrorKind::TypeMismatch, err.to_string())
            }
            error::Error::NotFound(_) => {
                Self::new(error::ScriptErrorKind::NotFound, err.to_string())
            }
            error::Error::Invalid(_) | error::Error::InvalidOperation(_) => {
                Self::new(error::ScriptErrorKind::Invalid, err.to_string())
            }
            error::Error::ScriptStructured {
                kind,
                command,
                owner,
                message,
            } => Self {
                kind: *kind,
                timeout_ms: None,
                command: command.clone(),
                owner: owner.clone(),
                message: message.clone(),
            },
            _ => Self::new(error::ScriptErrorKind::Canopy, err.to_string()),
        }
    }
}

impl From<&commands::CommandError> for CanopyErrorPayload {
    fn from(err: &commands::CommandError) -> Self {
        match err {
            commands::CommandError::UnknownCommand { id } => {
                Self::new(error::ScriptErrorKind::UnknownCommand, err.to_string())
                    .with_command(id.clone())
            }
            commands::CommandError::ConflictingCommand { id } => {
                Self::new(error::ScriptErrorKind::ConflictingCommand, err.to_string())
                    .with_command(id.clone())
            }
            commands::CommandError::InvalidCommand { id, .. } => {
                Self::new(error::ScriptErrorKind::InvalidCommand, err.to_string())
                    .with_command(id.clone())
            }
            commands::CommandError::NoTarget { id, owner } => {
                Self::new(error::ScriptErrorKind::NoTarget, err.to_string())
                    .with_command(id.clone())
                    .with_owner(owner.clone())
            }
            commands::CommandError::InvalidNode { .. } => {
                Self::new(error::ScriptErrorKind::InvalidNode, err.to_string())
            }
            commands::CommandError::ArityMismatch { .. } => {
                Self::new(error::ScriptErrorKind::ArityMismatch, err.to_string())
            }
            commands::CommandError::MissingNamedArg { .. } => Self::new(
                error::ScriptErrorKind::MissingNamedArgument,
                err.to_string(),
            ),
            commands::CommandError::UnknownNamedArg { .. } => Self::new(
                error::ScriptErrorKind::UnknownNamedArgument,
                err.to_string(),
            ),
            commands::CommandError::TypeMismatch { .. } => {
                Self::new(error::ScriptErrorKind::TypeMismatch, err.to_string())
            }
            commands::CommandError::MissingInjected { .. } => {
                Self::new(error::ScriptErrorKind::MissingInjected, err.to_string())
            }
            commands::CommandError::Conversion { .. } => {
                Self::new(error::ScriptErrorKind::Conversion, err.to_string())
            }
            commands::CommandError::TargetTypeMismatch => {
                Self::new(error::ScriptErrorKind::TargetTypeMismatch, err.to_string())
            }
            commands::CommandError::Exec(_) => {
                Self::new(error::ScriptErrorKind::CommandExecution, err.to_string())
            }
        }
    }
}

impl CanopyErrorPayload {
    /// Builds a payload without command routing context.
    pub(super) fn new(kind: error::ScriptErrorKind, message: String) -> Self {
        Self {
            kind,
            timeout_ms: None,
            command: None,
            owner: None,
            message,
        }
    }

    /// Attaches a timeout duration.
    pub(super) fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// Attaches a command id.
    pub(super) fn with_command(mut self, command: String) -> Self {
        self.command = Some(command);
        self
    }

    /// Attaches an owner name.
    pub(super) fn with_owner(mut self, owner: String) -> Self {
        self.owner = Some(owner);
        self
    }

    /// Convert this host payload into a core error while preserving traceback context.
    pub(super) fn to_canopy_error(&self, label: &str, traceback: Option<&str>) -> error::Error {
        if let Some(timeout_ms) = self.timeout_ms {
            return error::Error::ScriptTimeout { timeout_ms };
        }
        let message = match traceback {
            Some(traceback) => format!("{label} failed: {}\n{traceback}", self.message),
            None => format!("{label} failed: {}", self.message),
        };
        error::Error::ScriptStructured {
            kind: self.kind,
            command: self.command.clone(),
            owner: self.owner.clone(),
            message,
        }
    }
}

/// Convert a caught script error into a canopy error.
pub(super) fn script_error_to_canopy<'s>(
    scope: &Scope<'s>,
    error: &ScriptError<'s>,
    label: &str,
    timeout: Option<Duration>,
) -> error::Error {
    if let Some(timeout_error) = timeout_error(error.kind(), timeout) {
        return timeout_error;
    }
    if let Some(payload) = error.payload_ref::<CanopyErrorPayload>() {
        return payload.to_canopy_error(label, error.traceback());
    }
    let message = scoped_value_to_display(scope, error.value());
    match error.traceback() {
        Some(traceback) => error::Error::Script(format!("{label} failed: {message}\n{traceback}")),
        None => error::Error::Script(format!("{label} failed: {message}")),
    }
}

/// Convert a fatal VM error into a canopy error.
pub(super) fn runtime_error_to_canopy(
    error: &RuntimeError,
    label: &str,
    timeout: Option<Duration>,
) -> error::Error {
    if let Some(timeout_error) = timeout_error(error.kind(), timeout) {
        return timeout_error;
    }
    if let Some(payload) = error.payload_ref::<CanopyErrorPayload>() {
        return payload.to_canopy_error(label, None);
    }
    error::Error::Script(format!("{label} failed: {error}"))
}

/// Convert an async owned-entry execution error into a canopy error.
fn exec_error_to_canopy(error: &ExecError, label: &str, timeout: Option<Duration>) -> error::Error {
    if let Some(timeout_error) = timeout_error(error.kind(), timeout) {
        return timeout_error;
    }
    match error {
        ExecError::Script(error) => marshaled_script_error_to_canopy(error, label, timeout),
        ExecError::Stopped(_) => timeout_error(error.kind(), timeout).unwrap_or_else(|| {
            error::Error::Script(format!("{label} failed: script evaluation was cancelled"))
        }),
        ExecError::PanicPoison => error::Error::Script(format!(
            "{label} failed: script VM is poisoned and refuses further work"
        )),
        ExecError::Entry { message } => error::Error::Script(format!("{label} failed: {message}")),
        ExecError::Marshal { message } => error::Error::Script(format!(
            "{label} failed: marshaling script result failed: {message}"
        )),
    }
}

/// Convert a retained-runtime state or execution failure into a canopy error.
pub(super) fn retained_runtime_error_to_canopy(
    error: &LifecycleError,
    label: &str,
    timeout: Option<Duration>,
) -> error::Error {
    match error {
        LifecycleError::Exec(error) => exec_error_to_canopy(error, label, timeout),
        LifecycleError::Runtime(error) => runtime_error_to_canopy(error, label, timeout),
        LifecycleError::StaleHandle { .. }
        | LifecycleError::InUse { .. }
        | LifecycleError::PermanentHandle { .. }
        | LifecycleError::Load(_)
        | LifecycleError::PreparedLoad(_)
        | LifecycleError::BindEnvironment(_) => {
            error::Error::Script(format!("{label} failed: {error}"))
        }
    }
}

/// Convert an async owned script error into a canopy error.
fn marshaled_script_error_to_canopy(
    error: &MarshaledScriptError,
    label: &str,
    timeout: Option<Duration>,
) -> error::Error {
    if let Some(timeout_error) = timeout_error(error.kind(), timeout) {
        return timeout_error;
    }
    if let Some(payload) = error.payload_ref::<CanopyErrorPayload>() {
        return payload.to_canopy_error(label, error.traceback());
    }
    let message = marshaled_value_to_display(error.value());
    match error.traceback() {
        Some(traceback) => error::Error::Script(format!("{label} failed: {message}\n{traceback}")),
        None => error::Error::Script(format!("{label} failed: {message}")),
    }
}

/// Build the cooperative-timeout error for a cancelled or deadlined run.
fn timeout_error(kind: RuntimeErrorKind, timeout: Option<Duration>) -> Option<error::Error> {
    if !matches!(
        kind,
        RuntimeErrorKind::Cancelled | RuntimeErrorKind::Deadline
    ) {
        return None;
    }
    let timeout_ms = timeout
        .map(|timeout| u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    Some(error::Error::ScriptTimeout { timeout_ms })
}
