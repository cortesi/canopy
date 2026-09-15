//! Gruvbox theme - a retro groove color scheme.
//!
//! Based on the gruvbox theme by morhetz: <https://github.com/morhetz/gruvbox>

use super::{Color, Palette, StyleMap, theme};
use crate::rgb;

// Gruvbox dark background colors
/// Dark background (default).
const DARK0: Color = rgb!("#282828");
/// Dark background 1.
const DARK1: Color = rgb!("#3c3836");
/// Dark background 2.
const DARK2: Color = rgb!("#504945");
/// Dark background 4.
const DARK4: Color = rgb!("#7c6f64");

// Gruvbox light foreground colors (used as fg in dark mode)
/// Light foreground 0.
const LIGHT0: Color = rgb!("#fbf1c7");
/// Light foreground 1.
const LIGHT1: Color = rgb!("#ebdbb2");
/// Light foreground 3.
const LIGHT3: Color = rgb!("#bdae93");

// Gruvbox gray
/// Gray.
const GRAY: Color = rgb!("#928374");

// Gruvbox bright accent colors (for dark mode)
/// Bright red.
const RED: Color = rgb!("#fb4934");
/// Bright green.
const GREEN: Color = rgb!("#b8bb26");
/// Bright yellow.
const YELLOW: Color = rgb!("#fabd2f");
/// Bright blue.
const BLUE: Color = rgb!("#83a598");
/// Bright purple.
const PURPLE: Color = rgb!("#d3869b");
/// Bright aqua/cyan.
const AQUA: Color = rgb!("#8ec07c");
/// Bright orange.
const ORANGE: Color = rgb!("#fe8019");

/// Build a dark gruvbox style map.
pub fn gruvbox_dark() -> StyleMap {
    theme(&Palette {
        fg: LIGHT1,
        bg: DARK0,
        frame: DARK4,
        frame_focused: BLUE,
        frame_active: LIGHT3,
        frame_title: LIGHT0,
        accent: BLUE,
        muted_fg: LIGHT3,
        panel_bg: DARK1,
        element_bg: DARK0,
        selection_bg: DARK2,
        line_number: GRAY,
        key: AQUA,
        blue: BLUE,
        red: RED,
        magenta: PURPLE,
        violet: PURPLE,
        cyan: AQUA,
        green: GREEN,
        yellow: YELLOW,
        orange: ORANGE,
    })
}
