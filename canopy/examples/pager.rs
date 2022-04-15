use std::env;
use std::fs;

use canopy::{
    backend::crossterm::runloop,
    event::{key, mouse},
    inspector::Inspector,
    style::solarized,
    widgets::{frame, Text},
    wrap, BackendControl, Node, NodeState, Outcome, Render, Result, StatefulNode, ViewPort,
};

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    child: frame::Frame<Text>,
}

impl Root {
    fn new(contents: String) -> Self {
        Root {
            state: NodeState::default(),
            child: frame::Frame::new(Text::new(&contents)),
        }
    }
}

impl Node for Root {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn handle_mouse(&mut self, _: &mut dyn BackendControl, k: mouse::Mouse) -> Result<Outcome> {
        let txt = &mut self.child.child;
        match k {
            c if c == mouse::MouseAction::ScrollDown => txt.update_viewport(&|vp| vp.down()),
            c if c == mouse::MouseAction::ScrollUp => txt.update_viewport(&|vp| vp.up()),
            _ => return Ok(Outcome::ignore()),
        };
        canopy::taint_tree(self);
        Ok(Outcome::handle())
    }

    fn handle_key(&mut self, ctrl: &mut dyn BackendControl, k: key::Key) -> Result<Outcome> {
        let txt = &mut self.child.child;
        match k {
            c if c == 'g' => txt.update_viewport(&|vp| vp.scroll_to(0, 0)),
            c if c == 'j' || c == key::KeyCode::Down => txt.update_viewport(&|vp| vp.down()),
            c if c == 'k' || c == key::KeyCode::Up => txt.update_viewport(&|vp| vp.up()),
            c if c == 'h' || c == key::KeyCode::Left => txt.update_viewport(&|vp| vp.left()),
            c if c == 'l' || c == key::KeyCode::Up => txt.update_viewport(&|vp| vp.right()),
            c if c == ' ' || c == key::KeyCode::PageDown => {
                txt.update_viewport(&|vp| vp.page_down());
            }
            c if c == key::KeyCode::PageUp => txt.update_viewport(&|vp| vp.page_up()),
            c if c == 'q' => ctrl.exit(0),
            _ => return Ok(Outcome::ignore()),
        }
        canopy::taint_tree(self);
        Ok(Outcome::handle())
    }

    fn render(&mut self, _: &mut Render, vp: ViewPort) -> Result<()> {
        wrap(&mut self.child, vp)
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
        let mut colors = solarized::solarized_dark();
        let contents = fs::read_to_string(args[1].clone())?;
        let mut root = Inspector::new(key::Ctrl + key::KeyCode::Right, Root::new(contents));
        runloop(&mut colors, &mut root)?;
    }
    Ok(())
}
