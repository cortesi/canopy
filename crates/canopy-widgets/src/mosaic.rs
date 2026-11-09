//! Block mosaics: the symbols that image views, font banners, and big text
//! draw with, from `canopy-mosaic`.
//!
//! Each of those widgets takes its tier from the terminal's glyphs, as the
//! application set them, unless the widget has a tier of its own. A gym
//! sets one to compare tiers side by side.

use canopy::{
    error::Result,
    geom::{Point, Rect},
    render::{Glyphs, Render},
    style::{AttrSet, Color, ResolvedStyle, Style},
};
pub use canopy_mosaic::{Cell, Family, Tier};

/// Paint one mosaic cell: `ch` at `at`, drawn in `fg` over `bg`.
///
/// Each color passes through the effect stack of `render`, resolved at `at`
/// within `bounds`, so an effect that dims a view dims its mosaic too.
pub fn put_cell(
    render: &mut Render,
    bounds: Rect,
    at: Point,
    fg: [u8; 3],
    bg: [u8; 3],
    ch: char,
) -> Result<()> {
    let pen = |[r, g, b]: [u8; 3]| {
        let color = Color::Rgb { r, g, b };
        render
            .apply_effects(Style {
                fg: color.into(),
                bg: color.into(),
                attrs: AttrSet::default(),
            })
            .resolve_at(bounds, at)
    };
    let (fg, bg) = (pen(fg), pen(bg));
    let style = ResolvedStyle {
        fg: fg.fg,
        bg: bg.fg,
        attrs: fg.attrs,
    };
    render.put_cell(style, at, ch)
}

/// The richest tier that the terminal's glyphs allow.
pub fn tier_for(glyphs: Glyphs) -> Tier {
    match glyphs {
        Glyphs::Blocks => Tier::Blocks,
        Glyphs::Full => Tier::Full,
    }
}

/// Step a widget's own tier: from following the terminal's glyphs, through
/// each tier from half blocks to the full tier, and back. A gym toggles
/// with it to compare tiers.
pub fn cycle(tier: Option<Tier>) -> Option<Tier> {
    match tier {
        None => Some(Tier::Halves),
        Some(Tier::Full) => None,
        Some(tier) => Some(tier.next()),
    }
}

/// The name of a widget's own tier, or `terminal` when it follows the
/// terminal's glyphs.
pub fn label(tier: Option<Tier>) -> &'static str {
    tier.map_or("terminal", Tier::name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cycle_visits_every_tier_and_returns_to_the_terminal() {
        let mut tier = None;
        let mut seen = Vec::new();
        for _ in 0..5 {
            tier = cycle(tier);
            seen.push(label(tier));
        }
        assert_eq!(seen, ["halves", "quadrants", "blocks", "full", "terminal"]);
    }
}
