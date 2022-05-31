use canopy::{
    backend::crossterm::runloop,
    derive_commands,
    event::{key, mouse},
    fit, focus,
    geom::Expanse,
    geom::Frame,
    inspector::Inspector,
    style::solarized,
    BackendControl, Node, NodeState, Outcome, Render, Result, StatefulNode,
};

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    child: Block,
}

#[derive_commands]
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

#[derive_commands]
impl Block {
    fn new(orientation: bool) -> Self {
        Block {
            state: NodeState::default(),
            children: vec![],
            horizontal: orientation,
        }
    }
}

impl Node for Root {
    fn render(&mut self, _: &mut Render) -> Result<()> {
        let vp = self.vp();
        fit(&mut self.child, vp)
    }

    fn handle_mouse(&mut self, _: &mut dyn BackendControl, k: mouse::Mouse) -> Result<Outcome> {
        Ok(match k {
            c if c == mouse::MouseAction::ScrollDown => focus::shift_next(self)?,
            c if c == mouse::MouseAction::ScrollUp => focus::shift_prev(self)?,
            _ => Outcome::Ignore,
        })
    }

    fn handle_key(&mut self, ctrl: &mut dyn BackendControl, k: key::Key) -> Result<Outcome> {
        Ok(match k {
            c if c == key::KeyCode::Tab => focus::shift_next(self)?,
            c if c == 'l' || c == key::KeyCode::Right => focus::shift_right(self)?,
            c if c == 'h' || c == key::KeyCode::Left => focus::shift_left(self)?,
            c if c == 'j' || c == key::KeyCode::Down => focus::shift_down(self)?,
            c if c == 'k' || c == key::KeyCode::Up => focus::shift_up(self)?,
            c if c == 'q' => ctrl.exit(0),
            _ => Outcome::Ignore,
        })
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)?;
        Ok(())
    }
}

impl Block {
    fn add(&mut self) -> Result<Outcome> {
        Ok(if self.children.is_empty() {
            Outcome::Ignore
        } else if self.size_limited(self.children[0].vp().view_rect().into()) {
            Outcome::Handle
        } else {
            self.children.push(Block::new(!self.horizontal));
            canopy::taint_tree(self);
            Outcome::Handle
        })
    }
    fn size_limited(&self, a: Expanse) -> bool {
        (self.horizontal && a.w <= 4) || (!self.horizontal && a.h <= 4)
    }
    fn split(&mut self) -> Result<Outcome> {
        Ok(if self.size_limited(self.vp().view_rect().into()) {
            Outcome::Handle
        } else {
            self.children = vec![Block::new(!self.horizontal), Block::new(!self.horizontal)];
            canopy::taint_tree(self);
            Outcome::Handle
        })
    }
}

impl Node for Block {
    fn render(&mut self, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        if !self.children.is_empty() {
            let vps = if self.horizontal {
                vp.split_horizontal(self.children.len() as u16)?
            } else {
                vp.split_vertical(self.children.len() as u16)?
            };
            for i in 0..self.children.len() {
                fit(&mut self.children[i], vps[i])?;
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

    fn accept_focus(&mut self) -> bool {
        self.children.is_empty()
    }

    fn handle_mouse(&mut self, _: &mut dyn BackendControl, k: mouse::Mouse) -> Result<Outcome> {
        Ok(match k {
            c if c == mouse::MouseAction::Down + mouse::Button::Left => {
                canopy::taint_tree(self);
                self.set_focus();
                Outcome::Handle
            }
            c if c == mouse::MouseAction::Down + mouse::Button::Middle => {
                self.split()?;
                if self.is_focused() {
                    focus::shift_next(self)?;
                };
                Outcome::Handle
            }
            c if c == mouse::MouseAction::Down + mouse::Button::Right => self.add()?,
            _ => Outcome::Ignore,
        })
    }

    fn handle_key(&mut self, _: &mut dyn BackendControl, k: key::Key) -> Result<Outcome> {
        Ok(match k {
            c if c == 's' => {
                self.split()?;
                focus::shift_next(self)?
            }
            c if c == 'a' => self.add()?,
            _ => Outcome::Ignore,
        })
    }

    fn children(self: &mut Self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        for i in &mut self.children {
            f(i)?
        }
        Ok(())
    }
}

pub fn main() -> Result<()> {
    let mut colors = solarized::solarized_dark();
    let root = Inspector::new(key::Ctrl + key::KeyCode::Right, Root::new());
    runloop(&mut colors, root)?;
    Ok(())
}
