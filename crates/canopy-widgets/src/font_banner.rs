use canopy::{
    ReadContext, Widget,
    error::Result,
    geom::{Expanse, Point, Rect},
    layout::{Align, Layout},
    render::Render,
    style::ResolvedStyle,
};

use crate::{
    Selectable,
    font::{FontEffects, FontLayout, FontRenderer, LayoutOptions},
};

/// Render large ASCII-font text into a bounded region.
pub struct FontBanner {
    /// Current banner text.
    text: String,
    /// Renderer used to rasterize the font.
    renderer: FontRenderer,
    /// Style path for text rendering.
    style: String,
    /// Optional style path when selected.
    selected_style: Option<String>,
    /// Selection state for list integration.
    selected: bool,
    /// Layout configuration for the banner.
    options: LayoutOptions,
    /// Rendering effects for the banner.
    effects: FontEffects,
    /// Cached layout keyed by size and text.
    cache: Option<LayoutCache>,
}

/// Cached layout data for a banner.
struct LayoutCache {
    /// Text used to build the layout.
    text: String,
    /// Target canvas size.
    size: Expanse,
    /// Layout options used for rendering.
    options: LayoutOptions,
    /// Rendering effects for the banner.
    effects: FontEffects,
    /// Rasterized layout.
    layout: FontLayout,
}

impl FontBanner {
    /// Construct a banner with text and a renderer.
    pub fn new(text: impl Into<String>, renderer: FontRenderer) -> Self {
        Self {
            text: text.into(),
            renderer,
            style: String::from("text"),
            selected_style: None,
            selected: false,
            options: LayoutOptions::default(),
            effects: FontEffects::default(),
            cache: None,
        }
    }

    /// Update the banner text.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cache = None;
    }

    /// Configure the banner style path.
    pub fn with_style(mut self, style: impl Into<String>) -> Self {
        self.style = style.into();
        self
    }

    /// Configure the banner style when selected.
    pub fn with_selected_style(mut self, style: impl Into<String>) -> Self {
        self.selected_style = Some(style.into());
        self
    }

    /// Configure layout options for the banner.
    pub fn with_layout_options(mut self, options: LayoutOptions) -> Self {
        self.options = options;
        self.cache = None;
        self
    }

    /// Configure rendering effects for the banner.
    pub fn with_effects(mut self, effects: FontEffects) -> Self {
        self.effects = effects;
        self
    }

    /// Update rendering effects for the banner.
    pub fn set_effects(&mut self, effects: FontEffects) {
        self.effects = effects;
    }

    /// Return a cached layout for the provided size.
    fn layout_for(&mut self, size: Expanse) -> &FontLayout {
        let rebuild = match &self.cache {
            Some(cache) => {
                cache.text != self.text
                    || cache.size != size
                    || cache.options != self.options
                    || cache.effects != self.effects
            }
            None => true,
        };
        if rebuild {
            let layout = self
                .renderer
                .layout(&self.text, size, self.options, self.effects);
            self.cache = Some(LayoutCache {
                text: self.text.clone(),
                size,
                options: self.options,
                effects: self.effects,
                layout,
            });
        }
        &self.cache.as_ref().expect("layout cached").layout
    }
}

impl Selectable for FontBanner {
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

impl Widget for FontBanner {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect_local();
        if view_rect.w == 0 || view_rect.h == 0 {
            return Ok(());
        }
        let size = Expanse::new(view_rect.w, view_rect.h);
        let style = if self.selected {
            self.selected_style
                .as_deref()
                .unwrap_or(&self.style)
                .to_string()
        } else {
            self.style.clone()
        };
        let options = self.options;
        let layout = self.layout_for(size);

        let bounds = content_rect(view_rect, layout, options);
        for (row_idx, row) in layout.cells.iter().enumerate() {
            let y = view_rect.tl.y.saturating_add(row_idx as u32);
            if y >= view_rect.tl.y.saturating_add(view_rect.h) {
                break;
            }
            for (col_idx, cell) in row.iter().enumerate() {
                if cell.fg_coverage == 0 && cell.bg_coverage == 0 {
                    continue;
                }
                let x = view_rect.tl.x.saturating_add(col_idx as u32);
                if x >= view_rect.tl.x.saturating_add(view_rect.w) {
                    continue;
                }
                let point = Point { x, y };
                let resolved = rndr.resolve_style_name_at(&style, bounds, point);
                let blended = blend_style(resolved, cell.fg_coverage, cell.bg_coverage);
                rndr.put_cell(blended, point, cell.ch)?;
            }
        }
        Ok(())
    }
}

/// Blend a resolved style by coverage weights.
fn blend_style(resolved: ResolvedStyle, fg_cov: u8, bg_cov: u8) -> ResolvedStyle {
    let fg_weight = f32::from(fg_cov) / 255.0;
    let bg_weight = f32::from(bg_cov) / 255.0;
    let fg = resolved.bg.blend(resolved.fg, fg_weight);
    let bg = resolved.bg.blend(resolved.fg, bg_weight);
    ResolvedStyle::new(fg, bg, resolved.attrs)
}

/// Compute a gradient bounds rect aligned to the rendered content.
fn content_rect(view_rect: Rect, layout: &FontLayout, options: LayoutOptions) -> Rect {
    if layout.content_size.w == 0 || layout.content_size.h == 0 {
        return view_rect;
    }

    let offset_x = align_offset(layout.content_size.w, layout.size.w, options.h_align);
    let offset_y = align_offset(layout.content_size.h, layout.size.h, options.v_align);

    Rect::new(
        view_rect.tl.x.saturating_add(offset_x),
        view_rect.tl.y.saturating_add(offset_y),
        layout.content_size.w,
        layout.content_size.h,
    )
}

/// Align content inside an available span.
fn align_offset(content: u32, available: u32, align: Align) -> u32 {
    if available <= content {
        return 0;
    }
    match align {
        Align::Start => 0,
        Align::Center => (available - content) / 2,
        Align::End => available - content,
    }
}
