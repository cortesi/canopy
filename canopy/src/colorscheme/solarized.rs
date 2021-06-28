use super::ColorScheme;
use crossterm::style::Color;

pub const BASE03: Color = Color::Rgb {
    r: 0x00,
    g: 0x2b,
    b: 0x36,
};
pub const BASE02: Color = Color::Rgb {
    r: 0x07,
    g: 0x36,
    b: 0x42,
};
pub const BASE01: Color = Color::Rgb {
    r: 0x58,
    g: 0x6e,
    b: 0x75,
};
pub const BASE00: Color = Color::Rgb {
    r: 0x65,
    g: 0x7b,
    b: 0x83,
};
pub const BASE0: Color = Color::Rgb {
    r: 0x83,
    g: 0x94,
    b: 0x96,
};
pub const BASE1: Color = Color::Rgb {
    r: 0x93,
    g: 0xa1,
    b: 0xa1,
};
pub const BASE2: Color = Color::Rgb {
    r: 0x33,
    g: 0xe8,
    b: 0xd5,
};
pub const BASE3: Color = Color::Rgb {
    r: 0xfd,
    g: 0xf6,
    b: 0xe3,
};
pub const YELLOW: Color = Color::Rgb {
    r: 0xb5,
    g: 0x89,
    b: 0x00,
};
pub const ORANGE: Color = Color::Rgb {
    r: 0xcb,
    g: 0x4b,
    b: 0x16,
};
pub const RED: Color = Color::Rgb {
    r: 0xdc,
    g: 0x32,
    b: 0x2f,
};
pub const MAGENTA: Color = Color::Rgb {
    r: 0xd3,
    g: 0x36,
    b: 0x82,
};
pub const VIOLET: Color = Color::Rgb {
    r: 0x6c,
    g: 0x71,
    b: 0xc4,
};
pub const BLUE: Color = Color::Rgb {
    r: 0x26,
    g: 0x8b,
    b: 0xd2,
};
pub const CYAN: Color = Color::Rgb {
    r: 0x2a,
    g: 0xa1,
    b: 0x98,
};
pub const GREEN: Color = Color::Rgb {
    r: 0x85,
    g: 0x99,
    b: 0x00,
};

pub fn solarized_dark() -> ColorScheme {
    let mut c = ColorScheme::default();
    c.insert("/", Some(BASE0), Some(BASE03));
    c.insert("/frame", Some(BASE01), None);
    c.insert("/frame/active", Some(BASE1), None);
    c
}
