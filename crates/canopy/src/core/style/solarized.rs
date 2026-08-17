use super::{Color, Palette, StyleMap, theme};
use crate::rgb;

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
    theme(&Palette {
        fg: BASE0,
        bg: BASE03,
        frame: BASE01,
        frame_active: BASE1,
        frame_title: BASE3,
        accent: BLUE,
        muted_fg: BASE1,
        panel_bg: BASE02,
        tab_active_fg: BASE3,
        selection_bg: BASE02,
        line_number: BASE01,
        blue: BLUE,
        red: RED,
        magenta: MAGENTA,
        violet: VIOLET,
        cyan: CYAN,
        green: GREEN,
        yellow: YELLOW,
        orange: ORANGE,
        black: BLACK,
    })
}

/// Build a light solarized style map.
pub fn solarized_light() -> StyleMap {
    theme(&Palette {
        fg: BASE00,
        bg: BASE3,
        frame: BASE1,
        frame_active: BASE01,
        frame_title: BASE03,
        accent: BLUE,
        muted_fg: BASE01,
        panel_bg: BASE2,
        tab_active_fg: BASE3,
        selection_bg: BASE2,
        line_number: BASE1,
        blue: BLUE,
        red: RED,
        magenta: MAGENTA,
        violet: VIOLET,
        cyan: CYAN,
        green: GREEN,
        yellow: YELLOW,
        orange: ORANGE,
        black: BLACK,
    })
}
