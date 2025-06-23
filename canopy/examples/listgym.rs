use clap::Parser;
use rand::seq::SliceRandom;
use rand::Rng;

use canopy::{
    backend::crossterm::runloop,
    event::key,
    geom::{Expanse, Rect},
    style::solarized,
    widgets::{frame, list::*, Text},
    *,
};

const TEXT: &str = "What a struggle must have gone on during long centuries between the several kinds of trees, each annually scattering its seeds by the thousand; what war between insect and insect — between insects, snails, and other animals with birds and beasts of prey — all striving to increase, all feeding on each other, or on the trees, their seeds and seedlings, or on the other plants which first clothed the ground and thus checked the growth of the trees.";

const COLORS: &[&str] = &["red", "blue", "green"];

#[derive(StatefulNode)]
struct Block {
    state: NodeState,
    child: Text,
    color: String,
    selected: bool,
}

#[derive_commands]
impl Block {
    fn new() -> Self {
        let mut rng = rand::thread_rng();
        Block {
            state: NodeState::default(),
            child: Text::new(TEXT).with_fixed_width(rng.gen_range(10..150)),
            color: String::from(*(COLORS.choose(&mut rng).unwrap())),
            selected: false,
        }
    }
}

impl ListItem for Block {
    fn set_selected(&mut self, state: bool) {
        self.selected = state
    }
}

impl Node for Block {
    fn layout(&mut self, l: &Layout, target: Expanse) -> Result<()> {
        let loc = Rect::new(2, 0, target.w.saturating_sub(2), target.h);
        let vp = self.vp();
        l.place(&mut self.child, vp, loc)?;

        let vp = self.child.vp();
        let sz = Expanse {
            w: vp.canvas.w + 2,
            h: self.child.vp().canvas.h,
        };
        l.size(self, sz, sz)?;
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        if self.selected {
            let active = vp.view.carve_hstart(1).0;
            r.fill("blue", active, '\u{2588}')?;
        }
        r.style.push_layer(&self.color);
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }
}

#[derive(StatefulNode)]
struct StatusBar {
    state: NodeState,
}

#[derive_commands]
impl StatusBar {}

impl Node for StatusBar {
    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        r.style.push_layer("statusbar");
        r.text("text", self.vp().view.line(0), "listgym")?;
        Ok(())
    }
}

#[derive(StatefulNode)]
struct ListGym {
    state: NodeState,
    content: frame::Frame<List<Block>>,
    statusbar: StatusBar,
}

#[derive_commands]
impl ListGym {
    fn new() -> Self {
        let nodes: Vec<Block> = (0..10).map(|_| Block::new()).collect();
        ListGym {
            state: NodeState::default(),
            content: frame::Frame::new(List::new(nodes)),
            statusbar: StatusBar {
                state: NodeState::default(),
            },
        }
    }

    #[command]
    /// Add an item after the current focus
    fn add_item(&mut self, _c: &dyn Context) {
        self.content.child.insert_after(Block::new());
    }

    #[command]
    /// Add an item at the end of the list
    fn append_item(&mut self, _c: &dyn Context) {
        self.content.child.append(Block::new());
    }

    #[command]
    /// Add an item at the end of the list
    fn clear(&mut self, _c: &dyn Context) {
        self.content.child.clear();
    }
}

impl Node for ListGym {
    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        // First fill our viewport so that child placement calculations use the
        // correct geometry. Without this the initial layout runs with a zero
        // sized viewport, causing the list to appear empty until a subsequent
        // event triggers another layout.
        l.fill(self, sz)?;
        let vp = self.vp();
        let (a, b) = vp.view.carve_vend(1);
        l.place(&mut self.content, vp, a)?;
        l.place(&mut self.statusbar, vp, b)?;
        Ok(())
    }

    fn accept_focus(&mut self) -> bool {
        true
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.statusbar)?;
        f(&mut self.content)?;
        Ok(())
    }
}

impl Loader for ListGym {
    fn load(c: &mut Canopy) {
        c.add_commands::<List<Block>>();
        c.add_commands::<ListGym>();
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
    cnpy.style.add(
        "red/text",
        Some(solarized::RED),
        None,
        Some(canopy::style::AttrSet::default()),
    );
    cnpy.style.add(
        "blue/text",
        Some(solarized::BLUE),
        None,
        Some(canopy::style::AttrSet::default()),
    );
    cnpy.style.add(
        "green/text",
        Some(solarized::GREEN),
        None,
        Some(canopy::style::AttrSet::default()),
    );
    cnpy.style.add(
        "statusbar/text",
        Some(solarized::BLUE),
        None,
        Some(canopy::style::AttrSet::default()),
    );
    Root::<ListGym>::load(&mut cnpy);

    let args = Args::parse();
    if args.commands {
        cnpy.print_command_table(&mut std::io::stdout())?;
        return Ok(());
    }

    canopy::Binder::new(&mut cnpy)
        .defaults::<Root<ListGym>>()
        .with_path("list_gym")
        .key('p', "print(\"foo\")")
        .key('a', "list_gym::add_item()")
        .key('A', "list_gym::append_item()")
        .key('C', "list_gym::clear()")
        .key('q', "root::quit()")
        .key('g', "list::select_first()")
        .key('G', "list::select_last()")
        .key('d', "list::delete_selected()")
        .key('j', "list::select_next()")
        .mouse(event::mouse::Action::ScrollDown, "list::select_next()")
        .key('k', "list::select_prev()")
        .mouse(event::mouse::Action::ScrollUp, "list::select_prev()")
        .key(key::KeyCode::Down, "list::select_next()")
        .key(key::KeyCode::Up, "list::select_prev()")
        .key('J', "list::scroll_down()")
        .key('K', "list::scroll_up()")
        .key('h', "list::scroll_left()")
        .key('l', "list::scroll_right()")
        .key(key::KeyCode::Left, "list::scroll_left()")
        .key(key::KeyCode::Right, "list::scroll_right()")
        .key(key::KeyCode::PageDown, "list::page_down()")
        .key(' ', "list::page_down()")
        .key(key::KeyCode::PageUp, "list::page_up()");

    runloop(
        cnpy,
        Root::new(ListGym::new()).with_inspector(args.inspector),
    )?;
    Ok(())
}
