use canopy::{
    backend::crossterm::runloop,
    derive_commands,
    event::{key, mouse},
    geom::{Expanse, Frame},
    *,
};
use clap::Parser;

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

    fn size_limited(&self, a: Expanse) -> bool {
        (self.horizontal && a.w <= 4) || (!self.horizontal && a.h <= 4)
    }

    #[command]
    fn add(&mut self) {
        if !self.children.is_empty() && !self.size_limited(self.children[0].vp().view().into()) {
            self.children.push(Block::new(!self.horizontal));
        }
    }

    #[command]
    fn split(&mut self, c: &mut dyn Context) -> Result<()> {
        if !self.size_limited(self.vp().view().into()) {
            self.children = vec![Block::new(!self.horizontal), Block::new(!self.horizontal)];
            c.focus_next(self);
        }
        Ok(())
    }

    #[command]
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
                sz.rect().split_horizontal(self.children.len() as u16)?
            } else {
                sz.rect().split_vertical(self.children.len() as u16)?
            };
            for (i, child) in self.children.iter_mut().enumerate() {
                l.place_(child, vps[i])?;
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
        c.add_commands::<FocusGym>();
        c.add_commands::<Block>();
    }
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Number of times to greet
    #[clap(short, long)]
    commands: bool,

    /// Number of times to greet
    #[clap(short, long)]
    inspector: bool,
}

pub fn main() -> Result<()> {
    let mut cnpy = Canopy::new();
    Root::<FocusGym>::load(&mut cnpy);

    canopy::Binder::new(&mut cnpy)
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

    let args = Args::parse();
    if args.commands {
        cnpy.print_command_table(&mut std::io::stdout())?;
        return Ok(());
    }

    runloop(
        cnpy,
        Root::new(FocusGym::new()).with_inspector(args.inspector),
    )?;
    Ok(())
}
