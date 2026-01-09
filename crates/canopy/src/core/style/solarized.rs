use super::{Attr, Color, StyleMap};
use crate::{rgb, style::AttrSet};

// Solarized color constants using the new hex constructor.
/// Solarized base03.
pub const BASE03: Color = rgb!("#002b36");
/// Solarized base02.
pub const BASE02: Color = rgb!("#073642");
/// Solarized base01.
pub const BASE01: Color = rgb!("#586e75");
/// Solarized base00.
pub const BASE00: Color = rgb!("#657b83");
/// Solarized base0.
pub const BASE0: Color = rgb!("#839496");
/// Solarized base1.
pub const BASE1: Color = rgb!("#93a1a1");
/// Solarized base2.
pub const BASE2: Color = rgb!("#eee8d5");
/// Solarized base3.
pub const BASE3: Color = rgb!("#fdf6e3");
/// Solarized yellow.
pub const YELLOW: Color = rgb!("#b58900");
/// Solarized orange.
pub const ORANGE: Color = rgb!("#cb4b16");
/// Solarized red.
pub const RED: Color = rgb!("#dc322f");
/// Solarized magenta.
pub const MAGENTA: Color = rgb!("#d33682");
/// Solarized violet.
pub const VIOLET: Color = rgb!("#6c71c4");
/// Solarized blue.
pub const BLUE: Color = rgb!("#268bd2");
/// Solarized cyan.
pub const CYAN: Color = rgb!("#2aa198");
/// Solarized green.
pub const GREEN: Color = rgb!("#859900");
/// Black.
pub const BLACK: Color = rgb!("#000000");

/// Build a dark solarized style map.
pub fn solarized_dark() -> StyleMap {
    use super::StyleBuilder;

    let mut c = StyleMap::new();
    c.rules()
        .style(
            "/",
            StyleBuilder::new()
                .fg(BASE0)
                .bg(BASE03)
                .attrs(AttrSet::default()),
        )
        .fg("/frame", BASE01)
        .fg("/frame/focused", BLUE)
        .fg("/frame/active", BASE1)
        .fg("/frame/title", BASE3)
        .fg("/tab", BASE01)
        .style("/tab/inactive", StyleBuilder::new().fg(BASE1).bg(BASE02))
        .style("/tab/active", StyleBuilder::new().fg(BASE3).bg(BLUE))
        .fg("/blue", BLUE)
        .fg("/red", RED)
        .fg("/magenta", MAGENTA)
        .fg("/violet", VIOLET)
        .fg("/cyan", CYAN)
        .fg("/green", GREEN)
        .fg("/yellow", YELLOW)
        .fg("/orange", ORANGE)
        .fg("/black", BLACK)
        .attr("/text/bold", Attr::Bold)
        .attr("/text/italic", Attr::Italic)
        .attr("/text/underline", Attr::Underline)
        .fg("/selector", BASE0)
        .fg("/selector/selected", BLUE)
        .style("/selector/focus", StyleBuilder::new().fg(BASE03).bg(BLUE))
        .style(
            "/selector/focus/selected",
            StyleBuilder::new().fg(BASE03).bg(CYAN),
        )
        .fg("/dropdown", BASE0)
        .fg("/dropdown/selected", BLUE)
        .style(
            "/dropdown/highlight",
            StyleBuilder::new().fg(BASE03).bg(BLUE),
        )
        .style("/editor/text", StyleBuilder::new().fg(BASE0).bg(BASE03))
        .style(
            "/editor/selection",
            StyleBuilder::new().fg(BASE0).bg(BASE02),
        )
        .style(
            "/editor/search/match",
            StyleBuilder::new().fg(BASE03).bg(YELLOW),
        )
        .style(
            "/editor/search/current",
            StyleBuilder::new().fg(BASE03).bg(ORANGE),
        )
        .fg("/editor/line-number", BASE01)
        .fg("/editor/line-number/current", BLUE)
        .style("/editor/prompt", StyleBuilder::new().fg(BASE0).bg(BASE02))
        .style("/help/content", StyleBuilder::new().fg(BASE0).bg(BASE02))
        .style("/help/frame", StyleBuilder::new().bg(BASE02))
        .style("/help/frame/focused", StyleBuilder::new().bg(BASE02))
        .style("/help/frame/active", StyleBuilder::new().bg(BASE02))
        .style("/help/frame/title", StyleBuilder::new().bg(BASE02))
        .style(
            "/help/key",
            StyleBuilder::new()
                .fg(CYAN)
                .bg(BASE02)
                .attrs(AttrSet::new(Attr::Bold)),
        )
        .style("/help/label", StyleBuilder::new().fg(BASE1).bg(BASE02))
        .apply();
    c
}

