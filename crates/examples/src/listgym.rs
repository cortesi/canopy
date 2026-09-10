use canopy::{
    CanopyBuilder, derive_commands,
    error::{Error, Result},
    geom::Size,
    layout::{CanvasContext, MeasureConstraints, Measurement},
    prelude::*,
    style::solarized,
};
use canopy_widgets::{CanvasWidth, Frame, List, Panes, Selectable, Text, VStack};
use rand::RngExt;

/// Sample text content for list items.
const TEXT: &str = "What a struggle must have gone on during long centuries between the several kinds of trees, each annually scattering its seeds by the thousand; what war between insect and insect — between insects, snails, and other animals with birds and beasts of prey — all striving to increase, all feeding on each other, or on the trees, their seeds and seedlings, or on the other plants which first clothed the ground and thus checked the growth of the trees.";

/// Alternating color names for list items.
const COLORS: &[&str] = &["red", "blue"];

/// Default bindings for the list gym demo.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()

canopy.bind("p", { phase = "after_widget", path = "list_gym", description = "Log demo message" }, function()
    canopy.log("list gym")
end)
canopy.bind_command("a", { phase = "after_widget", path = "list_gym", description = "Add item" }, "list_gym::add_item")
canopy.bind_command("A", { phase = "after_widget", path = "list_gym", description = "Append item" }, "list_gym::append_item")
canopy.bind_command("C", { phase = "after_widget", path = "list_gym", description = "Clear list" }, "list_gym::clear")
canopy.bind_command("q", { phase = "after_widget", path = "list_gym", description = "Quit" }, "root::quit")
canopy.bind_command("g", { phase = "after_widget", path = "list_gym", description = "First item" }, "list::select_first")
canopy.bind_command("G", { phase = "after_widget", path = "list_gym", description = "Last item" }, "list::select_last")
canopy.bind_command("d", { phase = "after_widget", path = "list_gym", description = "Delete item" }, "list::delete_selected")
canopy.bind_command("j", { phase = "after_widget", path = "list_gym", description = "Next item" }, "list::select_by", 1)
canopy.bind_command("k", { phase = "after_widget", path = "list_gym", description = "Previous item" }, "list::select_by", -1)
canopy.bind_mouse("ScrollDown", { phase = "after_widget", path = "list_gym", description = "Next item" }, function()
    list.select_by(1)
end)
canopy.bind_mouse("ScrollUp", { phase = "after_widget", path = "list_gym", description = "Previous item" }, function()
    list.select_by(-1)
end)
canopy.bind_command("Down", { phase = "after_widget", path = "list_gym", description = "Next item" }, "list::select_by", 1)
canopy.bind_command("Up", { phase = "after_widget", path = "list_gym", description = "Previous item" }, "list::select_by", -1)
canopy.bind_command("J", { phase = "after_widget", path = "list_gym", description = "Scroll down" }, "list::scroll", "Down")
canopy.bind_command("K", { phase = "after_widget", path = "list_gym", description = "Scroll up" }, "list::scroll", "Up")
canopy.bind_command("h", { phase = "after_widget", path = "list_gym", description = "Scroll left" }, "list::scroll", "Left")
canopy.bind_command("l", { phase = "after_widget", path = "list_gym", description = "Scroll right" }, "list::scroll", "Right")
canopy.bind_command("Left", { phase = "after_widget", path = "list_gym", description = "Scroll left" }, "list::scroll", "Left")
canopy.bind_command("Right", { phase = "after_widget", path = "list_gym", description = "Scroll right" }, "list::scroll", "Right")
canopy.bind_command("s", { phase = "after_widget", path = "list_gym", description = "Add column" }, "list_gym::add_column")
canopy.bind_command("x", { phase = "after_widget", path = "list_gym", description = "Delete column" }, "list_gym::delete_column")
canopy.bind_command("Tab", { phase = "after_widget", path = "list_gym", description = "Next column" }, "panes::focus_column", 1)
canopy.bind_command("BackTab", { phase = "after_widget", path = "list_gym", description = "Previous column" }, "panes::focus_column", -1)
canopy.bind_command("PageDown", { phase = "after_widget", path = "list_gym", description = "Page down" }, "list::page", 1)
canopy.bind_command("Space", { phase = "after_widget", path = "list_gym", description = "Page down" }, "list::page", 1)
canopy.bind_command("PageUp", { phase = "after_widget", path = "list_gym", description = "Page up" }, "list::page", -1)
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

