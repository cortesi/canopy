use std::{cell::RefCell, collections::HashMap};

use rhai;

use crate::{commands, error, Canopy, Node, NodeName, Result};

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
    cnpy: &'a mut Canopy,
    root: &'a mut dyn Node,
}

thread_local! {
    static SCRIPT_GLOBAL: RefCell<Option<ScriptGlobal<'static>>> = RefCell::new(None);
}

struct ScriptGuard {}

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

pub struct ScriptHost {
    engine: rhai::Engine,
}

impl ScriptHost {
    pub fn new(cmds: &commands::CommandSet) -> Result<Self> {
        let mut engine = rhai::Engine::new();
        // We can't enable this yet - see:
        //      https://github.com/rhaiscript/rhai/issues/574
        // engine.set_strict_variables(true);
        let mut modules: HashMap<NodeName, rhai::Module> = HashMap::new();
        for i in cmds.commands.values() {
            if !modules.contains_key(&i.node) {
                let m = rhai::Module::new();
                modules.insert(i.node.clone(), m);
            }
            let m = modules.get_mut(&i.node).unwrap();
            m.set_raw_fn(
                i.command.to_string(),
                rhai::FnNamespace::Internal,
                rhai::FnAccess::Public,
                &[],
                move |context, args| {
                    SCRIPT_GLOBAL.with(|g| {
                        let mut b = g.borrow_mut();
                        let v = b.as_mut().unwrap();

                        println!("{:?} {:?}", context, args);
                    });
                    Ok(())
                },
            );
        }
        for (n, m) in modules {
            engine.register_static_module(n.to_string(), m.into());
        }
        Ok(ScriptHost { engine })
    }

    pub fn compile(&self, source: &str) -> Result<Script> {
        let ast = self
            .engine
            .compile(source)
            .map_err(|e| error::Error::Parse(error::ParseError {}))?;
        Ok(Script {
            ast,
            source: source.into(),
        })
    }

    pub fn execute(&self, cnpy: &mut Canopy, root: &mut dyn Node, s: &Script) -> Result<()> {
        let sg = ScriptGlobal { cnpy, root };
        let _g = ScriptGuard {};
        SCRIPT_GLOBAL.with(|g| {
            *g.borrow_mut() = Some(unsafe { extend_lifetime(sg) });
        });

        self.engine
            .run_ast(&s.ast)
            .map_err(|e| error::Error::Script(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tutils::utils::*;

    #[test]
    fn texecute() -> Result<()> {
        run(|c, _, mut root| {
            let se = ScriptHost::new(&c.commands)?;
            let scr = se.compile("t_leaf::c_leaf()")?;
            se.execute(c, &mut root, &scr)?;
            Ok(())
        })?;
        Ok(())
    }
}
