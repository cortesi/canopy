use std::time::Duration;

use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ViewContext, command, derive_commands,
    error::{Error, Result},
    event::{key, mouse},
    geom::{Expanse, Line, Point, Rect},
    layout::{Layout, Sizing},
    render::Render,
    style::{AttrSet, solarized},
    widget::Widget,
    widgets::{Panes, Root, frame, list::*},
};
use rand::Rng;

/// Sample text content for list items.
const TEXT: &str = "What a struggle must have gone on during long centuries between the several kinds of trees, each annually scattering its seeds by the thousand; what war between insect and insect — between insects, snails, and other animals with birds and beasts of prey — all striving to increase, all feeding on each other, or on the trees, their seeds and seedlings, or on the other plants which first clothed the ground and thus checked the growth of the trees.";

/// Alternating color names for list items.
const COLORS: &[&str] = &["red", "blue"];

/// List item block for the list gym demo.
pub struct Block {
    /// Color layer name.
    color: String,
    /// Fixed wrapping width.
    width: u32,
    /// Cached wrapped lines.
    lines: Vec<String>,
}

impl Block {
    /// Construct a block for the given index.
    pub fn new(index: usize) -> Self {
        let mut rng = rand::rng();
        let width = rng.random_range(10..150);
        let lines = Self::wrap_lines(TEXT, width);
        Self {
            color: String::from(COLORS[index % 2]),
            width,
            lines,
        }
    }

    /// Wrap and pad the block text for the configured width.
    fn wrap_lines(text: &str, width: u32) -> Vec<String> {
        let wrap_width = width.max(1) as usize;
        textwrap::wrap(text, wrap_width)
            .into_iter()
            .map(|line| format!("{:width$}", line, width = wrap_width))
            .collect()
    }
}

impl ListItem for Block {
    fn measure(&self, _available_width: u32) -> Expanse {
        let height = self.lines.len().max(1) as u32;
        Expanse::new(self.width.saturating_add(2), height)
    }

    fn render(
        &mut self,
        rndr: &mut Render,
        area: Rect,
        selected: bool,
        offset: Point,
        _full_size: Expanse,
    ) -> Result<()> {
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }

        let visible_start = offset.x;
        let visible_end = visible_start.saturating_add(area.w);

        if selected && visible_start == 0 {
            let (active, _) = area.carve_hstart(1);
            rndr.fill("blue", active, '\u{2588}')?;
        }

        let lines = &self.lines;
        let style = format!("{}/text", self.color);
        let text_start = 2u32;
        let text_end = text_start.saturating_add(self.width);
        let start_line = offset.y as usize;

        for (row, line) in lines
            .iter()
            .skip(start_line)
            .take(area.h as usize)
            .enumerate()
        {
            let row_y = area.tl.y.saturating_add(row as u32);
            let vis_text_start = text_start.max(visible_start);
            let vis_text_end = text_end.min(visible_end);
            if vis_text_start >= vis_text_end {
                continue;
            }

            let slice_start = (vis_text_start - text_start) as usize;
            let slice_len = (vis_text_end - vis_text_start) as usize;
            let text: String = line.chars().skip(slice_start).take(slice_len).collect();
            let screen_x = area.tl.x.saturating_add(vis_text_start - visible_start);
            rndr.text(
                &style,
                Line::new(screen_x, row_y, vis_text_end - vis_text_start),
                &text,
            )?;
        }
        Ok(())
    }
}

/// Status bar widget for the list gym demo.
pub struct StatusBar;

