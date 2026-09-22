//! Conversions between Luau host errors and canopy errors.

use std::time::Duration;

use ruau::{
    bytecode::CompileError,
    session::LifecycleError,
    surface::PrepareGraphError,
    vm::{
        ExecError, MarshaledScriptError, RuntimeError, RuntimeErrorKind, Scope, ScriptError,
        ScriptErrorField, VmErrorInfo,
    },
};

use super::{ScriptCheckResult, commands, error, module_diagnostic_to_script};

/// Convert a Ruau compile error to Canopy's parse error shape.
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
    error::Error::script(format!("preparing script graph failed: {error}"))
}

/// Convert a canopy error into a structured Ruau runtime error.
impl From<error::Error> for RuntimeError {
    fn from(error: error::Error) -> Self {
        let payload = CanopyErrorPayload::from(&error);
        let mut fields = vec![ScriptErrorField::new("kind", payload.kind.as_str())];
        if let Some(command) = payload.command.clone() {
            fields.push(ScriptErrorField::new("command", command));
        }
        if let Some(owner) = payload.owner.clone() {
            fields.push(ScriptErrorField::new("owner", owner));
        }
        Self::structured(payload.message.clone(), fields).with_payload(payload)
    }
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
        if let error::Error::Command(err) = err {
            return Self::from(err);
        }
        let payload = Self::new(err.script_kind(), err.to_string());
        match err {
            error::Error::ScriptTimeout { timeout_ms } => payload.with_timeout_ms(*timeout_ms),
            error::Error::NodeNotFound(node) | error::Error::NodeDetached(node) => {
                payload.with_owner(format!("{node:?}"))
            }
            error::Error::ScriptStructured {
                command,
                owner,
                message,
                ..
            } => Self {
                command: command.clone(),
                owner: owner.clone(),
                message: message.clone(),
                ..payload
            },
            _ => payload,
        }
    }
}

