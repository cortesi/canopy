use anyhow::Result as AnyResult;
use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ReadContext, Widget, command,
    commands::VerticalDirection,
    derive_commands,
    error::Result,
    event::{key, mouse},
    geom::Rect,
    key,
    layout::{Constraint, Direction, Layout, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    style::{effects, solarized},
};
use canopy_widgets::{Frame, Input, List, Modal, Root, Selectable};

// Typed keys for keyed children
key!(MainSlot: MainContent);
key!(ModalSlot: Modal);

pub mod store;

/// Widget for a todo entry.
pub struct TodoEntry {
    /// Stored todo.
    pub todo: store::Todo,
    /// Selection state.
    selected: bool,
}

impl Selectable for TodoEntry {
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

#[derive_commands]
impl TodoEntry {
    /// Create a new todo entry widget.
    pub fn new(t: store::Todo) -> Self {
        Self {
            todo: t,
            selected: false,
        }
    }
}

impl Widget for TodoEntry {
    fn layout(&self) -> Layout {
        // Flex horizontally but use Measure for height so scrolling works
        Layout::column().flex_horizontal(1)
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let available_width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n.max(1),
            Constraint::Unbounded => 80,
        };
        let width = available_width as usize;
        let lines = textwrap::wrap(&self.todo.item, width);
        let height = lines.len().max(1) as u32;
        c.clamp(Size::new(available_width, height))
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        let view = ctx.view();
        let area = view.view_rect_local();

        if area.w == 0 || area.h == 0 {
            return Ok(());
        }

        // Column 0: Selection indicator (when selected)
        if self.selected && area.w >= 1 {
            let indicator_rect = Rect::new(area.tl.x, area.tl.y, 1, area.h);
            rndr.fill("list/selected", indicator_rect, '\u{2588}')?;
        }

        // Column 1: Spacer
        if area.w >= 2 {
            let spacer = Rect::new(area.tl.x + 1, area.tl.y, 1, area.h);
            rndr.fill("", spacer, ' ')?;
        }

        // Text content starts at column 2
        let text_start_x = area.tl.x + 2;
        let text_visible_width = area.w.saturating_sub(2);

        if text_visible_width > 0 {
            let width = text_visible_width as usize;
            let lines = textwrap::wrap(&self.todo.item, width);
            for (i, line) in lines.iter().enumerate().take(area.h as usize) {
                let line_rect =
                    Rect::new(text_start_x, area.tl.y + i as u32, text_visible_width, 1);
                rndr.text("text", line_rect.line(0), line)?;
            }
        }

        Ok(())
    }

    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("todo_entry")
    }
}

/// Status bar widget for the todo demo.
pub struct StatusBar;

#[derive_commands]
impl StatusBar {}

impl Widget for StatusBar {
    fn render(&mut self, r: &mut Render, ctx: &dyn canopy::ReadContext) -> Result<()> {
        r.push_layer("statusbar");
        r.text(
            "statusbar/text",
            ctx.view().outer_rect_local().line(0),
            "todo",
        )?;
        Ok(())
    }
}

/// Container for main content (list frame + status bar).
struct MainContent;

#[derive_commands]
impl MainContent {}

impl Widget for MainContent {
    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }
}

/// Root node for the todo demo.
pub struct Todo {
    pending: Vec<store::Todo>,
    adder_active: bool,
}

#[derive_commands]
impl Todo {
    pub fn new() -> AnyResult<Self> {
        let pending = store::get().todos()?;
        Ok(Self {
            pending,
            adder_active: false,
        })
    }

