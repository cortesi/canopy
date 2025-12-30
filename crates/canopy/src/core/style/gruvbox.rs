//! Gruvbox theme - a retro groove color scheme.
//!
//! Based on the gruvbox theme by morhetz: <https://github.com/morhetz/gruvbox>

use super::{Attr, Color, StyleMap};
use crate::{rgb, style::AttrSet};

// Gruvbox dark background colors
/// Dark background (hard contrast).
pub const DARK0_HARD: Color = rgb!("#1d2021");
/// Dark background (default).
pub const DARK0: Color = rgb!("#282828");
/// Dark background (soft contrast).
pub const DARK0_SOFT: Color = rgb!("#32302f");
/// Dark background 1.
pub const DARK1: Color = rgb!("#3c3836");
/// Dark background 2.
pub const DARK2: Color = rgb!("#504945");
/// Dark background 3.
pub const DARK3: Color = rgb!("#665c54");
/// Dark background 4.
pub const DARK4: Color = rgb!("#7c6f64");

// Gruvbox light foreground colors (used as fg in dark mode)
/// Light foreground 0.
pub const LIGHT0: Color = rgb!("#fbf1c7");
/// Light foreground 1.
pub const LIGHT1: Color = rgb!("#ebdbb2");
/// Light foreground 2.
pub const LIGHT2: Color = rgb!("#d5c4a1");
/// Light foreground 3.
pub const LIGHT3: Color = rgb!("#bdae93");
/// Light foreground 4.
pub const LIGHT4: Color = rgb!("#a89984");

// Gruvbox gray
/// Gray.
pub const GRAY: Color = rgb!("#928374");

// Gruvbox bright accent colors (for dark mode)
/// Bright red.
pub const RED: Color = rgb!("#fb4934");
/// Bright green.
pub const GREEN: Color = rgb!("#b8bb26");
/// Bright yellow.
pub const YELLOW: Color = rgb!("#fabd2f");
/// Bright blue.
pub const BLUE: Color = rgb!("#83a598");
/// Bright purple.
pub const PURPLE: Color = rgb!("#d3869b");
/// Bright aqua/cyan.
pub const AQUA: Color = rgb!("#8ec07c");
/// Bright orange.
pub const ORANGE: Color = rgb!("#fe8019");

/// Build a dark gruvbox style map.
pub fn gruvbox_dark() -> StyleMap {
    use super::StyleBuilder;

    let mut c = StyleMap::new();
    c.rules()
        .style(
            "/",
            StyleBuilder::new()
                .fg(LIGHT1)
                .bg(DARK0)
                .attrs(AttrSet::default()),
        )
        .fg("/frame", DARK4)
        .fg("/frame/focused", BLUE)
        .fg("/frame/active", LIGHT3)
        .fg("/frame/title", LIGHT0)
        .fg("/tab", DARK4)
        .style("/tab/inactive", StyleBuilder::new().fg(LIGHT3).bg(DARK1))
        .style("/tab/active", StyleBuilder::new().fg(LIGHT0).bg(BLUE))
        .fg("/blue", BLUE)
        .fg("/red", RED)
        .fg("/magenta", PURPLE)
        .fg("/violet", PURPLE)
        .fg("/cyan", AQUA)
        .fg("/green", GREEN)
        .fg("/yellow", YELLOW)
        .fg("/orange", ORANGE)
        .fg("/black", DARK0)
        .attr("/text/bold", Attr::Bold)
        .attr("/text/italic", Attr::Italic)
        .attr("/text/underline", Attr::Underline)
        .fg("/selector", LIGHT1)
        .fg("/selector/selected", BLUE)
        .style("/selector/focus", StyleBuilder::new().fg(DARK0).bg(BLUE))
        .style(
            "/selector/focus/selected",
            StyleBuilder::new().fg(DARK0).bg(AQUA),
        )
        .fg("/dropdown", LIGHT1)
        .fg("/dropdown/selected", BLUE)
        .style(
            "/dropdown/highlight",
            StyleBuilder::new().fg(DARK0).bg(BLUE),
        )
        .apply();
    c
}
