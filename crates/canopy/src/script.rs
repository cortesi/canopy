use scoped_tls::scoped_thread_local;
use std::collections::HashMap;

use rhai;

use canopy_core::{Context, Node, NodeId, NodeName, Result, commands::*, error};

pub type ScriptId = u64;

#[derive(Debug, Clone)]
pub struct Script {
    ast: rhai::AST,
    source: String,
}

impl Script {
    pub fn source(&self) -> &str {
        &self.source
    }
}

struct ScriptGlobal<'a> {
    core: &'a mut dyn Context,
    root: &'a mut dyn Node,
    node_id: NodeId,
}

scoped_thread_local!(static SCRIPT_GLOBAL: *const ());

#[derive(Debug)]
pub(crate) struct ScriptHost {
    engine: rhai::Engine,
    scripts: HashMap<ScriptId, Script>,
    current_id: u64,
}

type FnCallArgs<'a> = [&'a mut rhai::Dynamic];

type ScriptResult<T> = std::result::Result<T, Box<rhai::EvalAltResult>>;

/// This is a re-implementation of the Module::set_raw_fn from rhai. It turns out that set_raw_fn wants to assume that
/// the function is a module, which imposes some internal constraints on the number of arguments.
// Helper function removed - using FuncRegistration API directly instead
impl ScriptHost {
    pub fn new() -> Self {
        let mut engine = rhai::Engine::new();
        engine.on_debug(move |s, src, pos| {
            let src = src.unwrap_or("");
            tracing::debug!("{} [{}:{}]", s, src, pos)
        });
        engine.on_print(move |s| tracing::info!("{}", s));

        ScriptHost {
            engine,
            scripts: HashMap::new(),
            current_id: 0,
        }
    }

    pub fn load_commands(&mut self, cmds: &[CommandSpec]) {
        // We can't enable this yet - see:
        //      https://github.com/rhaiscript/rhai/issues/574
        // engine.set_strict_variables(true);
        let mut modules: HashMap<NodeName, rhai::Module> = HashMap::new();
        for i in cmds {
            if !modules.contains_key(&i.node) {
                let m = rhai::Module::new();
                modules.insert(i.node.clone(), m);
            }
            let m = modules.get_mut(&i.node).unwrap();

            let node = i.node.clone();
            let command = i.command.clone();

            let arg_types = i.args.clone();
            let mut rhai_arg_types = vec![];
            for a in &arg_types {
                match a {
                    ArgTypes::Context => {}
                    ArgTypes::ISize => {
                        rhai_arg_types.push(rhai::plugin::TypeId::of::<i64>());
                    }
                }
            }

            // For dynamic argument handling, we need to use the module's raw function API
            // Since FuncRegistration doesn't support our use case directly
            let func = move |_context: Option<rhai::NativeCallContext>,
                             args: &mut FnCallArgs|
                  -> ScriptResult<rhai::Dynamic> {
                SCRIPT_GLOBAL.with(|ptr| {
                    // SAFETY: `ptr` was created from a pointer to `ScriptGlobal`
                    // which lives for the duration of this closure.
                    let sg = unsafe { &mut *(*ptr as *mut ScriptGlobal) };
                    let core = &mut *sg.core;
                    let root = &mut *sg.root;

                    let mut ciargs = vec![];
                    let mut arg_types = arg_types.clone();
                    if !arg_types.is_empty() && arg_types[0] == ArgTypes::Context {
                        ciargs.push(Args::Context);
                        arg_types.remove(0);
                    }
                    // I believe this is guaranteed by rhai
                    assert!(args.len() == arg_types.len());
                    for (i, a) in arg_types.iter().enumerate() {
                        match a {
                            ArgTypes::Context => {
                                panic!("unexpected")
                            }
                            ArgTypes::ISize => {
                                // The type here should be guaranteed by rhai
                                ciargs.push(Args::ISize(args[i].as_int().unwrap() as isize));
                            }
                        }
                    }

                    let ci = CommandInvocation {
                        node: node.clone(),
                        command: command.clone(),
                        args: ciargs,
                    };
                    if let Some(ret) = dispatch(core, sg.node_id.clone(), root, &ci).unwrap() {
                        Ok(match ret {
                            ReturnValue::Void => rhai::Dynamic::UNIT,
                            ReturnValue::String(s) => rhai::Dynamic::from(s),
                        })
                    } else {
                        Ok(rhai::Dynamic::UNIT)
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
            .map_err(|_e| error::Error::Parse(error::ParseError {}))?;
        let s = Script {
            ast,
            source: source.into(),
        };
        self.scripts.insert(self.current_id, s);
        Ok(self.current_id)
    }

    pub fn execute(
        &mut self,
        core: &mut dyn Context,
        root: &mut dyn Node,
        node_id: NodeId,
        sid: ScriptId,
    ) -> Result<()> {
        let s = self.scripts.get(&sid).unwrap();
        let sg = ScriptGlobal {
            core,
            root,
            node_id,
        };
        let ptr = &sg as *const _ as *const ();
        SCRIPT_GLOBAL.set(&ptr, || {
            self.engine
                .run_ast(&s.ast)
                .map_err(|e| error::Error::Script(e.to_string()))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StatefulNode;
    use crate::tutils::*;

    #[test]
    fn texecute() -> Result<()> {
        run(|c, _, mut root| {
            let scr = c.script_host.compile("bb_la::c_leaf()")?;
            let id = root.a.a.id();
            c.run_script(&mut root, id, scr)?;
            assert_eq!(get_state().path, ["bb_la.c_leaf()"]);
            Ok(())
        })?;
        Ok(())
    }
}
