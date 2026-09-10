#![deny(unsafe_code)]
//! Todo application used as Canopy's end-to-end example and smoke-test target.

use std::{collections::HashMap, fmt::Display, path::Path};

use anyhow::Result as AnyResult;
use canopy::{
    CanopyBuilder, InteractionToken, ModalBindings, ModalOptions,
    commands::CommandStatus,
    derive_commands,
    error::{Error, Result},
    layout::LayoutOverride,
    prelude::*,
    style::solarized,
};
use canopy_widgets::{Center, Frame, Input, List, Root, Selectable, ValueExposure};

// Typed keys for keyed children
canopy::slot!(MainSlot: MainContent);
canopy::slot!(ModalSlot: Center);

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
    /// Owned interaction scope for the add-item modal.
    adder: Option<InteractionToken>,
}

#[derive_commands]
impl Todo {
    /// Load a todo widget from its database.
    pub(crate) fn new(store: store::Store) -> Self {
        Self { store, adder: None }
    }

    /// Build the Todo widget subtree once.
    fn ensure_tree(&self, c: &mut dyn Context) -> Result<()> {
        if c.has_slot::<MainSlot>()? {
            return Ok(());
        }

        let scope = c.node_id();
        c.compose(scope, |tree| {
            tree.keyed::<MainSlot>(MainContent, |main| {
                main.layout_override(LayoutOverride::full(Layout::fill()))?;
                main.child(Frame::new(), |frame| {
                    frame.child(List::<TodoEntry, i64>::new(), |list| {
                        list.semantic_key(scope, "todo.list")
                    })?;
                    Ok(())
                })?;
                main.child(StatusBar, |status| {
                    status.layout_override(LayoutOverride::full(
                        Layout::row().flex_horizontal(1).fixed_height(1),
                    ))
                })?;
                Ok(())
            })?;
            Ok(())
        })?;
        let items = self.store.todos().map_err(store_error)?;
        self.reconcile_items(c, items)?;
        self.with_list(c, |list, ctx| list.select_first(ctx))?;

        Ok(())
    }

    /// Build the add-item modal once.
    fn ensure_modal(&self, c: &mut dyn Context) -> Result<()> {
        if c.has_slot::<ModalSlot>()? {
            return Ok(());
        }

        let scope = c.node_id();
        c.compose(scope, |tree| {
            tree.keyed::<ModalSlot>(Center::new(), |modal| {
                modal.child(Frame::new(), |frame| {
                    frame.layout_override(LayoutOverride {
                        min_width: Some(Some(30)),
                        max_width: Some(Some(50)),
                        ..LayoutOverride::new().fixed_height(3)
                    })?;
                    frame.child(
                        Input::new("")
                            .with_label("New todo item")
                            .with_value_exposure(ValueExposure::Public),
                        |input| {
                            input.layout_override(LayoutOverride::full(Layout::fill()))?;
                            input.semantic_key(scope, "todo.input")
                        },
                    )?;
                    Ok(())
                })?;
                Ok(())
            })?;
            Ok(())
        })
    }

    /// Close only this application's modal effects and restore prior focus.
    fn close_adder(&self, c: &mut dyn Context) -> Result<()> {
        if let Some(token) = self.adder {
            c.close_modal(token)?;
        }
        Ok(())
    }

    /// Reconcile database rows by their persistent identifiers.
    fn reconcile_items(&self, c: &mut dyn Context, items: Vec<store::Todo>) -> Result<()> {
        let keys: Vec<i64> = items.iter().map(|item| item.id).collect();
        let items: HashMap<i64, store::Todo> =
            items.into_iter().map(|item| (item.id, item)).collect();
        self.with_list(c, |list, ctx| {
            list.reconcile(
                ctx,
                keys,
                |key| Ok(TodoEntry::new(items[key].clone())),
                |key, id, ctx| {
                    ctx.with_widget_mut(id, |entry: &mut TodoEntry, _| {
                        entry.todo = items[key].clone();
                        Ok(())
                    })
                },
            )?;
            Ok(())
        })
    }

    /// Resolve the node for the unique todo list.
    fn list_id(&self, c: &dyn ViewContext) -> Result<NodeId> {
        c.find_identity(c.node_id(), "todo.list")?
            .ok_or_else(|| Error::Internal("todo list is not initialized".into()))
    }

    /// Resolve the node for the unique modal input.
    fn input_id(&self, c: &dyn ViewContext) -> Result<NodeId> {
        c.find_identity(c.node_id(), "todo.input")?
            .ok_or_else(|| Error::Internal("todo input is not initialized".into()))
    }

