use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ReadContext, TypedId, Widget, command,
    derive_commands,
    error::{Error, Result},
    event::{key, mouse},
    layout::{CanvasContext, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    style::solarized,
};
use canopy_widgets::{CanvasWidth, Frame, List, Panes, Root, Selectable, Text, VStack};
use rand::Rng;

/// Sample text content for list items.
const TEXT: &str = "What a struggle must have gone on during long centuries between the several kinds of trees, each annually scattering its seeds by the thousand; what war between insect and insect — between insects, snails, and other animals with birds and beasts of prey — all striving to increase, all feeding on each other, or on the trees, their seeds and seedlings, or on the other plants which first clothed the ground and thus checked the growth of the trees.";

/// Alternating color names for list items.
const COLORS: &[&str] = &["red", "blue"];

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
pub fn setup_bindings(cnpy: &mut Canopy) {
    cnpy.style
        .rules()
        .fg("red/text", solarized::RED)
        .fg("blue/text", solarized::BLUE)
        .fg("statusbar/text", solarized::BLUE)
        .fg("list/selected", solarized::BLUE)
        .apply();

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
        .key('k', "list::select_prev()")
        .mouse(mouse::Action::ScrollDown, "list::select_next()")
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
        .key(key::KeyCode::Tab, "panes::next_column()")
        .key(key::KeyCode::BackTab, "panes::prev_column()")
        .key(key::KeyCode::PageDown, "list::page_down()")
        .key(' ', "list::page_down()")
        .key(key::KeyCode::PageUp, "list::page_up()");
}
