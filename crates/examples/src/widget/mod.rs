//! Widget demo entry points.

use std::time::Duration;

use canopy::{
    Context, ContextExt, NodeId, NodeName, TypedId, Widget,
    error::{Error, Result},
    layout::{Edges, Layout, MeasureOverflow},
    style::{Color, Paint, StyleMap},
};
use canopy_widgets::{Center, Frame, List, Pad, Text};
use unicode_width::UnicodeWidthStr;

mod font;
mod term;

pub use font::{FontDemo, FontSource};
pub use term::TermDemo;
pub(crate) use term::TerminalStack;

/// Style path used for list items.
const LIST_STYLE_PATH: &str = "widget/list/item";
/// Style path used for selected list items.
const LIST_SELECTED_STYLE_PATH: &str = "widget/list/selected";
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
    /// Padding inside the demo host.
    inner_padding: u32,
    /// Padding outside the demo host.
    outer_padding: u32,
}

impl DemoHost {
    /// Build a demo host for the provided widget.
    pub fn new(child: impl Into<Box<dyn Widget>>, size: DemoSize, frame: bool) -> Self {
        Self {
            child: Some(child.into()),
            size,
            frame,
            inner_padding: DEMO_PADDING,
            outer_padding: 0,
        }
    }

    /// Set the inner padding for the demo host.
    pub fn with_inner_padding(mut self, padding: u32) -> Self {
        self.inner_padding = padding;
        self
    }

    /// Set the outer padding for the demo host.
    pub fn with_outer_padding(mut self, padding: u32) -> Self {
        self.outer_padding = padding;
        self
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
        let parent_id: NodeId = if self.outer_padding > 0 {
            let outer_pad_id = ctx.add_child_to(center_id, Pad::uniform(self.outer_padding))?;
            let outer_layout = Layout::fill().padding(Edges::all(self.outer_padding));
            ctx.set_layout_of(outer_pad_id, outer_layout)?;
            outer_pad_id.into()
        } else {
            center_id.into()
        };
        let pad_id = ctx.add_child_to(parent_id, Pad::uniform(self.inner_padding))?;
        let sized_id: NodeId = if self.frame {
            let frame_id = ctx.add_child_to(pad_id, Frame::new())?;
            ctx.add_child_to_boxed(frame_id.into(), child)?;
            frame_id.into()
        } else {
            ctx.add_child_to_boxed(pad_id.into(), child)?
        };
        let mut layout = Layout::fill().padding(Edges::all(self.inner_padding));
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

    fn name(&self) -> NodeName {
        NodeName::convert("widget-demo-host")
    }
}

/// Items shown by the list demo.
const LIST_ITEMS: [&str; 5] = [
    "Item One",
    "Item Two",
    "Item Three",
    "Item Four",
    "Item Five",
];

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

    /// Return the width and height that exactly fit the demo's items.
    pub fn natural_size() -> (u32, u32) {
        let width = LIST_ITEMS
            .iter()
            .map(|item| UnicodeWidthStr::width(*item) as u32 + 1)
            .max()
            .unwrap_or(1)
            .max(1);
        (width, LIST_ITEMS.len() as u32)
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

        let item_texts: Vec<String> = LIST_ITEMS.iter().map(|item| format!(" {item}")).collect();
        let max_width = Self::natural_size().0;

        let center_id = ctx.add_child(Center::new())?;
        let list_id = ctx.add_child_to(center_id, List::<Text>::new())?;
        let list_layout = Layout::column()
            .overflow_x(MeasureOverflow::Unbounded)
            .fixed_width(max_width);
        ctx.set_layout_of(list_id, list_layout)?;
        ctx.with_widget_mut(list_id, |list: &mut List<Text>, ctx| {
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
        ctx.with_widget_mut(list_id, |list: &mut List<Text>, ctx| {
            let len = list.len();
            if len == 0 {
                return Ok(());
            }
            match list.selected_index() {
                Some(idx) if idx + 1 < len => list.select_by(ctx, 1),
                _ => list.select_first(ctx),
            }
        })
        .ok();
        Some(interval)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("widget-list-demo")
    }
}