    fn ensure_tree(&mut self, c: &mut dyn Context) -> Result<()> {
        if c.has_child::<MainSlot>() {
            return Ok(());
        }

        // Create the main content container (list + status bar in column layout)
        let main_content_id = c.add_keyed::<MainSlot>(MainContent)?;
        let frame_id = c.add_child_to(main_content_id, Frame::new())?;
        let list_id = c.add_child_to(frame_id, List::<TodoEntry>::new())?;
        let status_id = c.add_child_to(main_content_id, StatusBar)?;
        let main_content_node = NodeId::from(main_content_id);

        // Set Todo (self) to use Stack direction for modal overlay support
        c.set_layout(Layout::fill().direction(Direction::Stack))?;

        // Main content fills the space
        c.set_layout_of(main_content_id, Layout::fill())?;

        c.set_layout_of(list_id, Layout::fill())?;

        c.set_layout_of(status_id, Layout::row().flex_horizontal(1).fixed_height(1))?;

        // Initially only show main content
        c.set_children(vec![main_content_node])?;

        if !self.pending.is_empty() {
            let pending = std::mem::take(&mut self.pending);
            c.with_typed(list_id, |list: &mut List<TodoEntry>, ctx| {
                for item in pending.iter().cloned() {
                    list.append(ctx, TodoEntry::new(item))?;
                }
                Ok(())
            })?;
        }

        Ok(())
    }

    fn ensure_modal(&mut self, c: &mut dyn Context) -> Result<()> {
        if c.has_child::<ModalSlot>() {
            return Ok(());
        }

        // Create the modal with an input frame
        let modal_id = c.add_keyed::<ModalSlot>(Modal::new())?;
        let adder_frame_id = c.add_child_to(modal_id, Frame::new())?;
        let input_id = c.add_child_to(adder_frame_id, Input::new(""))?;

        let mut layout = Frame::new().layout();
        layout.min_height = Some(3);
        layout.max_height = Some(3);
        layout.min_width = Some(30);
        layout.max_width = Some(50);
        c.set_layout_of(adder_frame_id, layout)?;

        c.set_layout_of(input_id, Layout::fill())?;

        Ok(())
    }

    fn sync_modal_state(&mut self, c: &mut dyn Context) -> Result<()> {
        let main_content_id = c
            .get_child::<MainSlot>()
            .expect("main content not initialized");
        let main_content_node = NodeId::from(main_content_id);

        if self.adder_active {
            self.ensure_modal(c)?;
            c.push_effect(main_content_node, effects::dim(0.5))?;
            c.with_child::<ModalSlot, _>(|_, ctx| {
                ctx.set_hidden(false);
                Ok(())
            })?;
        } else {
            // Clear dimming when modal is not active
            c.clear_effects(main_content_node)?;
            let _ = c.try_with_child::<ModalSlot, _>(|_, ctx| {
                ctx.set_hidden(true);
                Ok(())
            })?;
        }
        Ok(())
    }

    fn with_list<F>(&mut self, c: &mut dyn Context, mut f: F) -> Result<()>
    where
        F: FnMut(&mut List<TodoEntry>, &mut dyn Context) -> Result<()>,
    {
        c.with_unique_descendant::<List<TodoEntry>, _>(|list, ctx| f(list, ctx))?;
        Ok(())
    }

    fn with_input<F>(&mut self, c: &mut dyn Context, mut f: F) -> Result<()>
    where
        F: FnMut(&mut Input) -> Result<()>,
    {
        c.with_child::<ModalSlot, _>(|_, ctx| {
            ctx.with_unique_descendant::<Input, _>(|input, _| f(input))
        })?;
        Ok(())
    }

    #[command]
    pub fn enter_item(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        self.ensure_modal(c)?;
        self.adder_active = true;
        self.sync_modal_state(c)?;
        self.with_input(c, |input| {
            input.set_value("");
            Ok(())
        })?;
        if let Some(input_id) = c.unique_descendant::<Input>()? {
            c.set_focus(NodeId::from(input_id));
        }
        Ok(())
    }

    #[command]
    pub fn delete_item(&mut self, c: &mut dyn Context) -> Result<()> {
        // Get the selected item's todo id before deleting
        let mut to_delete = None;

        self.with_list(c, |list, ctx| {
            if let Some(item_id) = list.selected_item() {
                ctx.with_widget(item_id, |entry: &mut TodoEntry, _| {
                    to_delete = Some(entry.todo.id);
                    Ok(())
                })?;
            }
            let _ = list.delete_selected(ctx)?;
            Ok(())
        })?;

        if let Some(id) = to_delete {
            store::get().delete_todo(id).unwrap();
        }
        Ok(())
    }

