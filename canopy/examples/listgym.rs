use duplicate::duplicate;
use rand::Rng;

use canopy;
use canopy::{
    event::{key, mouse},
    geom::{Point, Rect},
    layout::{ConstrainedWidthLayout, Layout},
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
    size: Rect,
}

impl Block {
    fn new() -> Self {
        let mut rng = rand::thread_rng();
        Block {
            state: NodeState::default(),
            color: "blue".into(),
            size: Rect {
                tl: Point::default(),
                w: rng.gen_range(5..100),
                h: rng.gen_range(5..20),
            },
        }
    }
}

impl Node<Handle> for Block {
    fn render(&self, app: &mut Canopy<Handle>) -> Result<()> {
        app.render
            .fill(&self.color, self.screen_area().inner(1)?, '\u{2588}')?;
        Ok(())
    }
}

impl ConstrainedWidthLayout<Handle> for Block {
    fn constrain(&mut self, _app: &mut Canopy<Handle>, _width: u16) -> Result<()> {
        let sz = self.size;
        self.state_mut().view.resize_outer(sz);
        Ok(())
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

impl Layout<Handle> for Root {
    fn layout_children(&mut self, app: &mut Canopy<Handle>) -> Result<()> {
        self.content.layout(app, self.screen_area())
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
