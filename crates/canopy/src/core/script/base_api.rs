//! Base `canopy` scripting API declarations and native registration.

use std::{
    collections::{BTreeSet, HashSet},
    future::poll_fn,
    iter,
    result::Result as StdResult,
    sync::{Arc, Mutex},
    time::Duration,
};

use ruau::{
    declaration::{FunctionSignature, Type},
    module::{self, Binding},
    vm::{
        AsyncHostContext, AsyncHostFunction, ContextMut, FromLua, FromLuaMulti, Function,
        HostArgCursor, HostReturn, MultiValue, NativeModule, RuntimeError, Scope, ScopedValue,
        StashedClosure, Table, async_host_fn,
    },
};

use super::{
    ArgValue, Canopy, ChangeOutcome, CommandSet, Context, CoreContext, CoreViewContext, FocusScope,
    NodeId, PathFilter, Pin, Point, RectI32, ReentrantCanopyGuard, Result, ScriptCommandCall,
    ViewContext, available_bindings_to_arg, base_api, binding_info_to_arg, command_call_from_value,
    command_call_type, command_info_to_arg, commands, defs, dispatch_command,
    dispatch_command_by_name, dispatch_explicit, error, fixtures_to_arg, host_return, host_value,
    inputmap, key, luau_global_owner_name, mouse, node_handle_type, node_id_from_value,
    node_info_to_arg, node_list_to_arg, owned_truthy, ret_arg, ret_none, ret_one,
    route_trace_to_arg, screen_cells_to_arg, screen_text, screen_text_for_rect, screen_to_arg,
    script_callback_label, script_journal_to_arg, snapshot_to_arg, tree_node_to_arg,
    validate_node_handle, values_to_args, with_current_canopy,
};
use crate::{FocusDirection, geom::PointI32};

/// The native implementation behind one base API function.
enum Handler {
    /// A borrowed synchronous host function.
    Sync(HostHandler),
    /// A factory for an asynchronous host function.
    Async(fn() -> Box<dyn AsyncHostFunction>),
}

/// One native function exposed on the global `canopy` library table.
struct BaseFunction {
    /// Function name inside the `canopy` table.
    name: &'static str,
    /// Luau doc comment rendered above the declaration, when it adds semantics.
    docs: Option<&'static str>,
    /// Luau function type signature.
    signature: fn() -> FunctionSignature,
    /// Native host implementation.
    handler: Handler,
}

