use std::{any::Any, time::Duration};

use canopy::{
    Canopy, Context, Loader, NodeId, ViewContext, command, derive_commands,
    error::Result,
    event::{Event, key, mouse},
    geom::{Expanse, Rect},
    render::Render,
    style::solarized,
    widget::{EventOutcome, Widget},
    widgets::{frame, list::*},
};
use taffy::style::{Dimension, Display, FlexDirection, Style};

/// List item that increments on a timer.
pub struct IntervalItem {
    /// Current counter value.
    value: u64,
}

impl Default for IntervalItem {
    fn default() -> Self {
        Self::new()
    }
}

impl IntervalItem {
    /// Construct a new interval item.
    pub fn new() -> Self {
        Self { value: 0 }
    }

    /// Increment the counter.
    fn tick(&mut self) {
        self.value = self.value.saturating_add(1);
    }
}

impl ListItem for IntervalItem {
    fn measure(&self, available_width: u32) -> Expanse {
        Expanse::new(available_width.max(1), 1)
    }

    fn render(&mut self, rndr: &mut Render, area: Rect, selected: bool) -> Result<()> {
        let style = if selected { "blue/text" } else { "text" };
        let text = self.value.to_string();
        rndr.text(style, area.line(0), &text)?;
        Ok(())
    }
}

/// Status bar widget for the intervals demo.
pub struct StatusBar;

#[derive_commands]
impl StatusBar {}

impl Widget for StatusBar {
    fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("statusbar");
        r.text("statusbar/text", ctx.view().line(0), "intervals")?;
        Ok(())
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }
}

/// Root node for the intervals demo.
pub struct Intervals {
    /// Content frame node id.
    content_id: Option<NodeId>,
    /// List node id.
    list_id: Option<NodeId>,
}

impl Default for Intervals {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl Intervals {
    /// Construct a new intervals demo.
    pub fn new() -> Self {
        Self {
            content_id: None,
            list_id: None,
        }
    }

    /// Ensure the frame, list, and status bar are created.
    fn ensure_tree(&mut self, c: &mut dyn Context) {
        if self.content_id.is_some() {
            return;
        }

        let list_id = c.add(Box::new(List::new(Vec::<IntervalItem>::new())));
        let content_id = c.add(Box::new(frame::Frame::new()));
        c.mount_child(content_id, list_id)
            .expect("Failed to mount list");

        let status_id = c.add(Box::new(StatusBar));
        c.set_children(c.node_id(), vec![content_id, status_id])
            .expect("Failed to attach children");

        let mut update_root = |style: &mut Style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Column;
        };
        c.with_style(c.node_id(), &mut update_root)
            .expect("Failed to style root");

        let mut content_style = |style: &mut Style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        };
        c.with_style(content_id, &mut content_style)
            .expect("Failed to style content");
        c.with_style(list_id, &mut content_style)
            .expect("Failed to style list");

        let mut status_style = |style: &mut Style| {
            style.size.height = Dimension::Points(1.0);
            style.flex_shrink = 0.0;
        };
        c.with_style(status_id, &mut status_style)
            .expect("Failed to style statusbar");

        self.content_id = Some(content_id);
        self.list_id = Some(list_id);
    }

    #[command]
    /// Append a new list item.
    pub fn add_item(&mut self, c: &mut dyn Context) -> Result<()> {
        let list_id = self.list_id.expect("list not initialized");
        c.with_widget_mut(list_id, &mut |widget, _ctx| {
            let any = widget as &mut dyn Any;
            let list = any
                .downcast_mut::<List<IntervalItem>>()
                .expect("list type mismatch");
            list.append(IntervalItem::new());
            Ok(())
        })
    }
}

impl Widget for Intervals {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c);

        if let Some(list_id) = self.list_id {
            c.with_widget_mut(list_id, &mut |widget, _ctx| {
                let any = widget as &mut dyn Any;
                let list = any
                    .downcast_mut::<List<IntervalItem>>()
                    .expect("list type mismatch");
                list.for_each_mut(|item| item.tick());
                Ok(())
            })
            .ok();
        }

        Some(Duration::from_secs(1))
    }
}

impl Loader for Intervals {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<List<IntervalItem>>();
    }
}

/// Install key bindings for the intervals demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    cnpy.style.add(
        "statusbar/text",
        Some(solarized::BASE02),
        Some(solarized::BASE1),
        None,
    );

    cnpy.bind_key('a', "intervals", "intervals::add_item()")
        .unwrap();
    cnpy.bind_key('g', "intervals", "list::select_first()")
        .unwrap();
    cnpy.bind_key('j', "intervals", "list::select_next()")
        .unwrap();
    cnpy.bind_key('d', "intervals", "list::delete_selected()")
        .unwrap();
    cnpy.bind_mouse(
        mouse::Action::ScrollDown,
        "intervals",
        "list::select_next()",
    )
    .unwrap();
    cnpy.bind_key(key::KeyCode::Down, "intervals", "list::select_next()")
        .unwrap();
    cnpy.bind_key('k', "intervals", "list::select_prev()")
        .unwrap();
    cnpy.bind_key(key::KeyCode::Up, "intervals", "list::select_prev()")
        .unwrap();
    cnpy.bind_mouse(mouse::Action::ScrollUp, "intervals", "list::select_prev()")
        .unwrap();

    cnpy.bind_key(key::KeyCode::PageDown, "intervals", "list::page_down()")
        .unwrap();
    cnpy.bind_key(' ', "intervals", "list::page_down()")
        .unwrap();
    cnpy.bind_key(key::KeyCode::PageUp, "intervals", "list::page_up()")
        .unwrap();

    cnpy.bind_key('q', "intervals", "root::quit()").unwrap();
}
