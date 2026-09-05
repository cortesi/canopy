#![deny(unsafe_code)]
//! Todo application used as Canopy's end-to-end example and smoke-test target.

use std::{fmt::Display, mem, path::Path};

use anyhow::Result as AnyResult;
use canopy::{
    command,
    commands::CommandStatus,
    derive_commands,
    error::Error,
    layout::LayoutOverride,
    prelude::*,
    style::{effects, solarized},
};
use canopy_widgets::{Center, Frame, Input, List, Root, Selectable};

// Typed keys for keyed children
canopy::key!(MainSlot: MainContent);
canopy::key!(ModalSlot: Center);

pub mod store;

/// Stable sample data used by automation fixtures.
const FIXTURE_WITH_ITEMS: &[&str] = &[
    "Buy milk",
    "Ship fixture-driven smoke tests",
    "Review MCP bindings",
];

/// Columns reserved for the selection indicator and its spacer.
const TODO_GUTTER_WIDTH: u32 = 2;

/// Widget for a todo entry.
pub struct TodoEntry {
    /// Stored todo.
    pub(crate) todo: store::Todo,
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
    pub(crate) fn new(t: store::Todo) -> Self {
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
        let text_width = available_width.saturating_sub(TODO_GUTTER_WIDTH);
        let height = if text_width == 0 {
            1
        } else {
            textwrap::wrap(&self.todo.item, text_width as usize)
                .len()
                .max(1) as u32
        };
        c.clamp(Size::new(available_width, height))
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
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
        let text_start_x = area.tl.x + TODO_GUTTER_WIDTH;
        let text_visible_width = area.w.saturating_sub(TODO_GUTTER_WIDTH);

        if text_visible_width > 0 {
            let width = text_visible_width as usize;
            let lines = textwrap::wrap(&self.todo.item, width);
            for (i, line) in lines.iter().enumerate().take(area.h as usize) {
                let line_rect =
                    Rect::new(text_start_x, area.tl.y + i as u32, text_visible_width, 1);
                rndr.text("text", line_rect.line(0)?, line)?;
            }
        }

        Ok(())
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("todo_entry")
    }
}

/// Status bar widget for the todo demo.
pub(crate) struct StatusBar;

impl Widget for StatusBar {
    fn render(&mut self, r: &mut Render, ctx: &dyn canopy::ViewContext) -> Result<()> {
        r.push_layer("statusbar");
        r.text(
            "statusbar/text",
            ctx.view().outer_rect_local().line(0)?,
            "todo",
        )?;
        Ok(())
    }
}

/// Container for main content (list frame + status bar).
struct MainContent;

impl Widget for MainContent {}

/// Root node for the todo demo.
pub(crate) struct Todo {
    /// Database owned by this application.
    store: store::Store,
    /// Entries waiting for the widget tree to mount.
    pending: Vec<store::Todo>,
    /// Whether the add-item modal is active.
    adder_active: bool,
}

#[derive_commands]
impl Todo {
    /// Load a todo widget from its database.
    pub(crate) fn new(store: store::Store) -> AnyResult<Self> {
        let pending = store.todos()?;
        Ok(Self {
            store,
            pending,
            adder_active: false,
        })
    }

    /// Build the Todo widget subtree once.
    fn ensure_tree(&mut self, c: &mut dyn Context) -> Result<()> {
        if c.has_child::<MainSlot>()? {
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
            let pending = mem::take(&mut self.pending);
            c.with_widget(list_id, |list: &mut List<TodoEntry>, ctx| {
                for item in pending {
                    list.append(ctx, TodoEntry::new(item))?;
                }
                Ok(())
            })?;
        }

        Ok(())
    }

    /// Build the add-item modal once.
    fn ensure_modal(&self, c: &mut dyn Context) -> Result<()> {
        if c.has_child::<ModalSlot>()? {
            return Ok(());
        }

        // Create the modal with an input frame
        let modal_id = c.add_keyed::<ModalSlot>(Center::new())?;
        let adder_frame_id = c.add_child_to(modal_id, Frame::new())?;
        let input_id = c.add_child_to(adder_frame_id, Input::new(""))?;

        c.set_layout_override_of(
            adder_frame_id.into(),
            LayoutOverride {
                min_width: Some(Some(30)),
                max_width: Some(Some(50)),
                ..LayoutOverride::new().fixed_height(3)
            },
        )?;

        c.set_layout_of(input_id, Layout::fill())?;

        Ok(())
    }

