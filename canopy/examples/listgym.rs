use rand::seq::SliceRandom;
use rand::Rng;

use canopy::{
    backend::crossterm::runloop,
    event::key,
    geom::{Expanse, Rect},
    inspector::Inspector,
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
    fn fit(&mut self, target: Expanse) -> Result<Expanse> {
        self.child.fit(Expanse {
            w: target.w - 2,
            h: target.h,
        })
    }

    fn render(&mut self, _c: &dyn Core, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        let (_, screen) = vp.screen_rect().carve_hstart(2);
        let outer = self.child.fit(screen.into())?;
        let view = Rect {
            tl: vp.view_rect().tl,
            w: vp.view_rect().w.saturating_sub(2),
            h: vp.view_rect().h,
        };
        self.child
            .set_viewport(ViewPort::new(outer, view, screen.tl)?);
        if self.selected {
            let active = vp.view_rect().carve_hstart(1).0;
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
    fn render(&mut self, _c: &dyn Core, r: &mut Render) -> Result<()> {
        r.style.push_layer("statusbar");
        r.text("text", self.vp().view_rect().first_line(), "listgym")?;
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
    fn add_item(&mut self, _c: &dyn Core) {
        self.content.child.insert_after(Block::new());
    }

    #[command]
    /// Add an item at the end of the list
    fn append_item(&mut self, _c: &dyn Core) {
        self.content.child.append(Block::new());
    }

    #[command]
    /// Add an item at the end of the list
    fn delete_selected(&mut self, c: &dyn Core) {
        self.content.child.delete_selected(c);
    }

    #[command]
    /// Add an item at the end of the list
    fn clear(&mut self, _c: &dyn Core) {
        self.content.child.clear();
    }
}

impl Node for ListGym {
    fn render(&mut self, _c: &dyn Core, _: &mut Render) -> Result<()> {
        let (a, b) = self.vp().carve_vend(1);
        fit(&mut self.statusbar, b)?;
        fit(&mut self.content, a)?;
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
    cnpy.load_commands::<List<Block>>();
    cnpy.load_commands::<ListGym>();
    cnpy.load_commands::<Root<ListGym>>();

    cnpy.bind_key('a', "root", "listgym::add_item()")?;
    cnpy.bind_key('A', "root", "listgym::append_item()")?;
    cnpy.bind_key('C', "root", "listgym::clear()")?;
    cnpy.bind_key('d', "root", "listgym::delete_selected()")?;
    cnpy.bind_key('q', "root", "root::quit()")?;

    cnpy.bind_key('g', "root", "list::select_first()")?;
    cnpy.bind_key('G', "root", "list::select_last()")?;

    cnpy.bind_key('j', "root", "list::select_next()")?;
    cnpy.bind_mouse(
        event::mouse::Action::ScrollDown,
        "root",
        "list::select_next()",
    )?;

    cnpy.bind_key('k', "root", "list::select_prev()")?;
    cnpy.bind_mouse(
        event::mouse::Action::ScrollUp,
        "root",
        "list::select_prev()",
    )?;

    cnpy.bind_key(key::KeyCode::Down, "root", "list::select_next()")?;
    cnpy.bind_key(key::KeyCode::Up, "root", "list::select_prev()")?;

    cnpy.bind_key('J', "root", "list::scroll_down()")?;
    cnpy.bind_key('K', "root", "list::scroll_up()")?;
    cnpy.bind_key('h', "root", "list::scroll_left()")?;
    cnpy.bind_key('l', "root", "list::scroll_right()")?;
    cnpy.bind_key(key::KeyCode::Left, "root", "list::scroll_left()")?;
    cnpy.bind_key(key::KeyCode::Right, "root", "list::scroll_right()")?;

    cnpy.bind_key(key::KeyCode::PageDown, "root", "list::page_down()")?;
    cnpy.bind_key(' ', "root", "list::page_down()")?;
    cnpy.bind_key(key::KeyCode::PageUp, "root", "list::page_up()")?;

    let root = Inspector::new(key::Ctrl + key::KeyCode::Right, Root::new(ListGym::new()));
    runloop(cnpy, root)?;
    Ok(())
}
