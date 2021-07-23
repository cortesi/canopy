use duplicate::duplicate;

use canopy;
use canopy::{
    event::{key, mouse},
    geom::Frame,
    geom::Rect,
    render::term::runloop,
    style::solarized,
    Canopy, EventOutcome, Node, NodeState, Result, StatefulNode,
};

struct Handle {}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    child: Block,
}

impl Root {
    fn new() -> Self {
        Root {
            state: NodeState::default(),
            child: Block {
                state: NodeState::default(),
                children: vec![Block::new(false), Block::new(false)],
                horizontal: true,
            },
        }
    }
}

#[derive(StatefulNode)]
struct Block {
    state: NodeState,
    children: Vec<Block>,
    horizontal: bool,
}

impl Block {
    fn new(orientation: bool) -> Self {
        Block {
            state: NodeState::default(),
            children: vec![],
            horizontal: orientation,
        }
    }
}

impl Node<Handle> for Root {
    fn layout(&mut self, app: &mut Canopy<Handle>, screen: Rect) -> Result<()> {
        self.state_mut().viewport.set_fill(screen);
        self.child.layout(app, screen)
    }

    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<EventOutcome> {
        Ok(match k {
            c if c == mouse::Action::ScrollDown => app.focus_next(self)?,
            c if c == mouse::Action::ScrollUp => app.focus_prev(self)?,
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
            c if c == key::KeyCode::Tab => app.focus_next(self)?,
            c if c == 'l' || c == key::KeyCode::Right => app.focus_right(self)?,
            c if c == 'h' || c == key::KeyCode::Left => app.focus_left(self)?,
            c if c == 'j' || c == key::KeyCode::Down => app.focus_down(self)?,
            c if c == 'k' || c == key::KeyCode::Up => app.focus_up(self)?,
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
        f(reference([self.child]))?;
        Ok(())
    }
}

impl Block {
    fn add(&mut self, app: &mut Canopy<Handle>) -> Result<EventOutcome> {
        let r = self.screen();
        Ok(if self.children.len() == 0 {
            EventOutcome::Ignore { skip: false }
        } else if self.size_limited(r) {
            EventOutcome::Handle { skip: false }
        } else {
            self.children.push(Block::new(!self.horizontal));
            self.layout(app, r)?;
            app.taint_tree(self)?;
            EventOutcome::Handle { skip: false }
        })
    }
    fn size_limited(&self, a: Rect) -> bool {
        if self.horizontal && a.w <= 4 {
            true
        } else if !self.horizontal && a.h <= 4 {
            true
        } else {
            false
        }
    }
    fn split(&mut self, app: &mut Canopy<Handle>) -> Result<EventOutcome> {
        let r = self.screen();
        Ok(if self.children.len() != 0 {
            EventOutcome::Ignore { skip: false }
        } else if self.size_limited(r) {
            EventOutcome::Handle { skip: false }
        } else {
            self.children = vec![Block::new(!self.horizontal), Block::new(!self.horizontal)];
            self.layout(app, r)?;
            app.taint_tree(self)?;
            EventOutcome::Handle { skip: false }
        })
    }
}

impl Node<Handle> for Block {
    fn layout(&mut self, app: &mut Canopy<Handle>, screen: Rect) -> Result<()> {
        self.state_mut().viewport.set_fill(screen);
        if self.children.len() > 0 {
            let sizes = if self.horizontal {
                screen.split_horizontal(self.children.len() as u16)?
            } else {
                screen.split_vertical(self.children.len() as u16)?
            };
            for i in 0..self.children.len() {
                app.resize(&mut self.children[i], sizes[i])?
            }
        }
        Ok(())
    }
    fn can_focus(&self) -> bool {
        self.children.len() == 0
    }
    fn render(&self, app: &mut Canopy<Handle>) -> Result<()> {
        if self.children.len() == 0 {
            let bc = if app.is_focused(self) && self.children.len() == 0 {
                "violet"
            } else {
                "blue"
            };

            let r = self.screen();
            app.render.fill(bc, r.inner(1)?, '\u{2588}')?;
            app.render.solid_frame("black", Frame::new(r, 1)?, ' ')?;
        }
        Ok(())
    }
    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<EventOutcome> {
        Ok(match k {
            c if c == mouse::Action::Down + mouse::Button::Left => {
                app.taint_tree(self)?;
                app.set_focus(self)?
            }
            c if c == mouse::Action::Down + mouse::Button::Middle => {
                self.split(app)?;
                if app.is_focused(self) {
                    app.focus_next(self)?;
                };
                EventOutcome::Handle { skip: false }
            }
            c if c == mouse::Action::Down + mouse::Button::Right => self.add(app)?,
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
            c if c == 's' => {
                self.split(app)?;
                app.focus_next(self)?
            }
            c if c == 'a' => self.add(app)?,
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
        for i in reference([self.children]) {
            f(i)?
        }
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
