use canopy::{
    backend::crossterm::runloop,
    derive_commands,
    event::{key, mouse},
    geom::Expanse,
    geom::Frame,
    inspector::Inspector,
    style::solarized,
    *,
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
    fn render(&mut self, _c: &dyn Core, _: &mut Render) -> Result<()> {
        let vp = self.vp();
        fit(&mut self.child, vp)
    }

    fn handle_mouse(
        &mut self,
        c: &mut dyn Core,
        _: &mut dyn BackendControl,
        k: mouse::Mouse,
    ) -> Result<Outcome> {
        Ok(match k {
            ck if ck == mouse::MouseAction::ScrollDown => c.focus_next(self)?,
            ck if ck == mouse::MouseAction::ScrollUp => c.focus_prev(self)?,
            _ => Outcome::Ignore,
        })
    }

    fn handle_key(
        &mut self,
        c: &mut dyn Core,
        ctrl: &mut dyn BackendControl,
        k: key::Key,
    ) -> Result<Outcome> {
        Ok(match k {
            ck if ck == key::KeyCode::Tab => c.focus_next(self)?,
            ck if ck == 'l' || ck == key::KeyCode::Right => c.focus_right(self)?,
            ck if ck == 'h' || ck == key::KeyCode::Left => c.focus_left(self)?,
            ck if ck == 'j' || ck == key::KeyCode::Down => c.focus_down(self)?,
            ck if ck == 'k' || ck == key::KeyCode::Up => c.focus_up(self)?,
            ck if ck == 'q' => ctrl.exit(0),
            _ => Outcome::Ignore,
        })
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)?;
        Ok(())
    }
}

impl Block {
    fn add(&mut self, c: &mut dyn Core) -> Result<Outcome> {
        Ok(if self.children.is_empty() {
            Outcome::Ignore
        } else if self.size_limited(self.children[0].vp().view_rect().into()) {
            Outcome::Handle
        } else {
            self.children.push(Block::new(!self.horizontal));
            c.taint_tree(self);
            Outcome::Handle
        })
    }
    fn size_limited(&self, a: Expanse) -> bool {
        (self.horizontal && a.w <= 4) || (!self.horizontal && a.h <= 4)
    }
    fn split(&mut self, c: &mut dyn Core) -> Result<Outcome> {
        Ok(if self.size_limited(self.vp().view_rect().into()) {
            Outcome::Handle
        } else {
            self.children = vec![Block::new(!self.horizontal), Block::new(!self.horizontal)];
            c.taint_tree(self);
            Outcome::Handle
        })
    }
}

impl Node for Block {
    fn render(&mut self, c: &dyn Core, r: &mut Render) -> Result<()> {
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
            let bc = if c.is_focused(self) && self.children.is_empty() {
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

    fn handle_mouse(
        &mut self,
        c: &mut dyn Core,
        _: &mut dyn BackendControl,
        k: mouse::Mouse,
    ) -> Result<Outcome> {
        Ok(match k {
            ck if ck == mouse::MouseAction::Down + mouse::Button::Left => {
                c.taint_tree(self);
                c.set_focus(self);
                Outcome::Handle
            }
            ck if ck == mouse::MouseAction::Down + mouse::Button::Middle => {
                self.split(c)?;
                if c.is_focused(self) {
                    c.focus_next(self)?;
                };
                Outcome::Handle
            }
            ck if ck == mouse::MouseAction::Down + mouse::Button::Right => self.add(c)?,
            _ => Outcome::Ignore,
        })
    }

    fn handle_key(
        &mut self,
        c: &mut dyn Core,
        _: &mut dyn BackendControl,
        k: key::Key,
    ) -> Result<Outcome> {
        Ok(match k {
            ck if ck == 's' => {
                self.split(c)?;
                c.focus_next(self)?
            }
            ck if ck == 'a' => self.add(c)?,
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
