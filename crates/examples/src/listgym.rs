use canopy::{
    command, derive_commands,
    error::Error,
    layout::{CanvasContext, MeasureConstraints, Measurement, Size},
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

canopy.bind_with("p", { path = "list_gym", desc = "Log demo message" }, function()
    canopy.log("list gym")
end)
canopy.bind_with("a", { path = "list_gym", desc = "Add item" }, function()
    list_gym.add_item()
end)
canopy.bind_with("A", { path = "list_gym", desc = "Append item" }, function()
    list_gym.append_item()
end)
canopy.bind_with("C", { path = "list_gym", desc = "Clear list" }, function()
    list_gym.clear()
end)
canopy.bind_with("q", { path = "list_gym", desc = "Quit" }, function()
    root.quit()
end)
canopy.bind_with("g", { path = "list_gym", desc = "First item" }, function()
    list.select_first()
end)
canopy.bind_with("G", { path = "list_gym", desc = "Last item" }, function()
    list.select_last()
end)
canopy.bind_with("d", { path = "list_gym", desc = "Delete item" }, function()
    list.delete_selected()
end)
canopy.bind_with("j", { path = "list_gym", desc = "Next item" }, function()
    list.select_by(1)
end)
canopy.bind_with("k", { path = "list_gym", desc = "Previous item" }, function()
    list.select_by(-1)
end)
canopy.bind_mouse_with("ScrollDown", { path = "list_gym", desc = "Next item" }, function()
    list.select_by(1)
end)
canopy.bind_mouse_with("ScrollUp", { path = "list_gym", desc = "Previous item" }, function()
    list.select_by(-1)
end)
canopy.bind_with("Down", { path = "list_gym", desc = "Next item" }, function()
    list.select_by(1)
end)
canopy.bind_with("Up", { path = "list_gym", desc = "Previous item" }, function()
    list.select_by(-1)
end)
canopy.bind_with("J", { path = "list_gym", desc = "Scroll down" }, function()
    list.scroll("Down")
end)
canopy.bind_with("K", { path = "list_gym", desc = "Scroll up" }, function()
    list.scroll("Up")
end)
canopy.bind_with("h", { path = "list_gym", desc = "Scroll left" }, function()
    list.scroll("Left")
end)
canopy.bind_with("l", { path = "list_gym", desc = "Scroll right" }, function()
    list.scroll("Right")
end)
canopy.bind_with("Left", { path = "list_gym", desc = "Scroll left" }, function()
    list.scroll("Left")
end)
canopy.bind_with("Right", { path = "list_gym", desc = "Scroll right" }, function()
    list.scroll("Right")
end)
canopy.bind_with("s", { path = "list_gym", desc = "Add column" }, function()
    list_gym.add_column()
end)
canopy.bind_with("x", { path = "list_gym", desc = "Delete column" }, function()
    list_gym.delete_column()
end)
canopy.bind_with("Tab", { path = "list_gym", desc = "Next column" }, function()
    panes.focus_column(1)
end)
canopy.bind_with("BackTab", { path = "list_gym", desc = "Previous column" }, function()
    panes.focus_column(-1)
end)
canopy.bind_with("PageDown", { path = "list_gym", desc = "Page down" }, function()
    list.page(1)
end)
canopy.bind_with("Space", { path = "list_gym", desc = "Page down" }, function()
    list.page(1)
end)
canopy.bind_with("PageUp", { path = "list_gym", desc = "Page up" }, function()
    list.page(-1)
end)
"#;

/// Focusable list entry that renders text content.
pub struct ListEntry {
    /// Text content for the entry.
    text: Text,
}

#[derive_commands]
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
    fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        self.text.render(r, ctx)
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        self.text.measure(c)
    }

    fn canvas(&self, view: Size<u32>, ctx: &CanvasContext<'_>) -> Size<u32> {
        self.text.canvas(view, ctx)
    }

    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
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
pub struct StatusBar;

#[derive_commands]
impl StatusBar {
    /// Construct a status bar.
    pub fn new() -> Self {
        Self
    }

    /// Locate the panes node in the tree.
    fn panes_id(ctx: &dyn ReadContext) -> Option<NodeId> {
        ctx.first_in_tree::<Panes>().map(Into::into)
    }

    /// Build the status text based on the focused column.
    fn label(&self, ctx: &dyn ReadContext) -> String {
        let Some(panes_id) = Self::panes_id(ctx) else {
            return "listgym".to_string();
        };
        let columns = ctx.children_of(panes_id);
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
    fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        r.push_layer("statusbar");
        let label = self.label(ctx);
        r.text("text", ctx.view().outer_rect_local().line(0), &label)?;
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
        let frame_id = c.create_detached(Frame::new());
        let list_id = c.add_child_to(
            frame_id,
            List::<ListEntry>::new().with_selection_indicator("list/selected", "█ ", true),
        )?;
        // Add initial items
        c.with_typed(list_id, |list: &mut List<ListEntry>, ctx| {
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
        c.with_widget(list_id, |list: &mut List<ListEntry>, ctx| f(list, ctx))
    }

    /// Find the list to target for list commands.
    fn list_id(&self, c: &dyn Context) -> Result<TypedId<List<ListEntry>>> {
        c.focused_or_first_descendant::<List<ListEntry>>()
            .ok_or_else(|| Error::Invalid("list not initialized".into()))
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
        let frame_id = Self::create_column(c)?;
        c.with_unique_descendant::<Panes, _>(|panes, ctx| panes.insert_col(ctx, frame_id))
    }

    #[command]
    /// Delete the focused column.
    pub fn delete_column(&mut self, c: &mut dyn Context) -> Result<()> {
        c.with_unique_descendant::<Panes, _>(|panes, ctx| panes.delete_focus(ctx))?;
        Ok(())
    }
}

impl Widget for ListGym {
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let panes_id = c.create_detached(Panes::new());
        let status_id = c.create_detached(StatusBar::new());
        c.add_child(
            VStack::new()
                .push_flex(panes_id, 1)
                .push_fixed(status_id, 1),
        )?;

        let frame_id = Self::create_column(c)?;
        c.with_typed(panes_id, |panes: &mut Panes, ctx| {
            panes.insert_col(ctx, frame_id)
        })?;
        Ok(())
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
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

/// Install key bindings for the list gym demo.
pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {
    cnpy.style_mut()
        .rules()
        .fg("red/text", solarized::RED)
        .fg("blue/text", solarized::BLUE)
        .fg("statusbar/text", solarized::BLUE)
        .fg("list/selected", solarized::BLUE)
        .apply();

    cnpy.run_default_script(DEFAULT_BINDINGS)?;
    Ok(())
}
