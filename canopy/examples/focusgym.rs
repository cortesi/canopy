use canopy::{
    backend::crossterm::runloop,
    derive_commands,
    event::{key, mouse},
    geom::Expanse,
    geom::Frame,
    inspector::Inspector,
    *,
};

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

    #[command]
    fn add(&mut self, c: &mut dyn Core) {
        if !self.children.is_empty() && !self.size_limited(self.children[0].vp().view_rect().into())
        {
            self.children.push(Block::new(!self.horizontal));
            c.taint_tree(self);
        }
    }

    fn size_limited(&self, a: Expanse) -> bool {
        (self.horizontal && a.w <= 4) || (!self.horizontal && a.h <= 4)
    }

    #[command]
    fn split(&mut self, c: &mut dyn Core) -> Result<()> {
        if !self.size_limited(self.vp().view_rect().into()) {
            self.children = vec![Block::new(!self.horizontal), Block::new(!self.horizontal)];
            c.taint_tree(self);
            c.focus_next(self)?;
        }
        Ok(())
    }

    #[command]
    fn focus(&mut self, c: &mut dyn Core) -> Result<()> {
        c.set_focus(self);
        Ok(())
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

    fn children(self: &mut Self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        for i in &mut self.children {
            f(i)?
        }
        Ok(())
    }
}

#[derive(StatefulNode)]
struct FocusGym {
    state: NodeState,
    child: Block,
}

#[derive_commands]
impl FocusGym {
    fn new() -> Self {
        FocusGym {
            state: NodeState::default(),
            child: Block {
                state: NodeState::default(),
                children: vec![Block::new(false), Block::new(false)],
                horizontal: true,
            },
        }
    }
}

impl Node for FocusGym {
    fn render(&mut self, _c: &dyn Core, _: &mut Render) -> Result<()> {
        let vp = self.vp();
        fit(&mut self.child, vp)
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)?;
        Ok(())
    }
}

pub fn main() -> Result<()> {
    let mut cnpy = Canopy::new();

    cnpy.load_commands::<Root<FocusGym>>();
    cnpy.load_commands::<Block>();
    cnpy.load_commands::<FocusGym>();

    cnpy.bind_key(key::KeyCode::Tab, "root", "root::focus_next()")?;
    cnpy.bind_mouse(mouse::Action::ScrollDown, "root", "root::focus_next()")?;
    cnpy.bind_mouse(mouse::Action::ScrollUp, "root", "root::focus_prev()")?;

    cnpy.bind_key(key::KeyCode::Right, "root", "root::focus_right()")?;
    cnpy.bind_key('l', "root", "root::focus_right()")?;

    cnpy.bind_key(key::KeyCode::Left, "root", "root::focus_left()")?;
    cnpy.bind_key('h', "root", "root::focus_left()")?;

    cnpy.bind_key(key::KeyCode::Up, "root", "root::focus_up()")?;
    cnpy.bind_key('k', "root", "root::focus_up()")?;

    cnpy.bind_key(key::KeyCode::Down, "root", "root::focus_down()")?;
    cnpy.bind_key('j', "root", "root::focus_down()")?;

    cnpy.bind_key('q', "root", "root::quit()")?;

    cnpy.bind_key('s', "block", "block::split()")?;
    cnpy.bind_key('a', "block", "block::add()")?;
    cnpy.bind_mouse(mouse::Button::Left, "block", "block::focus()")?;
    cnpy.bind_mouse(mouse::Button::Middle, "block", "block::split()")?;
    cnpy.bind_mouse(mouse::Button::Right, "block", "block::add()")?;

    let root = Inspector::new(key::Ctrl + key::KeyCode::Right, Root::new(FocusGym::new()));
    runloop(cnpy, root)?;
    Ok(())
}
