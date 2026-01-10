use std::{
    any::TypeId,
    collections::{BTreeMap, HashMap, HashSet},
    result::Result as StdResult,
};

use rhai::{self, FuncRegistration, ImmutableString, NativeCallContext};
use scoped_tls::scoped_thread_local;

use crate::{
    NodeId,
    commands::{self, ArgValue, CommandArgs, CommandInvocation, CommandSpec},
    core::Core,
    error::{self, Result},
};

/// Script identifier.
pub type ScriptId = u64;

/// Maximum positional arity supported by the Rhai command helpers.
const MAX_CMD_ARITY: usize = 8;

/// Compiled script with source text.
#[derive(Debug, Clone)]
pub struct Script {
    /// Compiled AST.
    ast: rhai::AST,
    /// Original source text.
    source: String,
}

impl Script {
    /// Return the script source text.
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// Script execution context shared via thread-local pointer.
struct ScriptGlobal {
    /// Core context handle.
    core: *mut Core,
    /// Node identifier for dispatch.
    node_id: NodeId,
}

scoped_thread_local!(static SCRIPT_GLOBAL: ScriptGlobal);

#[derive(Debug)]
/// Script host that owns the Rhai engine and scripts.
pub(crate) struct ScriptHost {
    /// Rhai engine instance.
    engine: rhai::Engine,
    /// Loaded scripts by ID.
    scripts: HashMap<ScriptId, Script>,
    /// Next script ID.
    current_id: u64,
    /// Registered command modules by owner name.
    modules: HashMap<String, rhai::Module>,
}

/// Result type for script execution helpers.
type ScriptResult<T> = StdResult<T, Box<rhai::EvalAltResult>>;

/// Format a Rhai position for error messages.
fn format_position(pos: rhai::Position) -> String {
    let line = pos.line();
    let offset = pos.position();
    match (line, offset) {
        (Some(line), Some(offset)) => format!(" (line {line}, offset {offset})"),
        (Some(line), None) => format!(" (line {line})"),
        (None, Some(offset)) => format!(" (offset {offset})"),
        (None, None) => String::new(),
    }
}

/// Convert a Rhai parse error into a Canopy parse error.
fn format_parse_error(err: &rhai::ParseError) -> error::ParseError {
    let pos = err.position();
    error::ParseError::with_position(err.err_type().to_string(), pos.line(), pos.position())
}

/// Build a runtime error with source position.
fn script_error(message: String, pos: rhai::Position) -> Box<rhai::EvalAltResult> {
    Box::new(rhai::EvalAltResult::ErrorRuntime(
        rhai::Dynamic::from(message),
        pos,
    ))
}

/// Convert a Rhai value into an ArgValue.
fn dynamic_to_arg_value(value: &rhai::Dynamic) -> StdResult<ArgValue, String> {
    if value.is_unit() {
        return Ok(ArgValue::Null);
    }
    if value.is::<bool>() {
        return Ok(ArgValue::Bool(value.clone_cast::<bool>()));
    }
    if value.is::<rhai::INT>() {
        return Ok(ArgValue::Int(value.clone_cast::<rhai::INT>()));
    }
    if value.is::<rhai::FLOAT>() {
        return Ok(ArgValue::Float(value.clone_cast::<rhai::FLOAT>()));
    }
    if value.is::<ImmutableString>() {
        let s = value.clone_cast::<ImmutableString>().to_string();
        return Ok(ArgValue::String(s));
    }
    if value.is::<rhai::Array>() {
        let array = value.clone_cast::<rhai::Array>();
        let mut out = Vec::with_capacity(array.len());
        for item in &array {
            out.push(dynamic_to_arg_value(item)?);
        }
        return Ok(ArgValue::Array(out));
    }
    if value.is::<rhai::Map>() {
        let map = value.clone_cast::<rhai::Map>();
        let mut out = BTreeMap::new();
        for (key, value) in map {
            out.insert(key.to_string(), dynamic_to_arg_value(&value)?);
        }
        return Ok(ArgValue::Map(out));
    }
    Err(format!(
        "unsupported script value type: {}",
        value.type_name()
    ))
}

/// Convert an ArgValue into a Rhai value.
fn arg_value_to_dynamic(value: ArgValue) -> rhai::Dynamic {
    match value {
        ArgValue::Null => rhai::Dynamic::UNIT,
        ArgValue::Bool(value) => rhai::Dynamic::from(value),
        ArgValue::Int(value) => rhai::Dynamic::from(value),
        ArgValue::UInt(value) => {
            if let Ok(int_value) = rhai::INT::try_from(value) {
                rhai::Dynamic::from(int_value)
            } else {
                rhai::Dynamic::from(value as rhai::FLOAT)
            }
        }
        ArgValue::Float(value) => rhai::Dynamic::from(value),
        ArgValue::String(value) => rhai::Dynamic::from(value),
        ArgValue::Array(values) => {
            let array: rhai::Array = values.into_iter().map(arg_value_to_dynamic).collect();
            rhai::Dynamic::from(array)
        }
        ArgValue::Map(values) => {
            let mut map = rhai::Map::new();
            for (key, value) in values {
                map.insert(key.into(), arg_value_to_dynamic(value));
            }
            rhai::Dynamic::from(map)
        }
    }
}

/// Build command arguments from raw Rhai values.
fn build_args_from_values(
    spec: &CommandSpec,
    values: Vec<rhai::Dynamic>,
    allow_map_named: bool,
) -> StdResult<CommandArgs, String> {
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        out.push(dynamic_to_arg_value(&value)?);
    }
    if allow_map_named && out.len() == 1 {
        let arg = out.pop().unwrap();
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

/// Determine whether a map matches a command's named parameters.
fn map_matches_named(spec: &CommandSpec, map: &BTreeMap<String, ArgValue>) -> bool {
    if map.is_empty() {
        return false;
    }
    let allowed: HashSet<String> = spec
        .params
        .iter()
        .filter(|param| param.kind == commands::CommandParamKind::User)
        .map(|param| commands::normalize_key(param.name))
        .collect();
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

/// Dispatch a command and map errors to Rhai runtime errors.
fn dispatch_with<F>(
    ctx: NativeCallContext,
    name: &str,
    build_args: F,
) -> ScriptResult<rhai::Dynamic>
where
    F: FnOnce(&'static CommandSpec) -> StdResult<CommandArgs, String>,
{
    let call_pos = ctx.call_position();
    let _ctx = ctx;
    SCRIPT_GLOBAL.with(|sg| {
        // SAFETY: `sg.core` is set from `&mut Core` for the duration of execute.
        let core = unsafe { &mut *sg.core };
        let node_id = sg.node_id;
        let spec = core.commands.get(name).ok_or_else(|| {
            script_error(
                format!("command {name} on node {node_id:?}: unknown command"),
                call_pos,
            )
        })?;
        let args = build_args(spec).map_err(|message| {
            script_error(
                format!("command {name} on node {node_id:?}: {message}"),
                call_pos,
            )
        })?;
        let invocation = CommandInvocation { id: spec.id, args };
        commands::dispatch(core, node_id, &invocation)
            .map(arg_value_to_dynamic)
            .map_err(|err| {
                script_error(
                    format!("command {name} on node {node_id:?}: {err}"),
                    call_pos,
                )
            })
    })
}

/// Dispatch a command by spec and map errors to Rhai runtime errors.
fn dispatch_with_spec<F>(
    ctx: NativeCallContext,
    spec: &'static CommandSpec,
    build_args: F,
) -> ScriptResult<rhai::Dynamic>
where
    F: FnOnce(&'static CommandSpec) -> StdResult<CommandArgs, String>,
{
    let call_pos = ctx.call_position();
    let _ctx = ctx;
    SCRIPT_GLOBAL.with(|sg| {
        let core = unsafe { &mut *sg.core };
        let node_id = sg.node_id;
        let args = build_args(spec).map_err(|message| {
            script_error(
                format!("command {} on node {node_id:?}: {message}", spec.id.0),
                call_pos,
            )
        })?;
        let invocation = CommandInvocation { id: spec.id, args };
        commands::dispatch(core, node_id, &invocation)
            .map(arg_value_to_dynamic)
            .map_err(|err| {
                script_error(
                    format!("command {} on node {node_id:?}: {err}", spec.id.0),
                    call_pos,
                )
            })
    })
}

/// Dispatch a positional command call from Rhai arguments.
fn cmd_positional(
    ctx: NativeCallContext,
    name: &str,
    args: Vec<rhai::Dynamic>,
    allow_map_named: bool,
) -> ScriptResult<rhai::Dynamic> {
    dispatch_with(ctx, name, |spec| {
        build_args_from_values(spec, args, allow_map_named)
    })
}

/// Dispatch a positional command call to a specific command spec.
fn cmd_positional_spec(
    ctx: NativeCallContext,
    spec: &'static CommandSpec,
    args: Vec<rhai::Dynamic>,
    allow_map_named: bool,
) -> ScriptResult<rhai::Dynamic> {
    dispatch_with_spec(ctx, spec, |spec| {
        build_args_from_values(spec, args, allow_map_named)
    })
}

/// Register `cmd` overloads with arities up to MAX_CMD_ARITY.
fn register_cmd_overloads(engine: &mut rhai::Engine) {
    for arity in 0..=MAX_CMD_ARITY {
        let mut arg_types = Vec::with_capacity(arity + 1);
        arg_types.push(TypeId::of::<ImmutableString>());
        arg_types.extend((0..arity).map(|_| TypeId::of::<rhai::Dynamic>()));
        engine.register_raw_fn("cmd", arg_types, move |ctx, args| {
            let name = args[0].clone_cast::<ImmutableString>();
            let mut values = Vec::with_capacity(args.len().saturating_sub(1));
            for arg in args.iter_mut().skip(1) {
                values.push(arg.take());
            }
            let allow_map_named = args.len() == 2;
            cmd_positional(ctx, name.as_str(), values, allow_map_named)
        });
    }
}

/// Dispatch a positional command from an argument array.
fn cmdv(ctx: NativeCallContext, name: &str, values: rhai::Array) -> ScriptResult<rhai::Dynamic> {
    dispatch_with(ctx, name, |_| {
        let mut out = Vec::with_capacity(values.len());
        for value in values {
            out.push(dynamic_to_arg_value(&value)?);
        }
        Ok(CommandArgs::Positional(out))
    })
}

/// Dispatch a command with named arguments.
fn cmd_named(ctx: NativeCallContext, name: &str, values: rhai::Map) -> ScriptResult<rhai::Dynamic> {
    dispatch_with(ctx, name, |_| {
        let mut out = BTreeMap::new();
        for (key, value) in values {
            out.insert(key.to_string(), dynamic_to_arg_value(&value)?);
        }
        Ok(CommandArgs::Named(out))
    })
}

/// Dispatch a command with a single positional argument.
fn cmd_pos(
    ctx: NativeCallContext,
    name: &str,
    value: rhai::Dynamic,
) -> ScriptResult<rhai::Dynamic> {
    cmd_positional(ctx, name, vec![value], false)
}

/// Register command helpers in the Rhai engine.
fn register_command_api(engine: &mut rhai::Engine) {
    register_cmd_overloads(engine);
    engine.register_fn("cmdv", cmdv);
    engine.register_fn("cmd_named", cmd_named);
    engine.register_fn("cmd_pos", cmd_pos);
}

/// Register a direct command overload for a specific arity.
macro_rules! register_direct_arity {
    ($module:expr, $spec:expr, $name:expr, $allow_map_named:expr $(, $arg:ident)*) => {{
        let spec = $spec;
        FuncRegistration::new($name)
            .in_global_namespace()
            .with_purity(true)
            .with_volatility(true)
            .set_into_module($module, move |ctx: NativeCallContext, $($arg: rhai::Dynamic),*| {
                let args = vec![$($arg),*];
                cmd_positional_spec(ctx, spec, args, $allow_map_named)
            });
    }};
}

/// Register direct command calls in a module.
fn register_direct_command(module: &mut rhai::Module, spec: &'static CommandSpec, name: &str) {
    register_direct_arity!(module, spec, name, false);
    register_direct_arity!(module, spec, name, true, a0);
    register_direct_arity!(module, spec, name, false, a0, a1);
    register_direct_arity!(module, spec, name, false, a0, a1, a2);
    register_direct_arity!(module, spec, name, false, a0, a1, a2, a3);
    register_direct_arity!(module, spec, name, false, a0, a1, a2, a3, a4);
    register_direct_arity!(module, spec, name, false, a0, a1, a2, a3, a4, a5);
    register_direct_arity!(module, spec, name, false, a0, a1, a2, a3, a4, a5, a6);
    register_direct_arity!(module, spec, name, false, a0, a1, a2, a3, a4, a5, a6, a7);
}

impl ScriptHost {
    /// Construct a new script host.
    pub fn new() -> Self {
        let mut engine = rhai::Engine::new();
        engine.on_debug(move |s, src, pos| {
            let src = src.unwrap_or("");
            tracing::debug!("{s} [{src}:{pos}]");
        });
        engine.on_print(move |s| tracing::info!("{s}"));
        register_command_api(&mut engine);

        Self {
            engine,
            scripts: HashMap::new(),
            current_id: 0,
            modules: HashMap::new(),
        }
    }

    /// Register command specs for direct script invocation.
    pub fn register_commands(&mut self, specs: &'static [&'static CommandSpec]) {
        let mut touched = HashSet::new();
        for spec in specs {
            let Some((owner, name)) = spec.id.0.split_once("::") else {
                continue;
            };
            let owner = owner.to_string();
            let module = self.modules.entry(owner.clone()).or_default();
            register_direct_command(module, spec, name);
            touched.insert(owner);
        }
        for owner in touched {
            if let Some(module) = self.modules.get(&owner) {
                self.engine
                    .register_static_module(owner, module.clone().into());
            }
        }
    }

    /// Compile a script and store it. Returns a ScriptId that can be used to
    /// execute the script later.
    pub fn compile(&mut self, source: &str) -> Result<ScriptId> {
        self.current_id += 1;
        let ast = self
            .engine
            .compile(source)
            .map_err(|err| error::Error::Parse(format_parse_error(&err)))?;
        let s = Script {
            ast,
            source: source.into(),
        };
        self.scripts.insert(self.current_id, s);
        Ok(self.current_id)
    }

    /// Execute a script by id for the given node.
    pub fn execute(
        &self,
        core: &mut Core,
        node_id: impl Into<NodeId>,
        sid: ScriptId,
    ) -> Result<()> {
        self.execute_value(core, node_id, sid).map(|_| ())
    }

    /// Execute a script by id for the given node and return its value.
    pub fn execute_value(
        &self,
        core: &mut Core,
        node_id: impl Into<NodeId>,
        sid: ScriptId,
    ) -> Result<rhai::Dynamic> {
        let node_id = node_id.into();
        let s = self.scripts.get(&sid).ok_or_else(|| {
            error::Error::Script(format!("script {sid} not found for node {node_id:?}"))
        })?;
        let sg = ScriptGlobal {
            core: core as *mut Core,
            node_id,
        };
        SCRIPT_GLOBAL.set(&sg, || {
            self.engine.eval_ast::<rhai::Dynamic>(&s.ast).map_err(|e| {
                let location = format_position(e.position());
                error::Error::Script(format!(
                    "script {sid} on node {node_id:?} failed{location}: {e}"
                ))
            })
        })
    }

    /// Get the source text for a script by its id.
    pub fn script_source(&self, sid: ScriptId) -> Option<&str> {
        self.scripts.get(&sid).map(|s| s.source())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::ttree::{get_state, run_ttree};

    #[test]
    fn tcompile_error_reports_details() {
        let mut host = ScriptHost::new();
        let err = host.compile("let =").unwrap_err();
        assert!(matches!(err, error::Error::Parse(_)));
    }

    #[test]
    fn texecute() -> Result<()> {
        run_ttree(|c, _, tree| {
            let scr = c.script_host.compile(r#"bb_la::c_leaf()"#)?;
            c.run_script(tree.b_a, scr)?;
            assert_eq!(get_state().path, ["bb_la.c_leaf()"]);
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn truntime_error_returns_script_error() -> Result<()> {
        run_ttree(|c, _, tree| {
            let scr = c.script_host.compile(r#"nope::missing()"#)?;
            let err = c.run_script(tree.b_a, scr);
            assert!(matches!(err, Err(error::Error::Script(_))));
            Ok(())
        })?;
        Ok(())
    }
}
