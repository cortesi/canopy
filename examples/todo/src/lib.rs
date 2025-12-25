use std::any::Any;

use anyhow::Result as AnyResult;
use canopy::{
    Binder, Canopy, Context, Loader, NodeId, command, derive_commands,
    error::Result,
    event::{key, mouse},
    geom::{Expanse, Rect},
    render::Render,
    style::solarized,
    widget::Widget,
    widgets::{Input, frame, list::*},
};
use taffy::{
    geometry::Rect as TaffyRect,
    style::{
        AlignItems, Dimension, Display, FlexDirection, JustifyContent, LengthPercentage, Position,
    },
};

pub mod store;

/// List item for a todo entry.
pub struct TodoItem {
    /// Stored todo.
    todo: store::Todo,
}

impl TodoItem {
    pub fn new(t: store::Todo) -> Self {
        Self { todo: t }
    }
}

impl ListItem for TodoItem {
    fn measure(&self, available_width: u32) -> Expanse {
        let width = available_width.max(1) as usize;
        let lines = textwrap::wrap(&self.todo.item, width);
        let height = lines.len().max(1) as u32;
        Expanse::new(available_width.max(1), height)
    }

    fn render(&mut self, rndr: &mut Render, area: Rect, selected: bool) -> Result<()> {
        let width = area.w.max(1) as usize;
        let lines = textwrap::wrap(&self.todo.item, width);
        let style = if selected { "blue/text" } else { "text" };

        for (i, line) in lines.iter().enumerate() {
            if i as u32 >= area.h {
                break;
            }
            rndr.text(style, area.line(i as u32), line)?;
        }
        Ok(())
    }
}

/// Status bar widget for the todo demo.
pub struct StatusBar;

#[derive_commands]
impl StatusBar {}

impl Widget for StatusBar {
    fn render(
        &mut self,
        r: &mut Render,
        _area: canopy::geom::Rect,
        ctx: &dyn canopy::ViewContext,
    ) -> Result<()> {
        r.push_layer("statusbar");
        r.text("statusbar/text", ctx.view().line(0), "todo")?;
        Ok(())
    }
}

/// Overlay container for the add dialog.
pub struct Overlay;

#[derive_commands]
impl Overlay {}

impl Widget for Overlay {
    fn render(
        &mut self,
        _r: &mut Render,
        _area: canopy::geom::Rect,
        _ctx: &dyn canopy::ViewContext,
    ) -> Result<()> {
        Ok(())
    }
}

/// Root node for the todo demo.
pub struct Todo {
    pending: Vec<store::Todo>,
    content_id: Option<NodeId>,
    list_id: Option<NodeId>,
    status_id: Option<NodeId>,
    overlay_id: Option<NodeId>,
    input_id: Option<NodeId>,
    adder_active: bool,
}

#[derive_commands]
impl Todo {
    pub fn new() -> AnyResult<Self> {
        let pending = store::get().todos()?;
        Ok(Self {
            pending,
            content_id: None,
            list_id: None,
            status_id: None,
            overlay_id: None,
            input_id: None,
            adder_active: false,
        })
    }

    fn ensure_tree(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.content_id.is_some() {
            return Ok(());
        }

        let list_id = c.add(Box::new(List::new(Vec::<TodoItem>::new())));
        let content_id = c.add(Box::new(frame::Frame::new()));
        c.mount_child(content_id, list_id)?;

        let status_id = c.add(Box::new(StatusBar));
        c.set_children(c.node_id(), vec![content_id, status_id])?;

        let mut update_root = |style: &mut taffy::style::Style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Column;
        };
        c.with_style(c.node_id(), &mut update_root)?;