/// Status bar widget for the list gym demo.
pub(crate) struct StatusBar;

impl StatusBar {
    /// Construct a status bar.
    pub fn new() -> Self {
        Self
    }

    /// Locate the panes node in the tree.
    fn panes_id(ctx: &dyn ViewContext) -> Option<NodeId> {
        ctx.first_in_tree::<Panes>().map(Into::into)
    }

    /// Build the status text based on the focused column.
    fn label(&self, ctx: &dyn ViewContext) -> String {
        let Some(panes_id) = Self::panes_id(ctx) else {
            return "listgym".to_string();
        };
        let columns = ctx.children_of(panes_id);
        let total = columns.len();
        let focused = columns
            .iter()
            .position(|node| ctx.is_on_focus_path_of(*node));

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

    /// Create a framed list column and return the frame node id.
    fn create_column(c: &mut dyn Context) -> Result<TypedId<Frame>> {
        let frame_id = c.create_detached(Frame::new())?;
        let list_id = c.add_child_to(
            frame_id,
            List::<ListEntry>::new().with_selection_indicator("list/selected", "█ ", true),
        )?;
        // Add initial items
        c.with_widget_mut(list_id, |list: &mut List<ListEntry>, ctx| {
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
        F: FnMut(&mut List<ListEntry>, &mut dyn Context) -> Result<R>,
    {
        let list_id = self.list_id(c)?;
        c.with_widget_mut(list_id, |list: &mut List<ListEntry>, ctx| f(list, ctx))
    }

    /// Find the list to target for list commands.
    fn list_id(&self, c: &dyn Context) -> Result<TypedId<List<ListEntry>>> {
        (c as &dyn ViewContext)
            .focused_or_first_descendant::<List<ListEntry>>()
            .ok_or_else(|| Error::Invalid("list not initialized".into()))
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
    /// Add a new column containing a list.
    pub(crate) fn add_column(&self, c: &mut dyn Context) -> Result<()> {
        let frame_id = Self::create_column(c)?;
        c.with_unique_descendant::<Panes, _>(|panes, ctx| panes.insert_col(ctx, frame_id))
    }

    #[command]
    /// Delete the focused column.
    pub(crate) fn delete_column(&self, c: &mut dyn Context) -> Result<()> {
        c.with_unique_descendant::<Panes, _>(|panes, ctx| panes.delete_focus(ctx))?;
        Ok(())
    }
}

impl Widget for ListGym {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let panes_id = c.create_detached(Panes::new())?;
        let status_id = c.create_detached(StatusBar::new())?;
        c.add_child(
            VStack::new()
                .push_flex(panes_id, 1)
                .push_fixed(status_id, 1),
        )?;

        let frame_id = Self::create_column(c)?;
        c.with_widget_mut(panes_id, |panes: &mut Panes, ctx| {
            panes.insert_col(ctx, frame_id)
        })?;
        Ok(())
    }
}

impl Loader for ListGym {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<List<ListEntry>>()?;
        c.add_commands::<Panes>()?;
        c.add_commands::<Self>()?;
        Ok(())
    }
}

/// Install native styles during the configuration phase.
fn setup_style(cnpy: &mut Canopy) {
    cnpy.style_mut()
        .rules()
        .fg("red/text", solarized::RED)
        .fg("blue/text", solarized::BLUE)
        .fg("statusbar/text", solarized::BLUE)
        .fg("list/selected", solarized::BLUE)
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