/// Native functions exposed on the `canopy` library table.
const CANOPY_FUNCTIONS: &[BaseFunction] = &[
    BaseFunction {
        name: "root",
        docs: None,
        signature: || FunctionSignature::new().ret(Type::named("NodeId")),
        handler: Handler::Sync(host_root),
    },
    BaseFunction {
        name: "focused",
        docs: Some("Return the currently focused node, or nil when nothing is focused."),
        signature: || FunctionSignature::new().ret(Type::named("NodeId").optional()),
        handler: Handler::Sync(host_focused),
    },
    BaseFunction {
        name: "node_info",
        docs: None,
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::named("NodeInfo"))
        },
        handler: Handler::Sync(host_node_info),
    },
    BaseFunction {
        name: "find_identity",
        docs: Some("Find a semantic key within an explicit scope, defaulting to root."),
        signature: || {
            FunctionSignature::new()
                .param(("key", Type::String))
                .param(("scope", Type::named("NodeId").optional()))
                .ret(Type::named("NodeId").optional())
        },
        handler: Handler::Sync(host_find_identity),
    },
    BaseFunction {
        name: "find_node",
        docs: Some("Find the first node whose path matches a canopy path pattern."),
        signature: || {
            FunctionSignature::new()
                .param(("pattern", Type::String))
                .ret(Type::named("NodeId").optional())
        },
        handler: Handler::Sync(host_find_node),
    },
    BaseFunction {
        name: "find_nodes",
        docs: Some("Find every node whose path matches a canopy path pattern."),
        signature: || {
            FunctionSignature::new()
                .param(("pattern", Type::String))
                .ret(Type::named("NodeId").array())
        },
        handler: Handler::Sync(host_find_nodes),
    },
    BaseFunction {
        name: "parent",
        docs: Some("Return the parent of a node, or nil at the root."),
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::named("NodeId").optional())
        },
        handler: Handler::Sync(host_parent),
    },
    BaseFunction {
        name: "children",
        docs: None,
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::named("NodeId").array())
        },
        handler: Handler::Sync(host_children),
    },
    BaseFunction {
        name: "tree",
        docs: Some("Return a recursive snapshot of the entire tree rooted at `canopy.root()`."),
        signature: || FunctionSignature::new().ret(Type::named("TreeNode")),
        handler: Handler::Sync(host_tree),
    },
    BaseFunction {
        name: "node_at",
        docs: Some(
            "Hit-test a screen coordinate and return the deepest visible node at that point.",
        ),
        signature: || {
            FunctionSignature::new()
                .param(("x", Type::Number))
                .param(("y", Type::Number))
                .ret(Type::named("NodeId").optional())
        },
        handler: Handler::Sync(host_node_at),
    },
    BaseFunction {
        name: "set_focus",
        docs: Some("Attempt to move focus directly to a node."),
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::Boolean)
        },
        handler: Handler::Sync(host_set_focus),
    },
    BaseFunction {
        name: "focus_next",
        docs: Some("Move focus to the next focusable node in global focus order."),
        signature: FunctionSignature::new,
        handler: Handler::Sync(host_focus_next),
    },
    BaseFunction {
        name: "focus_prev",
        docs: Some("Move focus to the previous focusable node in global focus order."),
        signature: FunctionSignature::new,
        handler: Handler::Sync(host_focus_prev),
    },
    BaseFunction {
        name: "focus_dir",
        docs: None,
        signature: || {
            FunctionSignature::new().param(("dir", Type::literals(["Up", "Down", "Left", "Right"])))
        },
        handler: Handler::Sync(host_focus_dir),
    },
    BaseFunction {
        name: "send_key",
        docs: Some(
            "Inject a key event using a canopy key spec string such as `ctrl-c` or `PageDown`.",
        ),
        signature: || FunctionSignature::new().param(("key", Type::String)),
        handler: Handler::Sync(host_send_key),
    },
    BaseFunction {
        name: "send_click",
        docs: None,
        signature: || {
            FunctionSignature::new()
                .param(("x", Type::Number))
                .param(("y", Type::Number))
        },
        handler: Handler::Sync(host_send_click),
    },
    BaseFunction {
        name: "send_scroll",
        docs: None,
        signature: || {
            FunctionSignature::new()
                .param(("direction", Type::literals(["Up", "Down"])))
                .param(("x", Type::Number))
                .param(("y", Type::Number))
        },
        handler: Handler::Sync(host_send_scroll),
    },
    BaseFunction {
        name: "cmd",
        docs: Some("Dispatch a command by fully-qualified command id such as `root::quit`."),
        signature: || {
            FunctionSignature::new()
                .param(("name", Type::String))
                .varargs(Type::Any)
                .ret(Type::Any)
        },
        handler: Handler::Sync(host_cmd),
    },
    BaseFunction {
        name: "cmd_on",
        docs: Some("Search from a node using the legacy single-map argument inference."),
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .param(("name", Type::String))
                .varargs(Type::Any)
                .ret(Type::Any)
        },
        handler: Handler::Sync(host_cmd_on),
    },
    BaseFunction {
        name: "call_exact",
        docs: Some(
            "Call only the supplied node with positional arguments. Free commands are rejected.",
        ),
        signature: || {
            FunctionSignature::new()
                .param(("node", Type::named("NodeId")))
                .param(("id", Type::String))
                .varargs(Type::Any)
                .ret(Type::Any)
        },
        handler: Handler::Sync(host_call_exact),
    },
    BaseFunction {
        name: "call_from",
        docs: Some("Search from the supplied node and call with positional arguments."),
        signature: || {
            FunctionSignature::new()
                .param(("node", Type::named("NodeId")))
                .param(("id", Type::String))
                .varargs(Type::Any)
                .ret(Type::Any)
        },
        handler: Handler::Sync(host_call_from),
    },
    BaseFunction {
        name: "call_focus",
        docs: Some("Resolve the focus at invocation time and call with positional arguments."),
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::String))
                .varargs(Type::Any)
                .ret(Type::Any)
        },
        handler: Handler::Sync(host_call_focus),
    },
    BaseFunction {
        name: "call_named",
        docs: Some("Call with named fields. An omitted target searches from the script anchor."),
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::String))
                .param(("fields", Type::Any))
                .param(("target", Type::named("CommandTarget").optional()))
                .ret(Type::Any)
        },
        handler: Handler::Sync(host_call_named),
    },
    BaseFunction {
        name: "resolve",
        docs: Some("Return the command dispatch target for an owner, or nil if none is mounted."),
        signature: || {
            FunctionSignature::new()
                .param(("owner", Type::String))
                .ret(Type::named("NodeId").optional())
        },
        handler: Handler::Sync(host_resolve),
    },
    BaseFunction {
        name: "bindings",
        docs: Some("Return the active binding table across all modes."),
        signature: || FunctionSignature::new().ret(Type::named("BindingInfo").array()),
        handler: Handler::Sync(host_bindings),
    },
    BaseFunction {
        name: "commands",
        docs: Some("Return structured metadata for all registered commands."),
        signature: || {
            FunctionSignature::new()
                .param(("target", Type::named("CommandTarget").optional()))
                .ret(Type::named("CommandInfo").array())
        },
        handler: Handler::Sync(host_commands),
    },
    BaseFunction {
        name: "input_mode",
        docs: Some("Return the active input mode. The default mode is the empty string."),
        signature: || FunctionSignature::new().ret(Type::String),
        handler: Handler::Sync(host_input_mode),
    },
    BaseFunction {
        name: "set_mode",
        docs: Some(
            "Switch the active input mode. Passing the empty string returns to default mode.",
        ),
        signature: || FunctionSignature::new().param(("mode", Type::String)),
        handler: Handler::Sync(host_set_mode),
    },
    BaseFunction {
        name: "push_mode",
        docs: None,
        signature: || FunctionSignature::new().param(("mode", Type::String)),
        handler: Handler::Sync(host_push_mode),
    },
    BaseFunction {
        name: "pop_mode",
        docs: Some("Pop the top input mode and return the active mode after the pop."),
        signature: || FunctionSignature::new().ret(Type::String),
        handler: Handler::Sync(host_pop_mode),
    },
    BaseFunction {
        name: "snapshot",
        docs: Some("Return the last completed frame without running hooks or preparing a frame."),
        signature: || FunctionSignature::new().ret(Type::named("FrameSnapshot").optional()),
        handler: Handler::Sync(host_snapshot),
    },
    BaseFunction {
        name: "flush",
        docs: Some("Publish pending changes after native mutation callbacks have returned."),
        signature: FunctionSignature::new,
        handler: Handler::Sync(host_flush),
    },
    BaseFunction {
        name: "screen",
        docs: Some("Return the rendered screen as rows of cell strings."),
        signature: || FunctionSignature::new().ret(Type::String.array().array()),
        handler: Handler::Sync(host_screen),
    },
    BaseFunction {
        name: "screen_cells",
        docs: Some("Return the rendered screen as rows of styled cell records."),
        signature: || FunctionSignature::new().ret(Type::named("ScreenCell").array().array()),
        handler: Handler::Sync(host_screen_cells),
    },
    BaseFunction {
        name: "screen_text",
        docs: Some("Return the rendered screen as newline-joined plain text."),
        signature: || FunctionSignature::new().ret(Type::String),
        handler: Handler::Sync(host_screen_text),
    },
    BaseFunction {
        name: "screen_region",
        docs: Some("Return rendered plain text inside a screen rectangle."),
        signature: || {
            FunctionSignature::new()
                .param(("x", Type::Number))
                .param(("y", Type::Number))
                .param(("w", Type::Number))
                .param(("h", Type::Number))
                .ret(Type::String)
        },
        handler: Handler::Sync(host_screen_region),
    },
    BaseFunction {
        name: "node_region",
        docs: Some("Return rendered plain text inside a node's content rectangle."),
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::String)
        },
        handler: Handler::Sync(host_node_region),
    },
    BaseFunction {
        name: "route_trace",
        docs: Some("Return the most recent input route trace."),
        signature: || FunctionSignature::new().ret(Type::named("RouteTraceEntry").array()),
        handler: Handler::Sync(host_route_trace),
    },
    BaseFunction {
        name: "diagnostic_dump",
        docs: Some("Return a diagnostic dump for a node, or the current script anchor."),
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId").optional()))
                .ret(Type::String)
        },
        handler: Handler::Sync(host_diagnostic_dump),
    },
    BaseFunction {
        name: "available_bindings",
        docs: Some("Return effective key bindings for a node or the current focus."),
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId").optional()))
                .ret(Type::named("BindingSnapshot"))
        },
        handler: Handler::Sync(host_available_bindings),
    },
    BaseFunction {
        name: "script_journal",
        docs: Some("Return recorded script evaluations for replay and diagnostics."),
        signature: || FunctionSignature::new().ret(Type::named("ScriptJournalEntry").array()),
        handler: Handler::Sync(host_script_journal),
    },
    BaseFunction {
        name: "api",
        docs: Some("Return the generated Luau API definition for this app."),
        signature: || FunctionSignature::new().ret(Type::String),
        handler: Handler::Sync(host_api),
    },
    BaseFunction {
        name: "bind",
        docs: Some(
            "Bind a key spec to a CommandCall or a function, with required discovery metadata.",
        ),
        signature: || {
            FunctionSignature::new()
                .param(("key", Type::String))
                .param(("options", Type::named("BindOptions")))
                .param((
                    "action",
                    Type::union([
                        Type::named("CommandCall"),
                        Type::func(FunctionSignature::new()),
                    ]),
                ))
                .ret(Type::Number)
        },
        handler: Handler::Sync(host_bind),
    },
    BaseFunction {
        name: "keymap",
        docs: Some(
            "Install a keymap: shared options in the named fields, and entries in the array part. \
             Every entry is validated before any binding installs. Returns the binding IDs in entry order.",
        ),
        signature: || {
            FunctionSignature::new()
                .param(("keymap", Type::named("Keymap")))
                .ret(Type::Number.array())
        },
        handler: Handler::Sync(host_keymap),
    },
    BaseFunction {
        name: "bind_mouse",
        docs: Some(
            "Bind a mouse spec to a CommandCall or a function, with required discovery metadata.",
        ),
        signature: || {
            FunctionSignature::new()
                .param(("mouse", Type::named("MouseSpec")))
                .param(("options", Type::named("BindOptions")))
                .param((
                    "action",
                    Type::union([
                        Type::named("CommandCall"),
                        Type::func(FunctionSignature::new()),
                    ]),
                ))
                .ret(Type::Number)
        },
        handler: Handler::Sync(host_bind_mouse),
    },
    BaseFunction {
        name: "unbind",
        docs: None,
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::Number))
                .ret(Type::Boolean)
        },
        handler: Handler::Sync(host_unbind),
    },
    BaseFunction {
        name: "unbind_key",
        docs: Some("Remove key bindings matching the key spec and optional mode/path filter."),
        signature: || {
            FunctionSignature::new()
                .param(("key", Type::String))
                .param(("options", Type::named("UnbindSelector").optional()))
        },
        handler: Handler::Sync(host_unbind_key),
    },
    BaseFunction {
        name: "clear_bindings",
        docs: Some("Remove every binding from every mode."),
        signature: FunctionSignature::new,
        handler: Handler::Sync(host_clear_bindings),
    },
    BaseFunction {
        name: "on_start",
        docs: Some("Register a callback that runs after the first live render."),
        signature: || {
            FunctionSignature::new().param(("handler", Type::func(FunctionSignature::new())))
        },
        handler: Handler::Sync(host_on_start),
    },
    BaseFunction {
        name: "log",
        docs: Some("Append a log line to the evaluation result."),
        signature: || FunctionSignature::new().param(("message", Type::Any)),
        handler: Handler::Sync(host_log),
    },
    BaseFunction {
        name: "assert",
        docs: None,
        signature: || {
            FunctionSignature::new()
                .param(("condition", Type::Boolean))
                .param(("message", Type::String.optional()))
        },
        handler: Handler::Sync(host_assert),
    },
    BaseFunction {
        name: "wait_for",
        docs: Some("Wait until a predicate returns a truthy value."),
        signature: || {
            FunctionSignature::new()
                .param((
                    "predicate",
                    Type::func(FunctionSignature::new().ret(Type::Boolean)),
                ))
                .param(("timeout_ms", Type::Number.optional()))
                .ret(Type::Boolean)
        },
        handler: Handler::Async(|| async_host_fn(wait_for_predicate)),
    },
    BaseFunction {
        name: "wait_for_node",
        docs: Some("Wait until a command owner resolves to a mounted node."),
        signature: || {
            FunctionSignature::new()
                .param(("owner", Type::String))
                .param(("timeout_ms", Type::Number.optional()))
                .ret(Type::Boolean)
        },
        handler: Handler::Async(|| async_host_fn(wait_for_node)),
    },
    BaseFunction {
        name: "wait_for_screen_text",
        docs: Some("Wait until the rendered screen contains text."),
        signature: || {
            FunctionSignature::new()
                .param(("text", Type::String))
                .param(("timeout_ms", Type::Number.optional()))
                .ret(Type::Boolean)
        },
        handler: Handler::Async(|| async_host_fn(wait_for_screen_text)),
    },
];

