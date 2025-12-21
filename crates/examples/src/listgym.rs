use canopy::{
    Binder, Canopy, Context, Layout, Loader, command, derive_commands,
    error::Result,
    event::{key, mouse},
    geom::{Expanse, Rect},
    node::Node,
    render::Render,
    state::{NodeState, StatefulNode},
    style::{AttrSet, solarized},
    widgets::{Root, Text, frame, list::*},
};
use rand::Rng;

/// Sample text content for list items.
const TEXT: &str = "What a struggle must have gone on during long centuries between the several kinds of trees, each annually scattering its seeds by the thousand; what war between insect and insect — between insects, snails, and other animals with birds and beasts of prey — all striving to increase, all feeding on each other, or on the trees, their seeds and seedlings, or on the other plants which first clothed the ground and thus checked the growth of the trees.";

/// Alternating color names for list items.
const COLORS: &[&str] = &["red", "blue"];

#[derive(canopy::StatefulNode)]
/// List item block for the list gym demo.
pub struct Block {
    /// Node state.
    state: NodeState,
    /// Text content.
    child: Text,
    /// Color layer name.
    color: String,
    /// Selection state.
    selected: bool,
}

#[derive_commands]
impl Block {
    /// Construct a block for the given index.
    pub fn new(index: usize) -> Self {
        let mut rng = rand::rng();
        Self {
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

#[derive(canopy::StatefulNode)]
/// Status bar widget for the list gym demo.
pub struct StatusBar {
    /// Node state.
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

#[derive(canopy::StatefulNode)]
/// Root node for the list gym demo.
pub struct ListGym {
    /// Node state.
    state: NodeState,
    /// List content frame.
    content: frame::Frame<List<Block>>,
    /// Status bar widget.
    statusbar: StatusBar,
}

#[derive_commands]
impl Default for ListGym {
    fn default() -> Self {
        Self::new()
    }
}

impl ListGym {
    /// Construct a new list gym demo.
    pub fn new() -> Self {
        let nodes: Vec<Block> = (0..10).map(Block::new).collect();
        Self {
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
        let index = self.content.child().selected_index().unwrap_or(0) + 1;
        self.content.child_mut().insert_after(Block::new(index));
    }

    #[command]
    /// Add an item at the end of the list
    pub fn append_item(&mut self, _c: &dyn Context) {
        let index = self.content.child().len();
        self.content.child_mut().append(Block::new(index));
    }

    #[command]
    /// Add an item at the end of the list
    pub fn clear(&mut self, _c: &dyn Context) {
        self.content.child_mut().clear();
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
        c.add_commands::<Self>();
    }
}

/// Install key bindings for the list gym demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    cnpy.style.add(
        "red/text",
        Some(solarized::RED),
        None,
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "blue/text",
        Some(solarized::BLUE),
        None,
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "green/text",
        Some(solarized::GREEN),
        None,
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "statusbar/text",
        Some(solarized::BLUE),
        None,
        Some(AttrSet::default()),
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
    use canopy::{state::StatefulNode, testing::harness::Harness};

    use super::*;

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
        assert_eq!(harness.root.content.child().len(), 10);

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

        // Get initial selected
        let initial_selected = harness.root.content.child().selected_index();

        // Navigate using list commands (these are loaded by the List type)
        harness.script("list::select_last()")?;

        // Verify selected changed
        assert!(harness.root.content.child().selected_index() > initial_selected);

        // Navigate back to first
        harness.script("list::select_first()")?;

        // Verify we're back at the start
        assert_eq!(harness.root.content.child().selected_index(), Some(0));

        Ok(())
    }

    #[test]
    fn test_view_follows_selection() -> Result<()> {
        // Create a small harness that can only show a few items
        let mut harness = Harness::builder(ListGym::new())
            .size(40, 10) // Small height to force scrolling
            .build()?;

        // Load commands
        ListGym::load(&mut harness.canopy);

        // Initial render
        harness.render()?;

        // Get the initial view position of the list
        let initial_view = harness.root.content.child().vp().view();
        println!("Initial view: {initial_view:?}");

        // Move selection down past what's visible
        // The list has 10 items (0-9), and with a height of 10 minus frame and status bar,
        // only a few items are visible at once
        for i in 0..8 {
            harness.script("list::select_next()")?;
            let selected = harness.root.content.child().selected_index().unwrap();
            println!("After select_next {i}: selected = {selected}");
        }

        // The selected item should now be 8
        assert_eq!(harness.root.content.child().selected_index(), Some(8));

        // Get the current view position
        let current_view = harness.root.content.child().vp().view();
        println!("Current view after navigation: {current_view:?}");

        // The view should have scrolled down to keep the selected item visible
        // If the view hasn't moved, this test should fail
        assert!(
            current_view.tl.y > initial_view.tl.y,
            "View should have scrolled down to follow selection. Initial y: {}, Current y: {}",
            initial_view.tl.y,
            current_view.tl.y
        );

        // The test demonstrates that the view should follow the selection,
        // but currently it doesn't, so this assertion should fail

        Ok(())
    }
}
