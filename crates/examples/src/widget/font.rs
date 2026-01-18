//! Font demo widgets.

use std::time::Duration;

use canopy::{
    Context, ReadContext, TypedId, Widget,
    error::{Error, Result},
    layout::{Align, Layout},
    render::Render,
    state::NodeName,
    style::{Color, GradientSpec, GradientStop, Paint, StyleMap},
};
use canopy_widgets::{Font, FontBanner, FontEffects, FontRenderer, GlyphRamp, LayoutOptions};

/// Style path used for widget demo text.
const FONT_STYLE_PATH: &str = "widget/font";

/// Font source data for demo cycling.
#[derive(Debug, Clone)]
pub struct FontSource {
    /// Label used in diagnostics.
    label: String,
    /// Font bytes.
    bytes: Vec<u8>,
}

impl FontSource {
    /// Build a font source from raw bytes.
    pub fn new(label: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            label: label.into(),
            bytes,
        }
    }
}

/// Font widget configuration.
pub struct FontDemo {
    /// Text to render.
    text: String,
    /// Font sources to cycle through.
    fonts: Vec<FontSource>,
    /// Current font index.
    font_index: usize,
    /// Poll interval for font switching.
    interval: Duration,
    /// Font rendering effects.
    effects: FontEffects,
    /// Whether polling has started.
    started: bool,
    /// Exit after the final font has been displayed.
    exit_after_cycle: bool,
    /// Exit after holding the final font for one interval.
    pending_exit: bool,
    /// Font banner node id.
    banner_id: Option<TypedId<FontBanner>>,
}

impl FontDemo {
    /// Build a font demo widget.
    pub fn new(
        text: impl Into<String>,
        fonts: Vec<FontSource>,
        interval: Duration,
        exit_after_cycle: bool,
        effects: FontEffects,
    ) -> Self {
        Self {
            text: text.into(),
            fonts,
            font_index: 0,
            interval,
            effects,
            started: false,
            exit_after_cycle,
            pending_exit: false,
            banner_id: None,
        }
    }

    /// Build the font renderer for a specific source.
    fn renderer_for(&self, index: usize) -> Result<FontRenderer> {
        let source = self
            .fonts
            .get(index)
            .ok_or_else(|| Error::Internal(format!("font index out of range: {index}")))?;
        let font = Font::from_bytes(source.bytes.as_slice()).map_err(|err| {
            Error::Invalid(format!("font parse failed for {}: {err}", source.label))
        })?;
        Ok(Self::renderer_from_font(font))
    }

    /// Build a renderer with demo glyph settings.
    fn renderer_from_font(font: Font) -> FontRenderer {
        FontRenderer::new(font)
            .with_ramp(GlyphRamp::blocks())
            .with_fallback('?')
    }

    /// Swap the font renderer used by the banner.
    fn set_banner_font(&self, ctx: &mut dyn Context, index: usize) -> Result<()> {
        let banner_id = self
            .banner_id
            .ok_or_else(|| Error::Internal("font banner missing".into()))?;
        let renderer = self.renderer_for(index)?;
        ctx.with_typed(banner_id, |banner: &mut FontBanner, _ctx| {
            banner.set_renderer(renderer);
            Ok(())
        })?;
        Ok(())
    }
}

impl Widget for FontDemo {
    fn layout(&self) -> Layout {
        Layout::fill().align_center()
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.fonts.is_empty() {
            return Err(Error::Invalid("no fonts available".into()));
        }
        self.pending_exit = self.exit_after_cycle && self.fonts.len() == 1;
        ctx.set_style(font_gradient_style());

        let options = LayoutOptions {
            h_align: Align::Center,
            v_align: Align::Center,
            ..LayoutOptions::default()
        };
        let banner = FontBanner::new(self.text.clone(), self.renderer_for(self.font_index)?)
            .with_style(FONT_STYLE_PATH)
            .with_effects(self.effects)
            .with_layout_options(options);
        let banner_id = ctx.add_child(banner)?;
        ctx.set_layout_of(banner_id, Layout::fill())?;
        self.banner_id = Some(banner_id);
        Ok(())
    }

    fn poll(&mut self, ctx: &mut dyn Context) -> Option<Duration> {
        let interval = self.interval.max(Duration::from_millis(1));
        if !self.started {
            self.started = true;
            return Some(interval);
        }
        if self.pending_exit {
            ctx.exit(0);
            return None;
        }
        let next_index = if self.font_index + 1 < self.fonts.len() {
            self.font_index + 1
        } else if self.exit_after_cycle {
            self.pending_exit = true;
            return Some(interval);
        } else {
            0
        };
        if self.set_banner_font(ctx, next_index).is_err() {
            ctx.exit(1);
            return None;
        }
        self.font_index = next_index;
        if self.exit_after_cycle && self.font_index + 1 == self.fonts.len() {
            self.pending_exit = true;
        }
        Some(interval)
    }

    fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("widget-font-demo")
    }
}

/// Build the gradient style used by the font demo.
fn font_gradient_style() -> StyleMap {
    let mut style = StyleMap::new();
    style
        .rules()
        .fg(
            FONT_STYLE_PATH,
            Paint::gradient(GradientSpec::with_stops(
                25.0,
                gradient_stops([
                    Color::rgb("#00E5FF"),
                    Color::rgb("#008CFF"),
                    Color::rgb("#6A2DFF"),
                    Color::rgb("#FF2D2D"),
                ]),
            )),
        )
        .apply();
    style
}

/// Build gradient stops for a four-color palette.
fn gradient_stops(colors: [Color; 4]) -> Vec<GradientStop> {
    vec![
        GradientStop::new(0.0, colors[0]),
        GradientStop::new(0.35, colors[1]),
        GradientStop::new(0.7, colors[2]),
        GradientStop::new(1.0, colors[3]),
    ]
}
