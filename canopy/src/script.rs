use rhai;

use crate::{commands, error, Canopy, Node, Result};

pub struct Script {
    ast: rhai::AST,
}

pub struct ScriptHost {
    engine: rhai::Engine,
}

impl ScriptHost {
    pub fn new(&self, cmds: &commands::CommandSet) -> Result<Self> {
        let engine = rhai::Engine::new();
        Ok(ScriptHost { engine })
    }

    pub fn compile(&self, s: &str) -> Result<Script> {
        let ast = self
            .engine
            .compile(s)
            .map_err(|e| error::Error::Parse(error::ParseError {}))?;
        Ok(Script { ast })
    }

    pub fn execute(&self, cpy: &mut Canopy, root: &mut dyn Node, s: &Script) -> Result<()> {
        self.engine
            .run_ast(&s.ast)
            .map_err(|e| error::Error::Script(e.to_string()))?;
        Ok(())
    }
}
