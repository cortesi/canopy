//! Guards and the bridge between a running script scope and the live `Canopy`.

use std::{fmt, mem, result::Result as StdResult};

use ruau::vm::{
    HostType, HostTypeBuilder, MarshaledPair, RuntimeError, Scope, ScopedValue, ScriptErrorField,
    ValueSnapshot,
};

use super::{
    Canopy, ClosureRegistry, Core, LuauFunctionId, LuauState, NodeId, NonNull, Rc, RefCell, Result,
    ScriptCache, commands, error,
};

thread_local! {
    /// Stack of canopy pointers made available to reentrant host calls.
    pub(super) static REENTRANT_CANOPY: RefCell<Vec<NonNull<Canopy>>> = const { RefCell::new(Vec::new()) };
}

impl LuauState {
    /// Construct empty script host state.
    pub(super) fn new() -> Self {
        Self {
            scripts: ScriptCache::new(),
            closures: ClosureRegistry::new(),
            startup_requirements: vec![StartupRequirement {
                name: "setup".to_string(),
                type_text: "() -> ()".to_string(),
            }],
            ..Self::default()
        }
    }

    /// Mark a fully prepared script API as ready.
    pub(super) fn publish(&mut self) {
        self.finalized = true;
    }

    /// Drain deferred `on_start` hooks in registration order.
    pub(super) fn drain_on_start_hooks(&mut self) -> Vec<LuauFunctionId> {
        mem::take(&mut self.on_start_hooks)
    }
}

/// One required global definition for startup script roots.
#[derive(Clone)]
pub(super) struct StartupRequirement {
    /// Global name the root script must define.
    pub(super) name: String,
    /// Luau type text the definition must satisfy.
    pub(super) type_text: String,
}

/// Clears the retained VM active-eval flag when a top-level async eval exits.
pub(super) struct ActiveEvalGuard {
    /// Shared state whose active-eval flag should be cleared.
    pub(super) state: Rc<RefCell<LuauState>>,
}

impl Drop for ActiveEvalGuard {
    fn drop(&mut self) {
        self.state.borrow_mut().active_eval = false;
    }
}

/// Stack guard for the script dispatch anchor inside the borrowed Canopy
/// context.
pub(super) struct ScriptAnchorGuard<'a, 's> {
    /// Scope that owns the Canopy context borrow.
    scope: &'a Scope<'s>,
}

impl<'a, 's> ScriptAnchorGuard<'a, 's> {
    /// Push the active command dispatch anchor for this script call.
    pub(super) fn push(scope: &'a Scope<'s>, node_id: NodeId) -> StdResult<Self, RuntimeError> {
        push_script_anchor(scope, node_id)?;
        Ok(Self { scope })
    }
}

impl Drop for ScriptAnchorGuard<'_, '_> {
    fn drop(&mut self) {
        pop_script_anchor(self.scope);
    }
}

/// Guard exposing the current Canopy to nested script callbacks during routing.
pub(super) struct ReentrantCanopyGuard;

impl ReentrantCanopyGuard {
    /// Push a Canopy pointer for reentrant host calls in the same VM stack.
    pub(super) fn push(canopy: &mut Canopy) -> Self {
        REENTRANT_CANOPY.with(|stack| stack.borrow_mut().push(NonNull::from(canopy)));
        Self
    }
}

impl Drop for ReentrantCanopyGuard {
    fn drop(&mut self) {
        REENTRANT_CANOPY.with(|stack| {
            let _ = stack.borrow_mut().pop();
        });
    }
}

/// Execute a closure with the reentrant Canopy pointer, when one is installed.
pub(super) fn with_reentrant_canopy<R>(
    f: impl FnOnce(&mut Canopy) -> Result<R>,
) -> Option<Result<R>> {
    REENTRANT_CANOPY.with(|stack| {
        let canopy = stack.borrow().last().copied()?;
        // SAFETY: `ReentrantCanopyGuard` is installed only while the
        // script-originated routing call owns the live `&mut Canopy` on
        // this thread, and is popped before that borrow returns to
        // Ruau.
        Some(f(unsafe { &mut *canopy.as_ptr() }))
    })
}

/// Execute a closure with the live Canopy, through the normal context or the
/// reentrant bridge.
fn with_canopy<R>(scope: &Scope<'_>, f: impl FnOnce(&mut Canopy) -> Result<R>) -> Result<R> {
    if let Some(mut canopy) = scope.context_mut::<Canopy>() {
        return f(&mut canopy);
    }
    with_reentrant_canopy(f)
        .unwrap_or_else(|| Err(error::Error::script("no active canopy context")))
}

/// Push the active script anchor.
fn push_script_anchor(scope: &Scope<'_>, node_id: NodeId) -> StdResult<(), RuntimeError> {
    Ok(with_canopy(scope, |canopy| {
        canopy.script_context_stack.push(node_id);
        Ok(())
    })?)
}

/// Pop the active script anchor.
fn pop_script_anchor(scope: &Scope<'_>) {
    with_canopy(scope, |canopy| {
        canopy.script_context_stack.pop();
        Ok(())
    })
    .ok();
}

