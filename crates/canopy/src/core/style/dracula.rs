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
    use super::StyleBuilder;

    let mut c = StyleMap::new();
    c.rules()
        .style(
            "/",
            StyleBuilder::new()
                .fg(FOREGROUND)
                .bg(BACKGROUND)
                .attrs(AttrSet::default()),
        )
        .fg("/frame", COMMENT)
        .fg("/frame/focused", PURPLE)
        .fg("/frame/active", CYAN)
        .fg("/frame/title", FOREGROUND)
        .fg("/tab", COMMENT)
        .style(
            "/tab/inactive",
            StyleBuilder::new().fg(FOREGROUND).bg(CURRENT_LINE),
        )
        .style("/tab/active", StyleBuilder::new().fg(BACKGROUND).bg(PURPLE))
        .fg("/blue", CYAN)
        .fg("/red", RED)
        .fg("/magenta", PINK)
        .fg("/violet", PURPLE)
        .fg("/cyan", CYAN)
        .fg("/green", GREEN)
        .fg("/yellow", YELLOW)
        .fg("/orange", ORANGE)
        .fg("/black", ANSI_BLACK)
        .attr("/text/bold", Attr::Bold)
        .attr("/text/italic", Attr::Italic)
        .attr("/text/underline", Attr::Underline)
        .fg("/selector", FOREGROUND)
        .fg("/selector/selected", PURPLE)
        .style(
            "/selector/focus",
            StyleBuilder::new().fg(BACKGROUND).bg(PURPLE),
        )
        .style(
            "/selector/focus/selected",
            StyleBuilder::new().fg(BACKGROUND).bg(CYAN),
        )
        .fg("/dropdown", FOREGROUND)
        .fg("/dropdown/selected", PURPLE)
        .style(
            "/dropdown/highlight",
            StyleBuilder::new().fg(BACKGROUND).bg(PURPLE),
        )
        .style(
            "/editor/text",
            StyleBuilder::new().fg(FOREGROUND).bg(BACKGROUND),
        )
        .style(
            "/editor/selection",
            StyleBuilder::new().fg(FOREGROUND).bg(SELECTION),
        )
        .style(
            "/editor/search/match",
            StyleBuilder::new().fg(BACKGROUND).bg(YELLOW),
        )
        .style(
            "/editor/search/current",
            StyleBuilder::new().fg(BACKGROUND).bg(ORANGE),
        )
        .fg("/editor/line-number", COMMENT)
        .fg("/editor/line-number/current", PURPLE)
        .style(
            "/editor/prompt",
            StyleBuilder::new().fg(FOREGROUND).bg(CURRENT_LINE),
        )
        .apply();
    c
}
