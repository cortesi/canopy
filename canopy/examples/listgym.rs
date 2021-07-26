use rand::Rng;

use canopy;
use canopy::{
    event::{key, mouse},
    fit_and_update,
    geom::{Rect, Size},
    render::term::runloop,
    style::solarized,
    widgets::{frame, List, Text},
    Canopy, EventOutcome, Node, NodeState, Result, StatefulNode,
};

const TEXT: &str = "What a struggle must have gone on during long centuries between the several kinds of trees, each annually scattering its seeds by the thousand; what war between insect and insect — between insects, snails, and other animals with birds and beasts of prey — all striving to increase, all feeding on each other, or on the trees, their seeds and seedlings, or on the other plants which first clothed the ground and thus checked the growth of the trees.";

struct Handle {}

#[derive(StatefulNode)]
struct Block {
    state: NodeState,
    child: Text<Handle>,
}

impl Block {
    fn new() -> Self {
        let mut rng = rand::thread_rng();
        Block {
            state: NodeState::default(),
            child: Text::new(TEXT).with_fixed_width(rng.gen_range(10..150)),
        }
    }
}

impl Node<Handle> for Block {
    fn fit(&mut self, app: &mut Canopy<Handle>, target: Size) -> Result<Size> {
        Ok(self.child.fit(app, target)?)
    }

    fn layout(&mut self, app: &mut Canopy<Handle>, screen: Rect) -> Result<()> {
        fit_and_update(app, screen, &mut self.child)
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node<Handle>) -> Result<()>) -> Result<()> {
        f(&self.child)
    }

    fn children_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Node<Handle>) -> Result<()>,
    ) -> Result<()> {
        f(&mut self.child)
    }
}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    content: frame::Frame<Handle, List<Handle, Block>>,
}

impl Root {
    fn new() -> Self {
        let nodes: Vec<Block> = (0..100).map(|_| Block::new()).collect();
        Root {
            state: NodeState::default(),
            content: frame::Frame::new(List::new(nodes)),
        }
    }
}

impl Node<Handle> for Root {
    fn layout(&mut self, app: &mut Canopy<Handle>, screen: Rect) -> Result<()> {
        fit_and_update(app, screen, &mut self.content)
    }

    fn can_focus(&self) -> bool {
        true
    }

    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<EventOutcome> {
        let v = &mut self.content.child.state_mut().viewport;
        match k {
            c if c == mouse::Action::ScrollDown => v.down(),
            c if c == mouse::Action::ScrollUp => v.up(),
            _ => return Ok(EventOutcome::Ignore { skip: false }),
        };
        self.layout(app, self.screen())?;
        app.taint_tree(self)?;
        Ok(EventOutcome::Handle { skip: false })
    }

    fn handle_key(
        &mut self,
        app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: key::Key,
    ) -> Result<EventOutcome> {
        let v = &mut self.content.child.state_mut().viewport;
        match k {
            c if c == 'g' => v.scroll_to(0, 0),
            c if c == 'j' || c == key::KeyCode::Down => v.down(),
            c if c == 'k' || c == key::KeyCode::Up => v.up(),
            c if c == 'h' || c == key::KeyCode::Left => v.left(),
            c if c == 'l' || c == key::KeyCode::Up => v.right(),
            c if c == ' ' || c == key::KeyCode::PageDown => v.page_down(),
            c if c == key::KeyCode::PageUp => v.page_up(),
            c if c == 'q' => app.exit(0),
            _ => return Ok(EventOutcome::Ignore { skip: false }),
        };
        self.layout(app, self.screen())?;
        app.taint_tree(self)?;
        Ok(EventOutcome::Handle { skip: false })
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node<Handle>) -> Result<()>) -> Result<()> {
        f(&self.content)?;
        Ok(())
    }

    fn children_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Node<Handle>) -> Result<()>,
    ) -> Result<()> {
        f(&mut self.content)?;
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
