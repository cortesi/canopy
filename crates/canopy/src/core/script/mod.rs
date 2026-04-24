use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt, mem,
    rc::Rc,
    result::Result as StdResult,
};

use mlua::{
    Error as LuaError, Function, Lua, LuaOptions, MetaMethod, MultiValue, RegistryKey, StdLib,
    Table, UserDataMethods, Value,
};

use crate::{
    Canopy, NodeId,
    commands::{self, ArgValue, CommandArgs, CommandInvocation, CommandSet, CommandSpec},
    core::{
        context::{Context, CoreContext, CoreViewContext, ReadContext},
        inputmap::{self, BindingTarget},
    },
    error::{self, Result},
    event::{key, mouse},
    geom::{Point, RectI32, Size},
};

/// Render Luau definition files from the current command set.
pub mod defs;

/// Script identifier.
pub type ScriptId = u64;

/// Stable handle for a stored Luau closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LuauFunctionId(u64);

/// Recorded assertion outcome for a script evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptAssertion {
    /// Whether the assertion passed.
    pub passed: bool,
    /// Assertion message or fallback description.
    pub message: String,
}

/// Opaque wrapper used when a script needs to keep a node handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptNodeId(pub NodeId);

impl mlua::UserData for ScriptNodeId {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Eq, |_, this, other: Value| {
            Ok(userdata_to_node_id(other).is_ok_and(|other| this.0 == other))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("NodeId({:?})", this.0))
        });
    }
}

/// Cached compiled script.
#[derive(Clone)]
struct Script {
    /// Compiled Luau function.
    function: Function,
    /// Original source text.
    source: String,
}

impl Script {
    /// Return the original source text.
    fn source(&self) -> &str {
        &self.source
    }
}

/// Stored Luau closure with a stable host-side id.
struct StoredFunction {
    /// Lua registry entry for the closure.
    key: RegistryKey,
    /// Help/debug label for the closure.
    label: Option<String>,
    /// Number of live references held by bindings or hook queues.
    refs: usize,
}

/// Shared mutable host state.
#[derive(Default)]
struct LuauState {
    /// Cached compiled scripts.
    scripts: HashMap<ScriptId, Script>,
    /// Next script identifier.
    next_script_id: ScriptId,
    /// Stored Luau closures keyed by stable id.
    functions: HashMap<LuauFunctionId, StoredFunction>,
    /// Next stored function identifier.
    next_function_id: u64,
    /// Cached rendered d.luau definitions.
    definitions: Option<String>,
    /// Whether the command surface has been finalized.
    finalized: bool,
    /// Deferred hooks to execute after the first live render.
    on_start_hooks: Vec<LuauFunctionId>,
    /// Whether zero-ref closures need a post-callback registry sweep.
    pending_function_sweep: bool,
    /// Log messages emitted by the most recent script evaluation.
    logs: Vec<String>,
    /// Assertion results emitted by the most recent script evaluation.
    assertions: Vec<ScriptAssertion>,
}

/// Active script execution context.
#[derive(Clone, Copy)]
struct ScriptGlobal {
    /// Current canopy instance.
    canopy: *mut Canopy,
    /// Node used as the command dispatch anchor.
    node_id: NodeId,
}

thread_local! {
    static SCRIPT_GLOBAL: RefCell<Vec<ScriptGlobal>> = const { RefCell::new(Vec::new()) };
}

/// Luau host state shared by the canopy runtime.
#[derive(Clone)]
pub(crate) struct LuauHost {
    /// Lua VM handle.
    lua: Lua,
    /// Shared mutable host state.
    state: Rc<RefCell<LuauState>>,
}

/// Backwards-compatible type alias used throughout the current codebase.
pub(crate) type ScriptHost = LuauHost;

impl fmt::Debug for LuauHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LuauHost").finish_non_exhaustive()
    }
}

/// Prefix scripts with strict mode unless they already declare a mode.
fn strict_source(source: &str) -> String {
    let trimmed = source.trim_start();
    if trimmed.starts_with("--!") {
        source.to_string()
    } else {
        format!("--!strict\n{source}")
    }
}

/// Convert an mlua syntax error into a canopy parse error.
fn format_parse_error(err: LuaError) -> error::ParseError {
    match err {
        LuaError::SyntaxError { message, .. } => error::ParseError::new(message),
        other => error::ParseError::new(other.to_string()),
    }
}

#[cfg(all(feature = "typecheck", not(target_os = "macos")))]
/// Format Luau typecheck diagnostics for display.
fn format_typecheck_diagnostics(result: &luau_analyze::CheckResult) -> String {
    let mut lines = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == luau_analyze::Severity::Error)
        .map(|diagnostic| {
            format!(
                "{}:{}: {}",
                diagnostic.line + 1,
                diagnostic.col + 1,
                diagnostic.message
            )
        })
        .collect::<Vec<_>>();
    if result.timed_out {
        lines.push("type checking timed out".to_string());
    }
    if result.cancelled {
        lines.push("type checking was cancelled".to_string());
    }
    lines.join("\n")
}

/// Convert an mlua error into a canopy script error.
fn lua_to_canopy(err: impl fmt::Display) -> error::Error {
    error::Error::Script(err.to_string())
}

/// Convert raw integer coordinates into a canopy point.
fn point_from_coords(x: i64, y: i64) -> Result<Point> {
    let x = u32::try_from(x)
        .map_err(|_| error::Error::Script(format!("x coordinate must be >= 0, got {x}")))?;
    let y = u32::try_from(y)
        .map_err(|_| error::Error::Script(format!("y coordinate must be >= 0, got {y}")))?;
    Ok(Point { x, y })
}

/// Execute a closure with the current script context.
fn with_script_context<R>(
    canopy: &mut Canopy,
    node_id: NodeId,
    f: impl FnOnce() -> Result<R>,
) -> Result<R> {
    SCRIPT_GLOBAL.with(|stack| {
        stack.borrow_mut().push(ScriptGlobal {
            canopy: canopy as *mut Canopy,
            node_id,
        });
    });
    let result = f();
    SCRIPT_GLOBAL.with(|stack| {
        let _ = stack.borrow_mut().pop();
    });
    result
}

/// Execute a closure with mutable access to the active canopy instance.
fn with_current_canopy<R>(f: impl FnOnce(&mut Canopy, NodeId) -> Result<R>) -> Result<R> {
    SCRIPT_GLOBAL.with(|stack| {
        let current = *stack
            .borrow()
            .last()
            .ok_or_else(|| error::Error::Script("no active script context".into()))?;
        // SAFETY: the pointer is set from a live `&mut Canopy` for the duration of script execution.
        let canopy = unsafe { &mut *current.canopy };
        f(canopy, current.node_id)
    })
}

/// Return true when a Luau callback is currently executing in a canopy context.
fn script_context_active() -> bool {
    SCRIPT_GLOBAL.with(|stack| !stack.borrow().is_empty())
}

