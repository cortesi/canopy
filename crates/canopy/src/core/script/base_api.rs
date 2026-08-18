//! Base `canopy` scripting API declarations and native registration.

use std::{
    collections::BTreeSet,
    result::Result as StdResult,
    sync::Arc,
    time::{Duration, Instant},
    vec,
};

use ruau::{
    declaration::{FunctionSignature, Type},
    module::{self, Binding},
    vm::{
        AsyncHostContext, AsyncHostFunction, FromLuaMulti, Function, HostReturn, MultiValue,
        NativeModule, RuntimeError, Scope, ScopedValue, StashedClosure, Table, async_host_fn,
    },
};

use super::{
    ArgValue, Canopy, ChangeOutcome, CommandSet, Context, CoreContext, CoreViewContext, FocusScope,
    NodeId, PathFilter, Pin, Point, RectI32, ReentrantCanopyGuard, Result, ViewContext,
    available_bindings_to_arg, base_api, binding_info_to_arg, canopy_to_host, command_info_to_arg,
    commands, defs, dispatch_command, dispatch_command_by_name, error, fixtures_to_arg,
    host_return, host_value, inputmap, key, luau_global_owner_name, mouse, node_handle_type,
    node_id_from_value, node_id_to_arg, node_info_to_arg, node_list_to_arg, owned_truthy,
    owned_value_to_display, ret_arg, ret_none, ret_one, route_trace_to_arg,
    scoped_value_to_display, scoped_value_to_string, screen_cells_to_arg, screen_text,
    screen_text_for_rect, screen_to_arg, script_callback_label, script_journal_to_arg,
    tree_node_to_arg, validate_node_handle, values_to_args, with_current_canopy, yield_now,
};

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
    /// Luau doc comments rendered above the declaration.
    docs: &'static [&'static str],
    /// Luau function type signature.
    signature: fn() -> FunctionSignature,
    /// Native host implementation.
    handler: Handler,
}