    #[command]
    pub fn accept_add(&mut self, c: &mut dyn Context) -> Result<()> {
        let mut value = String::new();
        self.with_input(c, |input| {
            value = input.value().to_string();
            Ok(())
        })?;

        if !value.is_empty() {
            let item = store::get().add_todo(&value).unwrap();
            self.with_list(c, |list, ctx| {
                list.append(ctx, TodoEntry::new(item.clone()))?;
                list.select_last(ctx);
                Ok(())
            })?;
        }

        self.adder_active = false;
        self.sync_modal_state(c)?;
        c.set_focus(c.node_id());
        Ok(())
    }

    #[command]
    pub fn cancel_add(&mut self, c: &mut dyn Context) -> Result<()> {
        self.adder_active = false;
        self.sync_modal_state(c)?;
        c.set_focus(c.node_id());
        Ok(())
    }

    #[command]
    pub fn select_first(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.select_first(ctx);
            Ok(())
        })
    }

    #[command]
    pub fn select_by(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.select_by(ctx, delta);
            Ok(())
        })
    }

    #[command]
    pub fn page(&mut self, c: &mut dyn Context, dir: VerticalDirection) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.page(ctx, dir);
            Ok(())
        })
    }
}

impl Widget for Todo {
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn canopy::ReadContext) -> Result<()> {
        Ok(())
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<std::time::Duration> {
        let _ = self.ensure_tree(c);
        None
    }
}

impl Loader for Todo {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Todo>()?;
        c.add_commands::<List<TodoEntry>>()?;
        c.add_commands::<Input>()?;
        Ok(())
    }
}

pub fn style(cnpy: &mut Canopy) {
    use canopy::style::StyleBuilder;

    cnpy.style
        .rules()
        .style(
            "statusbar/text",
            StyleBuilder::new()
                .fg(solarized::BASE02)
                .bg(solarized::BASE1),
        )
        .fg("list/selected", solarized::BLUE)
        .apply();
}

pub fn bind_keys(cnpy: &mut Canopy) {
    Binder::new(cnpy)
        .with_path("todo/")
        .key('q', "root::quit()")
        .key('d', "todo::delete_item()")
        .key('a', "todo::enter_item()")
        .key('g', "todo::select_first()")
        .key('j', "todo::select_by(1)")
        .key(key::KeyCode::Down, "todo::select_by(1)")
        .key('k', "todo::select_by(-1)")
        .key(key::KeyCode::Up, "todo::select_by(-1)")
        .key(' ', "todo::page(\\\"down\\\")")
        .key(key::KeyCode::PageDown, "todo::page(\\\"down\\\")")
        .key(key::KeyCode::PageUp, "todo::page(\\\"up\\\")")
        .mouse(mouse::Action::ScrollUp, "todo::select_by(-1)")
        .mouse(mouse::Action::ScrollDown, "todo::select_by(1)")
        .with_path("input")
        .key(key::KeyCode::Left, "input::left()")
        .key(key::KeyCode::Right, "input::right()")
        .key(key::KeyCode::Backspace, "input::backspace()")
        .key(key::KeyCode::Enter, "todo::accept_add()")
        .key(key::KeyCode::Esc, "todo::cancel_add()");
}

pub fn open_store(path: &str) -> AnyResult<()> {
    store::open(path)
}

pub fn setup_app(cnpy: &mut Canopy) -> Result<()> {
    Root::load(cnpy)?;
    <Todo as Loader>::load(cnpy)?;
    style(cnpy);
    bind_keys(cnpy);
    Ok(())
}

pub fn create_app(db_path: &str) -> AnyResult<Canopy> {
    open_store(db_path)?;

    let mut cnpy = Canopy::new();
    setup_app(&mut cnpy)?;

    let todo = Todo::new()?;
    let app_id = cnpy.core.create_detached(todo);
    Root::install(&mut cnpy.core, app_id)?;
    Ok(cnpy)
}
