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
    let mut c = StyleMap::new();
    c.add("/", Some(LIGHT1), Some(DARK0), Some(AttrSet::default()));
    c.add_fg("/frame", DARK4);
    c.add_fg("/frame/focused", BLUE);
    c.add_fg("/frame/active", LIGHT3);
    c.add_fg("/frame/title", LIGHT0);
    c.add_fg("/tab", DARK4);
    c.add_fg("/tab/inactive", LIGHT3);
    c.add_bg("/tab/inactive", DARK1);
    c.add_fg("/tab/active", LIGHT0);
    c.add_bg("/tab/active", BLUE);

    c.add_fg("/blue", BLUE);
    c.add_fg("/red", RED);
    c.add_fg("/magenta", PURPLE);
    c.add_fg("/violet", PURPLE);
    c.add_fg("/cyan", AQUA);
    c.add_fg("/green", GREEN);
    c.add_fg("/yellow", YELLOW);
    c.add_fg("/orange", ORANGE);
    c.add_fg("/black", DARK0);

    // Text style variants
    c.add("/text/bold", None, None, Some(AttrSet::new(Attr::Bold)));
    c.add("/text/italic", None, None, Some(AttrSet::new(Attr::Italic)));
    c.add(
        "/text/underline",
        None,
        None,
        Some(AttrSet::new(Attr::Underline)),
    );

    // Selector widget styles
    c.add_fg("/selector", LIGHT1);
    c.add_fg("/selector/selected", BLUE);
    c.add("/selector/focus", Some(DARK0), Some(BLUE), None);
    c.add("/selector/focus/selected", Some(DARK0), Some(AQUA), None);

    // Dropdown widget styles
    c.add_fg("/dropdown", LIGHT1);
    c.add_fg("/dropdown/selected", BLUE);
    c.add("/dropdown/highlight", Some(DARK0), Some(BLUE), None);

    c
}