    /// Synchronize modal visibility and the main-content dimming effect.
    fn sync_modal_state(&self, c: &mut dyn Context) -> Result<()> {
        let main_content_id = c
            .get_child::<MainSlot>()?
            .expect("main content not initialized");
        let main_content_node = NodeId::from(main_content_id);

        c.clear_effects(main_content_node)?;
        if self.adder_active {
            self.ensure_modal(c)?;
            c.push_effect(main_content_node, effects::brightness(0.5))?;
            c.with_child::<ModalSlot, _>(|_, ctx| {
                ctx.set_hidden(false)?;
                Ok(())
            })?;
        } else {
            let _ = c.try_with_child::<ModalSlot, _>(|_, ctx| {
                ctx.set_hidden(true)?;
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Run a mutation against the unique todo list.
    fn with_list<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut List<TodoEntry>, &mut dyn Context) -> Result<R>,
    {
        c.with_unique_descendant::<List<TodoEntry>, _>(f)
    }

    /// Run a mutation against the unique modal input.
    fn with_input<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut Input) -> Result<R>,
    {
        c.with_child::<ModalSlot, _>(|_, ctx| {
            ctx.with_unique_descendant::<Input, _>(|input, _| f(input))
        })
    }

    /// Replace list state and set the requested modal state for a fixture.
    fn apply_items_fixture(
        &mut self,
        c: &mut dyn Context,
        items: Vec<store::Todo>,
        modal_open: bool,
    ) -> Result<()> {
        self.ensure_tree(c)?;
        let empty = items.is_empty();
        self.with_list(c, |list, ctx| {
            list.clear(ctx)?;
            for item in items {
                list.append(ctx, TodoEntry::new(item))?;
            }
            if !empty {
                list.select_first(ctx)?;
            }
            Ok(())
        })?;

        if modal_open {
            self.open_adder(c)?;
        } else {
            self.adder_active = false;
            self.sync_modal_state(c)?;
            if empty {
                c.set_focus(c.node_id())?;
            }
        }
        Ok(())
    }

    #[command]
    /// Open the add-item modal and focus its input.
    pub fn enter_item(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        self.open_adder(c)
    }

    /// Show the add-item modal with an empty input and focus it.
    fn open_adder(&mut self, c: &mut dyn Context) -> Result<()> {
        self.adder_active = true;
        self.sync_modal_state(c)?;
        self.with_input(c, |input| {
            input.set_value("");
            Ok(())
        })?;
        if let Some(input_id) = (c as &dyn ViewContext).unique_descendant::<Input>()? {
            c.set_focus(NodeId::from(input_id))?;
        }
        Ok(())
    }

    /// Delete eligibility follows the current list selection.
    fn can_delete(&self, ctx: &dyn ViewContext) -> Result<CommandStatus> {
        let Some(list) = ctx.unique_descendant::<List<TodoEntry>>()? else {
            return Ok(CommandStatus::Disabled("No item selected".into()));
        };
        ctx.with_widget_read(list, |list| {
            Ok(if list.selected_item().is_some() {
                CommandStatus::Enabled
            } else {
                CommandStatus::Disabled("No item selected".into())
            })
        })
    }

    #[command(enabled = "can_delete")]
    /// Delete the selected todo entry.
    pub fn delete_item(&self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            let Some(item_id) = list.selected_item() else {
                return Ok(());
            };
            let id = ctx.with_widget(item_id, |entry: &mut TodoEntry, _| Ok(entry.todo.id))?;
            self.store.delete_todo(id).map_err(store_error)?;
            let _ = list.delete_selected(ctx)?;
            Ok(())
        })
    }

    #[command]
    /// Store the pending input and close the add-item modal.
    pub fn accept_add(&mut self, c: &mut dyn Context) -> Result<()> {
        let value = self.with_input(c, |input| Ok(input.value().to_string()))?;

        if !value.is_empty() {
            let item = self.store.add_todo(&value).map_err(store_error)?;
            self.with_list(c, |list, ctx| {
                list.append(ctx, TodoEntry::new(item))?;
                list.select_last(ctx)?;
                Ok(())
            })?;
        }

        self.adder_active = false;
        self.sync_modal_state(c)?;
        c.set_focus(c.node_id())?;
        Ok(())
    }

    #[command]
    /// Discard pending input and close the add-item modal.
    pub fn cancel_add(&mut self, c: &mut dyn Context) -> Result<()> {
        self.adder_active = false;
        self.sync_modal_state(c)?;
        c.set_focus(c.node_id())?;
        Ok(())
    }

