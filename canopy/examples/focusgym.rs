use canopy::{
    backend::crossterm::runloop,
    derive_commands,
    event::{key, mouse},
    geom::Expanse,
    geom::Frame,
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

    canopy::Binder::new()
        .with_path("inspector")
        .key('a', "root::focus_app()")
        .with_path("root")
        .key(key::Ctrl + key::KeyCode::Right, "root::toggle_inspector()")
        .key('q', "root::quit()")
        .with_path("focus_gym/")
        .key(key::KeyCode::Tab, "root::focus_next()")
        .mouse(mouse::Action::ScrollDown, "root::focus_next()")
        .mouse(mouse::Action::ScrollUp, "root::focus_prev()")
        .key(key::KeyCode::Right, "root::focus_right()")
        .key('l', "root::focus_right()")
        .key(key::KeyCode::Left, "root::focus_left()")
        .key('h', "root::focus_left()")
        .key(key::KeyCode::Up, "root::focus_up()")
        .key('k', "root::focus_up()")
        .key(key::KeyCode::Down, "root::focus_down()")
        .key('j', "root::focus_down()")
        .with_path("block")
        .key('s', "block::split()")
        .key('a', "block::add()")
        .mouse(mouse::Button::Left, "block::focus()")
        .mouse(mouse::Button::Middle, "block::split()")
        .mouse(mouse::Button::Right, "block::add()")
        .build(&mut cnpy)?;

    let root = Root::new(FocusGym::new());
    runloop(cnpy, root)?;
    Ok(())
}
