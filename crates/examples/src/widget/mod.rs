//! Widget demo entry points.

use std::time::Duration;

use canopy::{
    Context, NodeId, ReadContext, TypedId, Widget,
    error::{Error, Result},
    layout::{Edges, Layout},
    render::Render,
    state::NodeName,
    style::{Color, Paint, StyleMap},
};
use canopy_widgets::{Center, Frame, List, Pad, Text};
use unicode_width::UnicodeWidthStr;

mod font;

pub use font::{FontDemo, FontSource};

/// Style path used for list items.
const LIST_STYLE_PATH: &str = "widget/list/item";
/// Style path used for selected list items.
const LIST_SELECTED_STYLE_PATH: &str = "widget/list/selected";
/// Default list advance interval in milliseconds.
const DEFAULT_LIST_INTERVAL_MS: u64 = 500;
/// Empty boundary around demo content.
const DEMO_PADDING: u32 = 1;

/// Common sizing configuration for widget demos.
#[derive(Debug, Clone, Copy, Default)]
pub struct DemoSize {
    /// Optional fixed width override.
    pub width: Option<u32>,
    /// Optional fixed height override.
    pub height: Option<u32>,
}

impl DemoSize {
    /// Create sizing overrides.
    pub fn new(width: Option<u32>, height: Option<u32>) -> Self {
        Self { width, height }
    }
}

/// Host widget that centers a padded child within optional sizing overrides.
pub struct DemoHost {
    /// Child widget to render.
    child: Option<Box<dyn Widget>>,
    /// Sizing overrides for the child.
    size: DemoSize,
    /// Whether to wrap the demo in a frame.
    frame: bool,
}

impl DemoHost {
    /// Build a demo host for the provided widget.
    pub fn new(child: impl Into<Box<dyn Widget>>, size: DemoSize, frame: bool) -> Self {
        Self {
            child: Some(child.into()),
            size,
            frame,
        }
    }
}

impl Widget for DemoHost {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let child = self
            .child
            .take()
            .ok_or_else(|| Error::Internal("demo child missing".into()))?;
        if self.frame {
            let mut style = StyleMap::new();
            style
                .rules()
                .fg("frame", Paint::solid(Color::Blue))
                .fg("frame/focused", Paint::solid(Color::Blue))
                .fg("frame/active", Paint::solid(Color::Blue))
                .apply();
            ctx.set_style(style);
        }
        let center_id = ctx.add_child(Center::new())?;
        let pad_id = ctx.add_child_to(center_id, Pad::uniform(DEMO_PADDING))?;
        let sized_id: NodeId = if self.frame {
            let frame_id = ctx.add_child_to(pad_id, Frame::new())?;
            let _child_id = ctx.add_child_to_boxed(frame_id.into(), child)?;
            frame_id.into()
        } else {
            ctx.add_child_to_boxed(pad_id.into(), child)?
        };
        let mut layout = Layout::fill().padding(Edges::all(DEMO_PADDING));
        if let Some(width) = self.size.width {
            layout = layout.fixed_width(width);
        }
        if let Some(height) = self.size.height {
            layout = layout.fixed_height(height);
        }
        ctx.set_layout_of(pad_id, layout)?;
        if !self.frame {
            ctx.set_layout_of(sized_id, Layout::fill())?;
        }
        Ok(())
    }

    fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("widget-demo-host")
    }
}

/// List widget configuration.
pub struct ListDemo {
    /// Poll interval for list selection updates.
    interval: Duration,
    /// Whether polling has started.
    started: bool,
    /// List widget id.
    list_id: Option<TypedId<List<Text>>>,
}

impl ListDemo {
    /// Build a list demo widget.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            started: false,
            list_id: None,
        }
    }
}

impl Default for ListDemo {
    fn default() -> Self {
        Self::new(Duration::from_millis(DEFAULT_LIST_INTERVAL_MS))
    }
}

impl Widget for ListDemo {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let mut style = StyleMap::new();
        style
            .rules()
            .bg(LIST_SELECTED_STYLE_PATH, Paint::solid(Color::DarkBlue))
            .apply();
        ctx.set_style(style);

        let items = [
            "Item One",
            "Item Two",
            "Item Three",
            "Item Four",
            "Item Five",
        ];
        let item_texts: Vec<String> = items.iter().map(|item| format!(" {item}")).collect();
        let max_width = item_texts
            .iter()
            .map(|item| UnicodeWidthStr::width(item.as_str()) as u32)
            .max()
            .unwrap_or(1)
            .max(1);

        let center_id = ctx.add_child(Center::new())?;
        let list_id = ctx.add_child_to(center_id, List::<Text>::new())?;
        let list_layout = Layout::column()
            .measured()
            .overflow_x()
            .fixed_width(max_width);
        ctx.set_layout_of(list_id, list_layout)?;
        ctx.with_typed(list_id, |list: &mut List<Text>, ctx| {
            for item in item_texts {
                let text = Text::new(item)
                    .with_style(LIST_STYLE_PATH)
                    .with_selected_style(LIST_SELECTED_STYLE_PATH);
                let item_id = list.append(ctx, text)?;
                ctx.set_layout_of(item_id, Layout::fill().fixed_height(1))?;
            }
            Ok(())
        })?;
        self.list_id = Some(list_id);
        Ok(())
    }

    fn poll(&mut self, ctx: &mut dyn Context) -> Option<Duration> {
        let list_id = self.list_id?;
        let interval = self.interval.max(Duration::from_millis(1));
        if !self.started {
            self.started = true;
            return Some(interval);
        }
        if ctx
            .with_typed(list_id, |list: &mut List<Text>, ctx| {
                let len = list.len();
                if len == 0 {
                    return Ok(());
                }
                let last = len.saturating_sub(1);
                match list.selected_index() {
                    Some(idx) if idx < last => list.select_next(ctx),
                    Some(_) => list.select_first(ctx),
                    None => list.select_first(ctx),
                }
                Ok(())
            })
            .is_err()
        {
            return Some(interval);
        }
        Some(interval)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("widget-list-demo")
    }
}
