use std::env;
use std::fs;

use canopy::{
    backend::crossterm::runloop,
    derive_commands,
    event::{key, mouse},
    widgets::{frame, Editor},
    *,
};

#[derive(StatefulNode)]
struct Ed {
    state: NodeState,
    child: frame::Frame<Editor>,
}

#[derive_commands]
impl Ed {
    fn new(contents: String) -> Self {
        Ed {
            state: NodeState::default(),
            child: frame::Frame::new(Editor::new(&contents)),
        }
    }
}

impl Node for Ed {
    fn render(&mut self, _c: &dyn Core, _: &mut Render) -> Result<()> {
        let vp = self.vp();
        fit(&mut self.child, vp)
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }
}

impl Loader for Ed {
    fn load(c: &mut Canopy) {
        c.add_commands::<Ed>();
        c.add_commands::<Editor>();
    }
}

pub fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: pager filename");
    } else {
        let mut cnpy = Canopy::new();
        Root::<Ed>::load(&mut cnpy);

        canopy::Binder::new(&mut cnpy)
            .defaults::<Root<Ed>>()
            .with_path("ed/")
            .key(key::KeyCode::Tab, "root::focus_next()")
            .key('p', "print(\"xxxx\")");

        let contents = fs::read_to_string(args[1].clone())?;
        runloop(cnpy, Root::new(Ed::new(contents)).with_inspector(false))?;
    }
    Ok(())
}
