use std::{any::Any, time::Duration};

use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ViewContext, command, derive_commands,
    error::Result,
    event::{Event, key, mouse},
    geom::{Expanse, Rect},
    layout::{Dimension, Display, FlexDirection, Style},
    render::Render,
    style::{AttrSet, solarized},
    widget::{EventOutcome, Widget},
    widgets::{Root, frame, list::*},
};
use rand::Rng;

/// Sample text content for list items.
const TEXT: &str = "What a struggle must have gone on during long centuries between the several kinds of trees, each annually scattering its seeds by the thousand; what war between insect and insect — between insects, snails, and other animals with birds and beasts of prey — all striving to increase, all feeding on each other, or on the trees, their seeds and seedlings, or on the other plants which first clothed the ground and thus checked the growth of the trees.";

/// Alternating color names for list items.
const COLORS: &[&str] = &["red", "blue"];

/// List item block for the list gym demo.
pub struct Block {
    /// Text content.
    text: String,
    /// Color layer name.
    color: String,
    /// Fixed wrapping width.
    width: u32,
}

impl Block {
    /// Construct a block for the given index.
    pub fn new(index: usize) -> Self {
        let mut rng = rand::rng();
        let width = rng.random_range(10..150);
        Self {
            text: TEXT.to_string(),
            color: String::from(COLORS[index % 2]),
            width,
        }
    }

    /// Wrap and pad the block text for the configured width.
    fn lines(&self) -> Vec<String> {
        let wrap_width = self.width.max(1) as usize;
        textwrap::wrap(&self.text, wrap_width)
            .into_iter()
            .map(|line| format!("{:width$}", line, width = wrap_width))
            .collect()
    }
}

impl ListItem for Block {
    fn measure(&self, _available_width: u32) -> Expanse {
        let lines = self.lines();
        let height = lines.len().max(1) as u32;
        Expanse::new(self.width.saturating_add(2), height)
    }

    fn render(&mut self, rndr: &mut Render, area: Rect, selected: bool) -> Result<()> {
        if selected {
            let (active, _) = area.carve_hstart(1);
            rndr.fill("blue", active, '\u{2588}')?;
        }

        let text_area = Rect::new(
            area.tl.x.saturating_add(2),
            area.tl.y,
            area.w.saturating_sub(2),
            area.h,
        );
        let lines = self.lines();
        let style = format!("{}/text", self.color);

        for (i, line) in lines.iter().enumerate() {
            if i as u32 >= text_area.h {
                break;
            }
            rndr.text(&style, text_area.line(i as u32), line)?;
        }
        Ok(())
    }
}

/// Status bar widget for the list gym demo.
pub struct StatusBar;

#[derive_commands]
impl StatusBar {}

impl Widget for StatusBar {
    fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("statusbar");
        r.text("text", ctx.view().line(0), "listgym")?;
        Ok(())
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }
}

/// Root node for the list gym demo.
pub struct ListGym {
    /// List frame node id.
    content_id: Option<NodeId>,
    /// List node id.
    list_id: Option<NodeId>,
}

impl Default for ListGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl ListGym {
    /// Construct a new list gym demo.
    pub fn new() -> Self {
        Self {
            content_id: None,
            list_id: None,
        }
    }

    /// Ensure the list, frame, and status bar are created.
    fn ensure_tree(&mut self, c: &mut dyn Context) {
        if self.content_id.is_some() {
            return;
        }

        let nodes: Vec<Block> = (0..10).map(Block::new).collect();
        let list_id = c.add(Box::new(List::new(nodes)));
        let content_id = c.add(Box::new(frame::Frame::new()));
        c.mount_child(content_id, list_id)
            .expect("Failed to mount list");

        let status_id = c.add(Box::new(StatusBar));
        c.set_children(c.node_id(), vec![content_id, status_id])
            .expect("Failed to attach children");

        let mut update_root = |style: &mut Style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Column;
        };
        c.with_style(c.node_id(), &mut update_root)
            .expect("Failed to style root");

        let mut content_style = |style: &mut Style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        };
        c.with_style(content_id, &mut content_style)
            .expect("Failed to style content");
        c.with_style(list_id, &mut content_style)
            .expect("Failed to style list");

        let mut status_style = |style: &mut Style| {
            style.size.height = Dimension::Points(1.0);
            style.flex_shrink = 0.0;
        };
        c.with_style(status_id, &mut status_style)
            .expect("Failed to style statusbar");

        self.content_id = Some(content_id);
        self.list_id = Some(list_id);
    }

    /// Execute a closure with mutable access to the list widget.
    fn with_list<F>(&self, c: &mut dyn Context, mut f: F) -> Result<()>
    where
        F: FnMut(&mut List<Block>) -> Result<()>,
    {
        let list_id = self.list_id.expect("list not initialized");
        c.with_widget_mut(list_id, &mut |widget, _ctx| {
            let any = widget as &mut dyn Any;
            let list = any
                .downcast_mut::<List<Block>>()
                .expect("list type mismatch");
            f(list)
        })
    }

    #[command]
    /// Add an item after the current focus
    pub fn add_item(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list| {
            let index = list.selected_index().unwrap_or(0) + 1;
            list.insert_after(Block::new(index));
            Ok(())
        })
    }

    #[command]
    /// Add an item at the end of the list
    pub fn append_item(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list| {
            let index = list.len();
            list.append(Block::new(index));
            Ok(())
        })
    }

    #[command]
    /// Add an item at the end of the list
    pub fn clear(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list| {
            list.clear();
            Ok(())
        })
    }
}

impl Widget for ListGym {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c);
        None
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
        .defaults::<Root>()
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
    use std::any::Any;

    use canopy::testing::harness::Harness;

    use super::*;

    fn list_id(harness: &mut Harness) -> NodeId {
        harness.with_root_widget(|widget| {
            let any = widget as &mut dyn Any;
            let root = any.downcast_mut::<ListGym>().expect("root type mismatch");
            root.list_id.expect("list not initialized")
        })
    }

    fn create_test_harness() -> Result<Harness> {
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
        harness.render()?;

        let list_node = list_id(&mut harness);
        let mut len = 0;
        harness.with_widget(list_node, |widget| {
            let any = widget as &mut dyn Any;
            let list = any
                .downcast_mut::<List<Block>>()
                .expect("list type mismatch");
            len = list.len();
        });

        assert_eq!(len, 10);

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

        let list_node = list_id(&mut harness);
        let mut initial_selected = None;
        harness.with_widget(list_node, |widget| {
            let any = widget as &mut dyn Any;
            let list = any
                .downcast_mut::<List<Block>>()
                .expect("list type mismatch");
            initial_selected = list.selected_index();
        });

        // Navigate using list commands (these are loaded by the List type)
        harness.script("list::select_last()")?;

        let mut selected = None;
        harness.with_widget(list_node, |widget| {
            let any = widget as &mut dyn Any;
            let list = any
                .downcast_mut::<List<Block>>()
                .expect("list type mismatch");
            selected = list.selected_index();
        });

        assert!(selected > initial_selected);

        Ok(())
    }
}
