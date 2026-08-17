//! Role colours shared by every built-in theme.
//!
//! A theme is a [`Palette`] of role colours plus the single [`theme`] rule builder. Adding a
//! rule here adds it to every theme at once.

use super::{Attr, AttrSet, Color, StyleBuilder, StyleMap};

/// The role colours a theme assigns.
///
/// Each field names the role a colour plays, not the colour itself, so the same rule set can
/// render a light theme, a dark theme, or any other palette.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Default foreground.
    pub fg: Color,
    /// Default background, and the foreground drawn on top of `accent`.
    pub bg: Color,
    /// Inactive frame and tab borders, and the tab bar itself.
    pub frame: Color,
    /// Border of the frame that owns the active subtree.
    pub frame_active: Color,
    /// Frame title text.
    pub frame_title: Color,
    /// Primary accent: focus, selection, and the active tab.
    pub accent: Color,
    /// Foreground on panel backgrounds, one step away from `fg`.
    pub muted_fg: Color,
    /// Background of panels such as the help overlay, prompt, and inactive tabs.
    pub panel_bg: Color,
    /// Foreground of the active tab, drawn on `accent`.
    pub tab_active_fg: Color,
    /// Editor selection background.
    pub selection_bg: Color,
    /// Editor line-number gutter.
    pub line_number: Color,
    /// Named blue.
    pub blue: Color,
    /// Named red.
    pub red: Color,
    /// Named magenta.
    pub magenta: Color,
    /// Named violet.
    pub violet: Color,
    /// Named cyan, also the help overlay's key colour.
    pub cyan: Color,
    /// Named green.
    pub green: Color,
    /// Named yellow, also the search-match background.
    pub yellow: Color,
    /// Named orange, also the current-search-match background.
    pub orange: Color,
    /// Named black.
    pub black: Color,
}

/// Build the shared rule set for one palette.
pub fn theme(p: &Palette) -> StyleMap {
    let mut c = StyleMap::new();
    c.rules()
        .style(
            "/",
            StyleBuilder::new()
                .fg(p.fg)
                .bg(p.bg)
                .attrs(AttrSet::default()),
        )
        .fg("/frame", p.frame)
        .fg("/frame/focused", p.accent)
        .fg("/frame/active", p.frame_active)
        .fg("/frame/title", p.frame_title)
        .fg("/tab", p.frame)
        .style(
            "/tab/inactive",
            StyleBuilder::new().fg(p.muted_fg).bg(p.panel_bg),
        )
        .style(
            "/tab/active",
            StyleBuilder::new().fg(p.tab_active_fg).bg(p.accent),
        )
        .fg("/blue", p.blue)
        .fg("/red", p.red)
        .fg("/magenta", p.magenta)
        .fg("/violet", p.violet)
        .fg("/cyan", p.cyan)
        .fg("/green", p.green)
        .fg("/yellow", p.yellow)
        .fg("/orange", p.orange)
        .fg("/black", p.black)
        .attr("/text/bold", Attr::Bold)
        .attr("/text/italic", Attr::Italic)
        .attr("/text/underline", Attr::Underline)
        .fg("/selector", p.fg)
        .fg("/selector/selected", p.accent)
        .style("/selector/focus", StyleBuilder::new().fg(p.bg).bg(p.accent))
        .style(
            "/selector/focus/selected",
            StyleBuilder::new().fg(p.bg).bg(p.cyan),
        )
        .fg("/dropdown", p.fg)
        .fg("/dropdown/selected", p.accent)
        .style(
            "/dropdown/highlight",
            StyleBuilder::new().fg(p.bg).bg(p.accent),
        )
        .style("/editor/text", StyleBuilder::new().fg(p.fg).bg(p.bg))
        .style(
            "/editor/selection",
            StyleBuilder::new().fg(p.fg).bg(p.selection_bg),
        )
        .style(
            "/editor/search/match",
            StyleBuilder::new().fg(p.bg).bg(p.yellow),
        )
        .style(
            "/editor/search/current",
            StyleBuilder::new().fg(p.bg).bg(p.orange),
        )
        .fg("/editor/line-number", p.line_number)
        .fg("/editor/line-number/current", p.accent)
        .style(
            "/editor/prompt",
            StyleBuilder::new().fg(p.fg).bg(p.panel_bg),
        )
        .style("/help/content", StyleBuilder::new().fg(p.fg).bg(p.panel_bg))
        .style("/help/frame", StyleBuilder::new().bg(p.panel_bg))
        .style("/help/frame/focused", StyleBuilder::new().bg(p.panel_bg))
        .style("/help/frame/active", StyleBuilder::new().bg(p.panel_bg))
        .style("/help/frame/title", StyleBuilder::new().bg(p.panel_bg))
        .style(
            "/help/key",
            StyleBuilder::new()
                .fg(p.cyan)
                .bg(p.panel_bg)
                .attrs(AttrSet::new(Attr::Bold)),
        )
        .style(
            "/help/label",
            StyleBuilder::new().fg(p.muted_fg).bg(p.panel_bg),
        )
        .apply();
    c
}