    /// Run a mutation against the unique todo list.
    fn with_list<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut List<TodoEntry, i64>, &mut dyn Context) -> Result<R>,
    {
        let id = self.list_id(c)?;
        c.with_widget_mut(id, f)
    }

    /// Run a mutation against the unique modal input.
    fn with_input<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut Input) -> Result<R>,
    {
        let id = self.input_id(c)?;
        c.with_widget_mut(id, |input: &mut Input, _| f(input))
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
        self.reconcile_items(c, items)?;
        self.with_list(c, |list, ctx| list.select(ctx, 0))?;
        if modal_open {
            self.open_adder(c)?;
        } else {
            let was_open = self.adder.is_some_and(|token| c.modal_is_open(token));
            self.close_adder(c)?;
            if !was_open {
                if empty {
                    c.set_focus(c.node_id())?;
                } else {
                    self.with_list(c, |list, ctx| list.select_first(ctx))?;
                }
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
        self.ensure_modal(c)?;
        self.with_input(c, |input| {
            input.set_value("");
            Ok(())
        })?;
        let input = self.input_id(c)?;
        if !self.adder.is_some_and(|token| c.modal_is_open(token)) {
            let modal = c.get_slot::<ModalSlot>()?.expect("modal initialized");
            let main = c.get_slot::<MainSlot>()?.expect("main initialized");
            self.adder = Some(c.open_modal(ModalOptions {
                owner: c.node_id(),
                modal: modal.into(),
                initial_focus: input,
                dim_target: Some(main.into()),
                bindings: ModalBindings::Application,
            })?);
        } else {
            c.set_focus(input)?;
        }
        Ok(())
    }

    /// Delete eligibility follows the current list selection.
    fn can_delete(&self, ctx: &dyn ViewContext) -> Result<CommandStatus> {
        let Ok(list) = self.list_id(ctx) else {
            return Ok(CommandStatus::Disabled("No item selected".into()));
        };
        ctx.with_widget(ctx.typed_id::<List<TodoEntry, i64>>(list)?, |list| {
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
            let Some(id) = list.selected_key().copied() else {
                return Ok(());
            };
            self.store.delete_todo(id).map_err(store_error)?;
            let _ = list.delete_selected(ctx)?;
            Ok(())
        })
    }

    #[command]
    /// Store the pending input and close the add-item modal.
    pub fn accept_add(&self, c: &mut dyn Context) -> Result<()> {
        let value = self.with_input(c, |input| Ok(input.value().to_string()))?;

        if !value.is_empty() {
            let item = self.store.add_todo(&value).map_err(store_error)?;
            self.reconcile_items(c, self.store.todos().map_err(store_error)?)?;
            self.with_list(c, |list, ctx| list.select_key(ctx, &item.id))?;
        }

        self.close_adder(c)
    }

    #[command]
    /// Discard pending input and close the add-item modal.
    pub fn cancel_add(&self, c: &mut dyn Context) -> Result<()> {
        self.close_adder(c)
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
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Stack)
    }

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
        c.add_commands::<List<TodoEntry, i64>>()?;
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

/// Build the todo command and fixture API without assembling a widget tree.
pub fn api_app() -> Result<Canopy> {
    app_builder(None).build()
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

/// Queue API registration and binding sources without constructing a database.
fn app_builder(config: Option<&Path>) -> CanopyBuilder {
    let builder = CanopyBuilder::new()
        .configure(|cnpy| {
            Root::load(cnpy)?;
            <Todo as Loader>::load(cnpy)?;
            style(cnpy);
            register_fixtures(cnpy)
        })
        .bindings("todo-defaults", DEFAULT_BINDINGS);
    if let Some(config) = config {
        builder.config(config.to_owned())
    } else {
        builder
    }
}

/// Create a todo application with an explicit database and optional user
/// config.
pub fn create_app(store: store::Store, config: Option<&Path>) -> AnyResult<Canopy> {
    Ok(app_builder(config)
        .assemble(move |cnpy| {
            let todo = Todo::new(store);
            Root::new().install(cnpy, todo)?;
            Ok(())
        })
        .build()?)
}

#[cfg(test)]
mod tests {
    use canopy::{
        geom::Point,
        style::{ResolvedStyle, effects},
        testing::harness::Harness,
    };

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
    fn semantic_controls_survive_decorative_wrappers_and_reorder() -> AnyResult<()> {
        let store = store::Store::open(":memory:")?;
        let first = store.add_todo("first")?;
        let second = store.add_todo("second")?;
        let mut canopy = create_app(store, None)?;
        with_todo(&mut canopy, |todo, ctx| {
            let scope = ctx.node_id();
            let list = ctx.find_identity(scope, "todo.list")?.expect("keyed list");
            let frame = ctx.parent_of(list).expect("list frame");
            ctx.edit_structure(&mut |ctx| {
                let wrapper = ctx.create_detached(Frame::new())?;
                ctx.detach(list)?;
                ctx.attach(wrapper.into(), list)?;
                ctx.attach(frame, wrapper.into())
            })?;
            assert_eq!(ctx.find_identity(scope, "todo.list")?, Some(list));
            let first_node = todo.with_list(ctx, |list, ctx| {
                list.select_key(ctx, &first.id)?;
                Ok(list.selected_item().expect("first entry"))
            })?;
            todo.reconcile_items(ctx, vec![second.clone(), first.clone()])?;
            todo.with_list(ctx, |list, _| {
                assert_eq!(list.selected_key(), Some(&first.id));
                assert_eq!(list.selected_index(), Some(1));
                assert_eq!(list.selected_item(), Some(first_node));
                Ok(())
            })?;
            todo.open_adder(ctx)?;
            let input = ctx
                .find_identity(scope, "todo.input")?
                .expect("keyed input");
            assert_eq!(ctx.focused_node(), Some(input));
            todo.close_adder(ctx)
        })?;
        Ok(())
    }

    #[test]
    fn repeated_modal_opening_keeps_one_dimming_effect() -> AnyResult<()> {
        let store = store::Store::open(":memory:")?;
        let mut harness = Harness::from_canopy(create_app(store, None)?, Size::new(80, 24))?;
        with_todo(&mut harness.canopy, |_, ctx| {
            let main = ctx.get_slot::<MainSlot>()?.expect("main initialized");
            ctx.push_effect(main.into(), effects::brightness(0.8))?;
            Ok(())
        })?;
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
