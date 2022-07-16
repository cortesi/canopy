use std::{cell::RefCell, collections::HashMap};

use rhai;

use crate::{commands, error, Core, Node, NodeId, NodeName, Result};

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
    core: &'a mut dyn Core,
    root: &'a mut dyn Node,
    node_id: NodeId,
}

thread_local! {
    static SCRIPT_GLOBAL: RefCell<Option<ScriptGlobal<'static>>> = RefCell::new(None);
}

pub(crate) struct ScriptGuard {}

impl ScriptGuard {
    pub fn new(core: &mut dyn Core, root: &mut dyn Node, node_id: NodeId) -> Self {
        let sg = ScriptGlobal {
            core,
            root,
            node_id,
        };
        SCRIPT_GLOBAL.with(|g| {
            *g.borrow_mut() = Some(unsafe { extend_lifetime(sg) });
        });
        ScriptGuard {}
    }
}

impl Drop for ScriptGuard {
    fn drop(&mut self) {
        SCRIPT_GLOBAL.with(|g| {
            *g.borrow_mut() = None;
        });
    }
}

unsafe fn extend_lifetime<'b>(r: ScriptGlobal<'b>) -> ScriptGlobal<'static> {
    std::mem::transmute::<ScriptGlobal<'b>, ScriptGlobal<'static>>(r)
}

#[derive(Debug)]
pub(crate) struct ScriptHost {
    engine: rhai::Engine,
    scripts: HashMap<ScriptId, Script>,
    current_id: u64,
}

impl ScriptHost {
    pub fn new() -> Self {
        ScriptHost {
            engine: rhai::Engine::new(),
            scripts: HashMap::new(),
            current_id: 0,
        }
    }

    pub fn load_commands(&mut self, cmds: &[commands::CommandDefinition]) {
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
            let ci = commands::CommandInvocation {
                node: i.node.clone(),
                command: i.command.clone(),
            };
            m.set_raw_fn(
                i.command.to_string(),
                rhai::FnNamespace::Internal,
                rhai::FnAccess::Public,
                &[],
                move |_context, _args| {
                    SCRIPT_GLOBAL.with(|g| {
                        let mut b = g.borrow_mut();
                        let v = b.as_mut().unwrap();
                        commands::dispatch(v.core, v.node_id, v.root, &ci).unwrap();
                        Ok(rhai::Dynamic::UNIT)
                    })
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

    pub fn execute(&self, sid: ScriptId) -> Result<()> {
        let s = self.scripts.get(&sid).unwrap();
        self.engine
            .run_ast(&s.ast)
            .map_err(|e| error::Error::Script(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tutils::*;
    use crate::StatefulNode;

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
