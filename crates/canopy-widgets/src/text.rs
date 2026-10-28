use std::cell::RefCell;

use canopy::{
    NodeName, ViewContext, Widget,
    error::Result,
    geom::{Line, Size},
    layout::{Constraint, MeasureConstraints, Measurement},
    render::Render,
    text::{self, WrapOptions},
};

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

/// How `Text` wraps unless a host chooses: `textwrap`'s defaults, with hyphen
/// breaks and no paragraphs.
const TEXT_WRAP: WrapOptions = WrapOptions {
    break_hyphens: true,
    paragraphs: false,
    max_lines: None,
};

/// Multiline text widget with wrapping and scrolling.
pub struct Text {
    /// Raw text content.
    raw: String,
    /// Optional fixed width for wrapping.
    wrap_width: Option<u32>,
    /// How the text breaks into lines.
    wrap_options: WrapOptions,
    /// Whether the text measures as wide as its widest wrapped line.
    fit: bool,
    /// Canvas width behavior.
    canvas_width: CanvasWidth,
    /// Style path for text rendering.
    style: String,
    /// Optional style path for selected text rendering.
    selected_style: Option<String>,
    /// Selection state for use in lists.
    selected: bool,
    /// Tab stop width in columns.
    tab_stop: usize,
    /// Cached wrapped text for the last wrap width.
    wrap_cache: RefCell<Option<WrapCache>>,
    /// Whether the text takes focus, so navigation keys scroll it.
    focusable: bool,
}

impl Selectable for Text {
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

impl Text {
    /// Construct a text widget with raw content.
    pub fn new(raw: impl Into<String>) -> Self {
        Self {
            raw: raw.into(),
            wrap_width: None,
            wrap_options: TEXT_WRAP,
            fit: false,
            canvas_width: CanvasWidth::View,
            style: String::from("text"),
            selected_style: None,
            selected: false,
            tab_stop: 4,
            wrap_cache: RefCell::new(None),
            focusable: false,
        }
    }

    /// Take focus, so the navigation intents scroll this text.
    #[must_use]
    pub fn with_focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }

    /// Add a fixed width for wrapping.
    #[must_use]
    pub fn with_wrap_width(mut self, width: u32) -> Self {
        self.wrap_width = Some(width);
        self.wrap_cache.borrow_mut().take();
        self
    }

    /// Wrap the text as `options` say, in place of `textwrap`'s defaults.
    #[must_use]
    pub fn with_wrap_options(mut self, options: WrapOptions) -> Self {
        self.wrap_options = options;
        self.wrap_cache.borrow_mut().take();
        self
    }

    /// Measure as wide as the widest wrapped line, not as the width offered.
    ///
    /// The text then wraps at the narrower of its view and its wrap width, so
    /// a short text takes a narrow view.
    #[must_use]
    pub fn with_fit(mut self, fit: bool) -> Self {
        self.fit = fit;
        self
    }

    /// Configure the canvas width behavior.
    #[must_use]
    pub fn with_canvas_width(mut self, width: CanvasWidth) -> Self {
        self.canvas_width = width;
        self
    }

    /// Set the text rendering style.
    #[must_use]
    pub fn with_style(mut self, style: impl Into<String>) -> Self {
        self.style = style.into();
        self
    }

    /// Replace the style of the text.
    pub fn set_style(&mut self, style: impl Into<String>) {
        self.style = style.into();
    }

    /// Set the text rendering style when selected.
    #[must_use]
    pub fn with_selected_style(mut self, style: impl Into<String>) -> Self {
        self.selected_style = Some(style.into());
        self
    }

    /// Set the tab stop width for tab expansion.
    #[must_use]
    pub fn with_tab_stop(mut self, tab_stop: usize) -> Self {
        self.tab_stop = tab_stop.max(1);
        self.wrap_cache.borrow_mut().take();
        self
    }

    /// Replace the raw text content.
    pub fn set_text(&mut self, raw: impl Into<String>) {
        self.raw = raw.into();
        self.wrap_cache.borrow_mut().take();
    }

    /// Return the widest unwrapped line, with tabs expanded.
    fn raw_width(&self) -> u32 {
        text::expand_tabs(&self.raw, self.tab_stop)
            .lines()
            .map(text::width)
            .max()
            .unwrap_or(0)
    }

