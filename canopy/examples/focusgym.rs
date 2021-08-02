use duplicate::duplicate;

use canopy;
use canopy::{
    event::{key, mouse},
    fit_and_update,
    geom::Frame,
    geom::{Rect, ViewPort},
    render::term::runloop,
    style::solarized,
    Canopy, Node, NodeState, Outcome, Result, StatefulNode,
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

impl Node<Handle, ()> for Root {
    fn render(&mut self, app: &mut Canopy<Handle, ()>, vp: ViewPort) -> Result<()> {
        fit_and_update(app, vp.screen(), &mut self.child)
    }

    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<Outcome<()>> {
        Ok(match k {
            c if c == mouse::Action::ScrollDown => app.focus_next(self)?,
            c if c == mouse::Action::ScrollUp => app.focus_prev(self)?,
            _ => Outcome::ignore(),
        })
    }

    fn handle_key(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        _: &mut Handle,
        k: key::Key,
    ) -> Result<Outcome<()>> {
        Ok(match k {
            c if c == key::KeyCode::Tab => app.focus_next(self)?,
            c if c == 'l' || c == key::KeyCode::Right => app.focus_right(self)?,
            c if c == 'h' || c == key::KeyCode::Left => app.focus_left(self)?,
            c if c == 'j' || c == key::KeyCode::Down => app.focus_down(self)?,
            c if c == 'k' || c == key::KeyCode::Up => app.focus_up(self)?,
            c if c == 'q' => app.exit(0),
            _ => Outcome::ignore(),
        })
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node<Handle, ()>) -> Result<()>) -> Result<()> {
        f(&self.child)?;
        Ok(())
    }

    fn children_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Node<Handle, ()>) -> Result<()>,
    ) -> Result<()> {
        f(&mut self.child)?;
        Ok(())
    }
}

impl Block {
    fn add(&mut self, app: &mut Canopy<Handle, ()>) -> Result<Outcome<()>> {
        Ok(if self.children.len() == 0 {
            Outcome::ignore()
        } else if self.size_limited(self.children[0].screen()) {
            Outcome::handle()
        } else {
            self.children.push(Block::new(!self.horizontal));
            app.taint_tree(self)?;
            Outcome::handle()
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
    fn split(&mut self, app: &mut Canopy<Handle, ()>) -> Result<Outcome<()>> {
        let r = self.screen();
        Ok(if self.children.len() != 0 {
            Outcome::ignore()
        } else if self.size_limited(r) {
            Outcome::handle()
        } else {
            self.children = vec![Block::new(!self.horizontal), Block::new(!self.horizontal)];
            app.taint_tree(self)?;
            Outcome::handle()
        })
    }
}

impl Node<Handle, ()> for Block {
    fn render(&mut self, app: &mut Canopy<Handle, ()>, vp: ViewPort) -> Result<()> {
        let screen = vp.screen();
        if self.children.len() > 0 {
            let sizes = if self.horizontal {
                screen.split_horizontal(self.children.len() as u16)?
            } else {
                screen.split_vertical(self.children.len() as u16)?
            };
            for i in 0..self.children.len() {
                fit_and_update(app, sizes[i], &mut self.children[i])?;
            }
        } else {
            let bc = if app.is_focused(self) && self.children.len() == 0 {
                "violet"
            } else {
                "blue"
            };
            app.render.fill(bc, vp.view().inner(1)?, '\u{2588}')?;
            app.render
                .solid_frame("black", Frame::new(vp.view(), 1)?, ' ')?;
        }

        Ok(())
    }

    fn can_focus(&self) -> bool {
        self.children.len() == 0
    }

    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<Outcome<()>> {
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
                Outcome::handle()
            }
            c if c == mouse::Action::Down + mouse::Button::Right => self.add(app)?,
            _ => Outcome::ignore(),
        })
    }

    fn handle_key(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        _: &mut Handle,
        k: key::Key,
    ) -> Result<Outcome<()>> {
        Ok(match k {
            c if c == 's' => {
                self.split(app)?;
                app.focus_next(self)?
            }
            c if c == 'a' => self.add(app)?,
            _ => Outcome::ignore(),
        })
    }

    #[duplicate(
        method          reference(type);
        [children]      [& type];
        [children_mut]  [&mut type];
    )]
    fn method(
        self: reference([Self]),
        f: &mut dyn FnMut(reference([dyn Node<Handle, ()>])) -> Result<()>,
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