/// Convert a stored Lua node userdata into a canopy node id.
fn userdata_to_node_id(value: Value) -> StdResult<NodeId, String> {
    match value {
        Value::UserData(ud) => ud
            .borrow::<ScriptNodeId>()
            .map(|node| node.0)
            .map_err(|err| err.to_string()),
        other => Err(format!(
            "expected NodeId userdata, got {}",
            other.type_name()
        )),
    }
}

/// Convert a node identifier into Luau userdata.
fn node_id_to_lua(lua: &Lua, node_id: NodeId) -> mlua::Result<Value> {
    Ok(Value::UserData(lua.create_userdata(ScriptNodeId(node_id))?))
}

/// Convert a Lua value into a displayable string for diagnostics.
fn lua_value_to_string(value: Value) -> mlua::Result<String> {
    match value {
        Value::Nil => Ok("nil".to_string()),
        other => other.to_string(),
    }
}

/// Build a simple Luau record table.
fn table_with_entries(
    lua: &Lua,
    entries: impl IntoIterator<Item = (&'static str, Value)>,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (key, value) in entries {
        table.set(key, value)?;
    }
    Ok(table)
}

/// Convert a string into a Luau value.
fn string_to_lua(lua: &Lua, value: &str) -> mlua::Result<Value> {
    Ok(Value::String(lua.create_string(value)?))
}

/// Convert a point into a Luau table.
fn point_to_lua(lua: &Lua, point: Point) -> mlua::Result<Value> {
    Ok(Value::Table(table_with_entries(
        lua,
        [
            ("x", Value::Integer(i64::from(point.x))),
            ("y", Value::Integer(i64::from(point.y))),
        ],
    )?))
}

/// Convert a size into a Luau table.
fn size_to_lua(lua: &Lua, size: Size) -> mlua::Result<Value> {
    Ok(Value::Table(table_with_entries(
        lua,
        [
            ("w", Value::Integer(i64::from(size.w))),
            ("h", Value::Integer(i64::from(size.h))),
        ],
    )?))
}

/// Convert a screen rect into a Luau table.
fn rect_to_lua(lua: &Lua, rect: RectI32) -> mlua::Result<Value> {
    Ok(Value::Table(table_with_entries(
        lua,
        [
            ("x", Value::Integer(i64::from(rect.tl.x))),
            ("y", Value::Integer(i64::from(rect.tl.y))),
            ("w", Value::Integer(i64::from(rect.w))),
            ("h", Value::Integer(i64::from(rect.h))),
        ],
    )?))
}

/// Convert a list of node ids into a Luau array.
fn node_list_to_lua(lua: &Lua, nodes: impl IntoIterator<Item = NodeId>) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    for (index, node_id) in nodes.into_iter().enumerate() {
        table.raw_set(index + 1, node_id_to_lua(lua, node_id)?)?;
    }
    Ok(Value::Table(table))
}

/// Convert a node into the `NodeInfo` Luau record.
fn node_info_to_lua(lua: &Lua, canopy: &Canopy, node_id: NodeId) -> Result<Table> {
    let Some(node) = canopy.core.nodes.get(node_id) else {
        return Err(error::Error::NotFound(format!("node {node_id:?}")));
    };
    let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
    let rect = if node.view.outer.w == 0 || node.view.outer.h == 0 {
        Value::Nil
    } else {
        rect_to_lua(lua, node.view.outer).map_err(lua_to_canopy)?
    };
    let content_rect = if node.view.content.w == 0 || node.view.content.h == 0 {
        Value::Nil
    } else {
        rect_to_lua(lua, node.view.content).map_err(lua_to_canopy)?
    };
    let accept_focus = match node.widget.try_borrow() {
        Ok(widget) => match widget.as_ref() {
            Some(widget) => {
                let ctx = CoreViewContext::new(&canopy.core, node_id);
                widget.accept_focus(&ctx)
            }
            None => false,
        },
        Err(_) => false,
    };
    table_with_entries(
        lua,
        [
            ("id", node_id_to_lua(lua, node_id).map_err(lua_to_canopy)?),
            (
                "name",
                Value::String(
                    lua.create_string(node.name().to_string())
                        .map_err(lua_to_canopy)?,
                ),
            ),
            ("focused", Value::Boolean(root_ctx.node_is_focused(node_id))),
            (
                "on_focus_path",
                Value::Boolean(root_ctx.node_is_on_focus_path(node_id)),
            ),
            ("hidden", Value::Boolean(node.hidden())),
            ("visible", Value::Boolean(!node.hidden())),
            (
                "children",
                node_list_to_lua(lua, node.children().iter().copied()).map_err(lua_to_canopy)?,
            ),
            ("rect", rect),
            ("content_rect", content_rect),
            (
                "canvas",
                size_to_lua(lua, node.canvas()).map_err(lua_to_canopy)?,
            ),
            (
                "scroll",
                point_to_lua(lua, node.scroll()).map_err(lua_to_canopy)?,
            ),
            ("accept_focus", Value::Boolean(accept_focus)),
        ],
    )
    .map_err(lua_to_canopy)
}

/// Convert a node into a recursive tree record.
fn tree_node_to_lua(lua: &Lua, canopy: &Canopy, node_id: NodeId) -> Result<Table> {
    let table = node_info_to_lua(lua, canopy, node_id)?;
    let Some(node) = canopy.core.nodes.get(node_id) else {
        return Err(error::Error::NotFound(format!("node {node_id:?}")));
    };
    let children = lua.create_table().map_err(lua_to_canopy)?;
    for (index, child_id) in node.children().iter().copied().enumerate() {
        children
            .raw_set(
                index + 1,
                Value::Table(tree_node_to_lua(lua, canopy, child_id).map_err(lua_to_canopy)?),
            )
            .map_err(lua_to_canopy)?;
    }
    table.set("children", children).map_err(lua_to_canopy)?;
    Ok(table)
}

/// Convert registered fixtures into a Luau array.
fn fixtures_to_lua(lua: &Lua, canopy: &Canopy) -> Result<Value> {
    let fixtures = canopy.fixture_infos();
    let table = lua.create_table().map_err(lua_to_canopy)?;
    for (index, fixture) in fixtures.iter().enumerate() {
        table
            .raw_set(
                index + 1,
                Value::Table(
                    table_with_entries(
                        lua,
                        [
                            (
                                "name",
                                string_to_lua(lua, &fixture.name).map_err(lua_to_canopy)?,
                            ),
                            (
                                "description",
                                string_to_lua(lua, &fixture.description).map_err(lua_to_canopy)?,
                            ),
                        ],
                    )
                    .map_err(lua_to_canopy)?,
                ),
            )
            .map_err(lua_to_canopy)?;
    }
    Ok(Value::Table(table))
}

/// Render a command invocation into a human-readable target string.
fn invocation_target(invocation: &CommandInvocation) -> String {
    let (owner, name) = invocation
        .id
        .0
        .split_once("::")
        .unwrap_or(("", invocation.id.0));
    let callee = if owner.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", luau_global_owner_name(owner), name)
    };
    match &invocation.args {
        CommandArgs::Positional(values) if values.is_empty() => format!("{callee}()"),
        CommandArgs::Named(values) if values.is_empty() => format!("{callee}()"),
        _ => format!("{callee}(...)"),
    }
}