/// Register the base `canopy` table and global helpers.
pub(super) fn register(builder: &mut module::Builder) {
    for function in CANOPY_FUNCTIONS {
        let mut binding = Binding::library("canopy", Type::func((function.signature)()));
        if let Some(docs) = function.docs {
            binding = binding.doc(docs);
        }
        match function.handler {
            Handler::Sync(handler) => {
                builder.borrowed_function(function.name, binding, handler);
            }
            Handler::Async(factory) => {
                builder.async_function(function.name, binding, Arc::from(factory()));
            }
        }
    }
    builder.borrowed_function(
        "fixtures",
        Binding::global(Type::func(
            FunctionSignature::new().ret(Type::named("FixtureInfo").array()),
        )),
        host_fixtures,
    );
}

/// Selector for application binding removal.
#[derive(Debug, Clone, Default)]
struct ScriptUnbindSelector {
    /// Optional named mode to select.
    mode: Option<String>,
    /// Optional exact path filter to select.
    path: Option<String>,
}

/// Read one optional string field from a script options table.
fn optional_string_field<'s>(
    scope: &Scope<'s>,
    options: &Table<'s>,
    name: &str,
) -> StdResult<Option<String>, RuntimeError> {
    match options.get::<_, ScopedValue>(scope, name)? {
        ScopedValue::Nil => Ok(None),
        value => String::from_lua(value, scope).map(Some),
    }
}

/// Parse `BindOptions` from a required script table.
fn parse_bind_options<'s>(
    scope: &Scope<'s>,
    options: Option<Table<'s>>,
) -> StdResult<inputmap::BindingOptions, RuntimeError> {
    let Some(options) = options else {
        return Err(RuntimeError::runtime("binding options table is required"));
    };
    let field = |name: &str| optional_string_field(scope, &options, name);
    // `InputMap::replace_application_action` rejects a blank description on the
    // next step.
    let description = field("description")?
        .ok_or_else(|| RuntimeError::runtime("binding description is required"))?;
    let mode = field("mode")?.filter(|mode| !mode.is_empty());
    let tier = field("tier")?;
    let binding_scope = match tier.as_deref() {
        None => mode.map_or(
            inputmap::BindingScope::Default,
            inputmap::BindingScope::Mode,
        ),
        Some("global") if mode.is_none() => inputmap::BindingScope::Global,
        Some("global") => {
            return Err(RuntimeError::runtime(
                "binding tier 'global' cannot be combined with a named mode",
            ));
        }
        Some(other) => {
            return Err(RuntimeError::runtime(format!(
                "unknown binding tier: {other}"
            )));
        }
    };
    let phase = match field("phase")?.as_deref() {
        None => inputmap::BindingPhase::default(),
        Some(label) => inputmap::BindingPhase::parse(label)
            .ok_or_else(|| RuntimeError::runtime(format!("unknown binding phase: {label}")))?,
    };
    Ok(inputmap::BindingOptions {
        scope: binding_scope,
        path: field("path")?
            .filter(|path| !path.is_empty())
            .map(|path| path.parse())
            .transpose()?,
        description,
        source: Some(script_callback_label(scope)),
        phase,
    })
}

/// Parse an application-only unbind selector.
fn parse_unbind_selector<'s>(
    scope: &Scope<'s>,
    options: Option<Table<'s>>,
) -> StdResult<ScriptUnbindSelector, RuntimeError> {
    let Some(options) = options else {
        return Ok(ScriptUnbindSelector::default());
    };
    let field = |name: &str| optional_string_field(scope, &options, name);
    Ok(ScriptUnbindSelector {
        mode: field("mode")?.filter(|mode| !mode.is_empty()),
        path: field("path")?.filter(|path| !path.is_empty()),
    })
}

/// Read a required node-id argument, validating the handle against the live
/// tree.
pub(super) fn read_node_id<'s>(
    scope: &Scope<'s>,
    args: &mut HostArgCursor<'_, 's>,
    name: &str,
) -> StdResult<NodeId, RuntimeError> {
    let value = args
        .raw()
        .ok_or_else(|| RuntimeError::runtime(format!("argument `{name}` is required")))?;
    let node_id = node_id_from_value(scope, value)?;
    with_current_canopy(scope, |canopy, _| {
        validate_node_handle(&canopy.core, node_id).map(|()| node_id)
    })
    .map_err(RuntimeError::from)
}

/// Read an optional node-id argument, validating a present handle against the
/// live tree.
fn read_opt_node_id<'s>(
    scope: &Scope<'s>,
    args: &mut HostArgCursor<'_, 's>,
) -> StdResult<Option<NodeId>, RuntimeError> {
    match args.raw() {
        None | Some(ScopedValue::Nil) => Ok(None),
        Some(value) => {
            let node_id = node_id_from_value(scope, value)?;
            with_current_canopy(scope, |canopy, _| {
                validate_node_handle(&canopy.core, node_id).map(|()| Some(node_id))
            })
            .map_err(RuntimeError::from)
        }
    }
}

/// Parsed arguments for `canopy.wait_for`.
struct WaitForArgs {
    /// Predicate closure to poll.
    predicate: StashedClosure,
    /// Optional timeout in milliseconds.
    timeout_ms: Option<u64>,
}

impl<'s> FromLuaMulti<'s> for WaitForArgs {
    fn from_lua_multi(values: MultiValue<'s>, scope: &Scope<'s>) -> StdResult<Self, RuntimeError> {
        let mut args = HostArgCursor::new(scope, values);
        let predicate = scope.stash_function(args.required::<Function<'_>>("predicate")?)?;
        let timeout_ms = args.optional::<u64>("timeout_ms")?;
        Ok(Self {
            predicate,
            timeout_ms,
        })
    }
}

/// Parsed arguments for `canopy.wait_for_node`.
struct WaitForNodeArgs {
    /// Command owner that should become available.
    owner: String,
    /// Optional timeout in milliseconds.
    timeout_ms: Option<u64>,
}

/// Read a required string argument and an optional timeout.
fn parse_wait_string<'s>(
    values: MultiValue<'s>,
    scope: &Scope<'s>,
    field: &str,
) -> StdResult<(String, Option<u64>), RuntimeError> {
    let mut args = HostArgCursor::new(scope, values);
    let value = args.required::<String>(field)?;
    let timeout_ms = args.optional::<u64>("timeout_ms")?;
    Ok((value, timeout_ms))
}

impl<'s> FromLuaMulti<'s> for WaitForNodeArgs {
    fn from_lua_multi(values: MultiValue<'s>, scope: &Scope<'s>) -> StdResult<Self, RuntimeError> {
        let (owner, timeout_ms) = parse_wait_string(values, scope, "owner")?;
        Ok(Self { owner, timeout_ms })
    }
}

/// Parsed arguments for `canopy.wait_for_screen_text`.
struct WaitForScreenTextArgs {
    /// Text fragment expected on screen.
    text: String,
    /// Optional timeout in milliseconds.
    timeout_ms: Option<u64>,
}

impl<'s> FromLuaMulti<'s> for WaitForScreenTextArgs {
    fn from_lua_multi(values: MultiValue<'s>, scope: &Scope<'s>) -> StdResult<Self, RuntimeError> {
        let (text, timeout_ms) = parse_wait_string(values, scope, "text")?;
        Ok(Self { text, timeout_ms })
    }
}

/// Build a timeout error for an async wait helper.
fn wait_timeout(timeout_ms: u64) -> RuntimeError {
    RuntimeError::from(error::Error::ScriptTimeout { timeout_ms })
}