/// Native functions exposed on the `canopy` library table.
const CANOPY_FUNCTIONS: &[BaseFunction] = &[
    BaseFunction {
        name: "root",
        docs: &["Return the root node."],
        signature: || FunctionSignature::new().ret(Type::named("NodeId")),
        handler: Handler::Sync(host_root),
    },
    BaseFunction {
        name: "focused",
        docs: &["Return the currently focused node, or nil when nothing is focused."],
        signature: || FunctionSignature::new().ret(Type::named("NodeId").optional()),
        handler: Handler::Sync(host_focused),
    },
    BaseFunction {
        name: "node_info",
        docs: &["Return structured information about a node."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::named("NodeInfo"))
        },
        handler: Handler::Sync(host_node_info),
    },
    BaseFunction {
        name: "find_node",
        docs: &["Find the first node whose path matches a canopy path pattern."],
        signature: || {
            FunctionSignature::new()
                .param(("pattern", Type::String))
                .ret(Type::named("NodeId").optional())
        },
        handler: Handler::Sync(host_find_node),
    },
    BaseFunction {
        name: "find_nodes",
        docs: &["Find every node whose path matches a canopy path pattern."],
        signature: || {
            FunctionSignature::new()
                .param(("pattern", Type::String))
                .ret(Type::named("NodeId").array())
        },
        handler: Handler::Sync(host_find_nodes),
    },
    BaseFunction {
        name: "parent",
        docs: &["Return the parent of a node, or nil at the root."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::named("NodeId").optional())
        },
        handler: Handler::Sync(host_parent),
    },
    BaseFunction {
        name: "children",
        docs: &["Return the direct children of a node."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::named("NodeId").array())
        },
        handler: Handler::Sync(host_children),
    },
    BaseFunction {
        name: "tree",
        docs: &["Return a recursive snapshot of the entire tree rooted at `canopy.root()`."],
        signature: || FunctionSignature::new().ret(Type::named("TreeNode")),
        handler: Handler::Sync(host_tree),
    },
    BaseFunction {
        name: "node_at",
        docs: &["Hit-test a screen coordinate and return the deepest visible node at that point."],
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
        docs: &["Attempt to move focus directly to a node."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::Boolean)
        },
        handler: Handler::Sync(host_set_focus),
    },
    BaseFunction {
        name: "focus_next",
        docs: &["Move focus to the next focusable node in global focus order."],
        signature: FunctionSignature::new,
        handler: Handler::Sync(host_focus_next),
    },
    BaseFunction {
        name: "focus_prev",
        docs: &["Move focus to the previous focusable node in global focus order."],
        signature: FunctionSignature::new,
        handler: Handler::Sync(host_focus_prev),
    },
    BaseFunction {
        name: "focus_dir",
        docs: &["Move focus in a geometric direction."],
        signature: || {
            FunctionSignature::new().param(("dir", Type::literals(["Up", "Down", "Left", "Right"])))
        },
        handler: Handler::Sync(host_focus_dir),
    },
    BaseFunction {
        name: "send_key",
        docs: &[
            "Inject a key event using a canopy key spec string such as `ctrl-c` or `PageDown`.",
        ],
        signature: || FunctionSignature::new().param(("key", Type::String)),
        handler: Handler::Sync(host_send_key),
    },
    BaseFunction {
        name: "send_click",
        docs: &["Inject a left click at screen coordinates."],
        signature: || {
            FunctionSignature::new()
                .param(("x", Type::Number))
                .param(("y", Type::Number))
        },
        handler: Handler::Sync(host_send_click),
    },
    BaseFunction {
        name: "send_scroll",
        docs: &["Inject a scroll event at screen coordinates."],
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
        docs: &["Dispatch a command by fully-qualified command id such as `root::quit`."],
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
        docs: &["Dispatch a command against a specific node."],
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
        name: "resolve",
        docs: &["Return the command dispatch target for an owner, or nil if none is mounted."],
        signature: || {
            FunctionSignature::new()
                .param(("owner", Type::String))
                .ret(Type::named("NodeId").optional())
        },
        handler: Handler::Sync(host_resolve),
    },
    BaseFunction {
        name: "bindings",
        docs: &["Return the active binding table across all modes."],
        signature: || FunctionSignature::new().ret(Type::named("BindingInfo").array()),
        handler: Handler::Sync(host_bindings),
    },
    BaseFunction {
        name: "commands",
        docs: &["Return structured metadata for all registered commands."],
        signature: || FunctionSignature::new().ret(Type::named("CommandInfo").array()),
        handler: Handler::Sync(host_commands),
    },
    BaseFunction {
        name: "input_mode",
        docs: &["Return the active input mode. The default mode is the empty string."],
        signature: || FunctionSignature::new().ret(Type::String),
        handler: Handler::Sync(host_input_mode),
    },
    BaseFunction {
        name: "set_mode",
        docs: &["Switch the active input mode. Passing the empty string returns to default mode."],
        signature: || FunctionSignature::new().param(("mode", Type::String)),
        handler: Handler::Sync(host_set_mode),
    },
    BaseFunction {
        name: "push_mode",
        docs: &["Push an input mode above the current mode."],
        signature: || FunctionSignature::new().param(("mode", Type::String)),
        handler: Handler::Sync(host_push_mode),
    },
    BaseFunction {
        name: "pop_mode",
        docs: &["Pop the top input mode and return the active mode after the pop."],
        signature: || FunctionSignature::new().ret(Type::String),
        handler: Handler::Sync(host_pop_mode),
    },
    BaseFunction {
        name: "screen",
        docs: &["Return the rendered screen as rows of cell strings."],
        signature: || FunctionSignature::new().ret(Type::String.array().array()),
        handler: Handler::Sync(host_screen),
    },
    BaseFunction {
        name: "screen_cells",
        docs: &["Return the rendered screen as rows of styled cell records."],
        signature: || FunctionSignature::new().ret(Type::named("ScreenCell").array().array()),
        handler: Handler::Sync(host_screen_cells),
    },
    BaseFunction {
        name: "screen_text",
        docs: &["Return the rendered screen as newline-joined plain text."],
        signature: || FunctionSignature::new().ret(Type::String),
        handler: Handler::Sync(host_screen_text),
    },
    BaseFunction {
        name: "screen_region",
        docs: &["Return rendered plain text inside a screen rectangle."],
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
        docs: &["Return rendered plain text inside a node's content rectangle."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::String)
        },
        handler: Handler::Sync(host_node_region),
    },
    BaseFunction {
        name: "route_trace",
        docs: &["Return the most recent input route trace."],
        signature: || FunctionSignature::new().ret(Type::named("RouteTraceEntry").array()),
        handler: Handler::Sync(host_route_trace),
    },
    BaseFunction {
        name: "diagnostic_dump",
        docs: &["Return a diagnostic dump for a node, or the current script anchor."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId").optional()))
                .ret(Type::String)
        },
        handler: Handler::Sync(host_diagnostic_dump),
    },
    BaseFunction {
        name: "available_bindings",
        docs: &["Return effective key bindings for a node or the current focus."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId").optional()))
                .ret(Type::named("BindingSnapshot"))
        },
        handler: Handler::Sync(host_available_bindings),
    },
    BaseFunction {
        name: "script_journal",
        docs: &["Return recorded script evaluations for replay and diagnostics."],
        signature: || FunctionSignature::new().ret(Type::named("ScriptJournalEntry").array()),
        handler: Handler::Sync(host_script_journal),
    },
    BaseFunction {
        name: "api",
        docs: &["Return the generated Luau API definition for this app."],
        signature: || FunctionSignature::new().ret(Type::String),
        handler: Handler::Sync(host_api),
    },
    BaseFunction {
        name: "bind",
        docs: &["Bind a key spec with required discovery metadata."],
        signature: || {
            FunctionSignature::new()
                .param(("key", Type::String))
                .param(("options", Type::named("BindOptions")))
                .param(("handler", Type::func(FunctionSignature::new())))
                .ret(Type::Number)
        },
        handler: Handler::Sync(host_bind),
    },
    BaseFunction {
        name: "bind_mouse",
        docs: &["Bind a mouse spec with required discovery metadata."],
        signature: || {
            FunctionSignature::new()
                .param(("mouse", Type::named("MouseSpec")))
                .param(("options", Type::named("BindOptions")))
                .param(("handler", Type::func(FunctionSignature::new())))
                .ret(Type::Number)
        },
        handler: Handler::Sync(host_bind_mouse),
    },
    BaseFunction {
        name: "unbind",
        docs: &["Remove a binding by numeric id."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::Number))
                .ret(Type::Boolean)
        },
        handler: Handler::Sync(host_unbind),
    },
    BaseFunction {
        name: "unbind_key",
        docs: &["Remove key bindings matching the key spec and optional mode/path filter."],
        signature: || {
            FunctionSignature::new()
                .param(("key", Type::String))
                .param(("options", Type::named("UnbindSelector").optional()))
        },
        handler: Handler::Sync(host_unbind_key),
    },
    BaseFunction {
        name: "clear_bindings",
        docs: &["Remove every binding from every mode."],
        signature: FunctionSignature::new,
        handler: Handler::Sync(host_clear_bindings),
    },
    BaseFunction {
        name: "on_start",
        docs: &["Register a callback that runs after the first live render."],
        signature: || {
            FunctionSignature::new().param(("handler", Type::func(FunctionSignature::new())))
        },
        handler: Handler::Sync(host_on_start),
    },
    BaseFunction {
        name: "log",
        docs: &["Append a log line to the evaluation result."],
        signature: || FunctionSignature::new().param(("message", Type::Any)),
        handler: Handler::Sync(host_log),
    },
    BaseFunction {
        name: "assert",
        docs: &["Fail the script when the condition is false."],
        signature: || {
            FunctionSignature::new()
                .param(("condition", Type::Boolean))
                .param(("message", Type::String.optional()))
        },
        handler: Handler::Sync(host_assert),
    },
    BaseFunction {
        name: "wait_for",
        docs: &["Wait until a predicate returns a truthy value."],
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
        docs: &["Wait until a command owner resolves to a mounted node."],
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
        docs: &["Wait until the rendered screen contains text."],
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
        let binding = documented_binding(
            Binding::library("canopy", Type::func((function.signature)())),
            function.docs,
        );
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
        ))
        .doc("List all registered fixtures available to the current app."),
        host_fixtures,
    );
}

