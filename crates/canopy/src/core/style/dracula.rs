//! Dracula theme - a dark theme with vibrant colors.
//!
//! Based on the Dracula theme: <https://draculatheme.com>

use super::{Color, Palette, StyleMap, theme};
use crate::rgb;

// Dracula background colors
/// Background.
pub const BACKGROUND: Color = rgb!("#282a36");
/// Current line / selection background.
pub const CURRENT_LINE: Color = rgb!("#44475a");
/// Selection.
pub const SELECTION: Color = rgb!("#44475a");

// Dracula foreground colors
/// Foreground.
pub const FOREGROUND: Color = rgb!("#f8f8f2");
/// Comment color (also used for subtle elements).
pub const COMMENT: Color = rgb!("#6272a4");

// Dracula accent colors
/// Red.
pub const RED: Color = rgb!("#ff5555");
/// Orange.
pub const ORANGE: Color = rgb!("#ffb86c");
/// Yellow.
pub const YELLOW: Color = rgb!("#f1fa8c");
/// Green.
pub const GREEN: Color = rgb!("#50fa7b");
/// Cyan.
pub const CYAN: Color = rgb!("#8be9fd");
/// Purple.
pub const PURPLE: Color = rgb!("#bd93f9");
/// Pink.
pub const PINK: Color = rgb!("#ff79c6");

// ANSI colors for terminal compatibility
/// ANSI black.
pub const ANSI_BLACK: Color = rgb!("#21222c");

/// Build a Dracula style map.
pub fn dracula() -> StyleMap {
    theme(&Palette {
        fg: FOREGROUND,
        bg: BACKGROUND,
        frame: COMMENT,
        frame_active: CYAN,
        frame_title: FOREGROUND,
        accent: PURPLE,
        muted_fg: FOREGROUND,
        panel_bg: CURRENT_LINE,
        tab_active_fg: BACKGROUND,
        selection_bg: SELECTION,
        line_number: COMMENT,
        blue: CYAN,
        red: RED,
        magenta: PINK,
        violet: PURPLE,
        cyan: CYAN,
        green: GREEN,
        yellow: YELLOW,
        orange: ORANGE,
        black: ANSI_BLACK,
    })
}