/// Borrow the active Canopy context from a live scope.
fn canopy_context<'a, 's>(scope: &'a Scope<'s>) -> StdResult<ContextMut<'a, Canopy>, RuntimeError> {
    scope
        .context_mut::<Canopy>()
        .ok_or_else(|| RuntimeError::runtime("no active canopy context"))
}

/// Poll app state and a predicate until it succeeds or times out.
async fn wait_until<F>(
    ctx: AsyncHostContext,
    timeout_ms: Option<u64>,
    mut ready: F,
) -> StdResult<HostReturn, RuntimeError>
where
    F: FnMut(
        AsyncHostContext,
    ) -> Pin<Box<dyn Future<Output = StdResult<bool, RuntimeError>> + Send>>,
{
    let observed = Arc::new(Mutex::new(None));
    let delivered = Arc::clone(&observed);
    ctx.scope(move |scope| {
        let canopy = canopy_context(scope)?;
        *delivered
            .lock()
            .map_err(|_| RuntimeError::runtime("wait state lock poisoned"))? =
            Some((canopy.publication_watch(), canopy.now()));
        Ok(())
    })
    .await?;
    let (watch, started) = observed
        .lock()
        .map_err(|_| RuntimeError::runtime("wait state lock poisoned"))?
        .take()
        .ok_or_else(|| RuntimeError::runtime("wait state was not delivered"))?;
    let deadline = timeout_ms
        .map(|timeout_ms| {
            started
                .checked_add(Duration::from_millis(timeout_ms))
                .ok_or_else(|| RuntimeError::runtime("wait timeout exceeds the clock range"))
        })
        .transpose()?;
    loop {
        // Capture before checking the predicate; poll_changed rechecks before
        // parking.
        let generation = watch.generation();
        if ready(ctx.clone()).await? {
            return Ok(host_return(true));
        }
        if let (Some(deadline), Some(timeout_ms)) = (deadline, timeout_ms) {
            let expired = ctx
                .scope(move |scope| {
                    let canopy = scope
                        .context_mut::<Canopy>()
                        .ok_or_else(|| RuntimeError::runtime("no active canopy context"))?;
                    Ok(canopy.now() >= deadline)
                })
                .await?;
            if expired {
                return Err(wait_timeout(timeout_ms));
            }
        }
        poll_fn(|cx| watch.poll_changed(generation, deadline, cx)).await;
    }
}

/// Async implementation of `canopy.wait_for`.
async fn wait_for_predicate(
    ctx: AsyncHostContext,
    args: WaitForArgs,
) -> StdResult<HostReturn, RuntimeError> {
    wait_until(ctx, args.timeout_ms, move |ctx| {
        let predicate = args.predicate.clone();
        Box::pin(async move {
            match ctx.call_protected(&predicate, ()).await? {
                Ok(values) => Ok(owned_truthy(values.values.first())),
                Err(error) => Err(RuntimeError::runtime(format!(
                    "wait predicate failed: {}",
                    error.value().display_lua()
                ))),
            }
        })
    })
    .await
}

/// Async implementation of `canopy.wait_for_node`.
async fn wait_for_node(
    ctx: AsyncHostContext,
    args: WaitForNodeArgs,
) -> StdResult<HostReturn, RuntimeError> {
    wait_until(ctx, args.timeout_ms, move |ctx| {
        let owner = args.owner.clone();
        Box::pin(async move {
            ctx.scope(move |scope| {
                let canopy = scope
                    .context_mut::<Canopy>()
                    .ok_or_else(|| RuntimeError::runtime("no active canopy context"))?;
                let registered = canopy.core.commands.iter().any(|(_, spec)| {
                    matches!(spec.dispatch,
                        commands::CommandDispatchKind::Node { owner: entry_owner }
                            if entry_owner == owner)
                });
                let start = canopy.core.focus.unwrap_or(canopy.core.root);
                Ok(registered
                    && commands::CommandResolver::for_target(
                        &canopy.core,
                        commands::CommandTarget::From(start),
                    )
                    .resolve_owner(&owner)
                    .is_some())
            })
            .await
        })
    })
    .await
}

/// Async implementation of `canopy.wait_for_screen_text`.
async fn wait_for_screen_text(
    ctx: AsyncHostContext,
    args: WaitForScreenTextArgs,
) -> StdResult<HostReturn, RuntimeError> {
    wait_until(ctx, args.timeout_ms, move |ctx| {
        let text = args.text.clone();
        Box::pin(async move {
            ctx.scope(move |scope| {
                let canopy = scope
                    .context_mut::<Canopy>()
                    .ok_or_else(|| RuntimeError::runtime("no active canopy context"))?;
                Ok(canopy
                    .buf()
                    .is_some_and(|buffer| buffer.screen_text().contains(&text)))
            })
            .await
        })
    })
    .await
}

/// A plain-function canopy host handler.
type HostHandler =
    for<'s> fn(&Scope<'s>, MultiValue<'s>) -> StdResult<MultiValue<'s>, RuntimeError>;

/// Run an owner's default-bindings script inside the current live scope.
fn run_default_bindings_in_scope(scope: &Scope<'_>, owner: &str) -> Result<()> {
    let run = with_current_canopy(scope, |canopy, _| {
        canopy.prepare_registered_default_bindings(owner)
    })?;
    let result = run.host.execute_in_scope(scope, run.root_id, run.script_id);
    with_current_canopy(scope, |canopy, _| {
        canopy.record_registered_default_bindings(owner, &run, &result);
        Ok(())
    })?;
    result
}

/// Store a binding closure and install the binding, releasing the closure if
/// installation fails.
fn install_function_binding<'s>(
    scope: &Scope<'s>,
    function: Function<'s>,
    input: inputmap::InputSpec,
    options: &inputmap::BindingOptions,
) -> StdResult<i64, RuntimeError> {
    let stashed = scope.stash_function(function)?;
    with_current_canopy(scope, |canopy, _| {
        let function_id = canopy.script_host.store_function(stashed)?;
        let result = canopy.core.input_map.replace_application_action(
            input,
            options.clone(),
            inputmap::BindingTarget::Script(function_id),
        );
        match result {
            Ok((binding_id, removed)) => {
                canopy.release_removed_bindings(removed);
                Ok(binding_id.as_u64() as i64)
            }
            Err(err) => {
                canopy.script_host.release_function(function_id);
                Err(err)
            }
        }
    })
    .map_err(RuntimeError::from)
}

/// Install a command binding, replacing any binding with the same selector.
fn install_command_binding(
    scope: &Scope<'_>,
    action: commands::CommandAction,
    input: inputmap::InputSpec,
    options: &inputmap::BindingOptions,
) -> StdResult<i64, RuntimeError> {
    with_current_canopy(scope, |canopy, _| {
        let (binding_id, removed) = canopy.core.input_map.replace_application_action(
            input,
            options.clone(),
            inputmap::BindingTarget::Command(action),
        )?;
        canopy.release_removed_bindings(removed);
        Ok(binding_id.as_u64() as i64)
    })
    .map_err(RuntimeError::from)
}

/// Dispatch a command and convert its result for the script.
fn run_script_command<'s>(
    scope: &Scope<'s>,
    name: &str,
    node: Option<NodeId>,
    values: Vec<ArgValue>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let result = dispatch_command_by_name(scope, name, node, values)?;
    ret_arg(scope, &result)
}

/// `canopy.cmd`: dispatch a command by fully-qualified id.
fn host_cmd<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let name = args.required::<String>("name")?;
    let values = values_to_args(scope, iter::from_fn(|| args.raw()).collect())?;
    run_script_command(scope, &name, None, values)
}

/// `canopy.cmd_on`: dispatch a command against a specific node.
fn host_cmd_on<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let node_id = read_node_id(scope, &mut args, "id")?;
    let name = args.required::<String>("name")?;
    let values = values_to_args(scope, iter::from_fn(|| args.raw()).collect())?;
    run_script_command(scope, &name, Some(node_id), values)
}