#[derive_commands]
impl StatusBar {
    /// Build the status text based on the focused column.
    fn label(&self, ctx: &dyn ViewContext) -> String {
        let columns = Self::column_nodes(ctx);
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

    /// Collect column container node ids from the panes widget.
    fn column_nodes(ctx: &dyn ViewContext) -> Vec<NodeId> {
        let root_children = ctx.children_of(ctx.root_id());
        let Some(content_id) = root_children.first().copied() else {
            return Vec::new();
        };
        let content_children = ctx.children_of(content_id);
        let Some(panes_id) = content_children.first().copied() else {
            return Vec::new();
        };
        ctx.children_of(panes_id)
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

        let content_id = c
            .add_child(frame::Frame::new())
            .expect("Failed to mount content frame");
        let panes_id = c
            .add_child_to(content_id, Panes::new())
            .expect("Failed to mount panes");
        let status_id = c.add_child(StatusBar).expect("Failed to mount statusbar");

        c.with_layout(&mut |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })
        .expect("Failed to configure layout");
        c.with_layout_of(content_id, &mut |layout| {
            layout.width = Sizing::Flex(1);
            layout.height = Sizing::Flex(1);
        })
        .expect("Failed to configure content layout");
        c.with_layout_of(panes_id, &mut |layout| {
            *layout = Layout::fill();
        })
        .expect("Failed to configure panes layout");
        c.with_layout_of(status_id, &mut |layout| {
            *layout = Layout::row().flex_horizontal(1).fixed_height(1);
        })
        .expect("Failed to configure status layout");

        let (frame_id, list_id) = Self::create_column(c)?;
        c.with_widget(panes_id, |panes: &mut Panes, ctx| {
            panes.insert_col(ctx, frame_id)
        })?;
        c.set_focus(list_id);

        Ok(())
    }

    /// Create a framed list column and return (frame, list) node ids.
    fn create_column(c: &mut dyn Context) -> Result<(NodeId, NodeId)> {
        let nodes: Vec<Block> = (0..10).map(Block::new).collect();
        let list_id = c.add_orphan(List::new(nodes));
        let frame_id = c.add_orphan(frame::Frame::new());
        c.mount_child_to(frame_id, list_id)?;
        c.with_layout_of(list_id, &mut |layout| {
            *layout = Layout::fill();
        })?;
        Ok((frame_id, list_id))
    }

    /// Execute a closure with mutable access to the list widget.
    fn with_list<F>(&self, c: &mut dyn Context, mut f: F) -> Result<()>
    where
        F: FnMut(&mut List<Block>) -> Result<()>,
    {
        self.ensure_tree(c)?;
        let list_id = Self::list_id(c)?;
        c.with_widget(list_id, |list: &mut List<Block>, _ctx| f(list))
    }

    /// Content frame node id.
    fn content_id(c: &dyn Context) -> Option<NodeId> {
        c.children().first().copied()
    }

    /// List node id inside the content frame.
    fn panes_id(c: &dyn Context) -> Option<NodeId> {
        let content_id = Self::content_id(c)?;
        let children = c.children_of(content_id);
        match children.as_slice() {
            [] => None,
            [panes_id] => Some(*panes_id),
            _ => panic!("expected a single panes child"),
        }
    }

    /// Find the list to target for list commands.
    fn list_id(c: &dyn Context) -> Result<NodeId> {
        let panes_id =
            Self::panes_id(c).ok_or_else(|| Error::Invalid("list not initialized".into()))?;
        let lists = Self::column_lists(c, panes_id);
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

    /// Move focus between columns.
    fn shift_column(&self, c: &mut dyn Context, forward: bool) -> Result<()> {
        self.ensure_tree(c)?;
        let panes_id = Self::panes_id(c).expect("panes not initialized");
        let columns = Self::column_lists(c, panes_id);
        if columns.is_empty() {
            return Ok(());
        }

        let current = columns.iter().position(|id| c.node_is_on_focus_path(*id));
        let next_idx = match (current, forward) {
            (Some(idx), true) => (idx + 1) % columns.len(),
            (Some(idx), false) => (idx + columns.len() - 1) % columns.len(),
            (None, true) => 0,
            (None, false) => columns.len() - 1,
        };
        c.set_focus(columns[next_idx]);
        Ok(())
    }

    /// Collect the list nodes for each column, in column order.
    fn column_lists(c: &dyn Context, panes_id: NodeId) -> Vec<NodeId> {
        let mut lists = Vec::new();
        for column_id in c.children_of(panes_id) {
            let Some(frame_id) = c.children_of(column_id).first().copied() else {
                continue;
            };
            let Some(list_id) = c.children_of(frame_id).first().copied() else {
                continue;
            };
            lists.push(list_id);
        }
        lists
    }

    #[command]
    /// Add an item after the current focus.
    pub fn add_item(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list| {
            let index = list.selected_index().unwrap_or(0) + 1;
            list.insert_after(Block::new(index));
            Ok(())
        })
    }

