use std::time::Duration;

use canopy::{
    Canopy, Context, Loader, NodeId, ViewContext, command, derive_commands,
    error::Result,
    event::{key, mouse},
    geom::{Expanse, Rect},
    layout::Dimension,
    render::Render,
    style::solarized,
    widget::Widget,
    widgets::{frame, list::*},
};

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
}

/// Root node for the intervals demo.
pub struct Intervals;

impl Default for Intervals {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl Intervals {
    /// Construct a new intervals demo.
    pub fn new() -> Self {
        Self
    }

    /// Ensure the frame, list, and status bar are created.
    fn ensure_tree(&self, c: &mut dyn Context) {
        if !c.children().is_empty() {
            return;
        }

        let content_id = c
            .add_child(frame::Frame::new())
            .expect("Failed to mount content frame");
        let list_id = c
            .add_child_to(content_id, List::new(Vec::<IntervalItem>::new()))
            .expect("Failed to mount list");
        let status_id = c.add_child(StatusBar).expect("Failed to mount statusbar");

        c.with_layout(&mut |layout| {
            layout.flex_col();
        })
        .expect("Failed to configure layout");
        c.with_layout_of(content_id, &mut |layout| {
            layout.flex_item(1.0, 1.0, Dimension::Auto);
        })
        .expect("Failed to configure content layout");
        c.with_layout_of(list_id, &mut |layout| {
            layout.flex_item(1.0, 1.0, Dimension::Auto);
        })
        .expect("Failed to configure list layout");
        c.with_layout_of(status_id, &mut |layout| {
            layout.height(Dimension::Points(1.0)).flex_shrink(0.0);
        })
        .expect("Failed to configure status layout");
    }

    /// Content frame node id.
    fn content_id(c: &dyn Context) -> Option<NodeId> {
        c.children().first().copied()
    }

    /// List node id inside the content frame.
    fn list_id(c: &dyn Context) -> Option<NodeId> {
        let content_id = Self::content_id(c)?;
        let children = c.children_of(content_id);
        match children.as_slice() {
            [] => None,
            [list_id] => Some(*list_id),
            _ => panic!("expected a single list child"),
        }
    }

    /// Execute a closure with mutable access to the list widget.
    fn with_list<F>(&self, c: &mut dyn Context, mut f: F) -> Result<()>
    where
        F: FnMut(&mut List<IntervalItem>) -> Result<()>,
    {
        self.ensure_tree(c);
        let list_id = Self::list_id(c).expect("list not initialized");
        c.with_widget(list_id, |list: &mut List<IntervalItem>, _ctx| f(list))
    }

    #[command]
    /// Append a new list item.
    pub fn add_item(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list| {
            list.append(IntervalItem::new());
            Ok(())
        })
    }
}

impl Widget for Intervals {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.with_list(c, |list| {
            list.for_each_mut(|item| item.tick());
            Ok(())
        })
        .ok();

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
