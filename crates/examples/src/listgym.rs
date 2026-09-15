use canopy::{
    Canopy, CanopyBuilder, Context, ContextExt, Loader, NodeId, NodeName, Render, ViewContext,
    ViewContextExt, Widget, derive_commands,
    error::{Error, Result},
    geom::Size,
    layout::{CanvasContext, MeasureConstraints, Measurement},
    style::canopy as palette,
};
use canopy_widgets::{CanvasWidth, Columns, Container, List, Selectable, Text};
use rand::RngExt;

use crate::{fixed_row, flex_row};

/// Sample text content for list items.
const TEXT: &str = "What a struggle must have gone on during long centuries between the several kinds of trees, each annually scattering its seeds by the thousand; what war between insect and insect — between insects, snails, and other animals with birds and beasts of prey — all striving to increase, all feeding on each other, or on the trees, their seeds and seedlings, or on the other plants which first clothed the ground and thus checked the growth of the trees.";

/// Alternating color names for list items.
const COLORS: &[&str] = &["red", "blue"];

/// Default bindings for the list gym demo.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()

canopy.keymap({
    path = "list_gym",
    {
        key = "p",
        description = "Log demo message",
        action = function()
            canopy.log("list gym")
        end,
    },
    { key = "a", description = "Add item", action = command.list_gym.add_item() },
    { key = "A", description = "Append item", action = command.list_gym.append_item() },
    { key = "C", description = "Clear list", action = command.list_gym.clear() },
    { key = "q", description = "Quit", action = command.root.quit() },
    { key = "g", description = "First item", action = command.list.select_first() },
    { key = "G", description = "Last item", action = command.list.select_last() },
    { key = "d", description = "Delete item", action = command.list.delete_selected() },
    { key = { "j", "Down" }, description = "Next item", action = command.list.select_by(1) },
    { key = { "k", "Up" }, description = "Previous item", action = command.list.select_by(-1) },
    { key = "J", description = "Scroll down", action = command.list.scroll("Down") },
    { key = "K", description = "Scroll up", action = command.list.scroll("Up") },
    { key = { "h", "Left" }, description = "Scroll left", action = command.list.scroll("Left") },
    {
        key = { "l", "Right" },
        description = "Scroll right",
        action = command.list.scroll("Right"),
    },
    { key = "s", description = "Add column", action = command.list_gym.add_column() },
    { key = "x", description = "Delete column", action = command.list_gym.delete_column() },
    { key = "Tab", description = "Next column", action = command.columns.focus_column(1) },
    { key = "BackTab", description = "Previous column", action = command.columns.focus_column(-1) },
    { key = { "PageDown", "Space" }, description = "Page down", action = command.list.page(1) },
    { key = "PageUp", description = "Page up", action = command.list.page(-1) },
})
"#;

/// Focusable list entry that renders text content.
pub(crate) struct ListEntry {
    /// Text content for the entry.
    text: Text,
}

impl ListEntry {
    /// Construct a new list entry from a text widget.
    pub fn new(text: Text) -> Self {
        Self { text }
    }
}

impl Selectable for ListEntry {
    fn set_selected(&mut self, selected: bool) {
        self.text.set_selected(selected);
    }
}

impl Widget for ListEntry {
    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        self.text.render(r, ctx)
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        self.text.measure(c)
    }

    fn canvas(&self, view: Size, ctx: &CanvasContext<'_>) -> Size {
        self.text.canvas(view, ctx)
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("list_entry")
    }
}

/// Build a text item for the list.
fn list_item(index: usize) -> ListEntry {
    let mut rng = rand::rng();
    let wrap_width = rng.random_range(10..150);
    let color = COLORS[index % COLORS.len()];

    let text = Text::new(TEXT)
        .with_wrap_width(wrap_width)
        .with_canvas_width(CanvasWidth::Intrinsic)
        .with_style(format!("{color}/text"));

    ListEntry::new(text)
}

/// Return the columns node, once mounted.
fn columns_id(ctx: &dyn ViewContext) -> Result<NodeId> {
    ctx.first_in_tree::<Columns>()
        .map(Into::into)
        .ok_or_else(|| Error::Invalid("columns not initialized".into()))
}

/// Return the column that holds focus, if any.
fn focused_column(ctx: &dyn ViewContext, columns: NodeId) -> Option<(usize, NodeId)> {
    ctx.children_of(columns)
        .into_iter()
        .enumerate()
        .find(|(_, pane)| ctx.is_on_focus_path_of(*pane))
}

/// Status bar widget for the list gym demo.
pub(crate) struct StatusBar;

