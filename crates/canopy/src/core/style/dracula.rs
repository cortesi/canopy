//! Dracula theme - a dark theme with vibrant colors.
//!
//! Based on the Dracula theme: <https://draculatheme.com>

use super::{Attr, Color, StyleMap};
use crate::{rgb, style::AttrSet};

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
    let mut c = StyleMap::new();
    c.add(
        "/",
        Some(FOREGROUND),
        Some(BACKGROUND),
        Some(AttrSet::default()),
    );
    c.add_fg("/frame", COMMENT);
    c.add_fg("/frame/focused", PURPLE);
    c.add_fg("/frame/active", CYAN);
    c.add_fg("/frame/title", FOREGROUND);
    c.add_fg("/tab", COMMENT);
    c.add_fg("/tab/inactive", FOREGROUND);
    c.add_bg("/tab/inactive", CURRENT_LINE);
    c.add_fg("/tab/active", BACKGROUND);
    c.add_bg("/tab/active", PURPLE);

    c.add_fg("/blue", CYAN);
    c.add_fg("/red", RED);
    c.add_fg("/magenta", PINK);
    c.add_fg("/violet", PURPLE);
    c.add_fg("/cyan", CYAN);
    c.add_fg("/green", GREEN);
    c.add_fg("/yellow", YELLOW);
    c.add_fg("/orange", ORANGE);
    c.add_fg("/black", ANSI_BLACK);

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
    c.add_fg("/selector", FOREGROUND);
    c.add_fg("/selector/selected", PURPLE);
    c.add("/selector/focus", Some(BACKGROUND), Some(PURPLE), None);
    c.add(
        "/selector/focus/selected",
        Some(BACKGROUND),
        Some(CYAN),
        None,
    );

    // Dropdown widget styles
    c.add_fg("/dropdown", FOREGROUND);
    c.add_fg("/dropdown/selected", PURPLE);
    c.add("/dropdown/highlight", Some(BACKGROUND), Some(PURPLE), None);

    c
}