    #[command]
    /// Select the first todo entry.
    pub fn select_first(&self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| list.select_first(ctx))
    }

    #[command]
    /// Move the todo selection by a signed number of entries.
    pub fn select_by(&self, c: &mut dyn Context, delta: i32) -> Result<()> {
        self.with_list(c, |list, ctx| list.select_by(ctx, delta))
    }

    #[command]
    /// Move the todo selection by a signed number of pages.
    pub fn page(&self, c: &mut dyn Context, delta: i32) -> Result<()> {
        self.with_list(c, |list, ctx| list.page(ctx, delta))
    }
}

impl Widget for Todo {
    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }
}

impl Loader for Todo {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        c.add_commands::<List<TodoEntry>>()?;
        c.add_commands::<Input>()?;
        Ok(())
    }
}

/// Default Luau bindings for the todo app.
pub(crate) const DEFAULT_BINDINGS: &str = r#"
canopy.bind_command("?", { phase = "before_widget",
    description = "Show key bindings",
    path = "/root/**/",
    tier = "global",
}, "root::toggle_help")
canopy.bind_command("q", { phase = "after_widget", description = "Quit" }, "root::quit")
canopy.bind_command("d", { phase = "after_widget", description = "Delete item" }, "todo::delete_item")
canopy.bind_command("a", { phase = "after_widget", description = "Add item" }, "todo::enter_item")
canopy.bind_command("g", { phase = "after_widget", description = "First item" }, "todo::select_first")
canopy.bind_command("j", { phase = "after_widget", description = "Next item" }, "todo::select_by", 1)
canopy.bind_command("Down", { phase = "after_widget", description = "Next item" }, "todo::select_by", 1)
canopy.bind_command("k", { phase = "after_widget", description = "Previous item" }, "todo::select_by", -1)
canopy.bind_command("Up", { phase = "after_widget", description = "Previous item" }, "todo::select_by", -1)
canopy.bind_command("Space", { phase = "after_widget", description = "Page down" }, "todo::page", 1)
canopy.bind_command("PageDown", { phase = "after_widget", description = "Page down" }, "todo::page", 1)
canopy.bind_command("PageUp", { phase = "after_widget", description = "Page up" }, "todo::page", -1)

canopy.bind_mouse("ScrollUp", { phase = "after_widget", description = "Previous item" }, function()
    todo.select_by(-1)
end)
canopy.bind_mouse("ScrollDown", { phase = "after_widget", description = "Next item" }, function()
    todo.select_by(1)
end)

canopy.bind_command("Left", { phase = "before_widget", path = "input", description = "Cursor left" }, "input::left")
canopy.bind_command("Right", { phase = "before_widget", path = "input", description = "Cursor right" }, "input::right")
canopy.bind_command("Backspace", { phase = "before_widget", path = "input", description = "Delete char" }, "input::backspace")
canopy.bind_command("Enter", { phase = "before_widget", path = "input", description = "Confirm new item" }, "todo::accept_add")
canopy.bind_command("Escape", { phase = "before_widget", path = "input", description = "Cancel add" }, "todo::cancel_add")
"#;