/// Build a light solarized style map.
pub fn solarized_light() -> StyleMap {
    use super::StyleBuilder;

    let mut c = StyleMap::new();
    c.rules()
        .style(
            "/",
            StyleBuilder::new()
                .fg(BASE00)
                .bg(BASE3)
                .attrs(AttrSet::default()),
        )
        .fg("/frame", BASE1)
        .fg("/frame/focused", BLUE)
        .fg("/frame/active", BASE01)
        .fg("/frame/title", BASE03)
        .fg("/tab", BASE1)
        .style("/tab/inactive", StyleBuilder::new().fg(BASE01).bg(BASE2))
        .style("/tab/active", StyleBuilder::new().fg(BASE3).bg(BLUE))
        .fg("/blue", BLUE)
        .fg("/red", RED)
        .fg("/magenta", MAGENTA)
        .fg("/violet", VIOLET)
        .fg("/cyan", CYAN)
        .fg("/green", GREEN)
        .fg("/yellow", YELLOW)
        .fg("/orange", ORANGE)
        .fg("/black", BLACK)
        .attr("/text/bold", Attr::Bold)
        .attr("/text/italic", Attr::Italic)
        .attr("/text/underline", Attr::Underline)
        .fg("/selector", BASE00)
        .fg("/selector/selected", BLUE)
        .style("/selector/focus", StyleBuilder::new().fg(BASE3).bg(BLUE))
        .style(
            "/selector/focus/selected",
            StyleBuilder::new().fg(BASE3).bg(CYAN),
        )
        .fg("/dropdown", BASE00)
        .fg("/dropdown/selected", BLUE)
        .style(
            "/dropdown/highlight",
            StyleBuilder::new().fg(BASE3).bg(BLUE),
        )
        .style("/editor/text", StyleBuilder::new().fg(BASE00).bg(BASE3))
        .style(
            "/editor/selection",
            StyleBuilder::new().fg(BASE00).bg(BASE2),
        )
        .style(
            "/editor/search/match",
            StyleBuilder::new().fg(BASE3).bg(YELLOW),
        )
        .style(
            "/editor/search/current",
            StyleBuilder::new().fg(BASE3).bg(ORANGE),
        )
        .fg("/editor/line-number", BASE1)
        .fg("/editor/line-number/current", BLUE)
        .style("/editor/prompt", StyleBuilder::new().fg(BASE00).bg(BASE2))
        .style("/help/content", StyleBuilder::new().fg(BASE00).bg(BASE2))
        .style("/help/frame", StyleBuilder::new().bg(BASE2))
        .style("/help/frame/focused", StyleBuilder::new().bg(BASE2))
        .style("/help/frame/active", StyleBuilder::new().bg(BASE2))
        .style("/help/frame/title", StyleBuilder::new().bg(BASE2))
        .style(
            "/help/key",
            StyleBuilder::new()
                .fg(CYAN)
                .bg(BASE2)
                .attrs(AttrSet::new(Attr::Bold)),
        )
        .style("/help/label", StyleBuilder::new().fg(BASE01).bg(BASE2))
        .apply();
    c
}
