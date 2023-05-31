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
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn render(&mut self, _c: &dyn Core, _: &mut Render) -> Result<()> {
        let vp = self.vp();
        fit(&mut self.child, vp)
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }
}

pub fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: pager filename");
    } else {
        let mut cnpy = Canopy::new();
        cnpy.add_commands::<Root<Ed>>();
        cnpy.add_commands::<Editor>();

        cnpy.bind_key('g', "pager", "text::scroll_to_top()")?;

        cnpy.bind_key('j', "pager", "text::scroll_down()")?;
        cnpy.bind_key(key::KeyCode::Down, "pager", "text::scroll_down()")?;
        cnpy.bind_mouse(mouse::Action::ScrollDown, "pager", "text::scroll_down()")?;
        cnpy.bind_key('k', "pager", "text::scroll_up()")?;
        cnpy.bind_key(key::KeyCode::Up, "pager", "text::scroll_up()")?;
        cnpy.bind_mouse(mouse::Action::ScrollUp, "pager", "text::scroll_up()")?;

        cnpy.bind_key('h', "pager", "text::scroll_left()")?;
        cnpy.bind_key(key::KeyCode::Left, "pager", "text::scroll_left()")?;
        cnpy.bind_key('l', "pager", "text::scroll_right()")?;
        cnpy.bind_key(key::KeyCode::Right, "pager", "text::scroll_right()")?;

        cnpy.bind_key(key::KeyCode::PageDown, "pager", "text::page_down()")?;
        cnpy.bind_key(' ', "pager", "text::page_down()")?;
        cnpy.bind_key(key::KeyCode::PageUp, "pager", "text::page_up()")?;

        cnpy.bind_key('q', "root", "root::quit()")?;

        let contents = fs::read_to_string(args[1].clone())?;
        let root = Root::new(Ed::new(contents));
        runloop(cnpy, root)?;
    }
    Ok(())
}