/// Attach the declaration docs recorded for one base function.
fn documented_binding(binding: Binding, docs: &[&str]) -> Binding {
    if docs.is_empty() {
        binding
    } else {
        binding.doc(docs.join("\n"))
    }
}

/// Convert raw integer coordinates into a canopy point.
fn point_from_coords(x: i64, y: i64) -> Result<Point> {
    let x = u32::try_from(x)
        .map_err(|_| error::Error::Script(format!("x coordinate must be >= 0, got {x}")))?;
    let y = u32::try_from(y)
        .map_err(|_| error::Error::Script(format!("y coordinate must be >= 0, got {y}")))?;
    Ok(Point { x, y })
}

/// Parsed options for script-created bindings.
#[derive(Debug, Clone)]
struct ScriptBindOptions {
    /// Binding scope.
    scope: inputmap::BindingScope,
    /// Optional path filter override.
    path: String,
    /// Required human-readable description.
    description: String,
}

/// Selector for application binding removal.
#[derive(Debug, Clone, Default)]
struct ScriptUnbindSelector {
    /// Optional named mode to select.
    mode: Option<String>,
    /// Optional exact path filter to select.
    path: Option<String>,
}

/// Parse `BindOptions` from a required script table.
fn parse_bind_options<'s>(
    scope: &Scope<'s>,
    options: Option<Table<'s>>,
) -> StdResult<ScriptBindOptions, RuntimeError> {
    let Some(options) = options else {
        return Err(RuntimeError::runtime("binding options table is required"));
    };
    let field = |name: &str| -> StdResult<Option<String>, RuntimeError> {
        match options.get::<_, ScopedValue>(scope, name)? {
            ScopedValue::Nil => Ok(None),
            value => scoped_value_to_string(scope, value)
                .map(Some)
                .map_err(RuntimeError::runtime),
        }
    };
    let description = field("description")?
        .ok_or_else(|| RuntimeError::runtime("binding description is required"))?;
    if description.trim().is_empty() {
        return Err(RuntimeError::runtime("binding description cannot be empty"));
    }
    let mode = field("mode")?.filter(|mode| !mode.is_empty());
    let tier = field("tier")?;
    let scope = match tier.as_deref() {
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
    Ok(ScriptBindOptions {
        scope,
        path: field("path")?.unwrap_or_default(),
        description,
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
    let field = |name: &str| -> StdResult<Option<String>, RuntimeError> {
        match options.get::<_, ScopedValue>(scope, name)? {
            ScopedValue::Nil => Ok(None),
            value => scoped_value_to_string(scope, value)
                .map(Some)
                .map_err(RuntimeError::runtime),
        }
    };
    Ok(ScriptUnbindSelector {
        mode: field("mode")?.filter(|mode| !mode.is_empty()),
        path: field("path")?.filter(|path| !path.is_empty()),
    })
}

/// Positional argument reader over a host call's values.
pub(super) struct ArgReader<'s> {
    /// Remaining argument values, in order.
    values: vec::IntoIter<ScopedValue<'s>>,
    /// One-based index of the next argument, for error messages.
    index: usize,
}

impl<'s> ArgReader<'s> {
    /// Wrap a host call's arguments.
    pub(super) fn new(args: MultiValue<'s>) -> Self {
        Self {
            values: args.into_vec().into_iter(),
            index: 0,
        }
    }

    /// Take the next argument, `Nil` when exhausted.
    pub(super) fn next_value(&mut self) -> ScopedValue<'s> {
        self.index += 1;
        self.values.next().unwrap_or(ScopedValue::Nil)
    }

    /// Take the remaining arguments.
    pub(super) fn rest(self) -> Vec<ScopedValue<'s>> {
        self.values.collect()
    }

    /// Take a required string argument.
    pub(super) fn string(&mut self, scope: &Scope<'s>) -> StdResult<String, RuntimeError> {
        let index = self.index + 1;
        scoped_value_to_string(scope, self.next_value())
            .map_err(|message| RuntimeError::runtime(format!("argument {index}: {message}")))
    }

    /// Take a required integer argument.
    pub(super) fn integer(&mut self, _scope: &Scope<'s>) -> StdResult<i64, RuntimeError> {
        let index = self.index + 1;
        match self.next_value() {
            ScopedValue::Integer(value) => Ok(value),
            ScopedValue::Number(value) if value.fract() == 0.0 => Ok(value as i64),
            other => Err(RuntimeError::runtime(format!(
                "argument {index}: expected integer, got {}",
                other.type_name()
            ))),
        }
    }

    /// Take a required node id argument.
    pub(super) fn node_id(&mut self, scope: &Scope<'s>) -> StdResult<NodeId, RuntimeError> {
        let node_id = node_id_from_value(scope, self.next_value())?;
        with_current_canopy(scope, |canopy, _| {
            validate_node_handle(&canopy.core, node_id).map(|()| node_id)
        })
        .map_err(|err| canopy_to_host(&err))
    }

    /// Take an optional node id argument.
    pub(super) fn opt_node_id(
        &mut self,
        scope: &Scope<'s>,
    ) -> StdResult<Option<NodeId>, RuntimeError> {
        let next = self.next_value();
        if matches!(next, ScopedValue::Nil) {
            return Ok(None);
        }
        let node_id = node_id_from_value(scope, next)?;
        with_current_canopy(scope, |canopy, _| {
            validate_node_handle(&canopy.core, node_id).map(|()| Some(node_id))
        })
        .map_err(|err| canopy_to_host(&err))
    }

    /// Take a required function argument.
    pub(super) fn function(&mut self, _scope: &Scope<'s>) -> StdResult<Function<'s>, RuntimeError> {
        let index = self.index + 1;
        match self.next_value() {
            ScopedValue::Function(function) => Ok(function),
            other => Err(RuntimeError::runtime(format!(
                "argument {index}: expected function, got {}",
                other.type_name()
            ))),
        }
    }

    /// Take an optional table argument.
    pub(super) fn opt_table(
        &mut self,
        _scope: &Scope<'s>,
    ) -> StdResult<Option<Table<'s>>, RuntimeError> {
        let index = self.index + 1;
        match self.next_value() {
            ScopedValue::Nil => Ok(None),
            ScopedValue::Table(table) => Ok(Some(table)),
            other => Err(RuntimeError::runtime(format!(
                "argument {index}: expected table, got {}",
                other.type_name()
            ))),
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
        let mut args = ArgReader::new(values);
        let predicate = scope.stash_function(args.function(scope)?)?;
        let timeout_ms = optional_timeout_ms(args.next_value())?;
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

impl<'s> FromLuaMulti<'s> for WaitForNodeArgs {
    fn from_lua_multi(values: MultiValue<'s>, scope: &Scope<'s>) -> StdResult<Self, RuntimeError> {
        let mut args = ArgReader::new(values);
        let owner = args.string(scope)?;
        let timeout_ms = optional_timeout_ms(args.next_value())?;
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
        let mut args = ArgReader::new(values);
        let text = args.string(scope)?;
        let timeout_ms = optional_timeout_ms(args.next_value())?;
        Ok(Self { text, timeout_ms })
    }
}

/// Parse an optional millisecond timeout from a script argument.
fn optional_timeout_ms(value: ScopedValue<'_>) -> StdResult<Option<u64>, RuntimeError> {
    match value {
        ScopedValue::Nil => Ok(None),
        ScopedValue::Integer(value) if value >= 0 => Ok(Some(value as u64)),
        ScopedValue::Number(value) if value.fract() == 0.0 && value >= 0.0 => {
            Ok(Some(value as u64))
        }
        other => Err(RuntimeError::runtime(format!(
            "expected non-negative timeout milliseconds, got {}",
            other.type_name()
        ))),
    }
}

/// Build a timeout error for an async wait helper.
fn wait_timeout(timeout_ms: u64) -> RuntimeError {
    canopy_to_host(&error::Error::ScriptTimeout { timeout_ms })
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
    let started = Instant::now();
    loop {
        ctx.scope(|scope| {
            let mut canopy = scope
                .context_mut::<Canopy>()
                .ok_or_else(|| RuntimeError::runtime("no active canopy context"))?;
            canopy.service_automation();
            Ok(())
        })
        .await?;
        if ready(ctx.clone()).await? {
            return Ok(host_return(true));
        }
        if let Some(timeout_ms) = timeout_ms
            && started.elapsed() >= Duration::from_millis(timeout_ms)
        {
            return Err(wait_timeout(timeout_ms));
        }
        yield_now().await;
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
                    owned_value_to_display(error.value())
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
                Ok(canopy
                    .command_availability_from_focus()
                    .iter()
                    .any(|entry| {
                        entry.resolution.is_some()
                            && matches!(
                                entry.spec.dispatch,
                                commands::CommandDispatchKind::Node { owner: entry_owner }
                                    if entry_owner == owner
                            )
                    }))
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
                let mut canopy = scope
                    .context_mut::<Canopy>()
                    .ok_or_else(|| RuntimeError::runtime("no active canopy context"))?;
                let screen = screen_text(&mut canopy)?;
                Ok(screen.contains(&text))
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
    options: &ScriptBindOptions,
) -> StdResult<i64, RuntimeError> {
    let stashed = scope.stash_function(function)?;
    let source = Some(script_callback_label(scope));
    with_current_canopy(scope, |canopy, _| {
        let function_id = canopy.script_host.store_function(stashed)?;
        let result = canopy.core.input_map.replace_application_binding(
            options.scope.clone(),
            input,
            &options.path,
            &options.description,
            source,
            function_id,
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
    .map_err(|err| canopy_to_host(&err))
}

/// `canopy.cmd`: dispatch a command by fully-qualified id.
fn host_cmd<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let name = args.string(scope)?;
    let values = values_to_args(scope, args.rest())?;
    let result = dispatch_command_by_name(scope, &name, None, values)?;
    ret_arg(scope, &result)
}

/// `canopy.cmd_on`: dispatch a command against a specific node.
fn host_cmd_on<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    let name = args.string(scope)?;
    let values = values_to_args(scope, args.rest())?;
    let result = dispatch_command_by_name(scope, &name, Some(node_id), values)?;
    ret_arg(scope, &result)
}

/// `canopy.log`: append a log line to the evaluation diagnostics.
fn host_log<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let message = scoped_value_to_display(scope, args.next_value());
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
    let mut args = ArgReader::new(args);
    let condition = !matches!(
        args.next_value(),
        ScopedValue::Nil | ScopedValue::Boolean(false)
    );
    let message = match args.next_value() {
        ScopedValue::Nil => "assertion failed".to_string(),
        value => scoped_value_to_string(scope, value).map_err(RuntimeError::runtime)?,
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
    host_value(scope, |canopy, _| Ok(node_id_to_arg(canopy.core.root_id())))
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
            .map(node_id_to_arg)
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.node_info`: return the `NodeInfo` record for a node.
fn host_node_info<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    host_value(scope, |canopy, _| {
        node_info_to_arg(canopy, node_id).map(ArgValue::Map)
    })
}

/// `canopy.find_node`: return the first node matching a path pattern.
fn host_find_node<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let pattern = args.string(scope)?;
    host_value(scope, |canopy, _| {
        let filter = PathFilter::normalized(&pattern)?;
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(root_ctx
            .find_node_matching(&filter)
            .map(node_id_to_arg)
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.find_nodes`: return all nodes matching a path pattern.
fn host_find_nodes<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let pattern = args.string(scope)?;
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
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    host_value(scope, |canopy, _| {
        let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
        Ok(root_ctx
            .parent_of(node_id)
            .map(node_id_to_arg)
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.children`: return a node's children.
fn host_children<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
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
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
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
    let mut args = ArgReader::new(args);
    let x = args.integer(scope)?;
    let y = args.integer(scope)?;
    host_value(scope, |canopy, _| {
        Ok(canopy
            .core
            .locate_node(canopy.core.root_id(), point_from_coords(x, y)?)?
            .map(node_id_to_arg)
            .unwrap_or(ArgValue::Null))
    })
}

/// `canopy.focus_next`: move focus to the next focusable node.
fn host_focus_next<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    with_current_canopy(scope, |canopy, _| {
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        ctx.focus_next(FocusScope::Root)?;
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.focus_prev`: move focus to the previous focusable node.
fn host_focus_prev<'s>(
    scope: &Scope<'s>,
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    with_current_canopy(scope, |canopy, _| {
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        ctx.focus_prev(FocusScope::Root)?;
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.focus_dir`: move focus in a direction.
fn host_focus_dir<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let dir = args.string(scope)?;
    with_current_canopy(scope, |canopy, _| {
        let dir = commands::FromArgValue::from_arg_value(&ArgValue::String(dir))
            .map_err(error::Error::from)?;
        let root_id = canopy.core.root_id();
        let mut ctx = CoreContext::new(&mut canopy.core, root_id);
        ctx.focus_dir(FocusScope::Root, dir)?;
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.send_key`: inject a key event.
fn host_send_key<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let key_spec = args.string(scope)?;
    with_current_canopy(scope, |canopy, _| {
        let key = key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?;
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
    let mut args = ArgReader::new(args);
    let x = args.integer(scope)?;
    let y = args.integer(scope)?;
    with_current_canopy(scope, |canopy, _| {
        let location = point_from_coords(x, y)?;
        let _reentrant = ReentrantCanopyGuard::push(canopy);
        canopy.mouse(
            Some(scope),
            mouse::MouseEvent {
                action: mouse::Action::Down,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location,
            },
        )?;
        canopy.mouse(
            Some(scope),
            mouse::MouseEvent {
                action: mouse::Action::Up,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location,
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
    let mut args = ArgReader::new(args);
    let dir = args.string(scope)?;
    let x = args.integer(scope)?;
    let y = args.integer(scope)?;
    with_current_canopy(scope, |canopy, _| {
        let action = if dir.eq_ignore_ascii_case("up") {
            mouse::Action::ScrollUp
        } else if dir.eq_ignore_ascii_case("down") {
            mouse::Action::ScrollDown
        } else {
            return Err(error::Error::Script(format!(
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
                location: point_from_coords(x, y)?,
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
    _args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    host_value(scope, |canopy, node_id| {
        let resolver = commands::CommandResolver::new(&canopy.core, node_id);
        let mut availability = resolver.availability();
        availability.sort_by_key(|item| item.spec.id.0);
        Ok(ArgValue::Array(
            availability
                .into_iter()
                .map(|item| command_info_to_arg(item.spec, item.resolution))
                .collect(),
        ))
    })
}

/// `canopy.resolve`: return the dispatch target for an owner.
fn host_resolve<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let owner = args.string(scope)?;
    host_value(scope, |canopy, node_id| {
        let resolver = commands::CommandResolver::new(&canopy.core, node_id);
        Ok(resolver
            .resolve_owner(&owner)
            .and_then(commands::CommandResolution::target)
            .map_or(ArgValue::Null, node_id_to_arg))
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
    let mut args = ArgReader::new(args);
    let mode = args.string(scope)?;
    with_current_canopy(scope, |canopy, _| {
        canopy.set_input_mode(&mode)?;
        Ok(())
    })?;
    Ok(ret_none())
}

/// `canopy.push_mode`: push an input mode above the current mode.
fn host_push_mode<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let mode = args.string(scope)?;
    with_current_canopy(scope, |canopy, _| {
        canopy.push_input_mode(&mode)?;
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

/// `canopy.bind`: bind a key spec to a Luau callback.
fn host_bind<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let key_spec = args.string(scope)?;
    let options = parse_bind_options(scope, args.opt_table(scope)?)?;
    let function = args.function(scope)?;
    let input =
        inputmap::InputSpec::Key(key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?);
    let id = install_function_binding(scope, function, input, &options)?;
    Ok(ret_one(ScopedValue::Number(id as f64)))
}

/// `canopy.bind_mouse`: bind a mouse spec to a Luau callback.
fn host_bind_mouse<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let mouse_spec = args.string(scope)?;
    let options = parse_bind_options(scope, args.opt_table(scope)?)?;
    let function = args.function(scope)?;
    let input = inputmap::InputSpec::Mouse(
        mouse::Mouse::parse_spec(&mouse_spec).map_err(error::Error::Script)?,
    );
    let id = install_function_binding(scope, function, input, &options)?;
    Ok(ret_one(ScopedValue::Number(id as f64)))
}

/// `canopy.unbind`: remove a binding by numeric id.
fn host_unbind<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let id = args.integer(scope)?;
    let removed = with_current_canopy(scope, |canopy, _| {
        canopy.unbind(inputmap::BindingId::from_u64(id as u64))
    })?;
    Ok(ret_one(ScopedValue::Boolean(removed)))
}

/// `canopy.unbind_key`: remove key bindings matching a spec and options.
fn host_unbind_key<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let key_spec = args.string(scope)?;
    let options = parse_unbind_selector(scope, args.opt_table(scope)?)?;
    with_current_canopy(scope, |canopy, _| {
        let key = key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?;
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

/// `canopy.screen_region`: return rendered plain text inside a screen rectangle.
fn host_screen_region<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let x = args.integer(scope)?;
    let y = args.integer(scope)?;
    let w = args.integer(scope)?;
    let h = args.integer(scope)?;
    let rect = RectI32::new(
        i32::try_from(x).unwrap_or(if x < 0 { i32::MIN } else { i32::MAX }),
        i32::try_from(y).unwrap_or(if y < 0 { i32::MIN } else { i32::MAX }),
        u32::try_from(w.max(0)).unwrap_or(u32::MAX),
        u32::try_from(h.max(0)).unwrap_or(u32::MAX),
    );
    let text = with_current_canopy(scope, |canopy, _| screen_text_for_rect(canopy, rect))?;
    Ok(ret_one(ScopedValue::String(scope.create_string(&text)?)))
}

/// `canopy.node_region`: return rendered plain text inside a node's content rect.
fn host_node_region<'s>(
    scope: &Scope<'s>,
    args: MultiValue<'s>,
) -> StdResult<MultiValue<'s>, RuntimeError> {
    let mut args = ArgReader::new(args);
    let node_id = args.node_id(scope)?;
    let text = with_current_canopy(scope, |canopy, _| {
        canopy.refresh_snapshot()?;
        let view = canopy
            .core
            .node(node_id)
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
    let mut args = ArgReader::new(args);
    let requested = args.opt_node_id(scope)?;
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
    let mut args = ArgReader::new(args);
    let requested = args.opt_node_id(scope)?;
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
    let mut args = ArgReader::new(args);
    let function = args.function(scope)?;
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
    defs::register_framework_types(&mut builder);
    builder.host_type(
        commands::declaration::Class::new("NodeId"),
        Arc::new(node_handle_type()),
    );
    base_api::register(&mut builder);
    builder.build().map_err(|error| {
        error::Error::Script(format!("building base script module failed: {error}"))
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
                    let values = values_to_args(scope, ArgReader::new(args).rest())?;
                    let allow_map_named = values.len() == 1;
                    let node_id = with_current_canopy(scope, |_, node_id| Ok(node_id))?;
                    let result = dispatch_command(scope, spec, node_id, values, allow_map_named)?;
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
            error::Error::Script(format!("building owner script module failed: {error}"))
        })?);
    }
    Ok(modules)
}
