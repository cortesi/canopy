//! Canopy theme - the default: a neutral near-black ground, grey chrome, and
//! a single accent.
//!
//! The grey ramp follows opencode's default theme, and the named
//! colours come from the One Dark and TokyoNight families that opencode and
//! Grok Build draw on.

use super::{Color, Palette, StyleMap, theme};
use crate::rgb;

/// Default background.
pub const BG: Color = rgb!("#0a0a0a");
/// Panel background: header and status bars, overlays.
pub const PANEL: Color = rgb!("#1a1a1a");
/// Element background: prompts, scrollbar tracks.
pub const ELEMENT: Color = rgb!("#1e1e1e");
/// Highlight background: raised elements and selections without focus.
pub const HIGHLIGHT: Color = rgb!("#282828");
/// Dividers, faint rules, and the selection background.
pub const BORDER_SUBTLE: Color = rgb!("#3c3c3c");
/// Frame borders.
pub const BORDER: Color = rgb!("#484848");
/// Borders of the active frame.
pub const BORDER_ACTIVE: Color = rgb!("#606060");
/// Muted text: comments, gutters, hints.
pub const MUTED: Color = rgb!("#808080");
/// Secondary text: labels and bars.
pub const SUBTEXT: Color = rgb!("#b4b4b4");
/// Default text.
pub const TEXT: Color = rgb!("#eeeeee");
/// Signature accent: focus and selection.
pub const ACCENT: Color = rgb!("#8aadf4");
/// Peach.
pub const PEACH: Color = rgb!("#fab283");
/// Blue.
pub const BLUE: Color = rgb!("#5c9cf5");
/// Violet.
pub const VIOLET: Color = rgb!("#bb9af7");
/// Magenta.
pub const MAGENTA: Color = rgb!("#e58fd6");
/// Cyan.
pub const CYAN: Color = rgb!("#56b6c2");
/// Green.
pub const GREEN: Color = rgb!("#7fd88f");
/// Yellow.
pub const YELLOW: Color = rgb!("#e5c07b");
/// Orange.
pub const ORANGE: Color = rgb!("#f5a742");
/// Red.
pub const RED: Color = rgb!("#e06c75");

/// Build the dark Canopy style map.
pub fn canopy_dark() -> StyleMap {
    theme(&Palette {
        fg: TEXT,
        bg: BG,
        frame: BORDER,
        frame_focused: MUTED,
        frame_thumb: BORDER_ACTIVE,
        frame_title: TEXT,
        accent: ACCENT,
        muted_fg: SUBTEXT,
        panel_bg: PANEL,
        element_bg: HIGHLIGHT,
        selection_bg: BORDER_SUBTLE,
        line_number: BORDER_ACTIVE,
        key: ACCENT,
        blue: BLUE,
        red: RED,
        magenta: MAGENTA,
        violet: VIOLET,
        cyan: CYAN,
        green: GREEN,
        yellow: YELLOW,
        orange: ORANGE,
    })
}
