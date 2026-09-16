//! Role colours shared by every built-in theme.
//!
//! A theme is a [`Palette`] of role colours plus the single [`theme`] rule
//! builder. Adding a rule here adds it to every theme at once.

use super::{Attr, AttrSet, Color, StyleBuilder, StyleMap};

/// The role colours a theme assigns.
///
/// Each field names the role a colour plays, not the colour itself, so the same
/// rule set can render a light theme, a dark theme, or any other palette.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Default foreground.
    pub fg: Color,
    /// Default background, and the foreground drawn on top of `accent`.
    pub bg: Color,
    /// Inactive frame borders.
    pub frame: Color,
    /// Border of the frame that holds focus.
    pub frame_focused: Color,
    /// Scrollbar thumbs on frame borders.
    pub frame_thumb: Color,
    /// Frame title text.
    pub frame_title: Color,
    /// Primary accent: focus and selection.
    pub accent: Color,
    /// Foreground on panel backgrounds, one step away from `fg`.
    pub muted_fg: Color,
    /// Background of panels such as the help overlay and prompt.
    pub panel_bg: Color,
    /// Background of raised elements set on a panel, such as inactive tabs.
    pub element_bg: Color,
    /// Selection background: editor selections and the active tab.
    pub selection_bg: Color,
    /// Editor line-number gutter.
    pub line_number: Color,
    /// Key names in the help overlay.
    pub key: Color,
    /// Named blue.
    pub blue: Color,
    /// Named red.
    pub red: Color,
    /// Named magenta.
    pub magenta: Color,
    /// Named violet.
    pub violet: Color,
    /// Named cyan, also the focused selected selector background.
    pub cyan: Color,
    /// Named green.
    pub green: Color,
    /// Named yellow, also the search-match background.
    pub yellow: Color,
    /// Named orange, also the current-search-match background.
    pub orange: Color,
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
        .fg("/frame/focused", p.frame_focused)
        .fg("/frame/thumb", p.frame_thumb)
        .fg("/frame/thumb/active", p.frame_focused)
        .fg("/frame/title", p.frame_title)
        .fg("/columns/divider", p.frame)
        .fg("/columns/thumb", p.frame_thumb)
        .fg("/columns/thumb/active", p.frame_focused)
        .fg("/blue", p.blue)
        .fg("/red", p.red)
        .fg("/magenta", p.magenta)
        .fg("/violet", p.violet)
        .fg("/cyan", p.cyan)
        .fg("/green", p.green)
        .fg("/yellow", p.yellow)
        .fg("/orange", p.orange)
        .attr("/text/bold", Attr::Bold)
        .attr("/text/italic", Attr::Italic)
        .attr("/text/underline", Attr::Underline)
        // A button's accelerator shares the help overlay's key colour, so one
        // letter names the key without the label repeating it. The button keeps
        // whatever ground it sits on.
        .style(
            "/button/key",
            StyleBuilder::new().fg(p.key).attrs(AttrSet::new(Attr::Bold)),
        )
        .fg("/button/focused/border", p.frame_focused)
        .fg("/button/disabled/border", p.muted_fg)
        .fg("/button/disabled/text", p.muted_fg)
        .fg("/button/disabled/key", p.muted_fg)
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
        .style(
            "/tabs/bar",
            StyleBuilder::new().fg(p.muted_fg).bg(p.panel_bg),
        )
        .style(
            "/tabs/tab",
            StyleBuilder::new().fg(p.muted_fg).bg(p.element_bg),
        )
        .style(
            "/tabs/tab/active",
            StyleBuilder::new()
                .fg(p.accent)
                .bg(p.selection_bg)
                .attrs(AttrSet::new(Attr::Bold)),
        )
        .style(
            "/tabs/tab/active/focused",
            StyleBuilder::new()
                .fg(p.bg)
                .bg(p.accent)
                .attrs(AttrSet::new(Attr::Bold)),
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
        .style("/help/panel", StyleBuilder::new().fg(p.fg).bg(p.panel_bg))
        .style_all(
            &[
                "/help/frame",
                "/help/frame/focused",
                "/help/frame/thumb",
                "/help/frame/thumb/active",
                "/help/frame/title",
            ],
            StyleBuilder::new().bg(p.panel_bg),
        )
        .style_all(
            &["/help/key", "/help/footer/key"],
            StyleBuilder::new()
                .fg(p.key)
                .bg(p.panel_bg)
                .attrs(AttrSet::new(Attr::Bold)),
        )
        .style_all(
            &[
                "/help/label",
                "/help/fallback",
                "/help/footer",
                "/help/footer/label",
            ],
            StyleBuilder::new().fg(p.muted_fg).bg(p.panel_bg),
        )
        .style(
            "/help/indicator",
            StyleBuilder::new().fg(p.accent).bg(p.panel_bg),
        )
        .style_all(
            &["/picker/background", "/picker/text"],
            StyleBuilder::new().fg(p.fg).bg(p.panel_bg),
        )
        .style(
            "/picker/selection",
            StyleBuilder::new().fg(p.bg).bg(p.accent),
        )
        // While the filter takes keys the list is not what the keyboard drives,
        // so its selection holds its place without claiming the eye.
        .style(
            "/picker/selection/dimmed",
            StyleBuilder::new().fg(p.fg).bg(p.selection_bg),
        )
        .style(
            "/picker/placeholder",
            StyleBuilder::new().fg(p.muted_fg).bg(p.panel_bg),
        )
        // The filter is a field rather than a row of the list, so it takes the
        // element ground to set it apart from the items above it. A filter that
        // has been given stays legible without competing with the list that the
        // keyboard has gone back to.
        .style_all(
            &["/picker/filter", "/picker/filter/text", "/picker/filter/prompt"],
            StyleBuilder::new().fg(p.muted_fg).bg(p.element_bg),
        )
        // Taking keys lights the field up, because it is what typing reaches.
        .style_all(
            &["/picker/filter/active", "/picker/filter/active/text"],
            StyleBuilder::new().fg(p.fg).bg(p.selection_bg),
        )
        .style(
            "/picker/filter/active/prompt",
            StyleBuilder::new()
                .fg(p.key)
                .bg(p.selection_bg)
                .attrs(AttrSet::new(Attr::Bold)),
        )
        .style_all(
            &[
                "/picker/frame",
                "/picker/frame/focused",
                "/picker/frame/thumb",
                "/picker/frame/thumb/active",
                "/picker/frame/title",
            ],
            StyleBuilder::new().bg(p.panel_bg),
        )
        .style_all(
            &["/confirm/background", "/confirm/message"],
            StyleBuilder::new().fg(p.fg).bg(p.panel_bg),
        )
        // The frame takes the panel behind it, so the dialog reads as one
        // surface rather than a border cut out of the view.
        .style_all(
            &[
                "/confirm/frame",
                "/confirm/frame/focused",
                "/confirm/frame/thumb",
                "/confirm/frame/thumb/active",
                "/confirm/frame/title",
            ],
            StyleBuilder::new().bg(p.panel_bg),
        )
        // The dialog's buttons take the panel behind them, so the row reads as
        // part of the dialog rather than as controls laid on the view.
        .style_all(
            &["/confirm/button/border", "/confirm/button/text"],
            StyleBuilder::new().fg(p.fg).bg(p.panel_bg),
        )
        .style(
            "/confirm/button/focused/border",
            StyleBuilder::new().fg(p.frame_focused).bg(p.panel_bg),
        )
        .style(
            "/confirm/button/key",
            StyleBuilder::new()
                .fg(p.key)
                .bg(p.panel_bg)
                .attrs(AttrSet::new(Attr::Bold)),
        )
        .apply();
    c
}