/// Return true when a live script scope is active on this thread.
///
/// True means the current Rust code was reached from inside a running script,
/// so any script execution started now is nested within that evaluation.
pub fn in_live_scope(canopy: &Canopy) -> bool {
    !canopy.script_context_stack.is_empty()
}

/// Execute a closure with mutable access to the active canopy instance.
pub(super) fn with_current_canopy<R>(
    scope: &Scope<'_>,
    f: impl FnOnce(&mut Canopy, NodeId) -> Result<R>,
) -> Result<R> {
    with_canopy(scope, |canopy| {
        let node_id = canopy
            .script_context_stack
            .last()
            .copied()
            .ok_or_else(|| error::Error::script("no active script context"))?;
        f(canopy, node_id)
    })
}

/// Build the host userdata descriptor for `NodeId` handles.
pub(super) fn node_handle_type() -> HostType {
    HostTypeBuilder::<NodeId>::new("NodeId")
        .class(&commands::declaration::Class::new("NodeId"))
        .eq_by(|left, right| left == right)
        .marshal(node_handle_marshal)
        .tostring(|node_id| commands::node_token(*node_id))
        .build()
}

/// Script-held command value: a command with checked arguments that a binding
/// runs later.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ScriptCommandCall(pub(super) commands::CommandAction);

impl fmt::Display for ScriptCommandCall {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}(", self.0.invocation.id.0)?;
        match &self.0.invocation.args {
            commands::CommandArgs::Positional(values) => {
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        formatter.write_str(", ")?;
                    }
                    formatter.write_str(&arg_value_text(value))?;
                }
            }
            commands::CommandArgs::Named(fields) => {
                formatter.write_str(&arg_value_text(&commands::ArgValue::Map(fields.clone())))?;
            }
        }
        formatter.write_str(")")
    }
}

/// Render one command argument as Luau-like source text.
fn arg_value_text(value: &commands::ArgValue) -> String {
    match value {
        commands::ArgValue::Null => "nil".to_string(),
        commands::ArgValue::Bool(value) => value.to_string(),
        commands::ArgValue::Int(value) => value.to_string(),
        commands::ArgValue::UInt(value) => value.to_string(),
        commands::ArgValue::Float(value) => value.to_string(),
        commands::ArgValue::String(value) => format!("{value:?}"),
        commands::ArgValue::Node(node) => commands::node_token(*node),
        commands::ArgValue::Array(values) => format!(
            "{{ {} }}",
            values
                .iter()
                .map(arg_value_text)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        commands::ArgValue::Map(fields) => format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(key, value)| format!("{key} = {}", arg_value_text(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Build the host userdata descriptor for `CommandCall` values.
pub(super) fn command_call_type() -> HostType {
    HostTypeBuilder::<ScriptCommandCall>::new("CommandCall")
        .class(&commands::declaration::Class::new("CommandCall"))
        .eq_by(|left, right| left == right)
        .marshal(|call| ValueSnapshot::String(call.to_string().into_bytes()))
        .tostring(ToString::to_string)
        .build()
}

/// Convert a script value into a command call, or report the value's type.
pub(super) fn command_call_from_value<'s>(
    scope: &Scope<'s>,
    value: &ScopedValue<'s>,
) -> StdResult<Option<ScriptCommandCall>, RuntimeError> {
    match value {
        ScopedValue::Userdata(userdata) => match userdata.borrow::<ScriptCommandCall>(scope) {
            Ok(call) => Ok(Some(call.clone())),
            Err(_) => Ok(None),
        },
        _ => Ok(None),
    }
}

/// Marshal a node handle to the external automation token record.
pub(super) fn node_handle_marshal(node_id: &NodeId) -> ValueSnapshot {
    ValueSnapshot::Table(
        commands::node_token_fields(*node_id)
            .into_iter()
            .map(|(key, value)| marshaled_string_pair(key, value))
            .collect(),
    )
}

/// Build a string-keyed marshaled table pair.
fn marshaled_string_pair(key: &str, value: impl Into<String>) -> MarshaledPair {
    MarshaledPair {
        key: ValueSnapshot::String(key.as_bytes().to_vec()),
        value: ValueSnapshot::String(value.into().into_bytes()),
    }
}

/// Convert a script node handle back into a node identifier.
pub(super) fn node_id_from_value<'s>(
    scope: &Scope<'s>,
    value: ScopedValue<'s>,
) -> StdResult<NodeId, RuntimeError> {
    match value {
        ScopedValue::Userdata(userdata) => Ok(*userdata.borrow::<NodeId>(scope)?),
        other => Err(RuntimeError::structured(
            format!("expected NodeId, got {}", other.type_name()),
            [
                ScriptErrorField::new("kind", "type_mismatch"),
                ScriptErrorField::new("expected", "NodeId"),
                ScriptErrorField::new("got", other.type_name()),
            ],
        )),
    }
}

/// Validate a script-held node handle against the live arena.
pub fn validate_node_handle(core: &Core, node_id: NodeId) -> Result<()> {
    if core.nodes.contains_key(node_id) {
        Ok(())
    } else {
        Err(error::Error::from(commands::CommandError::InvalidNode {
            id: node_id,
        }))
    }
}
