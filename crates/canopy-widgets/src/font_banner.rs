use std::cell::RefCell;

use canopy::{
    ViewContext, Widget,
    error::Result,
    geom::{Point, Rect, Size},
    layout::{Constraint, Layout, MeasureConstraints, Measurement},
    render::Render,
    style::Coverage,
};

use crate::font::{FontEffects, FontLayout, FontRenderer, LayoutOptions, align_offset};

/// Render large ASCII-font text into a bounded region.
///
/// The text scales to the height of the banner. A banner fills its area, and
/// a layout override that measures its width gives it the width of its text
/// at its height instead.
pub struct FontBanner {
    /// Current banner text.
    text: String,
    /// Renderer used to rasterize the font. Measuring lays the text out from
    /// a shared borrow, and the renderer caches glyphs as it lays out.
    renderer: RefCell<FontRenderer>,
    /// Style path for text rendering.
    style: String,
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
    size: Size,
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
            renderer: RefCell::new(renderer),
            style: String::from("text"),
            options: LayoutOptions::default(),
            effects: FontEffects::default(),
            cache: None,
        }
    }

    /// Update the banner text.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    /// Update the banner renderer.
    pub fn set_renderer(&mut self, renderer: FontRenderer) {
        self.renderer = RefCell::new(renderer);
        self.cache = None;
    }

    /// Configure the banner style path.
    #[must_use]
    pub fn with_style(mut self, style: impl Into<String>) -> Self {
        self.style = style.into();
        self
    }

    /// Configure layout options for the banner.
    #[must_use]
    pub fn with_layout_options(mut self, options: LayoutOptions) -> Self {
        self.options = options;
        self
    }

    /// Configure rendering effects for the banner.
    #[must_use]
    pub fn with_effects(mut self, effects: FontEffects) -> Self {
        self.effects = effects;
        self
    }

    /// Update rendering effects for the banner.
    pub fn set_effects(&mut self, effects: FontEffects) {
        self.effects = effects;
    }

    /// Rebuild the cached layout when the text, size, options, or effects
    /// changed.
    fn refresh_layout(&mut self, size: Size) {
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
            let layout =
                self.renderer
                    .borrow_mut()
                    .layout(&self.text, size, self.options, self.effects);
            self.cache = Some(LayoutCache {
                text: self.text.clone(),
                size,
                options: self.options,
                effects: self.effects,
                layout,
            });
        }
    }
}

impl Widget for FontBanner {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    /// Measure the text at the offered height, shrunk to the offered width
    /// when it is wider. Without a bound on the height the text has no
    /// scale, so the banner wraps.
    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let (Constraint::Exact(height) | Constraint::AtMost(height)) = c.height else {
            return c.wrap();
        };
        let width = match c.width {
            Constraint::Exact(width) | Constraint::AtMost(width) => width,
            Constraint::Unbounded => u32::MAX,
        };
        let content = self.renderer.borrow_mut().content_size(
            &self.text,
            Size::new(width, height),
            self.options,
            self.effects,
        );
        c.clamp(Size::new(content.w, height))
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect_local();
        if view_rect.w == 0 || view_rect.h == 0 {
            return Ok(());
        }
        let size = view_rect.size();
        self.refresh_layout(size);
        let options = self.options;
        let style = &self.style;
        let layout = &self.cache.as_ref().expect("layout cached").layout;

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
                let coverage = Coverage {
                    fg: cell.fg_coverage,
                    bg: cell.bg_coverage,
                };
                rndr.put_covered(style, bounds, point, cell.ch, coverage)?;
            }
        }
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::Font;

    const TEST_FONT: &[u8] = include_bytes!("../assets/fonts/Bungee-Regular.ttf");

    /// Measure `text` at `height` rows, with room for any width.
    fn measured(text: &str, height: Constraint) -> Result<Measurement> {
        let banner = FontBanner::new(text, FontRenderer::new(Font::from_bytes(TEST_FONT)?));
        Ok(banner.measure(MeasureConstraints {
            width: Constraint::AtMost(500),
            height,
        }))
    }

    #[test]
    fn a_banner_measures_its_text_at_its_height() -> Result<()> {
        let Measurement::Fixed(short) = measured("ab", Constraint::Exact(6))? else {
            panic!("a bounded height measures");
        };
        let Measurement::Fixed(long) = measured("abab", Constraint::Exact(6))? else {
            panic!("a bounded height measures");
        };
        let Measurement::Fixed(tall) = measured("ab", Constraint::Exact(12))? else {
            panic!("a bounded height measures");
        };
        assert_eq!(short.h, 6);
        assert!(short.w > 0 && long.w > short.w, "{short:?} {long:?}");
        assert!(tall.w > short.w, "the text scales with the height");
        assert_eq!(measured("ab", Constraint::Unbounded)?, Measurement::Wrap);
        Ok(())
    }

    #[test]
    fn a_banner_narrower_than_its_text_shrinks_the_text() -> Result<()> {
        let banner = |width| -> Result<Measurement> {
            let banner = FontBanner::new("abab", FontRenderer::new(Font::from_bytes(TEST_FONT)?));
            Ok(banner.measure(MeasureConstraints {
                width,
                height: Constraint::Exact(6),
            }))
        };
        let Measurement::Fixed(natural) = banner(Constraint::Unbounded)? else {
            panic!("a bounded height measures");
        };
        let room = natural.w / 2;
        let Measurement::Fixed(shrunk) = banner(Constraint::AtMost(room))? else {
            panic!("a bounded height measures");
        };
        assert!(
            shrunk.w <= room && shrunk.w > room / 2,
            "{natural:?} {shrunk:?}"
        );
        Ok(())
    }
}