        let mut content_style = |style: &mut taffy::style::Style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        };
        c.with_style(content_id, &mut content_style)?;
        c.with_style(list_id, &mut content_style)?;

        let mut status_style = |style: &mut taffy::style::Style| {
            style.size.height = Dimension::Points(1.0);
            style.flex_shrink = 0.0;
        };
        c.with_style(status_id, &mut status_style)?;

        self.content_id = Some(content_id);
        self.list_id = Some(list_id);
        self.status_id = Some(status_id);

        if !self.pending.is_empty() {
            let pending = std::mem::take(&mut self.pending);
            self.with_list(c, |list, _ctx| {
                for item in pending.iter().cloned() {
                    list.append(TodoItem::new(item));
                }
                Ok(())
            })?;
        }

        Ok(())
    }

    fn ensure_overlay(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.overlay_id.is_some() {
            return Ok(());
        }

        let overlay_id = c.add(Box::new(Overlay));
        let input_id = c.add(Box::new(Input::new("")));
        let adder_frame_id = c.add(Box::new(frame::Frame::new()));
        c.mount_child(adder_frame_id, input_id)?;
        c.set_children(overlay_id, vec![adder_frame_id])?;

        let mut overlay_style = |style: &mut taffy::style::Style| {
            style.position = Position::Absolute;
            style.inset = TaffyRect {
                left: LengthPercentage::Points(0.0).into(),
                right: LengthPercentage::Points(0.0).into(),
                top: LengthPercentage::Points(0.0).into(),
                bottom: LengthPercentage::Points(0.0).into(),
            };
            style.display = Display::Flex;
            style.justify_content = Some(JustifyContent::Center);
            style.align_items = Some(AlignItems::Center);
        };
        c.with_style(overlay_id, &mut overlay_style)?;

        let mut frame_style = |style: &mut taffy::style::Style| {
            style.size.width = Dimension::Percent(1.0);
            style.size.height = Dimension::Points(3.0);
            style.margin = TaffyRect {
                left: LengthPercentage::Points(2.0).into(),
                right: LengthPercentage::Points(2.0).into(),
                top: LengthPercentage::Points(0.0).into(),
                bottom: LengthPercentage::Points(0.0).into(),
            };
        };
        c.with_style(adder_frame_id, &mut frame_style)?;

        let mut input_style = |style: &mut taffy::style::Style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        };
        c.with_style(input_id, &mut input_style)?;

        self.overlay_id = Some(overlay_id);
        self.input_id = Some(input_id);

        Ok(())
    }

    fn sync_children(&mut self, c: &mut dyn Context) -> Result<()> {
        let content_id = self.content_id.expect("content not initialized");
        let status_id = self.status_id.expect("status not initialized");
        let mut children = vec![content_id, status_id];
        if self.adder_active {
            self.ensure_overlay(c)?;
            if let Some(overlay_id) = self.overlay_id {
                children.push(overlay_id);
            }
        }
        c.set_children(c.node_id(), children)?;
        Ok(())
    }

    fn with_list<F>(&mut self, c: &mut dyn Context, mut f: F) -> Result<()>
    where
        F: FnMut(&mut List<TodoItem>, &mut dyn Context) -> Result<()>,
    {
        let list_id = self.list_id.expect("list not initialized");
        c.with_widget_mut(list_id, &mut |widget, ctx| {
            let any = widget as &mut dyn Any;
            let list = any
                .downcast_mut::<List<TodoItem>>()
                .expect("list type mismatch");
            f(list, ctx)
        })
    }

    fn with_input<F>(&mut self, c: &mut dyn Context, mut f: F) -> Result<()>
    where
        F: FnMut(&mut Input) -> Result<()>,
    {
        let input_id = self.input_id.expect("input not initialized");
        c.with_widget_mut(input_id, &mut |widget, _ctx| {
            let any = widget as &mut dyn Any;
            let input = any.downcast_mut::<Input>().expect("input type mismatch");
            f(input)
        })
    }

    #[command]
    pub fn enter_item(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        self.ensure_overlay(c)?;
        self.adder_active = true;
        self.sync_children(c)?;
        self.with_input(c, |input| {
            input.set_value("");
            Ok(())
        })?;
        if let Some(input_id) = self.input_id {
            c.set_focus(input_id);
        }
        Ok(())
    }

    #[command]
    pub fn delete_item(&mut self, c: &mut dyn Context) -> Result<()> {
        let mut to_delete = None;
        self.with_list(c, |list, ctx| {
            to_delete = list.selected().map(|item| item.todo.id);
            let _ = list.delete_selected(ctx);
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
                list.append(TodoItem::new(item.clone()));
                list.select_last(ctx);
                Ok(())
            })?;
        }

        self.adder_active = false;
        self.sync_children(c)?;
        c.set_focus(c.node_id());
        Ok(())
    }

    #[command]
    pub fn cancel_add(&mut self, c: &mut dyn Context) -> Result<()> {
        self.adder_active = false;
        self.sync_children(c)?;
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
    pub fn select_next(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.select_next(ctx);
            Ok(())
        })
    }

    #[command]
    pub fn select_prev(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.select_prev(ctx);
            Ok(())
        })
    }

    #[command]
    pub fn page_down(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.page_down(ctx);
            Ok(())
        })
    }

    #[command]
    pub fn page_up(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.page_up(ctx);
            Ok(())
        })
    }
}

impl Widget for Todo {
    fn accept_focus(&self) -> bool {
        true
    }

    fn render(
        &mut self,
        _r: &mut Render,
        _area: canopy::geom::Rect,
        _ctx: &dyn canopy::ViewContext,
    ) -> Result<()> {
        Ok(())
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<std::time::Duration> {
        let _ = self.ensure_tree(c);
        None
    }
}

impl Loader for Todo {
    fn load(c: &mut Canopy) {
        c.add_commands::<Todo>();
        c.add_commands::<List<TodoItem>>();
        c.add_commands::<Input>();
    }
}

pub fn style(cnpy: &mut Canopy) {
    cnpy.style.add(
        "statusbar/text",
        Some(solarized::BASE02),
        Some(solarized::BASE1),
        None,
    );
    // Ensure text under blue layer gets blue foreground
    cnpy.style.add_fg("blue/text", solarized::BLUE);
}

pub fn bind_keys(cnpy: &mut Canopy) {
    Binder::new(cnpy)
        .with_path("todo/")
        .key('q', "root::quit()")
        .key('d', "todo::delete_item()")
        .key('a', "todo::enter_item()")
        .key('g', "todo::select_first()")
        .key('j', "todo::select_next()")
        .key(key::KeyCode::Down, "todo::select_next()")
        .key('k', "todo::select_prev()")
        .key(key::KeyCode::Up, "todo::select_prev()")
        .key(' ', "todo::page_down()")
        .key(key::KeyCode::PageDown, "todo::page_down()")
        .key(key::KeyCode::PageUp, "todo::page_up()")
        .mouse(mouse::Action::ScrollUp, "todo::select_prev()")
        .mouse(mouse::Action::ScrollDown, "todo::select_next()")
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

pub fn setup_app(cnpy: &mut Canopy) {
    canopy::widgets::Root::load(cnpy);
    <Todo as Loader>::load(cnpy);
    style(cnpy);
    bind_keys(cnpy);
}

pub fn create_app(db_path: &str) -> AnyResult<Canopy> {
    open_store(db_path)?;

    let mut cnpy = Canopy::new();
    setup_app(&mut cnpy);

    let todo = Todo::new()?;
    let app_id = cnpy.core.add(todo);
    canopy::widgets::Root::install(&mut cnpy.core, app_id)?;
    Ok(cnpy)
}