/// Parse a target selector and validate explicit node handles.
fn parse_command_target<'s>(
    scope: &Scope<'s>,
    value: Option<ScopedValue<'s>>,
) -> StdResult<commands::CommandTarget, RuntimeError> {
    let anchor = with_current_canopy(scope, |_, anchor| Ok(anchor))?;
    let Some(value) = value.filter(|value| !matches!(value, ScopedValue::Nil)) else {
        return Ok(commands::CommandTarget::From(anchor));
    };
    let ArgValue::Map(mut fields) =
        super::scoped_to_arg_value(scope, value).map_err(RuntimeError::runtime)?
    else {
        return Err(RuntimeError::runtime("command target must be a table"));
    };
    let kind = match fields.remove("kind") {
        Some(ArgValue::String(kind)) => kind,
        _ => return Err(RuntimeError::runtime("command target kind is required")),
    };
    let node = fields.remove("node");
    if !fields.is_empty() {
        return Err(RuntimeError::runtime("unknown command target field"));
    }
    match (kind.as_str(), node) {
        ("focus", None) => Ok(commands::CommandTarget::Focus),
        ("exact" | "from", Some(ArgValue::Node(node))) => {
            with_current_canopy(scope, |canopy, _| validate_node_handle(&canopy.core, node))?;
            Ok(if kind == "exact" {
                commands::CommandTarget::Exact(node)
            } else {
                commands::CommandTarget::From(node)
            })
        }
        _ => Err(RuntimeError::runtime(
            "target requires kind 'focus', or kind 'exact' or 'from' with a node",
        )),
    }
}

/// Dispatch the positional remainder of an explicit call.
fn host_positional_call<'s>(
    scope: &Scope<'s>,
    mut args: HostArgCursor<'_, 's>,
    target: commands::CommandTarget,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let name = args.required::<String>("id")?;
    let values = values_to_args(scope, iter::from_fn(|| args.raw()).collect())?;
    ret_arg(
        scope,
        &dispatch_explicit(
            scope,
            &name,
            target,
            commands::CommandArgs::Positional(values),
        )?,
    )
}

/// `canopy.call_exact`: dispatch only to the specified owner.
fn host_call_exact<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let node = read_node_id(scope, &mut args, "node")?;
    host_positional_call(scope, args, commands::CommandTarget::Exact(node))
}

/// `canopy.call_from`: dispatch relative to the supplied origin.
fn host_call_from<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let node = read_node_id(scope, &mut args, "node")?;
    host_positional_call(scope, args, commands::CommandTarget::From(node))
}

/// `canopy.call_focus`: resolve focus when invoked.
fn host_call_focus<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_positional_call(
        scope,
        HostArgCursor::new(scope, args),
        commands::CommandTarget::Focus,
    )
}

/// `canopy.call_named`: dispatch one explicit named-argument map.
fn host_call_named<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let name = args.required::<String>("id")?;
    let fields = args.required::<Table<'_>>("fields")?;
    let ArgValue::Map(fields) = super::scoped_to_arg_value(scope, ScopedValue::Table(fields))
        .map_err(RuntimeError::runtime)?
    else {
        return Err(RuntimeError::runtime("named fields must have string keys"));
    };
    let target = parse_command_target(scope, args.raw())?;
    if args.raw().is_some() {
        return Err(RuntimeError::runtime(
            "call_named accepts only id, fields, and target",
        ));
    }
    ret_arg(
        scope,
        &dispatch_explicit(scope, &name, target, commands::CommandArgs::Named(fields))?,
    )
}

/// `canopy.log`: append a log line to the evaluation diagnostics.
fn host_log<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let message = args.raw().unwrap_or(ScopedValue::Nil).display(scope);
    tracing::info!("{message}");
    with_current_canopy(scope, |canopy, _| {
        canopy.script_host.push_log(message);
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.assert`: record an assertion and fail the script when false.
fn host_assert<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let condition = !matches!(
        args.raw().unwrap_or(ScopedValue::Nil),
        ScopedValue::Nil | ScopedValue::Boolean(false)
    );
    let message = match args.raw().unwrap_or(ScopedValue::Nil) {
        ScopedValue::Nil => "assertion failed".to_string(),
        value => String::from_lua(value, scope)?,
    };
    with_current_canopy(scope, |canopy, _| {
        canopy
            .script_host
            .push_assertion(condition, message.clone());
        Ok(())
    })?;
    if condition {
        Ok(ret_none())
    } else {
        Err(RuntimeError::runtime(message))
    }
}

/// `canopy.root`: return the root node id.
fn host_root<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| Ok(ArgValue::Node(canopy.core.root_id())))
}

/// `canopy.focused`: return the focused node id, or nil.
fn host_focused<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| {
        Ok(canopy
            .core
            .focus_id()
            .map(ArgValue::Node)
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.node_info`: return the `NodeInfo` record for a node.
fn host_node_info<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let node_id = read_node_id(scope, &mut args, "id")?;
    host_value(scope, |canopy, _| {
        node_info_to_arg(canopy, node_id).map(ArgValue::Map)
    })
}

/// `canopy.find_identity`: resolve a key independently of decorative ancestors.
fn host_find_identity<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let key = args.required::<String>("key")?;
    let requested_scope = read_opt_node_id(scope, &mut args)?;
    host_value(scope, |canopy, _| {
        let root = canopy.core.root_id();
        let context = CoreViewContext::new(&canopy.core, root);
        Ok(context
            .find_identity(requested_scope.unwrap_or(root), &key)?
            .map(ArgValue::Node)
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.find_node`: return the first node matching a path pattern.
fn host_find_node<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let pattern = args.required::<String>("pattern")?;
    host_value(scope, |canopy, _| {
        let filter = PathFilter::normalized(&pattern)?;
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(root_ctx
            .find_node_matching(&filter)
            .map(ArgValue::Node)
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.find_nodes`: return all nodes matching a path pattern.
fn host_find_nodes<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let pattern = args.required::<String>("pattern")?;
    host_value(scope, |canopy, _| {
        let filter = PathFilter::normalized(&pattern)?;
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(node_list_to_arg(root_ctx.find_nodes_matching(&filter)))
    })
}

/// `canopy.parent`: return a node's parent, or nil for the root.
fn host_parent<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let node_id = read_node_id(scope, &mut args, "id")?;
    host_value(scope, |canopy, _| {
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(root_ctx
            .parent_of(node_id)
            .map(ArgValue::Node)
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.children`: return a node's children.
fn host_children<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let node_id = read_node_id(scope, &mut args, "id")?;
    host_value(scope, |canopy, _| {
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(node_list_to_arg(root_ctx.children_of(node_id)))
    })
}

/// `canopy.tree`: return the recursive node tree from the root.
fn host_tree<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| {
        tree_node_to_arg(canopy, canopy.core.root_id())
    })
}

/// `canopy.set_focus`: focus a node, returning whether focus moved.
fn host_set_focus<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let node_id = read_node_id(scope, &mut args, "id")?;
    let focused = with_current_canopy(scope, |canopy, _| {
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        ctx.set_focus(node_id).map(ChangeOutcome::changed)
    })?;
    Ok(ret_one(ScopedValue::Boolean(focused)))
}

/// `canopy.node_at`: return the node at screen coordinates, or nil.
fn host_node_at<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let x = args.required::<u32>("x")?;
    let y = args.required::<u32>("y")?;
    host_value(scope, |canopy, _| {
        Ok(canopy
            .core
            .locate_node(canopy.core.root_id(), Point { x, y })?
            .map(ArgValue::Node)
            .unwrap_or(ArgValue::Null))
    })
}

/// Move root focus in one direction.
fn host_focus_move<'s>(
    scope: &Scope<'s>,
    direction: FocusDirection,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    with_current_canopy(scope, |canopy, _| {
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        ctx.focus_move(FocusScope::Root, direction)?;
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.focus_next`: move focus to the next focusable node.
fn host_focus_next<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_focus_move(scope, FocusDirection::Next)
}

/// `canopy.focus_prev`: move focus to the previous focusable node.
fn host_focus_prev<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_focus_move(scope, FocusDirection::Prev)
}

/// `canopy.focus_dir`: move focus in a direction.
fn host_focus_dir<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let dir = args.required::<String>("dir")?;
    let dir = commands::FromArgValue::from_arg_value(&ArgValue::String(dir))
        .map_err(|error| RuntimeError::runtime(error.to_string()))?;
    host_focus_move(scope, dir)
}

/// `canopy.send_key`: inject a key event.
fn host_send_key<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let key_spec = args.required::<String>("key")?;
    with_current_canopy(scope, |canopy, _| {
        let key = key::Key::parse_spec(&key_spec)?;
        let _reentrant = ReentrantCanopyGuard::push(canopy);
        canopy.key(Some(scope), key)
    })?;
    Ok(ret_none())
}

/// `canopy.send_click`: inject a left click at screen coordinates.
fn host_send_click<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let x = args.required::<u32>("x")?;
    let y = args.required::<u32>("y")?;
    with_current_canopy(scope, |canopy, _| {
        let _reentrant = ReentrantCanopyGuard::push(canopy);
        canopy.mouse(
            Some(scope),
            mouse::MouseEvent {
                action: mouse::Action::Down,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location: Point { x, y },
            },
        )?;
        canopy.mouse(
            Some(scope),
            mouse::MouseEvent {
                action: mouse::Action::Up,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location: Point { x, y },
            },
        )
    })?;
    Ok(ret_none())
}

/// `canopy.send_scroll`: inject a scroll event at screen coordinates.
fn host_send_scroll<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let dir = args.required::<String>("direction")?;
    let x = args.required::<u32>("x")?;
    let y = args.required::<u32>("y")?;
    with_current_canopy(scope, |canopy, _| {
        let action = if dir.eq_ignore_ascii_case("up") {
            mouse::Action::ScrollUp
        } else if dir.eq_ignore_ascii_case("down") {
            mouse::Action::ScrollDown
        } else {
            return Err(error::Error::script(format!(
                "unknown scroll direction: {dir}"
            )));
        };
        let _reentrant = ReentrantCanopyGuard::push(canopy);
        canopy.mouse(
            Some(scope),
            mouse::MouseEvent {
                action,
                button: mouse::Button::None,
                modifiers: key::Empty,
                location: Point { x, y },
            },
        )
    })?;
    Ok(ret_none())
}
/// `canopy.bindings`: return the active binding table across all modes.
fn host_bindings<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| {
        Ok(ArgValue::Array(
            canopy
                .core
                .input_map
                .bindings()
                .iter()
                .map(binding_info_to_arg)
                .collect(),
        ))
    })
}

