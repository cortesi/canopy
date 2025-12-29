use std::time::Duration;

use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ViewContext, command, derive_commands,
    error::{Error, Result},
    event::{key, mouse},
    render::Render,
    style::{AttrSet, solarized},
    widget::Widget,
    widgets::{CanvasWidth, List, Panes, Root, Text, VStack, frame},
};
use rand::Rng;

/// Sample text content for list items.
const TEXT: &str = "What a struggle must have gone on during long centuries between the several kinds of trees, each annually scattering its seeds by the thousand; what war between insect and insect — between insects, snails, and other animals with birds and beasts of prey — all striving to increase, all feeding on each other, or on the trees, their seeds and seedlings, or on the other plants which first clothed the ground and thus checked the growth of the trees.";

/// Alternating color names for list items.
const COLORS: &[&str] = &["red", "blue"];

/// Build a text item for the list.
fn list_item(index: usize) -> Text {
    let mut rng = rand::rng();
    let wrap_width = rng.random_range(10..150);
    let color = COLORS[index % COLORS.len()];

    Text::new(TEXT)
        .with_wrap_width(wrap_width)
        .with_canvas_width(CanvasWidth::Intrinsic)
        .with_style(format!("{color}/text"))
}

/// Status bar widget for the list gym demo.
pub struct StatusBar {
    /// Panes node used to count columns and focus.
    panes_id: NodeId,
}

#[derive_commands]
impl StatusBar {
    /// Construct a status bar tied to the panes node.
    pub fn new(panes_id: NodeId) -> Self {
        Self { panes_id }
    }

    /// Build the status text based on the focused column.
    fn label(&self, ctx: &dyn ViewContext) -> String {
        let columns = ctx.children_of(self.panes_id);
        let total = columns.len();
        let focused = columns
            .iter()
            .position(|node| ctx.node_is_on_focus_path(*node));

        match (focused, total) {
            (Some(idx), total) if total > 0 => {
                format!("listgym  col {}/{}", idx + 1, total)
            }
            _ => "listgym".to_string(),
        }
    }
}

impl Widget for StatusBar {
    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("statusbar");
        let label = self.label(ctx);
        r.text("text", ctx.view().outer_rect_local().line(0), &label)?;
        Ok(())
    }
}

/// Root node for the list gym demo.
pub struct ListGym;

impl Default for ListGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl ListGym {
    /// Construct a new list gym demo.
    pub fn new() -> Self {
        Self
    }

    /// Ensure the list, panes, and status bar are created.
    fn ensure_tree(&self, c: &mut dyn Context) -> Result<()> {
        if !c.children().is_empty() {
            return Ok(());
        }

        let panes_id = c.add_orphan(Panes::new());
        let status_id = c.add_orphan(StatusBar::new(panes_id));
        c.add_child(
            VStack::new()
                .push_flex(panes_id, 1)
                .push_fixed(status_id, 1),
        )?;

        let frame_id = Self::create_column(c)?;
        c.with_widget(panes_id, |panes: &mut Panes, ctx| {
            panes.insert_col(ctx, frame_id)
        })?;

        Ok(())
    }

    /// Create a framed list column and return the frame node id.
    fn create_column(c: &mut dyn Context) -> Result<NodeId> {
        let list_id =
            c.add_orphan(List::<Text>::new().with_selection_indicator("list/selected", "█ ", true));
        let frame_id = frame::Frame::wrap(c, list_id)?;
        // Add initial items
        c.with_widget(list_id, |list: &mut List<Text>, ctx| {
            for i in 0..10 {
                list.append(ctx, list_item(i))?;
            }
            Ok(())
        })?;
        Ok(frame_id)
    }

    /// Execute a closure with mutable access to the list widget.
    fn with_list<F, R>(&self, c: &mut dyn Context, mut f: F) -> Result<R>
    where
        F: FnMut(&mut List<Text>, &mut dyn Context) -> Result<R>,
    {
        self.ensure_tree(c)?;
        let list_id = Self::list_id(c)?;
        c.with_widget(list_id, |list: &mut List<Text>, ctx| f(list, ctx))
    }

    /// Panes node id, if initialized.
    fn panes_id(c: &dyn Context) -> Result<NodeId> {
        c.find_node("*/panes")
            .ok_or_else(|| Error::Invalid("panes not initialized".into()))
    }

    /// Find the list to target for list commands.
    fn list_id(c: &dyn Context) -> Result<NodeId> {
        let lists = c.find_nodes("*/frame/list");
        if lists.is_empty() {
            return Err(Error::Invalid("list not initialized".into()));
        }
        if let Some(id) = lists
            .iter()
            .copied()
            .find(|id| c.node_is_on_focus_path(*id))
        {
            return Ok(id);
        }
        Ok(lists[0])
    }

