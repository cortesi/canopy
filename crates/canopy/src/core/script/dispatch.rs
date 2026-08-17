//! Command dispatch and call-into-Luau helpers.

use std::{
    collections::{BTreeMap, HashSet},
    result::Result as StdResult,
    time::Duration,
};

use ruau::{
    session::RootHandle,
    vm::{Function, HostReturn, MultiValue, OwnedValue, RuntimeError, Scope, ScopedValue},
};

use super::{
    ArgValue, Canopy, CommandArgs, CommandInvocation, CommandSpec, NodeId, Result,
    StoredFunctionTarget, arg_value_to_scoped, commands, error, lua_to_canopy,
    retained_runtime_error_to_canopy, runtime_error_to_canopy, scoped_to_arg_value,
    script_error_to_canopy, with_current_canopy,
};

/// Determine whether a map matches a command's named parameters.
fn map_matches_named(spec: &CommandSpec, map: &BTreeMap<String, ArgValue>) -> bool {
    if map.is_empty() {
        return false;
    }
    let allowed = spec
        .params
        .iter()
        .filter(|param| param.kind == commands::CommandParamKind::User)
        .map(|param| commands::normalize_key(param.name))
        .collect::<HashSet<_>>();
    let mut matched = false;
    for key in map.keys() {
        let normalized = commands::normalize_key(key);
        if allowed.contains(&normalized) {
            matched = true;
        } else {
            return false;
        }
    }
    matched
}

/// Build command arguments from converted script values.
fn build_args_from_values(
    spec: &CommandSpec,
    mut values: Vec<ArgValue>,
    allow_map_named: bool,
) -> StdResult<CommandArgs, String> {
    if allow_map_named && values.len() == 1 {
        let arg = values.pop().expect("single argument checked above");
        if let ArgValue::Map(map) = arg {
            if map_matches_named(spec, &map) {
                return Ok(CommandArgs::Named(map));
            }
            return Ok(CommandArgs::Positional(vec![ArgValue::Map(map)]));
        }
        return Ok(CommandArgs::Positional(vec![arg]));
    }
    Ok(CommandArgs::Positional(values))
}

/// Dispatch a command using the active script context.
pub(super) fn dispatch_command(
    scope: &Scope<'_>,
    spec: &'static CommandSpec,
    node_id: NodeId,
    values: Vec<ArgValue>,
    allow_map_named: bool,
) -> Result<ArgValue> {
    with_current_canopy(scope, |canopy, _| {
        let args = build_args_from_values(spec, values, allow_map_named).map_err(|message| {
            error::Error::from(commands::CommandError::conversion(format!(
                "command {}: {message}",
                spec.id.0
            )))
        })?;
        let invocation = CommandInvocation { id: spec.id, args };
        commands::dispatch(&mut canopy.core, node_id, &invocation).map_err(error::Error::from)
    })
}

/// Dispatch a command by id using the current focus-relative context.
pub(super) fn dispatch_command_by_name(
    scope: &Scope<'_>,
    name: &str,
    node_id: Option<NodeId>,
    values: Vec<ArgValue>,
) -> Result<ArgValue> {
    let allow_map_named = values.len() == 1;
    let (anchor, spec) = with_current_canopy(scope, |canopy, anchor| {
        let spec = canopy.core.commands.get(name).ok_or_else(|| {
            error::Error::from(commands::CommandError::UnknownCommand {
                id: name.to_string(),
            })
        })?;
        Ok((anchor, spec))
    })?;
    dispatch_command(
        scope,
        spec,
        node_id.unwrap_or(anchor),
        values,
        allow_map_named,
    )
}

/// Convert the remaining host-call values into command arguments.
pub(super) fn values_to_args<'s>(
    scope: &Scope<'s>,
    values: Vec<ScopedValue<'s>>,
) -> StdResult<Vec<ArgValue>, RuntimeError> {
    values
        .into_iter()
        .map(|value| scoped_to_arg_value(scope, value).map_err(RuntimeError::runtime))
        .collect()
}

/// Run a query against the live Canopy and return its value to the script.
pub(super) fn host_value<'s>(
    scope: &Scope<'s>,
    f: impl FnOnce(&mut Canopy, NodeId) -> Result<ArgValue>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let value = with_current_canopy(scope, f)?;
    ret_arg(scope, &value)
}

/// Build an empty host-call return.
pub(super) fn ret_none<'s>() -> MultiValue<'s> {
    MultiValue::new()
}

/// Build a single-value host-call return.
pub(super) fn ret_one(value: ScopedValue<'_>) -> MultiValue<'_> {
    MultiValue::from_values(vec![value])
}

/// Build a single-value host-call return from a command argument value.
pub(super) fn ret_arg<'s>(
    scope: &Scope<'s>,
    value: &ArgValue,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    Ok(ret_one(arg_value_to_scoped(scope, value)?))
}

/// Build a single async host return value.
pub(super) fn host_return(value: impl Into<OwnedValue>) -> HostReturn {
    HostReturn {
        values: vec![value.into()],
    }
}

/// Return true for Luau-truthy owned values.
pub(super) fn owned_truthy(value: Option<&OwnedValue>) -> bool {
    !matches!(
        value,
        None | Some(OwnedValue::Nil | OwnedValue::Boolean(false))
    )
}

/// Display an owned async host value in an error message.
pub(super) fn owned_value_to_display(value: &OwnedValue) -> String {
    match value {
        OwnedValue::Nil => "nil".to_string(),
        OwnedValue::Boolean(value) => value.to_string(),
        OwnedValue::Integer(value) => value.to_string(),
        OwnedValue::Number(value) => value.to_string(),
        OwnedValue::Bytes(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        other => format!("{other:?}"),
    }
}

/// A retained script root or stored callback resolvable inside a live VM scope.
pub(super) enum CallTarget {
    /// A compiled script root owned by the retained runtime.
    Root(RootHandle),
    /// A callback pending promotion or owned by the retained runtime.
    Stored(StoredFunctionTarget),
}

impl CallTarget {
    /// Resolve the callable inside the given scope.
    pub(super) fn resolve<'s>(
        &self,
        scope: &Scope<'s>,
        label: &str,
        timeout: Option<Duration>,
    ) -> Result<Function<'s>> {
        match self {
            Self::Root(root) => root
                .resolve(scope)
                .map_err(|error| retained_runtime_error_to_canopy(&error, label, timeout)),
            Self::Stored(StoredFunctionTarget::Pending(stashed)) => {
                scope.fetch_function(stashed).map_err(lua_to_canopy)
            }
            Self::Stored(StoredFunctionTarget::Retained(handle)) => handle
                .resolve(scope)
                .map_err(|error| retained_runtime_error_to_canopy(&error, label, timeout)),
        }
    }
}

/// Run a resolved callable inside a live scope and convert its result.
pub(super) fn call_in_scope<'s>(
    scope: &Scope<'s>,
    function: Function<'s>,
    label: &str,
    timeout: Option<Duration>,
) -> Result<ArgValue> {
    match scope.call_protected::<_, MultiValue>(function, ()) {
        Ok(Ok(values)) => {
            let value = values
                .into_vec()
                .into_iter()
                .next()
                .unwrap_or(ScopedValue::Nil);
            scoped_to_arg_value(scope, value)
                .map_err(|message| error::Error::Script(format!("{label}: {message}")))
        }
        Ok(Err(script_error)) => Err(script_error_to_canopy(scope, &script_error, label, timeout)),
        Err(runtime_error) => Err(runtime_error_to_canopy(&runtime_error, label, timeout)),
    }
}