/// `canopy.commands`: return metadata for all registered commands.
fn host_commands<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let target = parse_command_target(scope, args.raw())?;
    if args.raw().is_some() {
        return Err(RuntimeError::runtime("commands accepts only a target"));
    }
    host_value(scope, |canopy, _| {
        let mut availability = canopy.command_availability(target)?;
        availability.sort_by_key(|item| item.spec.id.0);
        Ok(ArgValue::Array(
            availability.into_iter().map(command_info_to_arg).collect(),
        ))
    })
}

/// `canopy.resolve`: return the dispatch target for an owner.
fn host_resolve<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let owner = args.required::<String>("owner")?;
    host_value(scope, |canopy, node_id| {
        let resolver = commands::CommandResolver::for_target(
            &canopy.core,
            commands::CommandTarget::From(node_id),
        );
        Ok(resolver
            .resolve_owner(&owner)
            .and_then(commands::CommandResolution::target)
            .map_or(ArgValue::Null, ArgValue::Node))
    })
}

/// `canopy.input_mode`: return the active input mode.
fn host_input_mode<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mode = with_current_canopy(scope, |canopy, _| Ok(canopy.input_mode().to_string()))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&mode)?)))
}

/// `canopy.set_mode`: switch the active input mode.
fn host_set_mode<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let mode = args.required::<String>("mode")?;
    with_current_canopy(scope, |canopy, _| {
        canopy.set_input_mode(&mode);
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.push_mode`: push an input mode above the current mode.
fn host_push_mode<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let mode = args.required::<String>("mode")?;
    with_current_canopy(scope, |canopy, _| {
        canopy.push_input_mode(&mode);
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.pop_mode`: pop the top input mode and return the active mode.
fn host_pop_mode<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mode = with_current_canopy(scope, |canopy, _| Ok(canopy.pop_input_mode().to_string()))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&mode)?)))
}

/// A binding action read from a script: a command value or a callback.
enum ScriptAction<'s> {
    /// A `CommandCall` built by a `command` constructor.
    Command(ScriptCommandCall),
    /// A Luau function.
    Function(Function<'s>),
}

/// Read a binding action argument, which is a `CommandCall` or a function.
fn read_action<'s>(
    scope: &Scope<'s>,
    args: &mut HostArgCursor<'_, 's>,
) -> StdResult<ScriptAction<'s>, RuntimeError> {
    let value = args
        .raw()
        .ok_or_else(|| RuntimeError::runtime("argument `action` is required"))?;
    if let Some(call) = command_call_from_value(scope, &value)? {
        return Ok(ScriptAction::Command(call));
    }
    match value {
        ScopedValue::Function(function) => Ok(ScriptAction::Function(function)),
        other => Err(RuntimeError::runtime(format!(
            "argument `action` must be a CommandCall or a function, got {}",
            other.type_name()
        ))),
    }
}

/// Install a binding for a script action.
fn install_action_binding<'s>(
    scope: &Scope<'s>,
    action: ScriptAction<'s>,
    input: inputmap::InputSpec,
    options: &inputmap::BindingOptions,
) -> StdResult<i64, RuntimeError> {
    match action {
        ScriptAction::Command(call) => install_command_binding(scope, call.0, input, options),
        ScriptAction::Function(function) => {
            install_function_binding(scope, function, input, options)
        }
    }
}

/// Bind one key or mouse spec to a command value or a Luau callback.
fn host_bind_input<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
    field: &str,
    make_input: fn(&str) -> StdResult<inputmap::InputSpec, RuntimeError>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let spec = args.required::<String>(field)?;
    let options = parse_bind_options(scope, args.optional::<Table<'_>>("options")?)?;
    let action = read_action(scope, &mut args)?;
    let input = make_input(&spec)?;
    let id = install_action_binding(scope, action, input, &options)?;
    Ok(ret_one(ScopedValue::Number(id as f64)))
}

/// `canopy.bind`: bind a key spec to a Luau callback.
fn host_bind<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_bind_input(scope, args, "key", |spec| {
        Ok(inputmap::InputSpec::Key(
            key::Key::parse_spec(spec).map_err(error::Error::from)?,
        ))
    })
}

/// Keymap option fields, which are the `BindOptions` fields other than
/// `description`.
const KEYMAP_OPTIONS: [&str; 4] = ["mode", "path", "phase", "tier"];

/// Keymap entry fields.
const KEYMAP_ENTRY_FIELDS: [&str; 4] = ["key", "mouse", "description", "action"];

/// One validated binding that a keymap call will install.
struct PlannedBinding<'s> {
    /// Normalized input.
    input: inputmap::InputSpec,
    /// Complete binding options, including the entry description.
    options: inputmap::BindingOptions,
    /// Action shared by every binding of the entry.
    action: ScriptAction<'s>,
}

impl Clone for ScriptAction<'_> {
    fn clone(&self) -> Self {
        match self {
            Self::Command(call) => Self::Command(call.clone()),
            Self::Function(function) => Self::Function(*function),
        }
    }
}

/// Read a field that is one string or an array of strings.
fn string_or_array<'s>(
    scope: &Scope<'s>,
    value: ScopedValue<'s>,
    context: &str,
) -> StdResult<Vec<String>, RuntimeError> {
    match value {
        ScopedValue::Nil => Ok(Vec::new()),
        ScopedValue::String(_) => Ok(vec![String::from_lua(value, scope)?]),
        ScopedValue::Table(table) => {
            let items = Vec::<ScopedValue<'_>>::from_lua(ScopedValue::Table(table), scope)
                .map_err(|_| {
                    RuntimeError::runtime(format!(
                        "{context} must be a string or an array of strings"
                    ))
                })?;
            if items.is_empty() {
                return Err(RuntimeError::runtime(format!(
                    "{context} is an empty array"
                )));
            }
            items
                .into_iter()
                .map(|item| {
                    String::from_lua(item, scope).map_err(|_| {
                        RuntimeError::runtime(format!(
                            "{context} must be a string or an array of strings"
                        ))
                    })
                })
                .collect()
        }
        other => Err(RuntimeError::runtime(format!(
            "{context} must be a string or an array of strings, got {}",
            other.type_name()
        ))),
    }
}