    #[command]
    /// Add an item at the end of the list.
    pub fn append_item(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list| {
            let index = list.len();
            list.append(Block::new(index));
            Ok(())
        })
    }

    #[command]
    /// Clear all items from the list.
    pub fn clear(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list| {
            list.clear();
            Ok(())
        })
    }

    #[command]
    /// Add a new column containing a list.
    pub fn add_column(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        let panes_id = Self::panes_id(c).expect("panes not initialized");
        let (frame_id, list_id) = Self::create_column(c)?;
        c.with_widget(panes_id, |panes: &mut Panes, ctx| {
            panes.insert_col(ctx, frame_id)
        })?;
        c.set_focus(list_id);
        Ok(())
    }

    #[command]
    /// Delete the focused column.
    pub fn delete_column(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        let panes_id = Self::panes_id(c).expect("panes not initialized");
        c.with_widget(panes_id, |panes: &mut Panes, ctx| panes.delete_focus(ctx))?;
        Ok(())
    }

    #[command]
    /// Move focus to the next column.
    pub fn next_column(&mut self, c: &mut dyn Context) -> Result<()> {
        self.shift_column(c, true)
    }

    #[command]
    /// Move focus to the previous column.
    pub fn prev_column(&mut self, c: &mut dyn Context) -> Result<()> {
        self.shift_column(c, false)
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
        let root_children = &harness
            .canopy
            .core
            .nodes
            .get(harness.root)
            .expect("root node missing")
            .children;
        let content_id = *root_children.first().expect("list not initialized");
        let content_children = &harness
            .canopy
            .core
            .nodes
            .get(content_id)
            .expect("content node missing")
            .children;
        match content_children.as_slice() {
            [] => panic!("panes not initialized"),
            [panes_id] => *panes_id,
            _ => panic!("expected a single panes child"),
        }
    }

    fn list_id(harness: &Harness) -> NodeId {
        let panes_id = panes_id(harness);
        let panes_children = &harness
            .canopy
            .core
            .nodes
            .get(panes_id)
            .expect("panes node missing")
            .children;
        let column_id = *panes_children.first().expect("pane column not initialized");
        let column_children = &harness
            .canopy
            .core
            .nodes
            .get(column_id)
            .expect("column node missing")
            .children;
        let frame_id = *column_children.first().expect("frame not initialized");
        let frame_children = &harness
            .canopy
            .core
            .nodes
            .get(frame_id)
            .expect("frame node missing")
            .children;
        *frame_children.first().expect("list not initialized")
    }

    fn column_list_ids(harness: &Harness) -> Vec<NodeId> {
        let panes_id = panes_id(harness);
        let panes_children = &harness
            .canopy
            .core
            .nodes
            .get(panes_id)
            .expect("panes node missing")
            .children;

        let mut lists = Vec::new();
        for column_id in panes_children {
            let column_children = &harness
                .canopy
                .core
                .nodes
                .get(*column_id)
                .expect("column node missing")
                .children;
            let Some(frame_id) = column_children.first().copied() else {
                continue;
            };
            let frame_children = &harness
                .canopy
                .core
                .nodes
                .get(frame_id)
                .expect("frame node missing")
                .children;
            let Some(list_id) = frame_children.first().copied() else {
                continue;
            };
            lists.push(list_id);
        }
        lists
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
        harness.with_widget(list_node, |list: &mut List<Block>| {
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
        harness.with_widget(list_node, |list: &mut List<Block>| {
            initial_selected = list.selected_index();
        });

        // Navigate using list commands (these are loaded by the List type)
        harness.script("list::select_last()")?;

        let mut selected = None;
        harness.with_widget(list_node, |list: &mut List<Block>| {
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
