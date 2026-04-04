//! Style effects system for transforming styles during rendering.
//!
//! Effects are transformations applied to styles that inherit through the node tree.
//! They can modify colors, attributes, or both.

use std::{fmt::Debug, mem, sync::Arc};

use super::{Attr, AttrSet, Color, Style};

/// A style transformation that can be applied during rendering.
///
/// Effects are stacked and applied in order during render traversal.
/// They inherit through the tree unless explicitly cleared.
pub trait StyleEffect: Send + Sync + Debug {
    /// Apply this effect to a style, returning the transformed style.
    fn apply(&self, style: Style) -> Style;
}

/// Shared handle for effects stored on nodes and stacked during rendering.
pub type Effect = Arc<dyn StyleEffect>;

// ============================================================================
// Built-in Effects
// ============================================================================

/// A built-in effect that maps colors.
#[derive(Debug, Clone, Copy)]
pub enum ColorEffect {
    /// Scale brightness by a factor.
    ScaleBrightness(f32),
    /// Adjust saturation.
    Saturation(f32),
    /// Invert RGB channels.
    Invert,
    /// Blend toward a target color.
    Tint(Color, f32),
    /// Shift hue by degrees.
    HueShift(f32),
}

impl StyleEffect for ColorEffect {
    fn apply(&self, mut style: Style) -> Style {
        match *self {
            Self::ScaleBrightness(f) => {
                style.fg = style.fg.map_colors(|c| c.scale_brightness(f));
                style.bg = style.bg.map_colors(|c| c.scale_brightness(f));
            }
            Self::Saturation(f) => {
                style.fg = style.fg.map_colors(|c| c.saturation(f));
                style.bg = style.bg.map_colors(|c| c.saturation(f));
            }
            Self::Invert => {
                style.fg = style.fg.map_colors(Color::invert_rgb);
                style.bg = style.bg.map_colors(Color::invert_rgb);
            }
            Self::Tint(t, r) => {
                style.fg = style.fg.map_colors(|c| c.blend(t, r));
                style.bg = style.bg.map_colors(|c| c.blend(t, r));
            }
            Self::HueShift(d) => {
                style.fg = style.fg.map_colors(|c| c.shift_hue(d));
                style.bg = style.bg.map_colors(|c| c.shift_hue(d));
            }
        }
        style
    }
}

/// Create a dim effect. Factor 0.0-1.0 dims, >1.0 brightens.
pub fn dim(factor: f32) -> Effect {
    Arc::new(ColorEffect::ScaleBrightness(factor))
}

/// Create a brighten effect. Factor > 1.0 brightens.
pub fn brighten(factor: f32) -> Effect {
    Arc::new(ColorEffect::ScaleBrightness(factor))
}

/// Create a saturation effect. 0.0 = grayscale, 1.0 = unchanged.
pub fn saturation(factor: f32) -> Effect {
    Arc::new(ColorEffect::Saturation(factor))
}

/// Create an effect that inverts RGB channels (255-value).
pub fn invert_rgb() -> Effect {
    Arc::new(ColorEffect::Invert)
}

/// Swap foreground and background colors.
#[derive(Debug, Clone, Copy)]
pub struct SwapFgBg;

impl StyleEffect for SwapFgBg {
    fn apply(&self, mut style: Style) -> Style {
        mem::swap(&mut style.fg, &mut style.bg);
        style
    }
}

/// Create an effect that swaps foreground and background colors.
pub fn swap_fg_bg() -> Effect {
    Arc::new(SwapFgBg)
}

/// Create a tint effect that blends colors toward a target.
pub fn tint(color: Color, ratio: f32) -> Effect {
    Arc::new(ColorEffect::Tint(color, ratio))
}

/// Create a hue shift effect.
pub fn hue_shift(degrees: f32) -> Effect {
    Arc::new(ColorEffect::HueShift(degrees))
}

// ============================================================================
// Attribute Effects
// ============================================================================

/// Add a single attribute.
#[derive(Debug, Clone, Copy)]
pub struct AddAttr(pub Attr);

impl StyleEffect for AddAttr {
    fn apply(&self, mut style: Style) -> Style {
        style.attrs = style.attrs.with(self.0);
        style
    }
}

/// Create an effect that adds bold attribute.
pub fn bold() -> Effect {
    Arc::new(AddAttr(Attr::Bold))
}

/// Create an effect that adds italic attribute.
pub fn italic() -> Effect {
    Arc::new(AddAttr(Attr::Italic))
}

/// Create an effect that adds underline attribute.
pub fn underline() -> Effect {
    Arc::new(AddAttr(Attr::Underline))
}

/// Create an effect that adds the terminal dim attribute.
pub fn attr_dim() -> Effect {
    Arc::new(AddAttr(Attr::Dim))
}

/// Replace the entire attribute set.
#[derive(Debug, Clone, Copy)]
pub struct SetAttrs(pub AttrSet);

impl StyleEffect for SetAttrs {
    fn apply(&self, mut style: Style) -> Style {
        style.attrs = self.0;
        style
    }
}

/// Create an effect that replaces all attributes.
pub fn set_attrs(attrs: AttrSet) -> Effect {
    Arc::new(SetAttrs(attrs))
}

/// Clear all attributes.
#[derive(Debug, Clone, Copy)]
pub struct ClearAttrs;

impl StyleEffect for ClearAttrs {
    fn apply(&self, mut style: Style) -> Style {
        style.attrs = AttrSet::default();
        style
    }
}

/// Create an effect that clears all attributes.
pub fn clear_attrs() -> Effect {
    Arc::new(ClearAttrs)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Paint;

    fn test_style() -> Style {
        Style {
            fg: Paint::solid(Color::Rgb {
                r: 200,
                g: 100,
                b: 50,
            }),
            bg: Paint::solid(Color::Rgb {
                r: 20,
                g: 20,
                b: 20,
            }),
            attrs: AttrSet::default(),
        }
    }

    #[test]
    fn test_dim_effect() {
        let style = test_style();
        let dimmed = dim(0.5).apply(style);
        let Some(Color::Rgb { r, g, b }) = dimmed.fg.solid_color() else {
            panic!("Expected solid RGB");
        };
        assert_eq!(r, 100);
        assert_eq!(g, 50);
        assert_eq!(b, 25);
    }

    #[test]
    fn test_saturation_effect() {
        let style = test_style();
        let gray = saturation(0.0).apply(style);
        let Some(Color::Rgb { r, g, b }) = gray.fg.solid_color() else {
            panic!("Expected solid RGB");
        };
        assert_eq!(r, g);
        assert_eq!(g, b);
    }

    #[test]
    fn test_swap_fg_bg() {
        let style = test_style();
        let swapped = swap_fg_bg().apply(style.clone());
        assert_eq!(swapped.fg, style.bg);
        assert_eq!(swapped.bg, style.fg);
    }

    #[test]
    fn test_bold_effect() {
        let style = test_style();
        assert!(!style.attrs.bold);
        let bold_style = bold().apply(style);
        assert!(bold_style.attrs.bold);
    }

    #[test]
    fn test_effect_stacking() {
        let style = test_style();
        // Apply dim, then bold
        let step1 = dim(0.5).apply(style);
        let step2 = bold().apply(step1);
        // Should have both dimmed colors and bold attribute
        let Some(Color::Rgb { r, .. }) = step2.fg.solid_color() else {
            panic!("Expected solid RGB");
        };
        assert_eq!(r, 100); // Dimmed
        assert!(step2.attrs.bold);
    }
}
