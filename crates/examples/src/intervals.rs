use std::time::Duration;

use canopy::{
    Binder, Canopy, Context, Loader, ViewContext, Widget, command, derive_commands,
    error::Result,
    event::{key, mouse},
    layout::{Edges, Layout, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    style::{AttrSet, solarized},
    widgets::{Box, Center, Frame, List, SINGLE, Selectable, Text, VStack},
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
        }
    }

    /// Increment the counter.
    pub fn tick(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.value = self.value.saturating_add(1);
        self.sync_label(ctx)?;
        ctx.invalidate_layout();
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
        let Some(box_id) = ctx.unique_descendant::<Box>()? else {
            return Ok(());
        };

        let desired_width = self.label_width().saturating_add(ENTRY_PADDING * 2).max(3);
        let desired_height = ENTRY_HEIGHT;

        ctx.with_layout_of(box_id.into(), &mut |layout| {
            *layout = Layout::column()
                .fixed_width(desired_width)
                .fixed_height(desired_height)
                .padding(Edges::all(ENTRY_PADDING));
        })?;

        Ok(())
    }

    /// Sync the text label to the current value.
    fn sync_label(&self, ctx: &mut dyn Context) -> Result<()> {
        let label = self.label();
        let _ = ctx.try_with_unique_descendant::<Text, _>(|text, _ctx| {
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
        let box_id = ctx.add_child(Box::new().with_glyphs(SINGLE).with_fill())?;
        let center_id = ctx.add_child_to(box_id, Center::new())?;
        ctx.add_child_to(center_id, Text::new(self.label()))?;
        self.update_box_layout(ctx)?;
        Ok(())
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

    /// Execute a closure with mutable access to the list widget.
    fn with_list<F, R>(&self, c: &mut dyn Context, mut f: F) -> Result<R>
    where
        F: FnMut(&mut List<CounterItem>, &mut dyn Context) -> Result<R>,
    {
        c.with_unique_descendant::<List<CounterItem>, _>(|list, ctx| f(list, ctx))
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

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let frame_id = c.create_detached(Frame::new());
        c.add_child_to(frame_id, List::<CounterItem>::new())?;
        let status_id = c.create_detached(StatusBar);
        c.add_child(
            VStack::new()
                .push_flex(frame_id, 1)
                .push_fixed(status_id, 1),
        )?;
        Ok(())
    }

    fn render(&mut self, r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("intervals");
        Ok(())
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        let item_ids = self
            .with_list(c, |list, _ctx| {
                let mut ids = Vec::with_capacity(list.len());
                for i in 0..list.len() {
                    if let Some(id) = list.item(i) {
                        ids.push(id.into());
                    }
                }
                Ok(ids)
            })
            .ok()?;

        for item_id in item_ids {
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
    use canopy::style::StyleBuilder;

    let selected_attrs = AttrSet {
        bold: true,
        ..AttrSet::default()
    };

    let normal = StyleBuilder::new()
        .fg(solarized::BASE0)
        .bg(solarized::BASE03);

    let selected = StyleBuilder::new()
        .fg(solarized::BASE3)
        .bg(solarized::BLUE)
        .attrs(selected_attrs);

    cnpy.style
        .rules()
        .prefix("intervals/entry")
        .style_all(&["border", "fill", "text"], normal)
        .style_all(
            &["selected/border", "selected/fill", "selected/text"],
            selected,
        )
        .no_prefix()
        .style(
            "statusbar/text",
            StyleBuilder::new()
                .fg(solarized::BASE02)
                .bg(solarized::BASE1),
        )
        .apply();

    Binder::new(cnpy)
        .with_path("intervals")
        .key('a', "intervals::add_item()")
        .key('g', "list::select_first()")
        .key('j', "list::select_by(1)")
        .key(key::KeyCode::Down, "list::select_by(1)")
        .mouse(mouse::Action::ScrollDown, "list::select_by(1)")
        .key('k', "list::select_by(-1)")
        .key(key::KeyCode::Up, "list::select_by(-1)")
        .mouse(mouse::Action::ScrollUp, "list::select_by(-1)")
        .key('d', "list::delete_selected()")
        .key(key::KeyCode::PageDown, "list::page_down()")
        .key(' ', "list::page_down()")
        .key(key::KeyCode::PageUp, "list::page_up()")
        .key('q', "root::quit()");
}
