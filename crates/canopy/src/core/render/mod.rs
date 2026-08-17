use super::termbuf::TermBuf;
use crate::{
    core::text,
    error::Result,
    geom,
    style::{Effect, ResolvedStyle, Style, StyleManager, StyleMap},
};

/// The trait implemented by renderers.
pub trait RenderBackend {
    /// Apply a style to the following text output
    fn style(&mut self, style: &ResolvedStyle) -> Result<()>;
    /// Output text to screen. This method is used for all text output.
    fn text(&mut self, loc: geom::Point, txt: &str) -> Result<()>;
    /// Return true if the backend can shift characters within a line.
    fn supports_char_shift(&self) -> bool {
        false
    }
    /// Shift characters within a line starting at the location.
    /// Positive counts insert blanks and shift right, negative counts delete and shift left.
    fn shift_chars(&mut self, _loc: geom::Point, _count: i32) -> Result<()> {
        Ok(())
    }
    /// Return true if the backend can shift lines within a region.
    fn supports_line_shift(&self) -> bool {
        false
    }
    /// Shift lines within the inclusive (top..=bottom) region.
    /// Positive counts shift content down, negative counts shift content up.
    fn shift_lines(&mut self, _top: u32, _bottom: u32, _count: i32) -> Result<()> {
        Ok(())
    }
    /// Flush output to the terminal.
    fn flush(&mut self) -> Result<()>;
    /// Reset the backend to a clean state.
    fn reset(&mut self) -> Result<()> {
        Ok(())
    }
}

/// A render backend that discards all output.
///
/// Rendering through this backend refreshes the terminal buffer without
/// producing user-visible output, so callers can inspect the buffer directly.
pub struct NopBackend;

impl NopBackend {
    /// Construct a no-op backend.
    pub fn new() -> Self {
        Self
    }
}

impl Default for NopBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderBackend for NopBackend {
    fn style(&mut self, _style: &ResolvedStyle) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, _loc: geom::Point, _txt: &str) -> Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Signed translation offset in cell coordinates.
struct Offset {
    /// Horizontal offset.
    x: i64,
    /// Vertical offset.
    y: i64,
}

impl Offset {
    /// Compute the translation from a source point to a destination point.
    fn between(dest: geom::Point, src: geom::Point) -> Self {
        Self {
            x: i64::from(dest.x) - i64::from(src.x),
            y: i64::from(dest.y) - i64::from(src.y),
        }
    }
}

/// Translate a point from buffer coordinates back to canvas coordinates.
fn untranslate(origin: Offset, p: geom::Point) -> geom::Point {
    let x = i64::from(p.x) - origin.x;
    let y = i64::from(p.y) - origin.y;
    geom::Point {
        x: u32::try_from(x.clamp(0, i64::from(u32::MAX))).unwrap_or(u32::MAX),
        y: u32::try_from(y.clamp(0, i64::from(u32::MAX))).unwrap_or(u32::MAX),
    }
}

/// A renderer that only renders to a specific rectangle within the target terminal buffer.
pub struct Render<'a> {
    /// The terminal buffer to render to.
    buf: &'a mut TermBuf,
    /// The style manager used to apply styles.
    style: &'a mut StyleManager,
    /// The style map used to resolve style names to styles.
    stylemap: &'a StyleMap,
    /// The rectangle in canvas coordinates that is visible for rendering.
    clip: geom::Rect,
    /// Translation offset from canvas coordinates to buffer coordinates.
    origin: Offset,
    /// Current effect stack, applied in order to resolved styles.
    effects: &'a [Effect],
}

impl<'a> Render<'a> {
    /// Construct a renderer that writes into `buf`.
    ///
    /// `clip` is the visible rectangle in canvas coordinates, and `screen_origin` is where the
    /// clip's top-left lands in the buffer.
    pub fn new(
        stylemap: &'a StyleMap,
        style: &'a mut StyleManager,
        buf: &'a mut TermBuf,
        clip: geom::Rect,
        screen_origin: geom::Point,
    ) -> Self {
        Render {
            buf,
            style,
            stylemap,
            clip,
            origin: Offset::between(screen_origin, clip.tl),
            effects: &[],
        }
    }

    /// Set the effect stack for this renderer.
    pub fn with_effects(mut self, effects: &'a [Effect]) -> Self {
        self.effects = effects;
        self
    }

    /// Apply the current effect stack to a style.
    /// Use this when you have a Style from a source other than the style manager.
    pub fn apply_effects(&self, style: Style) -> Style {
        let mut result = style;
        for effect in self.effects {
            result = effect.apply(result);
        }
        result
    }