/// Split a keymap table into its option fields and its dense entry array.
fn split_keymap<'s>(
    scope: &Scope<'s>,
    table: Table<'s>,
) -> StdResult<(Table<'s>, Vec<Table<'s>>), RuntimeError> {
    let options = scope.create_table()?;
    let mut entries = Vec::new();
    for (key, value) in table.pairs(scope)? {
        match key {
            ScopedValue::String(_) => {
                let name = String::from_lua(key, scope)?;
                if !KEYMAP_OPTIONS.contains(&name.as_str()) {
                    return Err(RuntimeError::runtime(format!(
                        "keymap has an unknown field `{name}`"
                    )));
                }
                options.set(scope, name.as_str(), value)?;
            }
            ScopedValue::Integer(_) | ScopedValue::Number(_) => {
                let index = i64::from_lua(key, scope)
                    .map_err(|_| RuntimeError::runtime("keymap entries must be a dense array"))?;
                let entry = match value {
                    ScopedValue::Table(entry) => entry,
                    other => {
                        return Err(RuntimeError::runtime(format!(
                            "keymap entry {index} must be a table, got {}",
                            other.type_name()
                        )));
                    }
                };
                entries.push((index, entry));
            }
            other => {
                return Err(RuntimeError::runtime(format!(
                    "keymap has a field with an unsupported key type {}",
                    other.type_name()
                )));
            }
        }
    }
    entries.sort_by_key(|(index, _)| *index);
    for (position, (index, _)) in entries.iter().enumerate() {
        if *index != i64::try_from(position + 1).unwrap_or(i64::MAX) {
            return Err(RuntimeError::runtime(
                "keymap entries must be a dense array",
            ));
        }
    }
    Ok((
        options,
        entries.into_iter().map(|(_, entry)| entry).collect(),
    ))
}

/// Parse and validate one keymap entry into its planned bindings.
fn plan_keymap_entry<'s>(
    scope: &Scope<'s>,
    index: usize,
    entry: Table<'s>,
    options: &inputmap::BindingOptions,
) -> StdResult<Vec<PlannedBinding<'s>>, RuntimeError> {
    let entry_error =
        |message: String| RuntimeError::runtime(format!("keymap entry {index} {message}"));
    for (key, _) in entry.pairs(scope)? {
        let name = match key {
            ScopedValue::String(_) => String::from_lua(key, scope)?,
            other => {
                return Err(entry_error(format!(
                    "has a field with an unsupported key type {}",
                    other.type_name()
                )));
            }
        };
        if !KEYMAP_ENTRY_FIELDS.contains(&name.as_str()) {
            return Err(entry_error(format!("has an unknown field `{name}`")));
        }
    }
    let keys = string_or_array(
        scope,
        entry.get(scope, "key")?,
        &format!("keymap entry {index} `key`"),
    )?;
    let mice = string_or_array(
        scope,
        entry.get(scope, "mouse")?,
        &format!("keymap entry {index} `mouse`"),
    )?;
    if keys.is_empty() && mice.is_empty() {
        return Err(entry_error("has neither `key` nor `mouse`".to_string()));
    }
    let description = optional_string_field(scope, &entry, "description")?
        .ok_or_else(|| entry_error("has no `description`".to_string()))?;
    let action = match entry.get::<_, ScopedValue<'_>>(scope, "action")? {
        ScopedValue::Nil => return Err(entry_error("has no `action`".to_string())),
        value => match command_call_from_value(scope, &value)? {
            Some(call) => ScriptAction::Command(call),
            None => match value {
                ScopedValue::Function(function) => ScriptAction::Function(function),
                other => {
                    return Err(entry_error(format!(
                        "`action` must be a CommandCall or a function, got {}",
                        other.type_name()
                    )));
                }
            },
        },
    };
    let mut options = options.clone();
    options.description = description;
    let mut planned = Vec::new();
    let mut push = |input: inputmap::InputSpec| -> StdResult<(), RuntimeError> {
        inputmap::validate_application_binding(input, &options)
            .map_err(|err| entry_error(format!("is invalid: {err}")))?;
        planned.push(PlannedBinding {
            input: input.normalize(),
            options: options.clone(),
            action: action.clone(),
        });
        Ok(())
    };
    for spec in &keys {
        let key = key::Key::parse_spec(spec)
            .map_err(|err| entry_error(format!("has an invalid key spec `{spec}`: {err}")))?;
        push(inputmap::InputSpec::Key(key))?;
    }
    for spec in &mice {
        let mouse = mouse::Mouse::parse_spec(spec)
            .map_err(|err| entry_error(format!("has an invalid mouse spec `{spec}`: {err}")))?;
        push(inputmap::InputSpec::Mouse(mouse))?;
    }
    Ok(planned)
}

/// `canopy.keymap`: install many bindings that share one option set.
///
/// Every option and entry is validated before the first binding installs, so
/// a keymap with an error installs nothing.
fn host_keymap<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let table = args.required::<Table<'_>>("keymap")?;
    let (options, entries) = split_keymap(scope, table)?;
    // The shared options carry a placeholder description that each entry
    // replaces before validation.
    options.set(scope, "description", "keymap")?;
    let options = parse_bind_options(scope, Some(options))?;
    let mut planned = Vec::new();
    let mut seen = HashSet::new();
    for (position, entry) in entries.into_iter().enumerate() {
        let index = position + 1;
        for binding in plan_keymap_entry(scope, index, entry, &options)? {
            if !seen.insert(binding.input) {
                return Err(RuntimeError::runtime(format!(
                    "keymap entry {index} binds `{}` more than once in this keymap",
                    binding.input
                )));
            }
            planned.push(binding);
        }
    }
    let mut ids = Vec::with_capacity(planned.len());
    for binding in planned {
        let id = install_action_binding(scope, binding.action, binding.input, &binding.options)?;
        ids.push(ArgValue::Int(id));
    }
    ret_arg(scope, &ArgValue::Array(ids))
}

/// `canopy.bind_mouse`: bind a mouse spec to a Luau callback.
fn host_bind_mouse<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_bind_input(scope, args, "mouse", |spec| {
        Ok(inputmap::InputSpec::Mouse(
            mouse::Mouse::parse_spec(spec).map_err(error::Error::from)?,
        ))
    })
}

/// `canopy.unbind`: remove a binding by numeric id.
fn host_unbind<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let id = args.required::<u64>("id")?;
    let removed = with_current_canopy(scope, |canopy, _| {
        canopy.unbind(inputmap::BindingId::from_u64(id))
    })?;
    Ok(ret_one(ScopedValue::Boolean(removed)))
}