/// Convert a binding target into a discoverable summary string.
fn binding_target_summary(target: &BindingTarget) -> String {
    match target {
        BindingTarget::Script(_) => "script".to_string(),
        BindingTarget::Command(invocation) => invocation_target(invocation),
        BindingTarget::CommandSequence(commands) => {
            format!("[sequence: {} commands]", commands.len())
        }
        BindingTarget::SetInputMode(mode) if mode.is_empty() => "canopy.set_mode(\"\")".to_string(),
        BindingTarget::SetInputMode(mode) => format!("canopy.set_mode({mode:?})"),
        BindingTarget::LuauFunction(_) => "luau".to_string(),
    }
}

/// Extract an optional human-readable binding description.
fn binding_desc(canopy: &Canopy, target: &BindingTarget) -> Option<String> {
    match target {
        BindingTarget::LuauFunction(id) => canopy.script_host.function_label(*id),
        _ => None,
    }
}

/// Convert one binding record into a Luau table.
fn binding_info_to_lua(
    lua: &Lua,
    canopy: &Canopy,
    mode: &str,
    binding: &inputmap::BindingInfo<'_>,
) -> Result<Table> {
    let input_type = match binding.input {
        inputmap::InputSpec::Key(_) => "key",
        inputmap::InputSpec::Mouse(_) => "mouse",
    };
    table_with_entries(
        lua,
        [
            (
                "input",
                string_to_lua(lua, &binding.input.to_string()).map_err(lua_to_canopy)?,
            ),
            (
                "input_type",
                string_to_lua(lua, input_type).map_err(lua_to_canopy)?,
            ),
            ("mode", string_to_lua(lua, mode).map_err(lua_to_canopy)?),
            (
                "path",
                string_to_lua(lua, binding.path_filter).map_err(lua_to_canopy)?,
            ),
            (
                "desc",
                binding_desc(canopy, binding.target)
                    .map(|desc| string_to_lua(lua, &desc))
                    .transpose()
                    .map_err(lua_to_canopy)?
                    .unwrap_or(Value::Nil),
            ),
            (
                "target",
                string_to_lua(lua, &binding_target_summary(binding.target))
                    .map_err(lua_to_canopy)?,
            ),
        ],
    )
    .map_err(lua_to_canopy)
}

/// Convert a command parameter specification into a Luau table.
fn command_param_to_lua(lua: &Lua, param: &commands::CommandParamSpec) -> Result<Table> {
    table_with_entries(
        lua,
        [
            (
                "name",
                string_to_lua(lua, param.name).map_err(lua_to_canopy)?,
            ),
            (
                "kind",
                string_to_lua(
                    lua,
                    match param.kind {
                        commands::CommandParamKind::Injected => "injected",
                        commands::CommandParamKind::User => "user",
                    },
                )
                .map_err(lua_to_canopy)?,
            ),
            (
                "rust_type",
                string_to_lua(lua, param.ty.rust).map_err(lua_to_canopy)?,
            ),
            (
                "luau_type",
                string_to_lua(lua, &defs::rust_type_to_luau(&param.ty)).map_err(lua_to_canopy)?,
            ),
            (
                "doc",
                param
                    .doc
                    .map(|doc| string_to_lua(lua, doc))
                    .transpose()
                    .map_err(lua_to_canopy)?
                    .unwrap_or(Value::Nil),
            ),
            ("optional", Value::Boolean(param.optional)),
            (
                "default",
                param
                    .default
                    .map(|value| string_to_lua(lua, value))
                    .transpose()
                    .map_err(lua_to_canopy)?
                    .unwrap_or(Value::Nil),
            ),
        ],
    )
    .map_err(lua_to_canopy)
}

/// Convert a command specification into a Luau table.
fn command_info_to_lua(lua: &Lua, spec: &CommandSpec) -> Result<Table> {
    let owner = match spec.dispatch {
        commands::CommandDispatchKind::Node { owner } => owner,
        commands::CommandDispatchKind::Free => "",
    };
    let params = lua.create_table().map_err(lua_to_canopy)?;
    for (index, param) in spec.params.iter().enumerate() {
        params
            .raw_set(
                index + 1,
                Value::Table(command_param_to_lua(lua, param).map_err(lua_to_canopy)?),
            )
            .map_err(lua_to_canopy)?;
    }
    table_with_entries(
        lua,
        [
            (
                "name",
                string_to_lua(lua, spec.name).map_err(lua_to_canopy)?,
            ),
            ("owner", string_to_lua(lua, owner).map_err(lua_to_canopy)?),
            (
                "doc",
                spec.doc
                    .long
                    .or(spec.doc.short)
                    .map(|doc| string_to_lua(lua, doc))
                    .transpose()
                    .map_err(lua_to_canopy)?
                    .unwrap_or(Value::Nil),
            ),
            ("params", Value::Table(params)),
        ],
    )
    .map_err(lua_to_canopy)
}

/// Convert the current rendered screen buffer into a Luau table.
fn screen_to_lua(lua: &Lua, canopy: &mut Canopy) -> Result<Value> {
    canopy.refresh_snapshot()?;
    let Some(buffer) = canopy.buf() else {
        return Err(error::Error::Script(
            "screen unavailable before render".into(),
        ));
    };
    let rows = lua.create_table().map_err(lua_to_canopy)?;
    for (row_index, row) in buffer.rows().into_iter().enumerate() {
        let row_table = lua.create_table().map_err(lua_to_canopy)?;
        for (column_index, cell) in row.into_iter().enumerate() {
            row_table
                .raw_set(
                    column_index + 1,
                    string_to_lua(lua, &cell).map_err(lua_to_canopy)?,
                )
                .map_err(lua_to_canopy)?;
        }
        rows.raw_set(row_index + 1, Value::Table(row_table))
            .map_err(lua_to_canopy)?;
    }
    Ok(Value::Table(rows))
}

/// Convert a Lua value into a dynamic command argument.
fn lua_value_to_arg_value(value: Value) -> StdResult<ArgValue, String> {
    match value {
        Value::Nil => Ok(ArgValue::Null),
        Value::Boolean(value) => Ok(ArgValue::Bool(value)),
        Value::Integer(value) => Ok(ArgValue::Int(value)),
        Value::Number(value) => Ok(ArgValue::Float(value)),
        Value::String(value) => Ok(ArgValue::String(
            value.to_str().map_err(|err| err.to_string())?.to_string(),
        )),
        Value::Table(table) => lua_table_to_arg_value(&table),
        other => Err(format!(
            "unsupported script value type: {}",
            other.type_name()
        )),
    }
}

