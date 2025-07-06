use rand::Rng;

use canopy::{
    event::{key, mouse},
    geom::{Expanse, Rect},
    style::solarized,
    widgets::{frame, list::*, Text},
    *,
};

const TEXT: &str = "What a struggle must have gone on during long centuries between the several kinds of trees, each annually scattering its seeds by the thousand; what war between insect and insect — between insects, snails, and other animals with birds and beasts of prey — all striving to increase, all feeding on each other, or on the trees, their seeds and seedlings, or on the other plants which first clothed the ground and thus checked the growth of the trees.";

const COLORS: &[&str] = &["red", "blue"];

#[derive(StatefulNode)]
pub struct Block {
    state: NodeState,
    child: Text,
    color: String,
    selected: bool,
}

#[derive_commands]
impl Block {
    pub fn new(index: usize) -> Self {
        let mut rng = rand::rng();
        Block {
            state: NodeState::default(),
            child: Text::new(TEXT).with_fixed_width(rng.random_range(10..150)),
            color: String::from(COLORS[index % 2]),
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
    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        // Set our viewport before laying out children so geometry calculations
        // are based on the correct size.
        self.fill(sz)?;
        let loc = Rect::new(2, 0, sz.w.saturating_sub(2), sz.h);
        l.place(&mut self.child, loc)?;

        let vp = self.child.vp();
        let sz = Expanse {
            w: vp.canvas().w + 2,
            h: self.child.vp().canvas().h,
        };
        self.fit_size(sz, sz);
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        if self.selected {
            let active = vp.view().carve_hstart(1).0;
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
pub struct StatusBar {
    state: NodeState,
}

#[derive_commands]
impl StatusBar {}

impl Node for StatusBar {
    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        r.style.push_layer("statusbar");
        r.text("text", self.vp().view().line(0), "listgym")?;
        Ok(())
    }
}

#[derive(StatefulNode)]
pub struct ListGym {
    state: NodeState,
    content: frame::Frame<List<Block>>,
    statusbar: StatusBar,
}

#[derive_commands]
impl Default for ListGym {
    fn default() -> Self {
        Self::new()
    }
}

impl ListGym {
    pub fn new() -> Self {
        let nodes: Vec<Block> = (0..10).map(Block::new).collect();
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
    pub fn add_item(&mut self, _c: &dyn Context) {
        let index = self.content.child.offset + 1;
        self.content.child.insert_after(Block::new(index));
    }

    #[command]
    /// Add an item at the end of the list
    pub fn append_item(&mut self, _c: &dyn Context) {
        let index = self.content.child.len();
        self.content.child.append(Block::new(index));
    }

    #[command]
    /// Add an item at the end of the list
    pub fn clear(&mut self, _c: &dyn Context) {
        self.content.child.clear();
    }
}

impl Node for ListGym {
    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        // First fill our viewport so that child placement calculations use the
        // correct geometry. Without this the initial layout runs with a zero
        // sized viewport, causing the list to appear empty until a subsequent
        // event triggers another layout.
        self.fill(sz)?;
        let vp = self.vp();
        let (a, b) = vp.screen_rect().carve_vend(1);
        l.place(&mut self.content, a)?;
        l.place(&mut self.statusbar, b)?;
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

pub fn setup_bindings(cnpy: &mut Canopy) {
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

    Binder::new(cnpy)
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
        .mouse(mouse::Action::ScrollDown, "list::select_next()")
        .key('k', "list::select_prev()")
        .mouse(mouse::Action::ScrollUp, "list::select_prev()")
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use canopy::tutils::harness::Harness;

    fn create_test_harness() -> Result<Harness<ListGym>> {
        let root = ListGym::new();
        let mut harness = Harness::new(root)?;

        // Load the commands so scripts can find them
        ListGym::load(&mut harness.canopy);

        Ok(harness)
    }

    #[test]
    fn test_listgym_creates_and_renders() -> Result<()> {
        let root = ListGym::new();
        let mut harness = Harness::new(root)?;

        // Test that we can render without crashing
        harness.render()?;

        Ok(())
    }

    #[test]
    fn test_listgym_initial_state() -> Result<()> {
        let root = ListGym::new();
        let mut harness = Harness::new(root)?;

        // Verify initial state
        assert_eq!(harness.root.content.child.len(), 10);

        // Render and verify it still works
        harness.render()?;

        Ok(())
    }

    #[test]
    fn test_listgym_with_harness() -> Result<()> {
        let mut harness = Harness::builder(ListGym::new()).size(80, 20).build()?;

        // Test that we can render with a specific size
        harness.render()?;

        // The harness should have created a render buffer
        let _buf = harness.buf();

        Ok(())
    }

    #[test]
    fn test_harness_script_method() -> Result<()> {
        let mut harness = create_test_harness()?;
        harness.render()?;

        // Test that we can execute a simple print script
        harness.script("print(\"Hello from script\")")?;

        Ok(())
    }

    #[test]
    fn test_harness_script_with_list_navigation() -> Result<()> {
        let mut harness = create_test_harness()?;
        harness.render()?;

        // Get initial offset
        let initial_offset = harness.root.content.child.offset;

        // Navigate using list commands (these are loaded by the List type)
        harness.script("list::select_last()")?;

        // Verify offset changed
        assert!(harness.root.content.child.offset > initial_offset);

        // Navigate back to first
        harness.script("list::select_first()")?;

        // Verify we're back at the start
        assert_eq!(harness.root.content.child.offset, 0);

        Ok(())
    }
}
