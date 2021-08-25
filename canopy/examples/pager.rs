use std::env;
use std::fs;

use canopy;
use canopy::{
    event::{key, mouse},
    inspector::Inspector,
    render::term::runloop,
    style::solarized,
    widgets::{frame, Text},
    Canopy, Node, NodeState, Outcome, Result, StatefulNode, ViewPort,
};

struct Handle {}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    child: frame::Frame<Handle, (), Text<Handle>>,
}

impl Root {
    fn new(contents: String) -> Self {
        Root {
            state: NodeState::default(),
            child: frame::Frame::new(Text::new(&contents)),
        }
    }
}

impl Node<Handle, ()> for Root {
    fn focus(&mut self, app: &mut Canopy<Handle, ()>) -> Result<Outcome<()>> {
        app.set_focus(self);
        Ok(Outcome::handle())
    }

    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<Outcome<()>> {
        let v = &mut self.child.child.state_mut().viewport;
        match k {
            c if c == mouse::Action::ScrollDown => v.down(),
            c if c == mouse::Action::ScrollUp => v.up(),
            _ => return Ok(Outcome::ignore()),
        };
        app.taint_tree(self)?;
        Ok(Outcome::handle())
    }

    fn handle_key(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        _: &mut Handle,
        k: key::Key,
    ) -> Result<Outcome<()>> {
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
            c if c == 'q' => app.exit(0),
            _ => return Ok(Outcome::ignore()),
        }
        app.taint_tree(self)?;
        Ok(Outcome::handle())
    }

    fn render(&mut self, app: &mut Canopy<Handle, ()>, vp: ViewPort) -> Result<()> {
        self.child.wrap(app, vp)
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node<Handle, ()>) -> Result<()>) -> Result<()> {
        f(&self.child)
    }

    fn children_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Node<Handle, ()>) -> Result<()>,
    ) -> Result<()> {
        f(&mut self.child)
    }
}

pub fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: pager filename");
    } else {
        let colors = solarized::solarized_dark();
        let mut h = Handle {};
        let contents = fs::read_to_string(args[1].clone())?;
        let mut root = Inspector::new(key::Ctrl + key::KeyCode::Right, Root::new(contents));
        runloop(colors, &mut root, &mut h)?;
    }
    Ok(())
}