    /// Determine the wrapping width for the given available space.
    fn wrap_width_for(&self, available_width: u32) -> usize {
        let width = match self.wrap_width {
            Some(wrap) if self.fit => wrap.min(available_width),
            Some(wrap) => wrap,
            None => available_width,
        };
        width.max(1) as usize
    }

    /// Access cached wrapped lines for the provided width.
    fn with_wrap_cache<R>(&self, width: usize, f: impl FnOnce(&WrapCache) -> R) -> R {
        let mut cache = self.wrap_cache.borrow_mut();
        let rebuild = cache.as_ref().is_none_or(|cached| cached.width != width);
        if rebuild {
            let expanded = text::expand_tabs(&self.raw, self.tab_stop);
            let columns = u32::try_from(width).unwrap_or(u32::MAX);
            let lines = text::wrap_with(&expanded, columns, self.wrap_options);
            let max_width = lines
                .iter()
                .map(|line| text::width(line))
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
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        self.focusable
    }

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
        let wrap_width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => {
                self.wrap_width.map_or(n, |wrap| wrap.min(n))
            }
            Constraint::Unbounded => self.wrap_width.unwrap_or_else(|| self.raw_width()),
        }
        .max(1);

        let (widest, height) = self.with_wrap_cache(wrap_width as usize, |cache| {
            (cache.max_width, cache.lines.len() as u32)
        });
        let width = if self.fit { widest } else { wrap_width };
        c.clamp(Size::new(width, height))
    }

    fn canvas(&self, view: Size, _ctx: &canopy::layout::CanvasContext) -> Size {
        let wrap_width = self.wrap_width_for(view.w.max(1));
        let wrapped_width = self
            .with_wrap_cache(wrap_width, |cache| cache.max_width)
            .max(1);
        let canvas_width = match self.canvas_width {
            CanvasWidth::View => view.w.max(1),
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

#[cfg(test)]
mod tests {
    use canopy::{
        layout::{Constraint, MeasureConstraints, Measurement},
        testing::harness::Harness,
    };

    use super::*;

    /// Return the size `text` measures to in at most `width` columns.
    fn measured(text: &Text, width: u32) -> (u32, u32) {
        match text.measure(MeasureConstraints::new(
            Constraint::AtMost(width),
            Constraint::Unbounded,
        )) {
            Measurement::Fixed(size) => (size.w, size.h),
            Measurement::Wrap => (0, 0),
        }
    }

    /// Text that fits its widest line, wraps at `columns`, and reads as prose.
    fn prose(text: &str, columns: u32) -> Text {
        Text::new(text)
            .with_wrap_width(columns)
            .with_wrap_options(WrapOptions::PROSE)
            .with_fit(true)
    }

    #[test]
    fn fitted_text_measures_its_widest_line() {
        assert_eq!(measured(&prose("reset it", 40), 80), (8, 1));
        let long = prose(&"word ".repeat(30), 40);
        let (width, height) = measured(&long, 80);
        assert!(width <= 40 && height == 4, "{width} x {height}");
        let (width, height) = measured(&long, 20);
        assert!(width <= 20 && height > 4, "{width} x {height}");
        // Without fit, text takes the width it wraps at.
        assert_eq!(
            measured(&Text::new("reset it").with_wrap_width(40), 80),
            (40, 1)
        );
    }

    #[test]
    fn fitted_text_wraps_at_its_limit_in_a_wide_view() -> Result<()> {
        let mut harness = Harness::builder(prose("one two three four\n\nfive six", 10))
            .size(20, 6)
            .build()?;
        harness.render()?;
        let lines = harness.tbuf().lines();
        assert_eq!(lines[0].trim_end(), "one two");
        assert_eq!(lines[1].trim_end(), "three four");
        assert_eq!(lines[2].trim_end(), "");
        assert_eq!(lines[3].trim_end(), "five six");
        Ok(())
    }

    #[test]
    fn set_text_and_set_style_replace_the_text() {
        let mut text = prose("one", 10);
        text.set_text("one two three");
        text.set_style("note");
        assert_eq!(measured(&text, 80), (7, 2));
        assert_eq!(text.style, "note");
    }
}