    #[command]
    /// Add an item after the current focus.
    pub fn add_item(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            let index = list.selected_index().unwrap_or(0) + 1;
            list.insert(ctx, index, list_item(index))?;
            Ok(())
        })
    }

    #[command]
    /// Add an item at the end of the list.
    pub fn append_item(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            let index = list.len();
            list.append(ctx, list_item(index))?;
            Ok(())
        })
    }

    #[command]
    /// Clear all items from the list.
    pub fn clear(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.clear(ctx)?;
            Ok(())
        })
    }

    #[command]
    /// Add a new column containing a list.
    pub fn add_column(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        let panes_id = Self::panes_id(c)?;
        let frame_id = Self::create_column(c)?;
        c.with_widget(panes_id, |panes: &mut Panes, ctx| {
            panes.insert_col(ctx, frame_id)
        })?;
        Ok(())
    }

    #[command]
    /// Delete the focused column.
    pub fn delete_column(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        let panes_id = Self::panes_id(c)?;
        c.with_widget(panes_id, |panes: &mut Panes, ctx| panes.delete_focus(ctx))?;
        Ok(())
    }

    #[command]
    /// Move focus to the next column.
    pub fn next_column(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        let panes_id = Self::panes_id(c)?;
        c.with_widget(panes_id, |panes: &mut Panes, ctx| {
            panes.focus_next_column(ctx);
            Ok(())
        })?;
        Ok(())
    }

    #[command]
    /// Move focus to the previous column.
    pub fn prev_column(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        let panes_id = Self::panes_id(c)?;
        c.with_widget(panes_id, |panes: &mut Panes, ctx| {
            panes.focus_prev_column(ctx);
            Ok(())
        })?;
        Ok(())
    }
}

impl Widget for ListGym {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c).ok()?;
        None
    }
}

impl Loader for ListGym {
    fn load(c: &mut Canopy) {
        c.add_commands::<List<Text>>();
        c.add_commands::<Panes>();
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
        "statusbar/text",
        Some(solarized::BLUE),
        None,
        Some(AttrSet::default()),
    );
    // Selection indicator style for list items
    cnpy.style.add_fg("list/selected", solarized::BLUE);

    Binder::new(cnpy)
        .defaults::<Root>()
        .with_path("list_gym")
        .key('p', "print(\"list gym\")")
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
        .key('s', "list_gym::add_column()")
        .key('x', "list_gym::delete_column()")
        .key(key::KeyCode::Tab, "list_gym::next_column()")
        .key(key::KeyCode::BackTab, "list_gym::prev_column()")
        .key(key::KeyCode::PageDown, "list::page_down()")
        .key(' ', "list::page_down()")
        .key(key::KeyCode::PageUp, "list::page_up()");
}

#[cfg(test)]
mod tests {
    use canopy::testing::harness::Harness;

    use super::*;

    fn panes_id(harness: &Harness) -> NodeId {
        harness
            .find_node("list_gym/*/panes")
            .expect("panes not initialized")
    }

    fn list_id(harness: &Harness) -> NodeId {
        harness
            .find_node("list_gym/*/frame/list")
            .expect("list not initialized")
    }

    fn column_list_ids(harness: &Harness) -> Vec<NodeId> {
        harness.find_nodes("list_gym/*/frame/list")
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

        let list_node = list_id(&harness);
        let mut len = 0;
        harness.with_widget(list_node, |list: &mut List<Text>| {
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

        let list_node = list_id(&harness);
        let mut initial_selected = None;
        harness.with_widget(list_node, |list: &mut List<Text>| {
            initial_selected = list.selected_index();
        });

        // Navigate using list commands (these are loaded by the List type)
        harness.script("list::select_last()")?;

        let mut selected = None;
        harness.with_widget(list_node, |list: &mut List<Text>| {
            selected = list.selected_index();
        });

        assert!(selected > initial_selected);

        Ok(())
    }

    #[test]
    fn test_listgym_adds_and_deletes_columns() -> Result<()> {
        let mut harness = create_test_harness()?;
        harness.render()?;

        let panes_id = panes_id(&harness);
        let initial_cols = harness
            .canopy
            .core
            .nodes
            .get(panes_id)
            .expect("panes node missing")
            .children
            .len();

        harness.script("list_gym::add_column()")?;
        let after_add = harness
            .canopy
            .core
            .nodes
            .get(panes_id)
            .expect("panes node missing")
            .children
            .len();
        assert_eq!(after_add, initial_cols + 1);

        harness.script("list_gym::delete_column()")?;
        let after_delete = harness
            .canopy
            .core
            .nodes
            .get(panes_id)
            .expect("panes node missing")
            .children
            .len();
        assert_eq!(after_delete, initial_cols);

        Ok(())
    }

    #[test]
    fn test_listgym_add_item_command() -> Result<()> {
        let mut harness = create_test_harness()?;
        harness.render()?;

        harness.script("list_gym::add_item()")?;
        harness.script("list_gym::add_column()")?;
        harness.script("list_gym::add_item()")?;

        Ok(())
    }

    #[test]
    fn test_listgym_tabs_between_columns() -> Result<()> {
        let mut harness = create_test_harness()?;
        harness.render()?;

        harness.script("list_gym::add_column()")?;
        let lists = column_list_ids(&harness);
        assert_eq!(lists.len(), 2);
        assert!(harness.canopy.core.is_on_focus_path(lists[1]));

        harness.script("list_gym::next_column()")?;
        assert!(harness.canopy.core.is_on_focus_path(lists[0]));

        harness.script("list_gym::prev_column()")?;
        assert!(harness.canopy.core.is_on_focus_path(lists[1]));

        Ok(())
    }
}