impl StatusBar {
    /// Construct a status bar.
    pub fn new() -> Self {
        Self
    }

    /// Build the status text based on the focused column.
    fn label(&self, ctx: &dyn ViewContext) -> String {
        let Ok(columns) = columns_id(ctx) else {
            return "listgym".to_string();
        };
        let total = ctx.children_of(columns).len();
        match focused_column(ctx, columns) {
            Some((index, _)) => format!("listgym  col {}/{}", index + 1, total),
            None => "listgym".to_string(),
        }
    }
}

impl Widget for StatusBar {
    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("statusbar");
        let label = self.label(ctx);
        r.text("text", ctx.view().outer_rect_local().line(0)?, &label)?;
        Ok(())
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
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

    /// Create a detached list column and return its node id.
    fn create_column(c: &mut dyn Context) -> Result<NodeId> {
        let list_id = c.create_detached(List::<ListEntry>::new().with_selection_indicator(
            "list/selected",
            "█ ",
            true,
        ))?;
        c.with_widget_mut(list_id, |list: &mut List<ListEntry>, ctx| {
            for i in 0..10 {
                list.append(ctx, list_item(i))?;
            }
            Ok(())
        })?;
        Ok(list_id.into())
    }

    /// Execute a closure with mutable access to the list widget.
    fn with_list<F, R>(&self, c: &mut dyn Context, mut f: F) -> Result<R>
    where
        F: FnMut(&mut List<ListEntry>, &mut dyn Context) -> Result<R>,
    {
        let list_id = (c as &dyn ViewContext)
            .focused_or_first_descendant::<List<ListEntry>>()
            .ok_or_else(|| Error::Invalid("list not initialized".into()))?;
        c.with_widget_mut(list_id, |list: &mut List<ListEntry>, ctx| f(list, ctx))
    }

    #[command]
    /// Add an item after the current focus.
    pub(crate) fn add_item(&self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            let index = list.selected_index().unwrap_or(0) + 1;
            list.insert(ctx, index, list_item(index))?;
            Ok(())
        })
    }

    #[command]
    /// Add an item at the end of the list.
    pub(crate) fn append_item(&self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            let index = list.len();
            list.append(ctx, list_item(index))?;
            Ok(())
        })
    }

    #[command]
    /// Clear all items from the list.
    pub(crate) fn clear(&self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.clear(ctx)?;
            Ok(())
        })
    }

    #[command]
    /// Add a column after the focused one and focus its list.
    pub(crate) fn add_column(&self, c: &mut dyn Context) -> Result<()> {
        let columns = columns_id(c)?;
        let list = Self::create_column(c)?;
        let mut panes = c.children_of(columns);
        let index = focused_column(c, columns).map_or(panes.len(), |(index, _)| index + 1);
        panes.insert(index, list);
        c.set_children_of(columns, panes)?;
        let target = c
            .focusable_leaves(list)
            .first()
            .copied()
            .or_else(|| c.first_leaf(list));
        if let Some(target) = target {
            c.set_focus(target)?;
        }
        Ok(())
    }

    #[command]
    /// Delete the focused column. Focus recovers to a neighbor.
    pub(crate) fn delete_column(&self, c: &mut dyn Context) -> Result<()> {
        let columns = columns_id(c)?;
        if let Some((_, pane)) = focused_column(c, columns) {
            c.remove_subtree(pane)?;
        }
        Ok(())
    }
}

impl Widget for ListGym {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let root = c.node_id();
        let layout = c.add_child_to(root, Container::column())?;
        let columns: NodeId = c.add_child_to(layout, Columns::new())?.into();
        let status: NodeId = c.add_child_to(layout, StatusBar::new())?.into();
        c.set_layout_override_of(columns, flex_row(1))?;
        c.set_layout_override_of(status, fixed_row(1))?;
        let list = Self::create_column(c)?;
        c.set_children_of(columns, vec![list])
    }
}

impl Loader for ListGym {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<List<ListEntry>>()?;
        c.add_commands::<Columns>()?;
        c.add_commands::<Self>()?;
        Ok(())
    }
}

/// Install native styles during the configuration phase.
fn setup_style(cnpy: &mut Canopy) {
    cnpy.style_mut()
        .rules()
        .fg("red/text", palette::RED)
        .fg("blue/text", palette::BLUE)
        .fg("statusbar/text", palette::ACCENT)
        .fg("list/selected", palette::ACCENT)
        .apply();
}

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder
        .configure(|cnpy| {
            setup_style(cnpy);
            Ok(())
        })
        .bindings("listgym", DEFAULT_BINDINGS)
}
