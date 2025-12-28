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
    let mut c = StyleMap::new();
    c.add("/", Some(BASE0), Some(BASE03), Some(AttrSet::default()));
    c.add_fg("/frame", BASE01);
    c.add_fg("/frame/focused", BLUE);
    c.add_fg("/frame/active", BASE1);
    c.add_fg("/frame/title", BASE3);
    c.add_fg("/tab", BASE01);
    c.add_fg("/tab/inactive", BASE1);
    c.add_bg("/tab/inactive", BASE02);
    c.add_fg("/tab/active", BASE3);
    c.add_bg("/tab/active", BLUE);

    c.add_fg("/blue", BLUE);
    c.add_fg("/red", RED);
    c.add_fg("/magenta", MAGENTA);
    c.add_fg("/violet", VIOLET);
    c.add_fg("/cyan", CYAN);
    c.add_fg("/green", GREEN);
    c.add_fg("/yellow", YELLOW);
    c.add_fg("/orange", ORANGE);
    c.add_fg("/black", BLACK);

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
    c.add_fg("/selector", BASE0);
    c.add_fg("/selector/selected", BLUE);
    c.add("/selector/focus", Some(BASE03), Some(BLUE), None);
    c.add("/selector/focus/selected", Some(BASE03), Some(CYAN), None);

    // Dropdown widget styles
    c.add_fg("/dropdown", BASE0);
    c.add_fg("/dropdown/selected", BLUE);
    c.add("/dropdown/highlight", Some(BASE03), Some(BLUE), None);

    c
}

/// Build a light solarized style map.
pub fn solarized_light() -> StyleMap {
    let mut c = StyleMap::new();
    // Light mode: swap base colors (BASE00 fg on BASE3 bg)
    c.add("/", Some(BASE00), Some(BASE3), Some(AttrSet::default()));
    c.add_fg("/frame", BASE1);
    c.add_fg("/frame/focused", BLUE);
    c.add_fg("/frame/active", BASE01);
    c.add_fg("/frame/title", BASE03);
    c.add_fg("/tab", BASE1);
    c.add_fg("/tab/inactive", BASE01);
    c.add_bg("/tab/inactive", BASE2);
    c.add_fg("/tab/active", BASE3);
    c.add_bg("/tab/active", BLUE);

    c.add_fg("/blue", BLUE);
    c.add_fg("/red", RED);
    c.add_fg("/magenta", MAGENTA);
    c.add_fg("/violet", VIOLET);
    c.add_fg("/cyan", CYAN);
    c.add_fg("/green", GREEN);
    c.add_fg("/yellow", YELLOW);
    c.add_fg("/orange", ORANGE);
    c.add_fg("/black", BLACK);

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
    c.add_fg("/selector", BASE00);
    c.add_fg("/selector/selected", BLUE);
    c.add("/selector/focus", Some(BASE3), Some(BLUE), None);
    c.add("/selector/focus/selected", Some(BASE3), Some(CYAN), None);

    // Dropdown widget styles
    c.add_fg("/dropdown", BASE00);
    c.add_fg("/dropdown/selected", BLUE);
    c.add("/dropdown/highlight", Some(BASE3), Some(BLUE), None);

    c
}