impl From<&commands::CommandError> for CanopyErrorPayload {
    fn from(err: &commands::CommandError) -> Self {
        let payload = Self::new(err.script_kind(), err.to_string());
        match err {
            commands::CommandError::UnknownCommand { id }
            | commands::CommandError::ConflictingCommand { id }
            | commands::CommandError::InvalidCommand { id, .. }
            | commands::CommandError::Disabled { id, .. } => payload.with_command(id.clone()),
            commands::CommandError::NoTarget { id, owner } => {
                payload.with_command(id.clone()).with_owner(owner.clone())
            }
            commands::CommandError::WrongOwner { id, expected, .. } => payload
                .with_command(id.clone())
                .with_owner(expected.clone()),
            _ => payload,
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

    /// Convert this host payload into a core error while preserving traceback
    /// context.
    fn to_canopy_error(&self, label: &str, traceback: Option<&str>) -> error::Error {
        if let Some(timeout_ms) = self.timeout_ms {
            return error::Error::ScriptTimeout { timeout_ms };
        }
        error::Error::ScriptStructured {
            kind: self.kind,
            command: self.command.clone(),
            owner: self.owner.clone(),
            message: labelled_failure(label, &self.message, traceback),
        }
    }
}

/// Render one `{label} failed: {message}` line, appending a traceback when one
/// was captured.
fn labelled_failure(label: &str, message: &str, traceback: Option<&str>) -> String {
    match traceback {
        Some(traceback) => format!("{label} failed: {message}\n{traceback}"),
        None => format!("{label} failed: {message}"),
    }
}

/// Convert any VM error surface into a canopy error.
///
/// Timeouts win, then a structured canopy payload, then the caller's message.
fn vm_error_to_canopy<E: VmErrorInfo>(
    error: &E,
    label: &str,
    timeout: Option<Duration>,
    message: impl FnOnce() -> String,
) -> error::Error {
    if let Some(timeout_error) = timeout_error(error.kind(), timeout) {
        return timeout_error;
    }
    if let Some(payload) = error.payload_ref::<CanopyErrorPayload>() {
        return payload.to_canopy_error(label, error.traceback());
    }
    error::Error::script(labelled_failure(label, &message(), error.traceback()))
}

/// Convert a caught script error into a canopy error.
pub(super) fn script_error_to_canopy<'s>(
    scope: &Scope<'s>,
    error: &ScriptError<'s>,
    label: &str,
    timeout: Option<Duration>,
) -> error::Error {
    vm_error_to_canopy(error, label, timeout, || error.value().display(scope))
}

/// Convert a fatal VM error into a canopy error.
pub(super) fn runtime_error_to_canopy(
    error: &RuntimeError,
    label: &str,
    timeout: Option<Duration>,
) -> error::Error {
    vm_error_to_canopy(error, label, timeout, || error.to_string())
}

/// Convert an async owned-entry execution error into a canopy error.
fn exec_error_to_canopy(error: &ExecError, label: &str, timeout: Option<Duration>) -> error::Error {
    match error {
        ExecError::Script(error) => marshaled_script_error_to_canopy(error, label, timeout),
        ExecError::Stopped(_) => script_timeout(timeout),
        ExecError::PanicPoison => error::Error::script(format!(
            "{label} failed: script VM is poisoned and refuses further work"
        )),
        ExecError::Entry { message } => error::Error::script(format!("{label} failed: {message}")),
        ExecError::Marshal { message } => error::Error::script(format!(
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
            error::Error::script(format!("{label} failed: {error}"))
        }
    }
}

/// Convert an async owned script error into a canopy error.
fn marshaled_script_error_to_canopy(
    error: &MarshaledScriptError,
    label: &str,
    timeout: Option<Duration>,
) -> error::Error {
    vm_error_to_canopy(error, label, timeout, || error.value().display_lua())
}

/// Build the cooperative-timeout error for a run that stopped early.
fn script_timeout(timeout: Option<Duration>) -> error::Error {
    let timeout_ms = timeout
        .map(|timeout| u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    error::Error::ScriptTimeout { timeout_ms }
}

/// Build the cooperative-timeout error for a cancelled or deadlined run.
fn timeout_error(kind: RuntimeErrorKind, timeout: Option<Duration>) -> Option<error::Error> {
    matches!(
        kind,
        RuntimeErrorKind::Cancelled | RuntimeErrorKind::Deadline
    )
    .then(|| script_timeout(timeout))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commands::CommandError,
        core::{error::NodeOperationKind, id::testing_node_id},
    };

    #[test]
    fn every_error_reaches_luau_with_its_script_kind() {
        let node = testing_node_id();
        let cases = [
            (error::Error::ScriptCancelled, "script_cancelled"),
            (error::Error::ScriptBusy("busy".into()), "script_busy"),
            (error::Error::ScriptTimeout { timeout_ms: 5 }, "timeout"),
            (error::Error::NodeDetached(node), "node_detached"),
            (
                error::Error::NodeOperation {
                    kind: NodeOperationKind::Access,
                    operation: "test",
                    node,
                    path: "/root".into(),
                    source: Box::new(error::Error::NodeNotFound(node)),
                },
                "node_not_found",
            ),
            (
                error::Error::Command(CommandError::UnknownCommand { id: "x::y".into() }),
                "unknown_command",
            ),
            (error::Error::Internal("oops".into()), "canopy_error"),
        ];
        for (err, label) in cases {
            let payload = CanopyErrorPayload::from(&err);
            assert_eq!(payload.kind, err.script_kind(), "{err}");
            assert_eq!(payload.kind.as_str(), label, "{err}");
        }
    }

    #[test]
    fn payloads_keep_command_and_owner_context() {
        let node = testing_node_id();
        let detached = CanopyErrorPayload::from(&error::Error::NodeDetached(node));
        assert_eq!(detached.owner, Some(format!("{node:?}")));
        let missing = CanopyErrorPayload::from(&error::Error::Command(CommandError::NoTarget {
            id: "list::select".into(),
            owner: "list".into(),
        }));
        assert_eq!(missing.kind, error::ScriptErrorKind::NoTarget);
        assert_eq!(missing.command.as_deref(), Some("list::select"));
        assert_eq!(missing.owner.as_deref(), Some("list"));
    }
}
