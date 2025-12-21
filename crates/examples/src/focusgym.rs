use canopy::{
    Binder, Canopy, Context, Layout, Loader, command, derive_commands,
    error::Result,
    event::{key, mouse},
    geom::{Expanse, Frame},
    node::Node,
    render::Render,
    state::{NodeState, StatefulNode},
    widgets::Root,
};

#[derive(canopy::StatefulNode)]
/// A focusable block that can split into children.
pub struct Block {
    /// Node state.
    state: NodeState,
    /// Child blocks.
    children: Vec<Self>,
    /// True for horizontal layout.
    horizontal: bool,
}

#[derive_commands]
impl Block {
    /// Construct a block with the requested orientation.
    fn new(orientation: bool) -> Self {
        Self {
            state: NodeState::default(),
            children: vec![],
            horizontal: orientation,
        }
    }

    /// Return true when the available area is too small to split.
    fn size_limited(&self, a: Expanse) -> bool {
        (self.horizontal && a.w <= 4) || (!self.horizontal && a.h <= 4)
    }

    #[command]
    /// Add a nested block if space permits.
    fn add(&mut self) {
        if !self.children.is_empty() && !self.size_limited(self.children[0].vp().view().into()) {
            self.children.push(Self::new(!self.horizontal));
        }
    }

    #[command]
    /// Split into two child blocks.
    fn split(&mut self, c: &mut dyn Context) -> Result<()> {
        if !self.size_limited(self.vp().view().into()) {
            self.children = vec![Self::new(!self.horizontal), Self::new(!self.horizontal)];
            c.focus_next(self);
        }
        Ok(())
    }

    #[command]
    /// Focus this block.
    fn focus(&mut self, c: &mut dyn Context) -> Result<()> {
        c.set_focus(self);
        Ok(())
    }
}

impl Node for Block {
    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        self.fill(sz)?;
        if !self.children.is_empty() {
            let vps = if self.horizontal {
                sz.rect().split_horizontal(self.children.len() as u32)?
            } else {
                sz.rect().split_vertical(self.children.len() as u32)?
            };
            for (i, child) in self.children.iter_mut().enumerate() {
                l.place(child, vps[i])?;
            }
        }
        Ok(())
    }

    fn render(&mut self, c: &dyn Context, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        if self.children.is_empty() {
            let bc = if c.is_focused(self) && self.children.is_empty() {
                "violet"
            } else {
                "blue"
            };
            r.fill(bc, vp.view().inner(1), '\u{2588}')?;
            r.solid_frame("black", Frame::new(vp.view(), 1), ' ')?;
        }
        Ok(())
    }

    fn accept_focus(&mut self) -> bool {
        self.children.is_empty()
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        for i in &mut self.children {
            f(i)?
        }
        Ok(())
    }
}

#[derive(canopy::StatefulNode)]
/// Root node for the focus gym demo.
pub struct FocusGym {
    /// Node state.
    state: NodeState,
    /// Root block.
    child: Block,
}

#[derive_commands]
impl Default for FocusGym {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusGym {
    /// Construct a new focus gym.
    pub fn new() -> Self {
        Self {
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
    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        self.child.layout(l, sz)?;
        self.wrap(self.child.vp())?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)?;
        Ok(())
    }
}

impl Loader for FocusGym {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<Block>();
    }
}

/// Install key bindings for the focus gym demo.
pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {
    Binder::new(cnpy)
        .defaults::<Root<FocusGym>>()
        .key('p', "print(\"xxxx\")")
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
        .mouse(mouse::Button::Right, "block::add()");
    Ok(())
}
