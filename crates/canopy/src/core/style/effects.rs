//! Style effects system for transforming styles during rendering.
//!
//! Effects are transformations applied to styles that inherit through the node
//! tree. They can modify colors, attributes, or both.

use std::{fmt::Debug, sync::Arc, time::Duration};

use super::{Animation, Attr, Color, Paint, Style, animation::ShownAt};

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
enum ColorEffect {
    /// Scale brightness by a factor.
    ScaleBrightness(f32),
    /// Adjust saturation.
    Saturation(f32),
    /// Invert RGB channels.
    Invert,
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
            Self::HueShift(d) => {
                style.fg = style.fg.map_colors(|c| c.shift_hue(d));
                style.bg = style.bg.map_colors(|c| c.shift_hue(d));
            }
        }
        style
    }
}

/// Brightness factor for what a modal covers, so a panel over the view reads
/// apart from the dimmed view behind it.
pub const MODAL_DIM: f32 = 0.5;

/// Time a modal takes to dim what it covers.
pub const MODAL_FADE: Duration = Duration::from_millis(120);

/// Create the effect of what a modal covers: a fade to [`MODAL_DIM`] over
/// [`MODAL_FADE`].
pub fn modal_dim() -> Effect {
    transition(brightness(MODAL_DIM), MODAL_FADE)
}

/// An effect whose colors fade to those of another effect.
#[derive(Debug)]
struct Transition {
    /// Effect the colors fade to.
    effect: Effect,
    /// Length of the fade.
    duration: Duration,
    /// Start of the fade, shared by every paint the effect produces.
    shown: ShownAt,
}

impl Transition {
    /// Fade one paint to its target. Only solid colors fade; other paints
    /// take their target at once.
    fn fade(&self, from: &Paint, to: Paint) -> Paint {
        match (from, &to) {
            (Paint::Solid(from), Paint::Solid(target)) if from != target => Paint::animated(
                Animation::fade(*from, *target, self.duration).share_start(&self.shown),
            ),
            _ => to,
        }
    }
}

impl StyleEffect for Transition {
    fn apply(&self, style: Style) -> Style {
        let target = self.effect.apply(style.clone());
        Style {
            fg: self.fade(&style.fg, target.fg),
            bg: self.fade(&style.bg, target.bg),
            attrs: target.attrs,
        }
    }
}

/// Create an effect that fades each color from its value to the value of
/// `effect` over `duration`.
///
/// The fade starts with the first frame that shows it, and frames at rest
/// show the faded colors. Keep the returned effect for as long as it applies:
/// a new transition fades again.
pub fn transition(effect: Effect, duration: Duration) -> Effect {
    Arc::new(Transition {
        effect,
        duration,
        shown: ShownAt::default(),
    })
}

/// Create a brightness effect. Factor below 1.0 dims, above 1.0 brightens.
pub fn brightness(factor: f32) -> Effect {
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

/// Create a hue shift effect.
pub fn hue_shift(degrees: f32) -> Effect {
    Arc::new(ColorEffect::HueShift(degrees))
}

// ============================================================================
// Attribute Effects
// ============================================================================

/// Add a single attribute.
#[derive(Debug, Clone, Copy)]
struct AddAttr(Attr);

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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{AttrSet, Paint};

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
        let dimmed = brightness(0.5).apply(style);
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
    fn test_bold_effect() {
        let style = test_style();
        assert!(!style.attrs.bold);
        let bold_style = bold().apply(style);
        assert!(bold_style.attrs.bold);
    }

    #[test]
    fn a_transition_fades_solid_colors_and_rests_at_its_target() {
        use std::time::Instant;

        use crate::style::MotionClocks;

        let style = test_style();
        let target = brightness(0.5).apply(style.clone());
        let faded = transition(brightness(0.5), Duration::from_millis(100)).apply(style.clone());
        let Paint::Animated(fg) = &faded.fg else {
            panic!("expected a fade");
        };
        assert_eq!(Some(fg.rest()), target.fg.solid_color());
        let t0 = Instant::now();
        fg.bind_shown(t0);
        assert_eq!(
            fg.color(&MotionClocks::at(t0)),
            Color::Rgb {
                r: 200,
                g: 100,
                b: 50
            }
        );
        // Paints from later renders share the start of the first.
        let again = transition(brightness(0.5), Duration::from_millis(100));
        let first = again.apply(style.clone());
        let second = again.apply(style);
        let (Paint::Animated(first), Paint::Animated(second)) = (&first.fg, &second.fg) else {
            panic!("expected fades");
        };
        first.bind_shown(t0);
        let late = MotionClocks {
            now: t0 + Duration::from_millis(100),
            ..MotionClocks::at(t0)
        };
        assert!(second.finished(&late));
    }

    #[test]
    fn test_effect_stacking() {
        let style = test_style();
        // Apply dim, then bold
        let step1 = brightness(0.5).apply(style);
        let step2 = bold().apply(step1);
        // Should have both dimmed colors and bold attribute
        let Some(Color::Rgb { r, .. }) = step2.fg.solid_color() else {
            panic!("Expected solid RGB");
        };
        assert_eq!(r, 100); // Dimmed
        assert!(step2.attrs.bold);
    }
}