/// Install the todo application's style rules.
pub(crate) fn style(cnpy: &mut Canopy) {
    use canopy::style::StyleBuilder;

    cnpy.style_mut()
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

/// Convert persistence failures into application errors.
fn store_error(error: impl Display) -> Error {
    Error::Invalid(error.to_string())
}

/// Register and finalize the todo application API with default bindings.
pub fn setup_app(cnpy: &mut Canopy) -> Result<()> {
    setup_app_with_config(cnpy, None)
}

/// Run a mutation against the unique Todo widget.
fn with_todo<R>(
    cnpy: &mut Canopy,
    f: impl FnOnce(&mut Todo, &mut dyn Context) -> Result<R>,
) -> Result<R> {
    cnpy.with_root_context(|ctx| ctx.with_unique_descendant::<Todo, _>(|todo, ctx| f(todo, ctx)))
}

/// Register the Todo automation fixtures.
fn register_fixtures(cnpy: &mut Canopy) -> Result<()> {
    cnpy.register_fixture(canopy::Fixture::new(
        "empty",
        "App with no todo items and no modal open",
        |cnpy| {
            with_todo(cnpy, |todo, ctx| {
                let items = todo.store.replace_todos([]).map_err(store_error)?;
                todo.apply_items_fixture(ctx, items, false)
            })
        },
    ))?;
    cnpy.register_fixture(canopy::Fixture::new(
        "with_items",
        "App with a pre-populated todo list",
        |cnpy| {
            with_todo(cnpy, |todo, ctx| {
                let items = todo
                    .store
                    .replace_todos(FIXTURE_WITH_ITEMS.iter().copied())
                    .map_err(store_error)?;
                todo.apply_items_fixture(ctx, items, false)
            })
        },
    ))?;
    cnpy.register_fixture(canopy::Fixture::new(
        "modal_open",
        "App with the add-item modal open and ready for typing",
        |cnpy| {
            with_todo(cnpy, |todo, ctx| {
                let items = todo
                    .store
                    .replace_todos(FIXTURE_WITH_ITEMS.iter().copied())
                    .map_err(store_error)?;
                todo.apply_items_fixture(ctx, items, true)
            })
        },
    ))?;
    Ok(())
}

/// Register commands, finalize the Luau API, and apply default/user bindings.
pub(crate) fn setup_app_with_config(cnpy: &mut Canopy, config: Option<&Path>) -> Result<()> {
    Root::load(cnpy)?;
    <Todo as Loader>::load(cnpy)?;
    style(cnpy);
    register_fixtures(cnpy)?;
    cnpy.finalize_api()?;
    cnpy.eval_script(DEFAULT_BINDINGS)?;
    if let Some(config) = config {
        cnpy.run_config(config)?;
    }
    Ok(())
}

/// Create a fully configured todo application backed by `db_path`.
pub fn create_app(db_path: &str) -> AnyResult<Canopy> {
    create_app_with_config(db_path, None)
}

/// Create a todo canopy app with optional user config.
pub fn create_app_with_config(db_path: &str, config: Option<&Path>) -> AnyResult<Canopy> {
    create_app_with_store(store::Store::open(db_path)?, config)
}

/// Create a todo application with an explicit database and optional user
/// config.
pub fn create_app_with_store(store: store::Store, config: Option<&Path>) -> AnyResult<Canopy> {
    let mut cnpy = Canopy::new();
    setup_app_with_config(&mut cnpy, config)?;

    let todo = Todo::new(store)?;
    Root::install_app(&mut cnpy, todo)?;
    Ok(cnpy)
}

#[cfg(test)]
mod tests {
    use canopy::{geom::Point, style::ResolvedStyle, testing::harness::Harness};

    use super::*;

    #[test]
    fn todo_entry_measures_text_after_the_gutter() {
        for (text, width, height) in [
            ("aaaa bbb", 8, 2),
            ("aaaa bbb", 10, 1),
            ("aaaa bbb", 0, 1),
            ("aaaa bbb", 1, 1),
            ("aaaa bbb", 2, 1),
            ("", 8, 1),
        ] {
            let entry = TodoEntry::new(store::Todo {
                id: 1,
                item: text.to_owned(),
            });
            assert_eq!(
                entry.measure(MeasureConstraints {
                    width: Constraint::Exact(width),
                    height: Constraint::Unbounded,
                }),
                Measurement::Fixed(Size::new(width, height))
            );
        }
    }

    #[test]
    fn narrow_todo_entry_renders_every_wrapped_line() -> AnyResult<()> {
        let entry = TodoEntry::new(store::Todo {
            id: 1,
            item: "aaaa bbb".to_owned(),
        });
        let Measurement::Fixed(size) = entry.measure(MeasureConstraints {
            width: Constraint::Exact(8),
            height: Constraint::Unbounded,
        }) else {
            panic!("todo entry has a fixed measurement");
        };
        let mut canopy = Canopy::new();
        canopy.replace_root(entry)?;
        let mut harness = Harness::from_canopy(canopy, size)?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("aaaa"));
        assert!(harness.tbuf().contains_text("bbb"));
        Ok(())
    }

    fn status_style(harness: &Harness) -> ResolvedStyle {
        let buffer = harness.buf();
        buffer
            .get(Point {
                x: 0,
                y: buffer.size().h - 1,
            })
            .expect("status cell")
            .style
    }

    #[test]
    fn repeated_modal_opening_keeps_one_dimming_effect() -> AnyResult<()> {
        let mut harness = Harness::from_canopy(create_app(":memory:")?, Size::new(80, 24))?;
        harness.render()?;
        let normal = status_style(&harness);
        harness.script("todo.enter_item()")?;
        let dimmed = status_style(&harness);
        assert_ne!(normal, dimmed);
        harness.script("todo.enter_item()")?;
        assert_eq!(status_style(&harness), dimmed);
        for _ in 0..2 {
            harness.canopy.apply_fixture("modal_open")?;
            harness.render()?;
            assert_eq!(status_style(&harness), dimmed);
        }
        harness.script("todo.cancel_add()")?;
        assert_eq!(status_style(&harness), normal);
        Ok(())
    }
}
