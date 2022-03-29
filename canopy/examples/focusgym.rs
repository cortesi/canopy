use duplicate::duplicate_item;

use canopy::{
    backend::crossterm::runloop,
    event::{key, mouse},
    geom::Frame,
    geom::Size,
    inspector::Inspector,
    style::solarized,
    BackendControl, Canopy, Node, NodeState, Outcome, Render, Result, StatefulNode, ViewPort,
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
    fn render(&mut self, app: &mut Canopy<Handle, ()>, _: &mut Render, vp: ViewPort) -> Result<()> {
        self.child.wrap(app, vp)
    }

    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        _: &mut dyn BackendControl,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<Outcome<()>> {
        Ok(match k {
            c if c == mouse::MouseAction::ScrollDown => app.focus_next(self)?,
            c if c == mouse::MouseAction::ScrollUp => app.focus_prev(self)?,
            _ => Outcome::ignore(),
        })
    }

    fn handle_key(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        ctrl: &mut dyn BackendControl,
        _: &mut Handle,
        k: key::Key,
    ) -> Result<Outcome<()>> {
        Ok(match k {
            c if c == key::KeyCode::Tab => app.focus_next(self)?,
            c if c == 'l' || c == key::KeyCode::Right => app.focus_right(self)?,
            c if c == 'h' || c == key::KeyCode::Left => app.focus_left(self)?,
            c if c == 'j' || c == key::KeyCode::Down => app.focus_down(self)?,
            c if c == 'k' || c == key::KeyCode::Up => app.focus_up(self)?,
            c if c == 'q' => app.exit(ctrl, 0),
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
    fn add(&mut self, _app: &mut Canopy<Handle, ()>) -> Result<Outcome<()>> {
        Ok(if self.children.is_empty() {
            Outcome::ignore()
        } else if self.size_limited(self.children[0].vp().view_rect().into()) {
            Outcome::handle()
        } else {
            self.children.push(Block::new(!self.horizontal));
            self.taint_tree()?;
            Outcome::handle()
        })
    }
    fn size_limited(&self, a: Size) -> bool {
        (self.horizontal && a.w <= 4) || (!self.horizontal && a.h <= 4)
    }
    fn split(&mut self) -> Result<Outcome<()>> {
        Ok(if self.children.is_empty() {
            Outcome::ignore()
        } else if self.size_limited(self.vp().view_rect().into()) {
            Outcome::handle()
        } else {
            self.children = vec![Block::new(!self.horizontal), Block::new(!self.horizontal)];
            self.taint_tree()?;
            Outcome::handle()
        })
    }
}

impl Node<Handle, ()> for Block {
    fn render(&mut self, app: &mut Canopy<Handle, ()>, r: &mut Render, vp: ViewPort) -> Result<()> {
        if self.children.is_empty() {
            let vps = if self.horizontal {
                vp.split_horizontal(self.children.len() as u16)?
            } else {
                vp.split_vertical(self.children.len() as u16)?
            };
            for i in 0..self.children.len() {
                self.children[i].wrap(app, vps[i])?;
            }
        } else {
            let bc = if self.is_focused() && self.children.is_empty() {
                "violet"
            } else {
                "blue"
            };
            r.fill(bc, vp.view_rect().inner(1), '\u{2588}')?;
            r.solid_frame("black", Frame::new(vp.view_rect(), 1), ' ')?;
        }

        Ok(())
    }

    fn handle_focus(&mut self, _app: &mut Canopy<Handle, ()>) -> Result<Outcome<()>> {
        Ok(if self.children.is_empty() {
            self.set_focus();
            Outcome::handle()
        } else {
            Outcome::ignore()
        })
    }

    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        _: &mut dyn BackendControl,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<Outcome<()>> {
        Ok(match k {
            c if c == mouse::MouseAction::Down + mouse::Button::Left => {
                self.taint_tree()?;
                self.handle_focus(app)?
            }
            c if c == mouse::MouseAction::Down + mouse::Button::Middle => {
                self.split()?;
                if self.is_focused() {
                    app.focus_next(self)?;
                };
                Outcome::handle()
            }
            c if c == mouse::MouseAction::Down + mouse::Button::Right => self.add(app)?,
            _ => Outcome::ignore(),
        })
    }

    fn handle_key(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        _: &mut dyn BackendControl,
        _: &mut Handle,
        k: key::Key,
    ) -> Result<Outcome<()>> {
        Ok(match k {
            c if c == 's' => {
                self.split()?;
                app.focus_next(self)?
            }
            c if c == 'a' => self.add(app)?,
            _ => Outcome::ignore(),
        })
    }

    #[duplicate_item(
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
    let mut root = Inspector::new(key::Ctrl + key::KeyCode::Right, Root::new());
    runloop(colors, &mut root, &mut h)?;
    Ok(())
}
