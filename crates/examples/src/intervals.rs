use std::time::Duration;

use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ViewContext, command, derive_commands,
    error::Result,
    event::{key, mouse},
    layout::{Edges, Layout, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    style::{AttrSet, solarized},
    widget::Widget,
    widgets::{
        Box, Center, Text, VStack, boxed, frame,
        list::{List, Selectable},
    },
};
use unicode_width::UnicodeWidthStr;

/// Padding inside each counter entry box.
const ENTRY_PADDING: u32 = 2;
/// Height for each counter entry row, including borders.
const ENTRY_HEIGHT: u32 = 1 + ENTRY_PADDING * 2;

/// Counter widget that increments on a timer.
pub struct CounterItem {
    /// Current counter value.
    value: u64,
    /// Selection state.
    selected: bool,
    /// Mounted box node ID.
    box_id: Option<NodeId>,
    /// Mounted center node ID.
    center_id: Option<NodeId>,
    /// Mounted text node ID.
    text_id: Option<NodeId>,
}

impl Selectable for CounterItem {
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

impl Default for CounterItem {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl CounterItem {
    /// Construct a new counter item.
    pub fn new() -> Self {
        Self {
            value: 0,
            selected: false,
            box_id: None,
            center_id: None,
            text_id: None,
        }
    }

    /// Increment the counter.
    pub fn tick(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.value = self.value.saturating_add(1);
        self.sync_label(ctx)?;
        ctx.taint();
        Ok(())
    }

    /// Current label string.
    fn label(&self) -> String {
        self.value.to_string()
    }

    /// Label display width in cells.
    fn label_width(&self) -> u32 {
        let label = self.label();
        UnicodeWidthStr::width(label.as_str()).max(1) as u32
    }

    /// Update the box layout based on the current label width.
    fn update_box_layout(&self, ctx: &mut dyn Context) -> Result<()> {
        let Some(box_id) = self.box_id else {
            return Ok(());
        };

        let desired_width = self.label_width().saturating_add(ENTRY_PADDING * 2).max(3);
        let desired_height = ENTRY_HEIGHT;

        ctx.with_layout_of(box_id, &mut |layout| {
            *layout = Layout::column()
                .fixed_width(desired_width)
                .fixed_height(desired_height)
                .padding(Edges::all(ENTRY_PADDING));
        })?;

        Ok(())
    }

    /// Ensure the child widget tree is mounted.
    fn ensure_tree(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.box_id.is_some() && self.center_id.is_some() && self.text_id.is_some() {
            return Ok(());
        }

        let box_id = ctx.add_orphan(Box::new().with_glyphs(boxed::SINGLE).with_fill());
        let center_id = ctx.add_orphan(Center::new());
        let text_id = ctx.add_orphan(Text::new(self.label()));

        ctx.mount_child_to(box_id, text_id)?;
        ctx.mount_child_to(center_id, box_id)?;
        ctx.mount_child_to(ctx.node_id(), center_id)?;

        self.box_id = Some(box_id);
        self.center_id = Some(center_id);
        self.text_id = Some(text_id);
        self.update_box_layout(ctx)?;

        Ok(())
    }

    /// Sync the text label to the current value.
    fn sync_label(&self, ctx: &mut dyn Context) -> Result<()> {
        let Some(text_id) = self.text_id else {
            return Ok(());
        };
        let label = self.label();
        ctx.with_widget(text_id, |text: &mut Text, _ctx| {
            text.set_raw(label.clone());
            Ok(())
        })?;
        self.update_box_layout(ctx)?;
        Ok(())
    }
}

impl Widget for CounterItem {
    fn layout(&self) -> Layout {
        Layout::fill().fixed_height(ENTRY_HEIGHT)
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.ensure_tree(ctx)
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let desired_width = self.label_width().saturating_add(ENTRY_PADDING * 2).max(3);
        c.clamp(Size::new(desired_width, ENTRY_HEIGHT))
    }

