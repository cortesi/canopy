use std::time::Duration;

use canopy::{
    CanopyBuilder, Context, ContextExt, NodeName, Register, Render, Setup, ViewContext,
    ViewContextExt, Widget, derive_commands,
    error::Result,
    geom::Size,
    layout::{Edges, Layout, MeasureConstraints, Measurement},
    style::StyleMap,
};
use canopy_widgets::{Border, Center, Container, Frame, List, SINGLE, Selectable, Text};
use unicode_width::UnicodeWidthStr;

use crate::flex_row;

/// Padding inside each counter entry box.
const ENTRY_PADDING: u32 = 2;
/// Height for each counter entry row, including borders.
const ENTRY_HEIGHT: u32 = 1 + ENTRY_PADDING * 2;

/// Default bindings for the intervals demo.
const DEFAULT_BINDINGS: &str = r#"
canopy.keymap({
    path = "intervals",
    { key = "a", description = "Add item", action = command.intervals.add_item() },
    { key = "g", description = "First item", action = command.list.select_first() },
    { key = { "j", "Down" }, description = "Next item", action = command.list.select_by(1) },
    {
        mouse = "ScrollDown",
        description = "Next item",
        action = function()
            list.select_by(1)
        end,
    },
    { key = { "k", "Up" }, description = "Previous item", action = command.list.select_by(-1) },
    {
        mouse = "ScrollUp",
        description = "Previous item",
        action = function()
            list.select_by(-1)
        end,
    },
    { key = "d", description = "Delete item", action = command.list.delete_selected() },
    { key = { "PageDown", "Space" }, description = "Page down", action = command.list.page(1) },
    { key = "PageUp", description = "Page up", action = command.list.page(-1) },
    { key = "q", description = "Quit", action = command.root.quit() },
})
"#;

/// Counter widget that increments on a timer.
pub(crate) struct CounterItem {
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
        let Some(box_id) = ctx.unique_descendant::<Border>(ctx.node_id())? else {
            return Ok(());
        };

        let desired_width = self.label_width().saturating_add(ENTRY_PADDING * 2).max(3);
        let desired_height = ENTRY_HEIGHT;

        ctx.set_layout_override(
            box_id.into(),
            Layout::column()
                .fixed_width(desired_width)
                .fixed_height(desired_height)
                .padding(Edges::all(ENTRY_PADDING))
                .into(),
        )?;

        Ok(())
    }

    /// Sync the text label to the current value.
    fn sync_label(&self, ctx: &mut dyn Context) -> Result<()> {
        let label = self.label();
        if let Some(text) = ctx.unique_descendant::<Text>(ctx.node_id())? {
            ctx.with_widget_mut(text, |text: &mut Text, _ctx| {
                text.set_text(label);
                Ok(())
            })?;
        }
        self.update_box_layout(ctx)?;
        Ok(())
    }
}

impl Widget for CounterItem {
    fn layout(&self) -> Layout {
        Layout::fill().fixed_height(ENTRY_HEIGHT)
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let box_id = ctx.add_child(ctx.node_id(), Border::new().with_glyphs(SINGLE).with_fill())?;
        let center_id = ctx.add_child(box_id, Center::new())?;
        ctx.add_child(center_id, Text::new(self.label()))?;
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
    pub(crate) fn add_item(&self, c: &mut dyn Context) -> Result<()> {
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
        let root = c.node_id();
        let column = c.add_child(root, Container::column())?;
        let frame_id = c.add_child(column, Frame::new())?;
        c.add_child(frame_id, List::<CounterItem>::new())?;
        c.set_layout_override(frame_id.into(), flex_row(1))
    }

    fn render(&mut self, r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("intervals");
        Ok(())
    }

    fn poll(&mut self, c: &mut dyn Context) -> Result<Option<Duration>> {
        let Ok(item_ids) = self.with_list(c, |list, _ctx| {
            let mut ids = Vec::with_capacity(list.len());
            for i in 0..list.len() {
                if let Some(id) = list.item(i) {
                    ids.push(id);
                }
            }
            Ok(ids)
        }) else {
            return Ok(None);
        };

        for item_id in item_ids {
            c.with_widget_mut(item_id, |item: &mut CounterItem, ctx| item.tick(ctx))
                .ok();
        }

        Ok(Some(Duration::from_secs(1)))
    }
}

impl Register for Intervals {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        setup.add_commands::<List<CounterItem>>()?;
        Ok(())
    }
}

/// Install native styles during the configuration phase.
fn setup_style(style: &mut StyleMap) {
    crate::selectable_entry_styles(style.rules(), "intervals/entry")
        .no_prefix()
        .apply();
}

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder
        .configure(|setup| {
            setup_style(setup.style_mut());
            Ok(())
        })
        .bindings("intervals", DEFAULT_BINDINGS)
}
