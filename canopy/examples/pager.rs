use std::env;
use std::fs;

use canopy::{
    backend::crossterm::runloop,
    derive_commands,
    event::{key, mouse},
    inspector::Inspector,
    style::solarized,
    widgets::{frame, Text},
    *,
};

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    child: frame::Frame<Text>,
}

#[derive_commands]
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

    fn handle_mouse(
        &mut self,
        c: &mut Canopy,
        _: &mut dyn BackendControl,
        k: mouse::Mouse,
    ) -> Result<Outcome> {
        let txt = &mut self.child.child;
        match k {
            c if c == mouse::MouseAction::ScrollDown => txt.update_viewport(&|vp| vp.down()),
            c if c == mouse::MouseAction::ScrollUp => txt.update_viewport(&|vp| vp.up()),
            _ => return Ok(Outcome::Ignore),
        };
        c.taint_tree(self);
        Ok(Outcome::Handle)
    }

    fn handle_key(
        &mut self,
        c: &mut Canopy,
        ctrl: &mut dyn BackendControl,
        k: key::Key,
    ) -> Result<Outcome> {
        let txt = &mut self.child.child;
        match k {
            ck if ck == 'g' => txt.update_viewport(&|vp| vp.scroll_to(0, 0)),
            ck if ck == 'j' || ck == key::KeyCode::Down => txt.update_viewport(&|vp| vp.down()),
            ck if ck == 'k' || ck == key::KeyCode::Up => txt.update_viewport(&|vp| vp.up()),
            ck if ck == 'h' || ck == key::KeyCode::Left => txt.update_viewport(&|vp| vp.left()),
            ck if ck == 'l' || ck == key::KeyCode::Up => txt.update_viewport(&|vp| vp.right()),
            ck if ck == ' ' || ck == key::KeyCode::PageDown => {
                txt.update_viewport(&|vp| vp.page_down());
            }
            ck if ck == key::KeyCode::PageUp => txt.update_viewport(&|vp| vp.page_up()),
            ck if ck == 'q' => ctrl.exit(0),
            _ => return Ok(Outcome::Ignore),
        }
        c.taint_tree(self);
        Ok(Outcome::Handle)
    }

    fn render(&mut self, _c: &Canopy, _: &mut Render) -> Result<()> {
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
        let mut colors = solarized::solarized_dark();
        let contents = fs::read_to_string(args[1].clone())?;
        let root = Inspector::new(key::Ctrl + key::KeyCode::Right, Root::new(contents));
        runloop(&mut colors, root)?;
    }
    Ok(())
}
