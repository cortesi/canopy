use canopy::{
    backend::crossterm::runloop,
    event::{key, mouse},
    geom::Expanse,
    geom::Frame,
    inspector::Inspector,
    style::solarized,
    wrap, BackendControl, Node, NodeState, Outcome, Render, Result, StatefulNode, ViewPort,
};

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

impl Node for Root {
    fn render(&mut self, _: &mut Render, vp: ViewPort) -> Result<()> {
        wrap(&mut self.child, vp)
    }

    fn handle_mouse(&mut self, _: &mut dyn BackendControl, k: mouse::Mouse) -> Result<Outcome> {
        Ok(match k {
            c if c == mouse::MouseAction::ScrollDown => canopy::focus_next(self)?,
            c if c == mouse::MouseAction::ScrollUp => canopy::focus_prev(self)?,
            _ => Outcome::ignore(),
        })
    }

    fn handle_key(&mut self, ctrl: &mut dyn BackendControl, k: key::Key) -> Result<Outcome> {
        Ok(match k {
            c if c == key::KeyCode::Tab => canopy::focus_next(self)?,
            c if c == 'l' || c == key::KeyCode::Right => canopy::focus_right(self)?,
            c if c == 'h' || c == key::KeyCode::Left => canopy::focus_left(self)?,
            c if c == 'j' || c == key::KeyCode::Down => canopy::focus_down(self)?,
            c if c == 'k' || c == key::KeyCode::Up => canopy::focus_up(self)?,
            c if c == 'q' => ctrl.exit(0),
            _ => Outcome::ignore(),
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
            Outcome::ignore()
        } else if self.size_limited(self.children[0].vp().view_rect().into()) {
            Outcome::handle()
        } else {
            self.children.push(Block::new(!self.horizontal));
            canopy::taint_tree(self);
            Outcome::handle()
        })
    }
    fn size_limited(&self, a: Expanse) -> bool {
        (self.horizontal && a.w <= 4) || (!self.horizontal && a.h <= 4)
    }
    fn split(&mut self) -> Result<Outcome> {
        Ok(if self.size_limited(self.vp().view_rect().into()) {
            Outcome::handle()
        } else {
            self.children = vec![Block::new(!self.horizontal), Block::new(!self.horizontal)];
            canopy::taint_tree(self);
            Outcome::handle()
        })
    }
}

impl Node for Block {
    fn render(&mut self, r: &mut Render, vp: ViewPort) -> Result<()> {
        if !self.children.is_empty() {
            let vps = if self.horizontal {
                vp.split_horizontal(self.children.len() as u16)?
            } else {
                vp.split_vertical(self.children.len() as u16)?
            };
            for i in 0..self.children.len() {
                wrap(&mut self.children[i], vps[i])?;
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
                Outcome::handle()
            }
            c if c == mouse::MouseAction::Down + mouse::Button::Middle => {
                self.split()?;
                if self.is_focused() {
                    canopy::focus_next(self)?;
                };
                Outcome::handle()
            }
            c if c == mouse::MouseAction::Down + mouse::Button::Right => self.add()?,
            _ => Outcome::ignore(),
        })
    }

    fn handle_key(&mut self, _: &mut dyn BackendControl, k: key::Key) -> Result<Outcome> {
        Ok(match k {
            c if c == 's' => {
                self.split()?;
                canopy::focus_next(self)?
            }
            c if c == 'a' => self.add()?,
            _ => Outcome::ignore(),
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
    let mut root = Inspector::new(key::Ctrl + key::KeyCode::Right, Root::new());
    runloop(&mut colors, &mut root)?;
    Ok(())
}
