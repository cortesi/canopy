//! Style effects system for transforming styles during rendering.
//!
//! Effects are transformations applied to styles that inherit through the node tree.
//! They can modify colors, attributes, or both.

use std::{fmt::Debug, sync::Arc};

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

/// Dim effect - scales brightness of both foreground and background.
#[derive(Debug, Clone, Copy)]
pub struct Dim {
    /// Brightness multiplier (0.0-1.0 dims, >1.0 brightens).
    factor: f32,
}

impl StyleEffect for Dim {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg.scale_brightness(self.factor),
            bg: style.bg.scale_brightness(self.factor),
            attrs: style.attrs,
        }
    }
}

/// Create a dim effect. Factor 0.0-1.0 dims, >1.0 brightens.
pub fn dim(factor: f32) -> Effect {
    Arc::new(Dim { factor })
}

/// Brighten effect - scales brightness (alias for dim with factor > 1).
#[derive(Debug, Clone, Copy)]
pub struct Brighten {
    /// Brightness multiplier (>1.0 brightens).
    factor: f32,
}

impl StyleEffect for Brighten {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg.scale_brightness(self.factor),
            bg: style.bg.scale_brightness(self.factor),
            attrs: style.attrs,
        }
    }
}

/// Create a brighten effect. Factor > 1.0 brightens.
pub fn brighten(factor: f32) -> Effect {
    Arc::new(Brighten { factor })
}

/// Saturation effect - adjusts color saturation.
#[derive(Debug, Clone, Copy)]
pub struct Saturation {
    /// Saturation multiplier (0.0 = grayscale, 1.0 = unchanged).
    factor: f32,
}

impl StyleEffect for Saturation {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg.saturation(self.factor),
            bg: style.bg.saturation(self.factor),
            attrs: style.attrs,
        }
    }
}

/// Create a saturation effect. 0.0 = grayscale, 1.0 = unchanged.
pub fn saturation(factor: f32) -> Effect {
    Arc::new(Saturation { factor })
}

/// Swap foreground and background colors.
#[derive(Debug, Clone, Copy)]
pub struct SwapFgBg;

impl StyleEffect for SwapFgBg {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.bg,
            bg: style.fg,
            attrs: style.attrs,
        }
    }
}

/// Create an effect that swaps foreground and background colors.
pub fn swap_fg_bg() -> Effect {
    Arc::new(SwapFgBg)
}

/// Invert RGB channels of both colors.
#[derive(Debug, Clone, Copy)]
pub struct InvertRgb;

impl StyleEffect for InvertRgb {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg.invert_rgb(),
            bg: style.bg.invert_rgb(),
            attrs: style.attrs,
        }
    }
}

/// Create an effect that inverts RGB channels (255-value).
pub fn invert_rgb() -> Effect {
    Arc::new(InvertRgb)
}

/// Tint effect - blends colors toward a target color.
#[derive(Debug, Clone, Copy)]
pub struct Tint {
    /// Target color to blend toward.
    color: Color,
    /// Blend ratio (0.0 = original, 1.0 = target color).
    ratio: f32,
}

impl StyleEffect for Tint {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg.blend(self.color, self.ratio),
            bg: style.bg.blend(self.color, self.ratio),
            attrs: style.attrs,
        }
    }
}

/// Create a tint effect that blends colors toward a target.
pub fn tint(color: Color, ratio: f32) -> Effect {
    Arc::new(Tint { color, ratio })
}

/// Hue shift effect.
#[derive(Debug, Clone, Copy)]
pub struct HueShift {
    /// Degrees to shift hue by (0-360).
    degrees: f32,
}

impl StyleEffect for HueShift {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg.shift_hue(self.degrees),
            bg: style.bg.shift_hue(self.degrees),
            attrs: style.attrs,
        }
    }
}

/// Create a hue shift effect.
pub fn hue_shift(degrees: f32) -> Effect {
    Arc::new(HueShift { degrees })
}

// ============================================================================
// Attribute Effects
// ============================================================================

/// Add bold attribute.
#[derive(Debug, Clone, Copy)]
pub struct Bold;

impl StyleEffect for Bold {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg,
            bg: style.bg,
            attrs: style.attrs.with(Attr::Bold),
        }
    }
}

/// Create an effect that adds bold attribute.
pub fn bold() -> Effect {
    Arc::new(Bold)
}

/// Add italic attribute.
#[derive(Debug, Clone, Copy)]
pub struct Italic;

impl StyleEffect for Italic {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg,
            bg: style.bg,
            attrs: style.attrs.with(Attr::Italic),
        }
    }
}

/// Create an effect that adds italic attribute.
pub fn italic() -> Effect {
    Arc::new(Italic)
}

/// Add underline attribute.
#[derive(Debug, Clone, Copy)]
pub struct Underline;

impl StyleEffect for Underline {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg,
            bg: style.bg,
            attrs: style.attrs.with(Attr::Underline),
        }
    }
}

/// Create an effect that adds underline attribute.
pub fn underline() -> Effect {
    Arc::new(Underline)
}

/// Add dim attribute (terminal dim, not brightness scaling).
#[derive(Debug, Clone, Copy)]
pub struct AttrDim;

impl StyleEffect for AttrDim {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg,
            bg: style.bg,
            attrs: style.attrs.with(Attr::Dim),
        }
    }
}

/// Create an effect that adds the terminal dim attribute.
pub fn attr_dim() -> Effect {
    Arc::new(AttrDim)
}

/// Replace the entire attribute set.
#[derive(Debug, Clone, Copy)]
pub struct SetAttrs {
    /// The attribute set to use.
    attrs: AttrSet,
}

impl StyleEffect for SetAttrs {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg,
            bg: style.bg,
            attrs: self.attrs,
        }
    }
}

/// Create an effect that replaces all attributes.
pub fn set_attrs(attrs: AttrSet) -> Effect {
    Arc::new(SetAttrs { attrs })
}

/// Clear all attributes.
#[derive(Debug, Clone, Copy)]
pub struct ClearAttrs;

impl StyleEffect for ClearAttrs {
    fn apply(&self, style: Style) -> Style {
        Style {
            fg: style.fg,
            bg: style.bg,
            attrs: AttrSet::default(),
        }
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

    fn test_style() -> Style {
        Style {
            fg: Color::Rgb {
                r: 200,
                g: 100,
                b: 50,
            },
            bg: Color::Rgb {
                r: 20,
                g: 20,
                b: 20,
            },
            attrs: AttrSet::default(),
        }
    }

    #[test]
    fn test_dim_effect() {
        let style = test_style();
        let dimmed = dim(0.5).apply(style);
        if let Color::Rgb { r, g, b } = dimmed.fg {
            assert_eq!(r, 100);
            assert_eq!(g, 50);
            assert_eq!(b, 25);
        } else {
            panic!("Expected RGB");
        }
    }

    #[test]
    fn test_saturation_effect() {
        let style = test_style();
        let gray = saturation(0.0).apply(style);
        if let Color::Rgb { r, g, b } = gray.fg {
            assert_eq!(r, g);
            assert_eq!(g, b);
        } else {
            panic!("Expected RGB");
        }
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
        if let Color::Rgb { r, .. } = step2.fg {
            assert_eq!(r, 100); // Dimmed
        }
        assert!(step2.attrs.bold);
    }
}
