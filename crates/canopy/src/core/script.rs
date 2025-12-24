use std::{collections::HashMap, result::Result as StdResult};

use rhai::{self, plugin::TypeId};
use scoped_tls::scoped_thread_local;

use crate::{
    NodeId,
    commands::*,
    core::Core,
    error::{self, Result},
    state::NodeName,
};

/// Script identifier.
pub type ScriptId = u64;

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
struct ScriptGlobal<'a> {
    /// Core context handle.
    core: &'a mut Core,
    /// Node identifier for dispatch.
    node_id: NodeId,
}

scoped_thread_local!(static SCRIPT_GLOBAL: *const ());

#[derive(Debug)]
/// Script host that owns the Rhai engine and scripts.
pub(crate) struct ScriptHost {
    /// Rhai engine instance.
    engine: rhai::Engine,
    /// Loaded scripts by ID.
    scripts: HashMap<ScriptId, Script>,
    /// Next script ID.
    current_id: u64,
}

/// Argument list passed to Rhai function handlers.
type FnCallArgs<'a> = [&'a mut rhai::Dynamic];

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

/// This is a re-implementation of the Module::set_raw_fn from rhai. It turns out that set_raw_fn wants to assume that
/// the function is a module, which imposes some internal constraints on the number of arguments.
// Helper function removed - using FuncRegistration API directly instead
impl ScriptHost {
    /// Construct a new script host.
    pub fn new() -> Self {
        let mut engine = rhai::Engine::new();
        engine.on_debug(move |s, src, pos| {
            let src = src.unwrap_or("");
            tracing::debug!("{} [{}:{}]", s, src, pos)
        });
        engine.on_print(move |s| tracing::info!("{}", s));

        Self {
            engine,
            scripts: HashMap::new(),
            current_id: 0,
        }
    }

    /// Load command specs into the script engine.
    pub fn load_commands(&mut self, cmds: &[CommandSpec]) {
        // We can't enable this yet - see:
        //      https://github.com/rhaiscript/rhai/issues/574
        // engine.set_strict_variables(true);
        let mut modules: HashMap<NodeName, rhai::Module> = HashMap::new();
        for i in cmds {
            let m = modules.entry(i.node.clone()).or_default();

            let node = i.node.clone();
            let command = i.command.clone();

            let arg_types = i.args.clone();
            let mut rhai_arg_types = vec![];
            for a in &arg_types {
                match a {
                    ArgTypes::Context => {}
                    ArgTypes::ISize => {
                        rhai_arg_types.push(TypeId::of::<i64>());
                    }
                }
            }

            // For dynamic argument handling, we need to use the module's raw function API
            // Since FuncRegistration doesn't support our use case directly
            let func = move |context: Option<rhai::NativeCallContext>,
                             args: &mut FnCallArgs|
                  -> ScriptResult<rhai::Dynamic> {
                SCRIPT_GLOBAL.with(|ptr| {
                    // SAFETY: `ptr` was created from a pointer to `ScriptGlobal`
                    // which lives for the duration of this closure.
                    let sg = unsafe { &mut *(*ptr as *mut ScriptGlobal) };
                    let core = &mut *sg.core;
                    let node_id = sg.node_id;
                    let command_label = format!("{node}::{command}");
                    let call_pos = context
                        .as_ref()
                        .map(|ctx| ctx.call_position())
                        .unwrap_or(rhai::Position::NONE);

                    let make_error = |message: String| {
                        Box::new(rhai::EvalAltResult::ErrorRuntime(
                            rhai::Dynamic::from(format!(
                                "command {command_label} on node {node_id:?}: {message}"
                            )),
                            call_pos,
                        ))
                    };

                    let mut ciargs = vec![];
                    let mut arg_types = arg_types.clone();
                    if !arg_types.is_empty() && arg_types[0] == ArgTypes::Context {
                        ciargs.push(Args::Context);
                        arg_types.remove(0);
                    }
                    if args.len() != arg_types.len() {
                        return Err(make_error(format!(
                            "expected {} arguments, got {}",
                            arg_types.len(),
                            args.len()
                        )));
                    }
                    for (i, a) in arg_types.iter().enumerate() {
                        match a {
                            ArgTypes::Context => {
                                return Err(make_error(
                                    "unexpected context argument in command signature".into(),
                                ));
                            }
                            ArgTypes::ISize => {
                                let val = args[i].as_int().map_err(|err| {
                                    make_error(format!("argument {i} expected integer: {err}"))
                                })?;
                                ciargs.push(Args::ISize(val as isize));
                            }
                        }
                    }

                    let ci = CommandInvocation {
                        node: node.clone(),
                        command: command.clone(),
                        args: ciargs,
                    };
                    match dispatch(core, sg.node_id, &ci) {
                        Ok(Some(ret)) => Ok(match ret {
                            ReturnValue::Void => rhai::Dynamic::UNIT,
                            ReturnValue::String(s) => rhai::Dynamic::from(s),
                        }),
                        Ok(None) => Ok(rhai::Dynamic::UNIT),
                        Err(err) => Err(make_error(err.to_string())),
                    }
                })
            };

            // Use the deprecated set_fn API since we need dynamic type handling
            // We'll suppress the warning for now
            #[allow(deprecated)]
            m.set_fn(
                i.command.clone(),
                rhai::FnNamespace::Internal,
                rhai::FnAccess::Public,
                None,
                &rhai_arg_types,
                rhai::RhaiFunc::Pure {
                    func: rhai::Shared::new(func),
                    has_context: true,
                    is_pure: true,
                    is_volatile: false,
                },
            );
        }
        for (n, m) in modules {
            self.engine.register_static_module(n.to_string(), m.into());
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
    pub fn execute(&self, core: &mut Core, node_id: NodeId, sid: ScriptId) -> Result<()> {
        let s = self.scripts.get(&sid).ok_or_else(|| {
            error::Error::Script(format!("script {sid} not found for node {node_id:?}"))
        })?;
        let sg = ScriptGlobal { core, node_id };
        let ptr = &sg as *const _ as *const ();
        SCRIPT_GLOBAL.set(&ptr, || {
            self.engine.run_ast(&s.ast).map_err(|e| {
                let location = format_position(e.position());
                error::Error::Script(format!(
                    "script {sid} on node {node_id:?} failed{location}: {e}"
                ))
            })
        })?;
        Ok(())
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
            let scr = c.script_host.compile("bb_la::c_leaf()")?;
            c.run_script(tree.b_a, scr)?;
            assert_eq!(get_state().path, ["bb_la.c_leaf()"]);
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn truntime_error_returns_script_error() -> Result<()> {
        run_ttree(|c, _, tree| {
            let scr = c.script_host.compile("nope::missing()")?;
            let err = c.run_script(tree.b_a, scr);
            assert!(matches!(err, Err(error::Error::Script(_))));
            Ok(())
        })?;
        Ok(())
    }
}
