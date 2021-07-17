use duplicate::duplicate;
use std::env;
use std::fs;

use canopy;
use canopy::{
    event::{key, mouse},
    layout::FillLayout,
    runloop::runloop,
    style::solarized,
    widgets::{frame, Scroll, Text},
    Canopy, EventOutcome, Node, NodeState, Rect, Result, StatefulNode,
};

struct Handle {}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    child: frame::Frame<Handle, Scroll<Handle, Text<Handle>>>,
}

impl Root {
    fn new(contents: String) -> Self {
        Root {
            state: NodeState::default(),
            child: frame::Frame::new(Scroll::new(Text::new(&contents))),
        }
    }
}

impl FillLayout<Handle> for Root {
    fn layout_children(&mut self, app: &mut Canopy<Handle>, rect: Rect) -> Result<()> {
        self.child.layout(app, rect)?;
        Ok(())
    }
}

impl Node<Handle> for Root {
    fn can_focus(&self) -> bool {
        true
    }
    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<EventOutcome> {
        Ok(match k {
            c if c == mouse::Action::ScrollDown => self.child.child.down(app)?,
            c if c == mouse::Action::ScrollUp => self.child.child.up(app)?,
            _ => EventOutcome::Ignore { skip: false },
        })
    }
    fn handle_key(
        &mut self,
        app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: key::Key,
    ) -> Result<EventOutcome> {
        Ok(match k {
            c if c == 'g' => self.child.child.scroll_to(app, 0, 0)?,
            c if c == 'j' || c == key::KeyCode::Down => self.child.child.down(app)?,
            c if c == 'k' || c == key::KeyCode::Up => self.child.child.up(app)?,
            c if c == 'h' || c == key::KeyCode::Left => self.child.child.left(app)?,
            c if c == 'l' || c == key::KeyCode::Up => self.child.child.right(app)?,
            c if c == ' ' || c == key::KeyCode::PageDown => self.child.child.page_down(app)?,
            c if c == key::KeyCode::PageUp => self.child.child.page_up(app)?,
            c if c == 'q' => EventOutcome::Exit,
            _ => EventOutcome::Ignore { skip: false },
        })
    }

    #[duplicate(
        method          reference(type);
        [children]      [& type];
        [children_mut]  [&mut type];
    )]
    fn method(
        self: reference([Self]),
        f: &mut dyn FnMut(reference([dyn Node<Handle>])) -> Result<()>,
    ) -> Result<()> {
        f(reference([self.child]))
    }
}

pub fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: pager filename");
    } else {
        let mut app = Canopy::new();
        let mut h = Handle {};
        let contents = fs::read_to_string(args[1].clone())?;
        let mut root = Root::new(contents);
        let mut style = solarized::solarized_dark();
        runloop(&mut app, &mut style, &mut root, &mut h)?;
    }
    Ok(())
}