    fn render(&mut self, rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        rndr.push_layer("entry");
        if self.selected {
            rndr.push_layer("selected");
        }
        Ok(())
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("counter_item")
    }
}

/// Status bar widget for the intervals demo.
pub struct StatusBar;

#[derive_commands]
impl StatusBar {}

impl Widget for StatusBar {
    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("statusbar");
        r.text(
            "statusbar/text",
            ctx.view().outer_rect_local().line(0),
            "intervals",
        )?;
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

        let list_id = c.add_orphan(List::<CounterItem>::new());
        let frame_id = frame::Frame::wrap(c, list_id).expect("Failed to wrap frame");
        let status_id = c.add_orphan(StatusBar);
        c.add_child(
            VStack::new()
                .push_flex(frame_id, 1)
                .push_fixed(status_id, 1),
        )
        .expect("Failed to mount layout");
    }

    /// List node id inside the content frame.
    fn list_id(c: &dyn Context) -> Option<NodeId> {
        c.find_node("*/frame/list")
    }

    /// Execute a closure with mutable access to the list widget.
    fn with_list<F, R>(&self, c: &mut dyn Context, mut f: F) -> Result<R>
    where
        F: FnMut(&mut List<CounterItem>, &mut dyn Context) -> Result<R>,
    {
        self.ensure_tree(c);
        let list_id = Self::list_id(c).expect("list not initialized");
        c.with_widget(list_id, |list: &mut List<CounterItem>, ctx| f(list, ctx))
    }

    #[command]
    /// Append a new list item.
    pub fn add_item(&mut self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.append(ctx, CounterItem::new())?;
            Ok(())
        })
    }
}

impl Widget for Intervals {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("intervals");
        Ok(())
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c);
        let list_id = Self::list_id(c)?;

        // Tick each counter item
        let len = c
            .with_widget(list_id, |list: &mut List<CounterItem>, _ctx| Ok(list.len()))
            .ok()?;

        for i in 0..len {
            let item_id = c
                .with_widget(list_id, |list: &mut List<CounterItem>, _ctx| {
                    Ok(list.item(i).map(|id| id.into()))
                })
                .ok()??;

            c.with_widget(item_id, |item: &mut CounterItem, ctx| item.tick(ctx))
                .ok();
        }

        Some(Duration::from_secs(1))
    }
}

impl Loader for Intervals {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<List<CounterItem>>();
    }
}

/// Install key bindings for the intervals demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    let selected_attrs = AttrSet {
        bold: true,
        ..AttrSet::default()
    };

    cnpy.style.add(
        "intervals/entry/border",
        Some(solarized::BASE0),
        Some(solarized::BASE03),
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "intervals/entry/fill",
        Some(solarized::BASE0),
        Some(solarized::BASE03),
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "intervals/entry/text",
        Some(solarized::BASE0),
        Some(solarized::BASE03),
        Some(AttrSet::default()),
    );
    cnpy.style.add(
        "intervals/entry/selected/border",
        Some(solarized::BASE3),
        Some(solarized::BLUE),
        Some(selected_attrs),
    );
    cnpy.style.add(
        "intervals/entry/selected/fill",
        Some(solarized::BASE3),
        Some(solarized::BLUE),
        Some(selected_attrs),
    );
    cnpy.style.add(
        "intervals/entry/selected/text",
        Some(solarized::BASE3),
        Some(solarized::BLUE),
        Some(selected_attrs),
    );
    cnpy.style.add(
        "statusbar/text",
        Some(solarized::BASE02),
        Some(solarized::BASE1),
        None,
    );

    Binder::new(cnpy)
        .with_path("intervals")
        .key('a', "intervals::add_item()")
        .key('g', "list::select_first()")
        .key('j', "list::select_next()")
        .key(key::KeyCode::Down, "list::select_next()")
        .mouse(mouse::Action::ScrollDown, "list::select_next()")
        .key('k', "list::select_prev()")
        .key(key::KeyCode::Up, "list::select_prev()")
        .mouse(mouse::Action::ScrollUp, "list::select_prev()")
        .key('d', "list::delete_selected()")
        .key(key::KeyCode::PageDown, "list::page_down()")
        .key(' ', "list::page_down()")
        .key(key::KeyCode::PageUp, "list::page_up()")
        .key('q', "root::quit()");
}
