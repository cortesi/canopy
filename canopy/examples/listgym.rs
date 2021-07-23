use duplicate::duplicate;
use rand::Rng;

use canopy;
use canopy::{
    event::{key, mouse},
    geom::Size,
    render::term::runloop,
    style::solarized,
    widgets::{frame, List, Scroll},
    Canopy, EventOutcome, Node, NodeState, Result, StatefulNode,
};

struct Handle {}

#[derive(StatefulNode)]
struct Block {
    state: NodeState,
    color: String,
    size: Size,
}

impl Block {
    fn new() -> Self {
        let mut rng = rand::thread_rng();
        Block {
            state: NodeState::default(),
            color: "blue".into(),
            size: Size {
                w: rng.gen_range(5..100),
                h: rng.gen_range(5..20),
            },
        }
    }
}

impl Node<Handle> for Block {
    fn render(&self, app: &mut Canopy<Handle>) -> Result<()> {
        app.render
            .fill(&self.color, self.screen().inner(1)?, '\u{2588}')
    }
    fn fit(&mut self, _app: &mut Canopy<Handle>, _screen: Size) -> Result<Size> {
        Ok(self.size)
    }
}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    content: frame::Frame<Handle, Scroll<Handle, List<Handle, Block>>>,
}

impl Root {
    fn new() -> Self {
        let nodes: Vec<Block> = (0..100).map(|_| Block::new()).collect();
        Root {
            state: NodeState::default(),
            content: frame::Frame::new(Scroll::new(List::new(nodes))),
        }
    }
}

impl Node<Handle> for Root {
    fn can_focus(&self) -> bool {
        true
    }
    fn handle_mouse(
        &mut self,
        _app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<EventOutcome> {
        Ok(match k {
            c if c == mouse::Action::ScrollDown => self.content.child.down()?,
            c if c == mouse::Action::ScrollUp => self.content.child.up()?,
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
            c if c == 'g' => self.content.child.scroll_to(0, 0)?,
            c if c == 'j' || c == key::KeyCode::Down => self.content.child.down()?,
            c if c == 'k' || c == key::KeyCode::Up => self.content.child.up()?,
            c if c == 'h' || c == key::KeyCode::Left => self.content.child.left()?,
            c if c == 'l' || c == key::KeyCode::Up => self.content.child.right()?,
            c if c == ' ' || c == key::KeyCode::PageDown => self.content.child.page_down()?,
            c if c == key::KeyCode::PageUp => self.content.child.page_up()?,
            c if c == 'q' => app.exit(0),
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
        f(reference([self.content]))?;
        Ok(())
    }
}

pub fn main() -> Result<()> {
    let colors = solarized::solarized_dark();
    let mut h = Handle {};
    let mut root = Root::new();
    runloop(colors, &mut root, &mut h)?;
    Ok(())
}