/// `canopy.unbind_key`: remove key bindings matching a spec and options.
fn host_unbind_key<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let key_spec = args.required::<String>("key")?;
    let options = parse_unbind_selector(scope, args.optional::<Table<'_>>("options")?)?;
    with_current_canopy(scope, |canopy, _| {
        let key = key::Key::parse_spec(&key_spec)?;
        let scope = options
            .mode
            .as_ref()
            .map(|mode| inputmap::BindingScope::Mode(mode.clone()));
        let _ = canopy.unbind_input(
            inputmap::InputSpec::Key(key),
            &inputmap::BindingSelector {
                scope,
                path_filter: options.path.as_deref(),
            },
        );
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.clear_bindings`: remove every binding from every mode.
fn host_clear_bindings<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    with_current_canopy(scope, |canopy, _| {
        let _ = canopy.clear_bindings();
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.snapshot`: copy the last publication into detached Luau records.
fn host_snapshot<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| {
        Ok(canopy
            .snapshot()
            .map(|frame| snapshot_to_arg(&frame))
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.flush`: explicitly prepare pending changes at a host-call boundary.
fn host_flush<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    with_current_canopy(scope, |canopy, _| canopy.flush())?;
    Ok(ret_none())
}

/// `canopy.screen`: return the rendered screen as rows of cell strings.
fn host_screen<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let rows = with_current_canopy(scope, |canopy, _| screen_to_arg(canopy))?;
    ret_arg(scope, &rows)
}

/// `canopy.screen_cells`: return the rendered screen with style metadata.
fn host_screen_cells<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let rows = with_current_canopy(scope, |canopy, _| screen_cells_to_arg(canopy))?;
    ret_arg(scope, &rows)
}

/// `canopy.screen_text`: return the rendered screen as plain text.
fn host_screen_text<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let text = with_current_canopy(scope, |canopy, _| screen_text(canopy))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&text)?)))
}

/// `canopy.screen_region`: return rendered plain text inside a screen
/// rectangle.
fn host_screen_region<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let x = args.required::<i64>("x")?;
    let y = args.required::<i64>("y")?;
    let w = args.required::<i64>("w")?;
    let h = args.required::<i64>("h")?;
    // Out-of-range coordinates clamp to the screen bounds rather than failing.
    let top_left = PointI32::clamped_from_i64(x, y);
    let rect = RectI32::new(
        top_left.x,
        top_left.y,
        u32::try_from(w.max(0)).unwrap_or(u32::MAX),
        u32::try_from(h.max(0)).unwrap_or(u32::MAX),
    );
    let text = with_current_canopy(scope, |canopy, _| screen_text_for_rect(canopy, rect))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&text)?)))
}

/// `canopy.node_region`: return rendered plain text inside a node's content
/// rect.
fn host_node_region<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let node_id = read_node_id(scope, &mut args, "id")?;
    let text = with_current_canopy(scope, |canopy, _| {
        canopy.flush()?;
        let view = canopy
            .core
            .nodes
            .get(node_id)
            .ok_or_else(|| error::Error::from(commands::CommandError::InvalidNode { id: node_id }))?
            .view;
        screen_text_for_rect(canopy, view.content)
    })?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&text)?)))
}

/// `canopy.route_trace`: return the most recent input route trace.
fn host_route_trace<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| Ok(route_trace_to_arg(canopy)))
}

/// `canopy.diagnostic_dump`: return a diagnostic dump for a node.
fn host_diagnostic_dump<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let requested = read_opt_node_id(scope, &mut args)?;
    let dump = with_current_canopy(scope, |canopy, node_id| {
        let target = requested.unwrap_or(node_id);
        Ok(canopy.diagnostic_dump(target))
    })?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&dump)?)))
}

/// `canopy.available_bindings`: return effective key bindings.
fn host_available_bindings<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let requested = read_opt_node_id(scope, &mut args)?;
    host_value(scope, |canopy, _| {
        available_bindings_to_arg(canopy, requested)
    })
}

/// `canopy.script_journal`: return recorded script evaluations.
fn host_script_journal<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| Ok(script_journal_to_arg(canopy)))
}

/// `canopy.api`: return the generated Luau API definition.
fn host_api<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let api = with_current_canopy(scope, |canopy, _| canopy.script_api().map(str::to_string))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&api)?)))
}

/// `canopy.on_start`: register a callback to run after the first render.
fn host_on_start<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = HostArgCursor::new(scope, args);
    let function = args.required::<Function<'_>>("handler")?;
    let stashed = scope.stash_function(function)?;
    with_current_canopy(scope, |canopy, _| {
        let function_id = canopy.script_host.store_function(stashed)?;
        canopy
            .script_host
            .state
            .borrow_mut()
            .on_start_hooks
            .push(function_id);
        Ok(())
    })?;
    Ok(ret_none())
}

/// `fixtures`: list all registered fixtures.
fn host_fixtures<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, _| Ok(fixtures_to_arg(canopy)))
}

/// Build the declaration-coupled base Canopy module.
pub(super) fn build_base_module() -> Result<Arc<dyn NativeModule>> {
    let mut builder = module::Builder::new("canopy");
    defs::register_framework_declarations(&mut builder);
    builder.host_type(
        commands::declaration::Class::new("NodeId"),
        Arc::new(node_handle_type()),
    );
    builder.host_type(
        commands::declaration::Class::new("CommandCall"),
        Arc::new(command_call_type()),
    );
    base_api::register(&mut builder);
    builder.build().map_err(|error| {
        error::Error::script(format!("building base script module failed: {error}"))
    })
}

/// Luau global that holds the command constructors.
pub(super) const COMMAND_GLOBAL: &str = "command";

/// Build the `command` module: one constructor per node command, grouped by
/// owner, each returning a `CommandCall`.
///
/// The constructors of each owner live in a hidden table. A trusted source
/// value assembles the `command` global from those tables, so the rendered
/// declaration is one table of owner tables.
pub(super) fn build_command_module(commands: &CommandSet) -> Result<Arc<dyn NativeModule>> {
    use commands::declaration::{Field, Type};

    let mut builder = module::Builder::new(COMMAND_GLOBAL);
    builder.extern_ty("CommandCall");
    let mut owner_fields = Vec::new();
    let mut inputs = Vec::new();
    for (owner, specs) in defs::owner_command_specs(commands, &BTreeSet::new()) {
        if specs.is_empty() {
            continue;
        }
        let global_name = luau_global_owner_name(&owner);
        if global_name == COMMAND_GLOBAL {
            return Err(error::Error::script(format!(
                "owner `{owner}` collides with the reserved `{COMMAND_GLOBAL}` global"
            )));
        }
        defs::register_extern_dependencies(&mut builder, &specs);
        let hidden = format!("{COMMAND_GLOBAL}.{global_name}");
        let mut fields = Vec::new();
        for spec in specs {
            let mut field = Field::new(spec.name, Type::func(defs::command_call_sig(spec)));
            if let Some(documentation) = defs::command_doc(spec) {
                field = field.doc(documentation);
            }
            fields.push(field);
            builder.borrowed_function(
                spec.name,
                Binding::hidden(hidden.clone()),
                move |scope: &Scope<'_>, args: MultiValue<'_>| {
                    let mut args = HostArgCursor::new(scope, args);
                    let values = values_to_args(scope, iter::from_fn(|| args.raw()).collect())?;
                    let args = commands::CommandArgs::Positional(values);
                    (spec.check)(&args)
                        .map_err(|err| RuntimeError::from(error::Error::from(err)))?;
                    let call = ScriptCommandCall(commands::CommandAction {
                        invocation: commands::CommandInvocation { id: spec.id, args },
                        target: None,
                    });
                    Ok(ret_one(ScopedValue::Userdata(scope.create_userdata(call)?)))
                },
            );
        }
        owner_fields.push(Field::new(global_name.clone(), Type::table(fields)));
        inputs.push((global_name, hidden));
    }
    let locals = inputs
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    let source = if locals.is_empty() {
        "return {}".to_string()
    } else {
        format!(
            "local {} = ...\nreturn {{ {} }}",
            locals.join(", "),
            locals
                .iter()
                .map(|name| format!("{name} = {name}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    builder.source_value_with(
        COMMAND_GLOBAL,
        Binding::global(Type::table(owner_fields)).doc(
            "Command constructors, grouped by owner. Each constructor checks its arguments \
             and returns a CommandCall for use as a binding action.",
        ),
        source,
        inputs.into_iter().map(|(_, hidden)| hidden),
    );
    builder.build().map_err(|error| {
        error::Error::script(format!("building command script module failed: {error}"))
    })
}

/// Build declaration-coupled per-owner command modules for the surface.
pub(super) fn build_owner_modules(
    commands: &CommandSet,
    default_binding_owners: &BTreeSet<String>,
) -> Result<Vec<Arc<dyn NativeModule>>> {
    let mut modules = Vec::new();
    for (owner, specs) in defs::owner_command_specs(commands, default_binding_owners) {
        let global_name = luau_global_owner_name(&owner);
        let mut builder = module::Builder::new(global_name.clone());
        defs::register_owner_dependencies(&mut builder, &specs);
        for spec in specs {
            let mut binding = Binding::library(
                global_name.clone(),
                commands::declaration::Type::func(defs::command_fn_sig(spec)),
            );
            if let Some(documentation) = defs::command_doc(spec) {
                binding = binding.doc(documentation);
            }
            builder.borrowed_function(
                spec.name,
                binding,
                move |scope: &Scope<'_>, args: MultiValue<'_>| {
                    let mut args = HostArgCursor::new(scope, args);
                    let values = values_to_args(scope, iter::from_fn(|| args.raw()).collect())?;
                    let node_id = with_current_canopy(scope, |_, node_id| Ok(node_id))?;
                    let result = dispatch_command(scope, spec, node_id, values)?;
                    ret_arg(scope, &result)
                },
            );
        }
        if default_binding_owners.contains(&owner) {
            builder.borrowed_function(
                "default_bindings",
                Binding::library(
                    global_name,
                    commands::declaration::Type::func(
                        commands::declaration::FunctionSignature::new(),
                    ),
                )
                .doc("Register this widget's default bindings."),
                move |scope: &Scope<'_>, _args: MultiValue<'_>| {
                    run_default_bindings_in_scope(scope, &owner)?;
                    Ok(ret_none())
                },
            );
        }
        modules.push(builder.build().map_err(|error| {
            error::Error::script(format!("building owner script module failed: {error}"))
        })?);
    }
    Ok(modules)
}