    /// Resolve a style by name and apply the current effect stack.
    fn resolve_style(&self, name: &str) -> Style {
        let base = self.style.get(self.stylemap, name);
        self.apply_effects(base)
    }

    /// Resolve a style by name without applying effects.
    pub fn resolve_style_name_raw(&self, name: &str) -> Style {
        self.style.get(self.stylemap, name)
    }

    /// Resolve a custom style at a point, applying the current effect stack.
    pub fn resolve_style_at(
        &self,
        style: Style,
        bounds: geom::Rect,
        point: geom::Point,
    ) -> ResolvedStyle {
        self.apply_effects(style).resolve_at(bounds, point)
    }

    /// Resolve a style by name at a point within bounds.
    pub fn resolve_style_name_at(
        &self,
        name: &str,
        bounds: geom::Rect,
        point: geom::Point,
    ) -> ResolvedStyle {
        self.resolve_style(name).resolve_at(bounds, point)
    }

    /// Push a style layer.
    pub fn push_layer(&mut self, name: &str) {
        self.style.push_layer(name);
    }

    /// Fill a rectangle with a specified character. Writes out of bounds will be clipped.
    pub fn fill(&mut self, style: &str, r: geom::Rect, c: char) -> Result<()> {
        let Some(intersection) = r.intersect(self.clip) else {
            return Ok(());
        };
        let style = self.resolve_style(style);
        let adjusted = self.translate_rect(intersection);
        let origin = self.origin;
        self.buf
            .fill_with(adjusted, c, |p| style.resolve_at(r, untranslate(origin, p)))
    }

    /// Print text in the specified line. If the text is wider than the
    /// rectangle, it will be truncated; if it is shorter, it will be padded.
    pub fn text(&mut self, style: &str, l: geom::Line, txt: &str) -> Result<()> {
        let line_rect = geom::Rect::new(l.tl.x, l.tl.y, l.w, 1);
        let Some(intersection) = line_rect.intersect(self.clip) else {
            return Ok(());
        };
        let style = self.resolve_style(style);

        let skip_amount = intersection.tl.x.saturating_sub(l.tl.x) as usize;
        let (out, _) = text::slice_by_columns(txt, skip_amount, intersection.w as usize);
        let adjusted_line = geom::Line {
            tl: self.translate_point(intersection.tl),
            w: intersection.w,
        };
        let origin = self.origin;
        self.buf.text_with(adjusted_line, out, |p| {
            style.resolve_at(line_rect, untranslate(origin, p))
        })
    }

    /// Write a single cell with a resolved style.
    pub fn put_cell(&mut self, style: ResolvedStyle, p: geom::Point, ch: char) -> Result<()> {
        if self.clip.contains_point(p) {
            let adjusted = self.translate_point(p);
            self.buf.put(adjusted, ch, style)?;
        }
        Ok(())
    }

    /// Write a grapheme with a resolved style, including continuation cells.
    pub fn put_grapheme(
        &mut self,
        style: ResolvedStyle,
        p: geom::Point,
        grapheme: &str,
    ) -> Result<()> {
        let width = text::grapheme_width(grapheme);
        if width == 0 {
            return Ok(());
        }
        let glyph_rect = geom::Rect::new(p.x, p.y, width as u32, 1);
        if self.clip.contains_rect(glyph_rect) {
            let adjusted = self.translate_point(p);
            self.buf.put_grapheme(adjusted, grapheme, style)?;
        }
        Ok(())
    }

    /// Translate a point from canvas coordinates to buffer coordinates.
    fn translate_point(&self, p: geom::Point) -> geom::Point {
        let x = i64::from(p.x) + self.origin.x;
        let y = i64::from(p.y) + self.origin.y;
        debug_assert!(
            x >= 0 && y >= 0,
            "translated point out of bounds: {:?} + {:?}",
            p,
            self.origin
        );
        geom::Point {
            x: u32::try_from(x.clamp(0, i64::from(u32::MAX))).unwrap_or(u32::MAX),
            y: u32::try_from(y.clamp(0, i64::from(u32::MAX))).unwrap_or(u32::MAX),
        }
    }

    /// Translate a rectangle from canvas coordinates to buffer coordinates.
    fn translate_rect(&self, rect: geom::Rect) -> geom::Rect {
        geom::Rect {
            tl: self.translate_point(rect.tl),
            w: rect.w,
            h: rect.h,
        }
    }
}

/// Tests for the render surface.
#[cfg(test)]
mod tests;
