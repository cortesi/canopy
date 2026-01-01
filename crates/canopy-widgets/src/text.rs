use std::cell::RefCell;

use canopy::{
    Context, ViewContext, Widget, command,
    commands::{ScrollDirection, VerticalDirection},
    derive_commands,
    error::Result,
    geom::Line,
    layout::{Constraint, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    text,
};
use unicode_width::UnicodeWidthStr;

use crate::Selectable;

/// Canvas width behavior for text widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasWidth {
    /// Match the view width.
    View,
    /// Use the maximum wrapped line width.
    Intrinsic,
    /// Use a fixed canvas width.
    Fixed(u32),
}

/// Multiline text widget with wrapping and scrolling.
pub struct Text {
    /// Raw text content.
    raw: String,
    /// Optional fixed width for wrapping.
    wrap_width: Option<u32>,
    /// Canvas width behavior.
    canvas_width: CanvasWidth,
    /// Style path for text rendering.
    style: String,
    /// Optional style path for selected text rendering.
    selected_style: Option<String>,
    /// Selection state for use in lists.
    selected: bool,
    /// Cached wrapped text for the last wrap width.
    wrap_cache: RefCell<Option<WrapCache>>,
}

impl Selectable for Text {
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

#[derive_commands]
impl Text {
    /// Construct a text widget with raw content.
    pub fn new(raw: impl Into<String>) -> Self {
        Self {
            raw: raw.into(),
            wrap_width: None,
            canvas_width: CanvasWidth::View,
            style: String::from("text"),
            selected_style: None,
            selected: false,
            wrap_cache: RefCell::new(None),
        }
    }

    /// Add a fixed width for wrapping.
    pub fn with_wrap_width(mut self, width: u32) -> Self {
        self.wrap_width = Some(width);
        self.wrap_cache.borrow_mut().take();
        self
    }

    /// Configure the canvas width behavior.
    pub fn with_canvas_width(mut self, width: CanvasWidth) -> Self {
        self.canvas_width = width;
        self
    }

    /// Set the text rendering style.
    pub fn with_style(mut self, style: impl Into<String>) -> Self {
        self.style = style.into();
        self
    }

    /// Set the text rendering style when selected.
    pub fn with_selected_style(mut self, style: impl Into<String>) -> Self {
        self.selected_style = Some(style.into());
        self
    }

    /// Return the raw text content.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Replace the raw text content.
    pub fn set_raw(&mut self, raw: impl Into<String>) {
        self.raw = raw.into();
        self.wrap_cache.borrow_mut().take();
    }

    #[command]
    /// Scroll to an absolute content position.
    pub fn scroll_to(&mut self, c: &mut dyn Context, x: u32, y: u32) {
        c.scroll_to(x, y);
    }

    /// Scroll by one line in the specified direction.
    pub fn scroll(&mut self, c: &mut dyn Context, dir: ScrollDirection) {
        match dir {
            ScrollDirection::Up => c.scroll_up(),
            ScrollDirection::Down => c.scroll_down(),
            ScrollDirection::Left => c.scroll_left(),
            ScrollDirection::Right => c.scroll_right(),
        };
    }

    /// Page in the specified direction.
    pub fn page(&mut self, c: &mut dyn Context, dir: VerticalDirection) {
        match dir {
            VerticalDirection::Up => c.page_up(),
            VerticalDirection::Down => c.page_down(),
        };
    }

    #[command]
    /// Scroll up by one line.
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Up);
    }

    #[command]
    /// Scroll down by one line.
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Down);
    }

    #[command]
    /// Scroll left by one column.
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Left);
    }

    #[command]
    /// Scroll right by one column.
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Right);
    }

    #[command]
    /// Page up by one screen.
    pub fn page_up(&mut self, c: &mut dyn Context) {
        self.page(c, VerticalDirection::Up);
    }

    #[command]
    /// Page down by one screen.
    pub fn page_down(&mut self, c: &mut dyn Context) {
        self.page(c, VerticalDirection::Down);
    }

    /// Determine the wrapping width for the given available space.
    fn wrap_width_for(&self, available_width: u32) -> usize {
        let width = self.wrap_width.unwrap_or(available_width).max(1);
        width as usize
    }

    /// Access cached wrapped lines for the provided width.
    fn with_wrap_cache<R>(&self, width: usize, f: impl FnOnce(&WrapCache) -> R) -> R {
        let mut cache = self.wrap_cache.borrow_mut();
        let rebuild = cache.as_ref().is_none_or(|cached| cached.width != width);
        if rebuild {
            let lines = textwrap::wrap(&self.raw, width)
                .into_iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>();
            let max_width = lines
                .iter()
                .map(|line| UnicodeWidthStr::width(line.as_str()))
                .max()
                .unwrap_or(0) as u32;
            *cache = Some(WrapCache {
                width,
                lines,
                max_width,
            });
        }
        f(cache.as_ref().expect("wrap cache initialized"))
    }
}

/// Cached wrapped lines for a specific width.
struct WrapCache {
    /// Width used for wrapping.
    width: usize,
    /// Wrapped lines at the width.
    lines: Vec<String>,
    /// Maximum wrapped line width.
    max_width: u32,
}

impl Widget for Text {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let content_origin = view.content_origin();
        let width = self.wrap_width_for(view_rect.w);
        let style = if self.selected {
            self.selected_style.as_deref().unwrap_or(&self.style)
        } else {
            &self.style
        };

        self.with_wrap_cache(width, |cache| -> Result<()> {
            for i in 0..view_rect.h {
                let line_idx = (view_rect.tl.y + i) as usize;
                if let Some(line) = cache.lines.get(line_idx) {
                    let start_col = view_rect.tl.x as usize;
                    let (out, _) = text::slice_by_columns(line, start_col, view_rect.w as usize);
                    let line_rect = Line::new(
                        content_origin.x,
                        content_origin.y.saturating_add(i),
                        view_rect.w,
                    );
                    rndr.text(style, line_rect, out)?;
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let raw_width = self
            .raw
            .lines()
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0) as u32;

        let max_width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n,
            Constraint::Unbounded => self.wrap_width.unwrap_or(raw_width),
        };

        let wrap_width = match c.width {
            Constraint::Unbounded => self.wrap_width.unwrap_or(raw_width),
            _ => self
                .wrap_width
                .map(|w| w.min(max_width))
                .unwrap_or(max_width),
        }
        .max(1);

        let height = self.with_wrap_cache(wrap_width as usize, |cache| cache.lines.len() as u32);
        c.clamp(Size::new(wrap_width, height))
    }

    fn canvas(&self, view: Size<u32>, _ctx: &canopy::layout::CanvasContext) -> Size<u32> {
        let wrap_width = self.wrap_width_for(view.width.max(1));
        let wrapped_width = self
            .with_wrap_cache(wrap_width, |cache| cache.max_width)
            .max(1);
        let canvas_width = match self.canvas_width {
            CanvasWidth::View => view.width.max(1),
            CanvasWidth::Intrinsic => wrapped_width,
            CanvasWidth::Fixed(width) => width.max(1),
        };
        let height = self.with_wrap_cache(wrap_width, |cache| cache.lines.len() as u32);
        Size::new(canvas_width, height)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("text")
    }
}