/// Convert a Lua table into an `ArgValue`.
fn lua_table_to_arg_value(table: &Table) -> StdResult<ArgValue, String> {
    let mut indexed = BTreeMap::new();
    let mut named = BTreeMap::new();

    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair.map_err(|err| err.to_string())?;
        match key {
            Value::Integer(index) if index > 0 => {
                indexed.insert(index as usize, lua_value_to_arg_value(value)?);
            }
            Value::String(key) => {
                let key = key.to_str().map_err(|err| err.to_string())?.to_string();
                named.insert(key, lua_value_to_arg_value(value)?);
            }
            other => {
                return Err(format!(
                    "unsupported table key type for command args: {}",
                    other.type_name()
                ));
            }
        }
    }

    if named.is_empty() && !indexed.is_empty() {
        let mut values = Vec::with_capacity(indexed.len());
        for expected in 1..=indexed.len() {
            let value = indexed
                .remove(&expected)
                .ok_or_else(|| "sparse arrays are not supported in command args".to_string())?;
            values.push(value);
        }
        return Ok(ArgValue::Array(values));
    }

    if indexed.is_empty() {
        return Ok(ArgValue::Map(named));
    }

    Err("mixed array/map tables are not supported in command args".into())
}

/// Convert an `ArgValue` back into a Lua value.
fn arg_value_to_lua(lua: &Lua, value: ArgValue) -> mlua::Result<Value> {
    match value {
        ArgValue::Null => Ok(Value::Nil),
        ArgValue::Bool(value) => Ok(Value::Boolean(value)),
        ArgValue::Int(value) => Ok(Value::Integer(value)),
        ArgValue::UInt(value) => match i64::try_from(value) {
            Ok(value) => Ok(Value::Integer(value)),
            Err(_) => Ok(Value::Number(value as f64)),
        },
        ArgValue::Float(value) => Ok(Value::Number(value)),
        ArgValue::String(value) => Ok(Value::String(lua.create_string(&value)?)),
        ArgValue::Array(values) => {
            let table = lua.create_table_with_capacity(values.len(), 0)?;
            for (index, value) in values.into_iter().enumerate() {
                table.raw_set(index + 1, arg_value_to_lua(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
        ArgValue::Map(values) => {
            let table = lua.create_table_with_capacity(0, values.len())?;
            for (key, value) in values {
                table.set(key, arg_value_to_lua(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

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

/// Build command arguments from raw Lua values.
fn build_args_from_values(
    spec: &CommandSpec,
    values: Vec<Value>,
    allow_map_named: bool,
) -> StdResult<CommandArgs, String> {
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        out.push(lua_value_to_arg_value(value)?);
    }
    if allow_map_named && out.len() == 1 {
        let arg = out.pop().expect("single argument checked above");
        if let ArgValue::Map(map) = arg {
            if map_matches_named(spec, &map) {
                return Ok(CommandArgs::Named(map));
            }
            return Ok(CommandArgs::Positional(vec![ArgValue::Map(map)]));
        }
        return Ok(CommandArgs::Positional(vec![arg]));
    }
    Ok(CommandArgs::Positional(out))
}

/// Dispatch a command using the active script context.
fn dispatch_command(
    spec: &'static CommandSpec,
    node_id: NodeId,
    values: Vec<Value>,
    allow_map_named: bool,
) -> mlua::Result<Value> {
    with_current_canopy(|canopy, _| {
        let args = build_args_from_values(spec, values, allow_map_named)
            .map_err(|message| error::Error::Script(format!("command {}: {message}", spec.id.0)))?;
        let invocation = CommandInvocation { id: spec.id, args };
        let value = commands::dispatch(&mut canopy.core, node_id, &invocation)
            .map_err(error::Error::from)?;
        arg_value_to_lua(&canopy.script_host.lua, value).map_err(|err| {
            error::Error::Script(format!(
                "command {} return conversion failed: {err}",
                spec.id.0
            ))
        })
    })
    .map_err(LuaError::external)
}

/// Dispatch a command by id using the current focus-relative context.
fn dispatch_command_by_name(name: &str, values: Vec<Value>) -> mlua::Result<Value> {
    let allow_map_named = values.len() == 1;
    with_current_canopy(|canopy, node_id| {
        let spec = canopy
            .core
            .commands
            .get(name)
            .ok_or_else(|| error::Error::Script(format!("unknown command: {name}")))?;
        dispatch_command(spec, node_id, values, allow_map_named)
            .map_err(|err| error::Error::Script(format!("command {name} failed: {err}")))
    })
    .map_err(LuaError::external)
}

/// Return the Luau-safe global name for a command owner.
pub(crate) fn luau_global_owner_name(owner: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "and", "break", "continue", "do", "else", "elseif", "end", "export", "false", "for",
        "function", "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true",
        "type", "until", "while",
    ];
    if KEYWORDS.contains(&owner) {
        format!("{owner}_cmd")
    } else {
        owner.to_string()
    }
}

/// Parsed options for script-created bindings.
#[derive(Debug, Clone, Default)]
struct ScriptBindOptions {
    /// Optional mode override.
    mode: String,
    /// Optional path filter override.
    path: String,
    /// Optional human-readable description.
    desc: Option<String>,
}

/// Parse `BindOptions` from an optional Lua table.
fn parse_bind_options(options: Option<Table>) -> mlua::Result<ScriptBindOptions> {
    let Some(options) = options else {
        return Ok(ScriptBindOptions::default());
    };
    Ok(ScriptBindOptions {
        mode: options.get::<Option<String>>("mode")?.unwrap_or_default(),
        path: options.get::<Option<String>>("path")?.unwrap_or_default(),
        desc: options.get::<Option<String>>("desc")?,
    })
}

/// Convert a binding id into a Luau number.
fn binding_id_to_lua(id: inputmap::BindingId) -> Value {
    Value::Integer(id.as_u64() as i64)
}

impl LuauHost {
    /// Construct a new Luau host.
    pub fn new() -> Self {
        let root_lua = Lua::new_with(StdLib::ALL_SAFE, LuaOptions::default())
            .expect("constructing Luau VM should not fail");
        // Rust callbacks stored inside the VM capture `LuauHost` clones. If the final
        // `Lua` handle runs `gc_collect()` during drop, Luau can collect those callbacks
        // while GC is already active, and dropping the captured host re-enters `Lua::drop()`
        // and aborts. Keep the runtime through a cloned handle, which disables mlua's
        // GC-on-drop behavior while still allowing the VM itself to be destroyed normally.
        let lua = root_lua.clone();
        drop(root_lua);
        let host = Self {
            lua,
            state: Rc::new(RefCell::new(LuauState {
                next_script_id: 1,
                next_function_id: 1,
                ..LuauState::default()
            })),
        };
        host.register_base_api()
            .expect("registering Luau base API should not fail");
        host
    }

    /// Return true if the API has been finalized.
    pub fn is_finalized(&self) -> bool {
        self.state.borrow().finalized
    }

    #[cfg(all(feature = "typecheck", not(target_os = "macos")))]
    /// Type-check a Luau source string against the finalized canopy API.
    pub fn check_script(&self, source: &str) -> Result<luau_analyze::CheckResult> {
        let definitions = self.state.borrow().definitions.clone().ok_or_else(|| {
            error::Error::InvalidOperation(
                "cannot type-check scripts before finalize_api()".to_string(),
            )
        })?;
        let mut checker = luau_analyze::Checker::new()
            .map_err(|err| error::Error::Script(format!("creating Luau checker failed: {err}")))?;
        checker.add_definitions(&definitions).map_err(|err| {
            error::Error::Script(format!(
                "loading Luau definitions into checker failed: {err}"
            ))
        })?;
        checker
            .check(&strict_source(source))
            .map_err(|err| error::Error::Script(format!("checking Luau script failed: {err}")))
    }

    #[cfg(all(feature = "typecheck", not(target_os = "macos")))]
    /// Enforce Luau type checking for finalized APIs in debug builds.
    fn maybe_typecheck(&self, source: &str) -> Result<()> {
        if !cfg!(debug_assertions) || !self.is_finalized() {
            return Ok(());
        }
        let result = self.check_script(source)?;
        if result.is_ok() {
            Ok(())
        } else {
            Err(error::Error::Parse(error::ParseError::new(
                format_typecheck_diagnostics(&result),
            )))
        }
    }

    /// Skip Luau type checking when the feature is disabled or unsupported on this target.
    #[cfg(not(all(feature = "typecheck", not(target_os = "macos"))))]
    fn maybe_typecheck(&self, _source: &str) -> Result<()> {
        Ok(())
    }

    /// Clear recorded logs and assertions for the next script evaluation.
    fn clear_diagnostics(&self) {
        let mut state = self.state.borrow_mut();
        state.logs.clear();
        state.assertions.clear();
    }

    /// Append a log line to the current evaluation state.
    fn push_log(&self, message: String) {
        self.state.borrow_mut().logs.push(message);
    }

    /// Append an assertion result to the current evaluation state.
    fn push_assertion(&self, passed: bool, message: String) {
        self.state
            .borrow_mut()
            .assertions
            .push(ScriptAssertion { passed, message });
    }

    /// Drain deferred `on_start` hooks in registration order.
    pub fn drain_on_start_hooks(&self) -> Vec<LuauFunctionId> {
        let mut state = self.state.borrow_mut();
        mem::take(&mut state.on_start_hooks)
    }

    /// Return true when deferred `on_start` hooks are pending.
    pub fn has_on_start_hooks(&self) -> bool {
        !self.state.borrow().on_start_hooks.is_empty()
    }

    /// Take the logs collected during the most recent evaluation.
    pub fn take_logs(&self) -> Vec<String> {
        mem::take(&mut self.state.borrow_mut().logs)
    }

    /// Take the assertions collected during the most recent evaluation.
    pub fn take_assertions(&self) -> Vec<ScriptAssertion> {
        mem::take(&mut self.state.borrow_mut().assertions)
    }

    /// Register base canopy globals that are available before finalization.
    fn register_base_api(&self) -> mlua::Result<()> {
        let canopy_table = self.lua.create_table()?;
        let host = self.clone();

        canopy_table.set(
            "cmd",
            self.lua
                .create_function(|_, (name, values): (String, MultiValue)| {
                    dispatch_command_by_name(&name, values.into_vec())
                })?,
        )?;

        canopy_table.set(
            "cmd_on",
            self.lua
                .create_function(|_, (node, name, values): (Value, String, MultiValue)| {
                    let node_id = userdata_to_node_id(node).map_err(LuaError::runtime)?;
                    let allow_map_named = values.len() == 1;
                    with_current_canopy(|canopy, _| {
                        let spec = canopy.core.commands.get(&name).ok_or_else(|| {
                            error::Error::Script(format!("unknown command: {name}"))
                        })?;
                        dispatch_command(spec, node_id, values.into_vec(), allow_map_named).map_err(
                            |err| error::Error::Script(format!("command {name} failed: {err}")),
                        )
                    })
                    .map_err(LuaError::external)
                })?,
        )?;

        canopy_table.set(
            "log",
            self.lua.create_function(move |_, value: Value| {
                let message = lua_value_to_string(value)?;
                tracing::info!("{message}");
                host.push_log(message);
                Ok(())
            })?,
        )?;

        let host = self.clone();
        canopy_table.set(
            "assert",
            self.lua
                .create_function(move |_, (condition, message): (bool, Option<String>)| {
                    let message = message.unwrap_or_else(|| "assertion failed".to_string());
                    host.push_assertion(condition, message.clone());
                    if condition {
                        Ok(())
                    } else {
                        Err(LuaError::runtime(message))
                    }
                })?,
        )?;

        canopy_table.set(
            "root",
            self.lua.create_function(|lua, ()| {
                with_current_canopy(|canopy, _| {
                    node_id_to_lua(lua, canopy.core.root_id()).map_err(lua_to_canopy)
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "focused",
            self.lua.create_function(|lua, ()| {
                with_current_canopy(|canopy, _| {
                    let Some(node_id) = canopy.core.focus_id() else {
                        return Ok(Value::Nil);
                    };
                    node_id_to_lua(lua, node_id).map_err(lua_to_canopy)
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "node_info",
            self.lua.create_function(|_, node: Value| {
                let node_id = userdata_to_node_id(node).map_err(LuaError::runtime)?;
                with_current_canopy(|canopy, _| {
                    node_info_to_lua(&canopy.script_host.lua, canopy, node_id)
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "find_node",
            self.lua.create_function(|lua, pattern: String| {
                with_current_canopy(|canopy, _| {
                    let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
                    let Some(node_id) = root_ctx.find_node(&pattern) else {
                        return Ok(Value::Nil);
                    };
                    node_id_to_lua(lua, node_id).map_err(lua_to_canopy)
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "find_nodes",
            self.lua.create_function(|lua, pattern: String| {
                with_current_canopy(|canopy, _| {
                    let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
                    node_list_to_lua(lua, root_ctx.find_nodes(&pattern)).map_err(lua_to_canopy)
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "parent",
            self.lua.create_function(|lua, node: Value| {
                let node_id = userdata_to_node_id(node).map_err(LuaError::runtime)?;
                with_current_canopy(|canopy, _| {
                    let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
                    match root_ctx.parent_of(node_id) {
                        Some(parent) => node_id_to_lua(lua, parent).map_err(lua_to_canopy),
                        None => Ok(Value::Nil),
                    }
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "children",
            self.lua.create_function(|lua, node: Value| {
                let node_id = userdata_to_node_id(node).map_err(LuaError::runtime)?;
                with_current_canopy(|canopy, _| {
                    let root_ctx = CoreViewContext::new(&canopy.core, canopy.core.root_id());
                    node_list_to_lua(lua, root_ctx.children_of(node_id)).map_err(lua_to_canopy)
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "tree",
            self.lua.create_function(|_, ()| {
                with_current_canopy(|canopy, _| {
                    tree_node_to_lua(&canopy.script_host.lua, canopy, canopy.core.root_id())
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "set_focus",
            self.lua.create_function(|_, node: Value| {
                let node_id = userdata_to_node_id(node).map_err(LuaError::runtime)?;
                with_current_canopy(|canopy, _| {
                    let root_id = canopy.core.root_id();
                    let mut ctx = CoreContext::new(&mut canopy.core, root_id);
                    Ok(ctx.set_focus(node_id))
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "node_at",
            self.lua.create_function(|lua, (x, y): (i64, i64)| {
                with_current_canopy(|canopy, _| {
                    let Some(node_id) = canopy
                        .core
                        .locate_node(canopy.core.root_id(), point_from_coords(x, y)?)?
                    else {
                        return Ok(Value::Nil);
                    };
                    node_id_to_lua(lua, node_id).map_err(lua_to_canopy)
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "focus_next",
            self.lua.create_function(|_, ()| {
                with_current_canopy(|canopy, _| {
                    let root_id = canopy.core.root_id();
                    let mut ctx = CoreContext::new(&mut canopy.core, root_id);
                    ctx.focus_next_global();
                    Ok(())
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "focus_prev",
            self.lua.create_function(|_, ()| {
                with_current_canopy(|canopy, _| {
                    let root_id = canopy.core.root_id();
                    let mut ctx = CoreContext::new(&mut canopy.core, root_id);
                    ctx.focus_prev_global();
                    Ok(())
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "focus_dir",
            self.lua.create_function(|_, dir: String| {
                with_current_canopy(|canopy, _| {
                    let dir = commands::FromArgValue::from_arg_value(&ArgValue::String(dir))
                        .map_err(error::Error::from)?;
                    let root_id = canopy.core.root_id();
                    let mut ctx = CoreContext::new(&mut canopy.core, root_id);
                    ctx.focus_dir_global(dir);
                    Ok(())
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "send_key",
            self.lua.create_function(|_, key_spec: String| {
                with_current_canopy(|canopy, _| {
                    let key = key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?;
                    canopy.key(key)
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "send_click",
            self.lua.create_function(|_, (x, y): (i64, i64)| {
                with_current_canopy(|canopy, _| {
                    let location = point_from_coords(x, y)?;
                    canopy.mouse(mouse::MouseEvent {
                        action: mouse::Action::Down,
                        button: mouse::Button::Left,
                        modifiers: key::Empty,
                        location,
                    })?;
                    canopy.mouse(mouse::MouseEvent {
                        action: mouse::Action::Up,
                        button: mouse::Button::Left,
                        modifiers: key::Empty,
                        location,
                    })
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "send_scroll",
            self.lua
                .create_function(|_, (dir, x, y): (String, i64, i64)| {
                    with_current_canopy(|canopy, _| {
                        let action = if dir.eq_ignore_ascii_case("up") {
                            mouse::Action::ScrollUp
                        } else if dir.eq_ignore_ascii_case("down") {
                            mouse::Action::ScrollDown
                        } else {
                            return Err(error::Error::Script(format!(
                                "unknown scroll direction: {dir}"
                            )));
                        };
                        canopy.mouse(mouse::MouseEvent {
                            action,
                            button: mouse::Button::None,
                            modifiers: key::Empty,
                            location: point_from_coords(x, y)?,
                        })
                    })
                    .map_err(LuaError::external)
                })?,
        )?;

        canopy_table.set(
            "bindings",
            self.lua.create_function(|lua, ()| {
                with_current_canopy(|canopy, _| {
                    let bindings = lua.create_table().map_err(lua_to_canopy)?;
                    for (index, binding) in canopy.keymap.bindings().into_iter().enumerate() {
                        bindings
                            .raw_set(
                                index + 1,
                                Value::Table(
                                    binding_info_to_lua(lua, canopy, binding.mode, &binding.info)
                                        .map_err(lua_to_canopy)?,
                                ),
                            )
                            .map_err(lua_to_canopy)?;
                    }
                    Ok(Value::Table(bindings))
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "commands",
            self.lua.create_function(|lua, ()| {
                with_current_canopy(|canopy, _| {
                    let mut specs = canopy
                        .core
                        .commands
                        .iter()
                        .map(|(_, spec)| spec)
                        .collect::<Vec<_>>();
                    specs.sort_by_key(|spec| spec.id.0);
                    let commands = lua.create_table().map_err(lua_to_canopy)?;
                    for (index, spec) in specs.into_iter().enumerate() {
                        commands
                            .raw_set(
                                index + 1,
                                Value::Table(
                                    command_info_to_lua(lua, spec).map_err(lua_to_canopy)?,
                                ),
                            )
                            .map_err(lua_to_canopy)?;
                    }
                    Ok(Value::Table(commands))
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "input_mode",
            self.lua.create_function(|lua, ()| {
                with_current_canopy(|canopy, _| {
                    string_to_lua(lua, canopy.input_mode()).map_err(lua_to_canopy)
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "set_mode",
            self.lua.create_function(|_, mode: String| {
                with_current_canopy(|canopy, _| {
                    canopy.set_input_mode(&mode)?;
                    Ok(())
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "bind",
            self.lua
                .create_function(move |_, (key_spec, function): (String, Function)| {
                    with_current_canopy(|canopy, _| {
                        let input = inputmap::InputSpec::Key(
                            key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?,
                        );
                        let label = Some("script".to_string());
                        let function_id = canopy.script_host.store_function(function, label)?;
                        let result = canopy.keymap.replace_binding(
                            "",
                            input,
                            "",
                            BindingTarget::LuauFunction(function_id),
                        );
                        match result {
                            Ok((binding_id, removed)) => {
                                canopy.release_removed_bindings(removed);
                                Ok(binding_id_to_lua(binding_id))
                            }
                            Err(err) => {
                                canopy.script_host.release_function(function_id);
                                Err(err)
                            }
                        }
                    })
                    .map_err(LuaError::external)
                })?,
        )?;

        canopy_table.set(
            "bind_with",
            self.lua.create_function(
                move |_, (key_spec, options, function): (String, Table, Function)| {
                    with_current_canopy(|canopy, _| {
                        let options = parse_bind_options(Some(options)).map_err(lua_to_canopy)?;
                        let input = inputmap::InputSpec::Key(
                            key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?,
                        );
                        let label = options.desc.clone().or(Some("script".to_string()));
                        let function_id = canopy.script_host.store_function(function, label)?;
                        let result = canopy.keymap.replace_binding(
                            &options.mode,
                            input,
                            &options.path,
                            BindingTarget::LuauFunction(function_id),
                        );
                        match result {
                            Ok((binding_id, removed)) => {
                                canopy.release_removed_bindings(removed);
                                Ok(binding_id_to_lua(binding_id))
                            }
                            Err(err) => {
                                canopy.script_host.release_function(function_id);
                                Err(err)
                            }
                        }
                    })
                    .map_err(LuaError::external)
                },
            )?,
        )?;

        canopy_table.set(
            "bind_mouse",
            self.lua
                .create_function(move |_, (mouse_spec, function): (String, Function)| {
                    with_current_canopy(|canopy, _| {
                        let input = inputmap::InputSpec::Mouse(
                            mouse::Mouse::parse_spec(&mouse_spec).map_err(error::Error::Script)?,
                        );
                        let function_id = canopy
                            .script_host
                            .store_function(function, Some("script".to_string()))?;
                        let result = canopy.keymap.replace_binding(
                            "",
                            input,
                            "",
                            BindingTarget::LuauFunction(function_id),
                        );
                        match result {
                            Ok((binding_id, removed)) => {
                                canopy.release_removed_bindings(removed);
                                Ok(binding_id_to_lua(binding_id))
                            }
                            Err(err) => {
                                canopy.script_host.release_function(function_id);
                                Err(err)
                            }
                        }
                    })
                    .map_err(LuaError::external)
                })?,
        )?;

        canopy_table.set(
            "bind_mouse_with",
            self.lua.create_function(
                move |_, (mouse_spec, options, function): (String, Table, Function)| {
                    with_current_canopy(|canopy, _| {
                        let options = parse_bind_options(Some(options)).map_err(lua_to_canopy)?;
                        let input = inputmap::InputSpec::Mouse(
                            mouse::Mouse::parse_spec(&mouse_spec).map_err(error::Error::Script)?,
                        );
                        let label = options.desc.clone().or(Some("script".to_string()));
                        let function_id = canopy.script_host.store_function(function, label)?;
                        let result = canopy.keymap.replace_binding(
                            &options.mode,
                            input,
                            &options.path,
                            BindingTarget::LuauFunction(function_id),
                        );
                        match result {
                            Ok((binding_id, removed)) => {
                                canopy.release_removed_bindings(removed);
                                Ok(binding_id_to_lua(binding_id))
                            }
                            Err(err) => {
                                canopy.script_host.release_function(function_id);
                                Err(err)
                            }
                        }
                    })
                    .map_err(LuaError::external)
                },
            )?,
        )?;

        canopy_table.set(
            "unbind",
            self.lua.create_function(|_, id: i64| {
                with_current_canopy(|canopy, _| {
                    Ok(canopy.unbind(inputmap::BindingId::from_u64(id as u64)))
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "unbind_key",
            self.lua
                .create_function(|_, (key_spec, options): (String, Option<Table>)| {
                    with_current_canopy(|canopy, _| {
                        let options = parse_bind_options(options).map_err(lua_to_canopy)?;
                        let mode = (!options.mode.is_empty()).then_some(options.mode.as_str());
                        let path = (!options.path.is_empty()).then_some(options.path.as_str());
                        let key = key::Key::parse_spec(&key_spec).map_err(error::Error::Script)?;
                        let _ = canopy.unbind_key_input(key, mode, path);
                        Ok(())
                    })
                    .map_err(LuaError::external)
                })?,
        )?;

        canopy_table.set(
            "clear_bindings",
            self.lua.create_function(|_, ()| {
                with_current_canopy(|canopy, _| {
                    let _ = canopy.clear_bindings();
                    Ok(())
                })
                .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "screen",
            self.lua.create_function(|lua, ()| {
                with_current_canopy(|canopy, _| screen_to_lua(lua, canopy))
                    .map_err(LuaError::external)
            })?,
        )?;

        canopy_table.set(
            "screen_text",
            self.lua.create_function(|lua, ()| {
                with_current_canopy(|canopy, _| {
                    canopy.refresh_snapshot()?;
                    let Some(buffer) = canopy.buf() else {
                        return Err(error::Error::Script(
                            "screen unavailable before render".into(),
                        ));
                    };
                    string_to_lua(lua, &buffer.screen_text()).map_err(lua_to_canopy)
                })
                .map_err(LuaError::external)
            })?,
        )?;

        let host = self.clone();
        canopy_table.set(
            "on_start",
            self.lua.create_function(move |_, function: Function| {
                let function_id = host
                    .store_function(function, Some("script".to_string()))
                    .map_err(LuaError::external)?;
                host.state.borrow_mut().on_start_hooks.push(function_id);
                Ok(())
            })?,
        )?;

        self.lua.globals().set("canopy", canopy_table)?;
        self.lua.globals().set(
            "fixtures",
            self.lua.create_function(|lua, ()| {
                with_current_canopy(|canopy, _| fixtures_to_lua(lua, canopy))
                    .map_err(LuaError::external)
            })?,
        )?;
        Ok(())
    }

    /// Finalize the command surface and cache rendered definitions.
    pub fn finalize(
        &self,
        commands: &CommandSet,
        default_binding_owners: &BTreeSet<String>,
        definitions: String,
    ) -> Result<()> {
        if self.is_finalized() {
            return Err(error::Error::InvalidOperation(
                "Luau API already finalized".into(),
            ));
        }
        self.register_commands(commands, default_binding_owners)?;
        self.lua
            .sandbox(true)
            .map_err(|err| error::Error::Script(format!("enabling Luau sandbox failed: {err}")))?;
        let mut state = self.state.borrow_mut();
        state.definitions = Some(definitions);
        state.finalized = true;
        Ok(())
    }

    /// Register per-owner command tables in the Lua globals.
    fn register_commands(
        &self,
        commands: &CommandSet,
        default_binding_owners: &BTreeSet<String>,
    ) -> Result<()> {
        let mut owners: HashMap<String, Vec<&'static CommandSpec>> = HashMap::new();
        for (_, spec) in commands.iter() {
            let commands::CommandDispatchKind::Node { owner } = spec.dispatch else {
                continue;
            };
            owners.entry(owner.to_string()).or_default().push(spec);
        }
        for owner in default_binding_owners {
            owners.entry(owner.clone()).or_default();
        }

        let globals = self.lua.globals();
        for (owner, specs) in owners {
            let table = self.lua.create_table().map_err(|err| {
                error::Error::Script(format!("creating Luau table for {owner} failed: {err}"))
            })?;
            for spec in specs {
                let function = self
                    .lua
                    .create_function(move |_, values: MultiValue| {
                        let allow_map_named = values.len() == 1;
                        with_current_canopy(|_, node_id| {
                            dispatch_command(spec, node_id, values.into_vec(), allow_map_named)
                                .map_err(|err| {
                                    error::Error::Script(format!(
                                        "command {} failed: {err}",
                                        spec.id.0
                                    ))
                                })
                        })
                        .map_err(LuaError::external)
                    })
                    .map_err(|err| {
                        error::Error::Script(format!(
                            "registering Luau function for {} failed: {err}",
                            spec.id.0
                        ))
                    })?;
                table.set(spec.name, function).map_err(|err| {
                    error::Error::Script(format!(
                        "installing Luau function for {} failed: {err}",
                        spec.id.0
                    ))
                })?;
            }
            if default_binding_owners.contains(&owner) {
                let owner_name = owner.clone();
                let function = self
                    .lua
                    .create_function(move |_, ()| {
                        with_current_canopy(|canopy, _| {
                            canopy.run_registered_default_bindings(&owner_name)?;
                            Ok(Value::Nil)
                        })
                        .map_err(LuaError::external)
                    })
                    .map_err(|err| {
                        error::Error::Script(format!(
                            "registering default bindings for {owner} failed: {err}"
                        ))
                    })?;
                table.set("default_bindings", function).map_err(|err| {
                    error::Error::Script(format!(
                        "installing default bindings for {owner} failed: {err}"
                    ))
                })?;
            }
            globals
                .set(luau_global_owner_name(&owner), table)
                .map_err(|err| {
                    error::Error::Script(format!(
                        "registering Luau owner table for {owner} failed: {err}"
                    ))
                })?;
        }

        Ok(())
    }

    /// Compile a script and return its id.
    pub fn compile(&self, source: &str) -> Result<ScriptId> {
        self.maybe_typecheck(source)?;
        let function = self
            .lua
            .load(strict_source(source))
            .set_name("canopy")
            .into_function()
            .map_err(|err| error::Error::Parse(format_parse_error(err)))?;
        let mut state = self.state.borrow_mut();
        let id = state.next_script_id;
        state.next_script_id = state.next_script_id.saturating_add(1);
        state.scripts.insert(
            id,
            Script {
                function,
                source: source.to_string(),
            },
        );
        Ok(id)
    }

    /// Execute a compiled script.
    pub fn execute(
        &self,
        canopy: &mut Canopy,
        node_id: impl Into<NodeId>,
        sid: ScriptId,
    ) -> Result<()> {
        self.execute_value(canopy, node_id, sid).map(|_| ())
    }

    /// Execute a compiled script and return its value.
    pub fn execute_value(
        &self,
        canopy: &mut Canopy,
        node_id: impl Into<NodeId>,
        sid: ScriptId,
    ) -> Result<ArgValue> {
        let script = self
            .state
            .borrow()
            .scripts
            .get(&sid)
            .cloned()
            .ok_or_else(|| error::Error::Script(format!("script {sid} not found")))?;
        let node_id = node_id.into();
        self.clear_diagnostics();
        let result = with_script_context(canopy, node_id, || {
            let value = script.function.call::<Value>(()).map_err(|err| {
                error::Error::Script(format!("script {sid} on node {node_id:?} failed: {err}"))
            })?;
            lua_value_to_arg_value(value)
                .map_err(|message| error::Error::Script(format!("script {sid}: {message}")))
        });
        self.flush_released_functions();
        result
    }

    /// Return the source for a cached script.
    pub fn script_source(&self, sid: ScriptId) -> Option<String> {
        self.state
            .borrow()
            .scripts
            .get(&sid)
            .map(|script| script.source().to_string())
    }

    /// Store a Luau closure and return a stable host-side id.
    pub fn store_function(
        &self,
        function: Function,
        label: Option<String>,
    ) -> Result<LuauFunctionId> {
        let key = self
            .lua
            .create_registry_value(function)
            .map_err(|err| error::Error::Script(format!("storing Luau closure failed: {err}")))?;
        let mut state = self.state.borrow_mut();
        let id = LuauFunctionId(state.next_function_id);
        state.next_function_id = state.next_function_id.saturating_add(1);
        state.functions.insert(
            id,
            StoredFunction {
                key,
                label,
                refs: 1,
            },
        );
        Ok(id)
    }

    /// Release a stored function reference.
    pub fn release_function(&self, id: LuauFunctionId) {
        let removed = {
            let mut state = self.state.borrow_mut();
            let Some(function) = state.functions.get_mut(&id) else {
                return;
            };
            function.refs = function.refs.saturating_sub(1);
            if function.refs == 0 {
                if script_context_active() {
                    state.pending_function_sweep = true;
                    None
                } else {
                    state.functions.remove(&id)
                }
            } else {
                None
            }
        };

        if let Some(function) = removed
            && let Err(err) = self.lua.remove_registry_value(function.key)
        {
            tracing::warn!("failed to remove Luau registry value for {id:?}: {err}");
        }
    }

    /// Return the help/debug label for a stored function.
    pub fn function_label(&self, id: LuauFunctionId) -> Option<String> {
        self.state
            .borrow()
            .functions
            .get(&id)
            .and_then(|function| function.label.clone())
    }

    /// Execute a stored Luau closure in the current script context.
    pub fn call_function(
        &self,
        canopy: &mut Canopy,
        node_id: NodeId,
        id: LuauFunctionId,
    ) -> Result<()> {
        let function = {
            let state = self.state.borrow();
            let stored = state
                .functions
                .get(&id)
                .ok_or_else(|| error::Error::Script(format!("Luau function {id:?} not found")))?;
            self.lua
                .registry_value::<Function>(&stored.key)
                .map_err(|err| {
                    error::Error::Script(format!("loading Luau function {id:?} failed: {err}"))
                })?
        };
        let result = with_script_context(canopy, node_id, || {
            function.call::<()>(()).map_err(|err| {
                error::Error::Script(format!("Luau binding on node {node_id:?} failed: {err}"))
            })
        });
        self.flush_released_functions();
        result
    }

    /// Remove any zero-ref closures deferred during active Luau callbacks.
    fn flush_released_functions(&self) {
        let removed = {
            let mut state = self.state.borrow_mut();
            if !state.pending_function_sweep {
                return;
            }
            state.pending_function_sweep = false;
            let to_remove = state
                .functions
                .iter()
                .filter_map(|(id, function)| (function.refs == 0).then_some(*id))
                .collect::<Vec<_>>();
            let mut removed = Vec::with_capacity(to_remove.len());
            for id in to_remove {
                if let Some(function) = state.functions.remove(&id) {
                    removed.push((id, function));
                }
            }
            removed
        };
        for (id, function) in removed {
            if let Err(err) = self.lua.remove_registry_value(function.key) {
                tracing::warn!("failed to remove deferred Luau registry value for {id:?}: {err}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::ttree::{get_state, run_ttree};

    #[test]
    fn tcompile_error_reports_details() {
        let host = ScriptHost::new();
        let err = host.compile("local =").unwrap_err();
        assert!(matches!(err, error::Error::Parse(_)));
    }

    #[test]
    fn texecute() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.finalize_api()?;
            let scr = c.script_host.compile(r#"bb_la.c_leaf()"#)?;
            let host = c.script_host.clone();
            host.execute(c, tree.b_a, scr)?;
            assert_eq!(get_state().path, ["bb_la.c_leaf()"]);
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn truntime_error_returns_script_error() -> Result<()> {
        run_ttree(|c, _, tree| {
            c.finalize_api()?;
            let scr = c.script_host.compile(r#"canopy.assert(false, "boom")"#)?;
            let host = c.script_host.clone();
            let err = host.execute(c, tree.b_a, scr);
            assert!(matches!(err, Err(error::Error::Script(_))));
            Ok(())
        })
    }

    #[cfg(all(feature = "typecheck", not(target_os = "macos")))]
    #[test]
    fn tcheck_script_reports_type_errors() -> Result<()> {
        run_ttree(|c, _, _| {
            c.finalize_api()?;
            let result = c.script_host.check_script("local value: string = 1")?;
            assert!(!result.is_ok());
            assert!(!result.errors().is_empty());
            Ok(())
        })
    }

    #[cfg(all(feature = "typecheck", not(target_os = "macos")))]
    #[test]
    fn tcompile_rejects_type_errors_when_finalized() -> Result<()> {
        run_ttree(|c, _, _| {
            c.finalize_api()?;
            let err = c.script_host.compile("local value: string = 1");
            assert!(matches!(err, Err(error::Error::Parse(_))));
            Ok(())
        })
    }
}
