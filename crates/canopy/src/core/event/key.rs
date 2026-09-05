//! This module contains the core primitives to represent keyboard input.
use std::{fmt, ops::Add};

/// Modifier key state.
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Mods {
    /// Shift is active.
    pub shift: bool,
    /// Control is active.
    pub ctrl: bool,
    /// Alt is active.
    pub alt: bool,
}

impl Add<KeyCode> for Mods {
    type Output = Key;

    fn add(self, key: KeyCode) -> Self::Output {
        Key { mods: self, key }
    }
}

impl Add<char> for Mods {
    type Output = Key;

    fn add(self, other: char) -> Self::Output {
        Key {
            mods: self,
            key: other.into(),
        }
    }
}

impl Add<Self> for Mods {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self {
            shift: self.shift || other.shift,
            ctrl: self.ctrl || other.ctrl,
            alt: self.alt || other.alt,
        }
    }
}

/// No modifiers pressed.
#[allow(non_upper_case_globals)]
pub const Empty: Mods = Mods {
    shift: false,
    ctrl: false,
    alt: false,
};

/// Shift-only modifier state.
#[allow(non_upper_case_globals)]
pub const Shift: Mods = Mods {
    shift: true,
    ctrl: false,
    alt: false,
};

/// Control-only modifier state.
#[allow(non_upper_case_globals)]
pub const Ctrl: Mods = Mods {
    shift: false,
    ctrl: true,
    alt: false,
};

/// Alt-only modifier state.
#[allow(non_upper_case_globals)]
pub const Alt: Mods = Mods {
    shift: false,
    ctrl: false,
    alt: true,
};

/// Physical modifier key codes.
#[derive(Debug, PartialOrd, PartialEq, Hash, Eq, Clone, Copy)]
pub enum ModifierKeyCode {
    /// Left Shift key.
    LeftShift,
    /// Left Control key.
    LeftControl,
    /// Left Alt key.
    LeftAlt,
    /// Left Super key.
    LeftSuper,
    /// Left Hyper key.
    LeftHyper,
    /// Left Meta key.
    LeftMeta,
    /// Right Shift key.
    RightShift,
    /// Right Control key.
    RightControl,
    /// Right Alt key.
    RightAlt,
    /// Right Super key.
    RightSuper,
    /// Right Hyper key.
    RightHyper,
    /// Right Meta key.
    RightMeta,
    /// Iso Level3 Shift key.
    IsoLevel3Shift,
    /// Iso Level5 Shift key.
    IsoLevel5Shift,
}

/// Media key codes.
#[derive(Debug, PartialOrd, PartialEq, Hash, Eq, Clone, Copy)]
pub enum MediaKeyCode {
    /// Play media key.
    Play,
    /// Pause media key.
    Pause,
    /// Play/Pause media key.
    PlayPause,
    /// Reverse media key.
    Reverse,
    /// Stop media key.
    Stop,
    /// Fast-forward media key.
    FastForward,
    /// Rewind media key.
    Rewind,
    /// Next-track media key.
    TrackNext,
    /// Previous-track media key.
    TrackPrevious,
    /// Record media key.
    Record,
    /// Lower-volume media key.
    LowerVolume,
    /// Raise-volume media key.
    RaiseVolume,
    /// Mute media key.
    MuteVolume,
}

/// Logical key codes.
#[derive(Debug, PartialOrd, PartialEq, Hash, Eq, Clone, Copy)]
pub enum KeyCode {
    /// Backspace key.
    Backspace,
    /// Enter/return key.
    Enter,
    /// Left arrow key.
    Left,
    /// Right arrow key.
    Right,
    /// Up arrow key.
    Up,
    /// Down arrow key.
    Down,
    /// Home key.
    Home,
    /// End key.
    End,
    /// Page up key.
    PageUp,
    /// Page down key.
    PageDown,
    /// Tab key.
    Tab,
    /// Shift + Tab key.
    BackTab,
    /// Delete key.
    Delete,
    /// Insert key.
    Insert,
    /// Null key code.
    Null,
    /// Escape key.
    Esc,
    /// Caps lock key.
    CapsLock,
    /// Scroll lock key.
    ScrollLock,
    /// Num lock key.
    NumLock,
    /// Print screen key.
    PrintScreen,
    /// Pause key.
    Pause,
    /// Menu key.
    Menu,
    /// Keypad "begin" key.
    KeypadBegin,
    /// F key.
    ///
    /// `KeyCode::F(1)` represents the F1 key, and so on.
    F(u8),
    /// A character.
    ///
    /// `KeyCode::Char('c')` represents the `c` character, and so on.
    Char(char),
    /// Media key code.
    Media(MediaKeyCode),
    /// Modifier key code.
    Modifier(ModifierKeyCode),
}

impl From<char> for KeyCode {
    fn from(c: char) -> Self {
        Self::Char(c)
    }
}

/// A keystroke along with modifiers.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Key {
    /// Modifier state.
    pub mods: Mods,
    /// Key code.
    pub key: KeyCode,
}

impl Key {
    /// Normalize key inputs for binding and matching.
    ///
    /// Normalization handles two common sources of divergence across terminals:
    ///
    /// - **Ctrl-modified ASCII control codes** (0x00–0x1F and 0x7F) are mapped
    ///   to canonical printable equivalents (e.g. 0x01 → `A`, 0x1B → `[`, 0x7F
    ///   → `?`). Some terminals emit control codes without setting the Ctrl
    ///   modifier, so these codes are treated as Ctrl-combinations even if Ctrl
    ///   isn't reported. Ctrl+`_`, Ctrl+`?`, and Ctrl+`7` then alias to `/` to
    ///   align with common `Ctrl+/` help bindings, and Ctrl+`4`, Ctrl+`5`,
    ///   Ctrl+`6` alias to `\`, `]`, and `^`.
    /// - **Shift handling** is applied after Ctrl canonicalization.
    ///
    /// Handling of the shift key is the most intricate part of this module.
    /// When we receive an event, it includes the shift modifier and also the
    /// modified character - e.g. "shift + A" or "shift + (". However, when
    /// users bind keys, it's more intuitive to bind just "A" or "(". We don't
    /// know what the keyboard mapping or input method is for the user - so it's
    /// not possible in a general way for us to map between, say, an input like
    /// "shift + 0" to the shifted key "(". Conversely, if we see an input of
    /// "shift + (", we don't know if the user pressed "shift + 0" or if they
    /// have a weird keyboard layout that actually permits "shift + (" without a
    /// shift conversion.
    ///
    /// To handle this, we have to make a lossy compromise. We define a
    /// normalisation applied to input for the purpose of key binding matching
    /// as follows:
    ///
    /// - If shift is present:
    ///     - If the key is ascii lowercase, convert it to uppercase and remove
    ///       shift
    ///     - If the key is space, leave shift intact
    ///     - in all other cases, just remove shift
    ///
    /// | input             | normalization    |
    /// |-------------------|------------------|
    /// | shift + A         | A                |
    /// | shift + a         | A                |
    /// | shift + )         | )                |
    /// | shift + enter     | shift + enter    |
    /// | shift + ctrl + A  | ctrl + A         |
    ///
    /// `normalize` must be called explicitly when needed. Comparison is literal
    /// and straightforward and does not normalize. `parse_spec` normalizes
    /// its result.
    pub fn normalize(&self) -> Self {
        let mut normalized = *self;
        if let KeyCode::Char(c) = normalized.key {
            if let Some(mapped) = ctrl_control_code(c) {
                normalized.key = KeyCode::Char(mapped);
                normalized.mods.ctrl = true;
            }
            if normalized.mods.ctrl
                && let KeyCode::Char(c) = normalized.key
                && let Some(mapped) = ctrl_alias_char(c)
            {
                normalized.key = KeyCode::Char(mapped);
            }
        }

        // Shift is folded into the character it produced, except for space,
        // which keeps it.
        if normalized.mods.shift
            && let KeyCode::Char(c) = normalized.key
            && c != ' '
        {
            normalized.key = KeyCode::Char(c.to_ascii_uppercase());
            normalized.mods.shift = false;
        }
        normalized
    }

    /// Parse a key specification such as `ctrl-s`, `PageDown`, or `A`.
    pub fn parse_spec(spec: &str) -> Result<Self, String> {
        let spec = spec.trim();
        if spec.chars().count() == 1 {
            return Ok(Self::from(parse_key_code(spec)?).normalize());
        }
        let (mods, key_part) = parse_spec_parts(spec, &['-', '+'])?;
        Ok((mods + parse_key_code(key_part)?).normalize())
    }
}

/// Split an input specification into its modifier set and its trailing body.
///
/// The separators differ per input kind: key specs accept `-` and `+`, mouse
/// specs only `-`.
pub(crate) fn parse_spec_parts<'a>(
    spec: &'a str,
    separators: &[char],
) -> Result<(Mods, &'a str), String> {
    let parts = spec
        .trim()
        .split(separators)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let Some((body, modifier_parts)) = parts.split_last() else {
        return Err("input specification cannot be empty".into());
    };

    let mut mods = Empty;
    for part in modifier_parts {
        if part.eq_ignore_ascii_case("ctrl") || part.eq_ignore_ascii_case("control") {
            mods.ctrl = true;
        } else if part.eq_ignore_ascii_case("alt") {
            mods.alt = true;
        } else if part.eq_ignore_ascii_case("shift") {
            mods.shift = true;
        } else {
            return Err(format!("unknown modifier: {part}"));
        }
    }
    Ok((mods, body))
}

/// Parse a single key name into a key code.
fn parse_key_code(spec: &str) -> Result<KeyCode, String> {
    if spec.chars().count() == 1 {
        return Ok(KeyCode::Char(
            spec.chars().next().expect("single-character key spec"),
        ));
    }

    let lower = spec.to_ascii_lowercase();
    let code = match lower.as_str() {
        "backspace" => KeyCode::Backspace,
        "enter" | "return" => KeyCode::Enter,
        "left" | "arrowleft" => KeyCode::Left,
        "right" | "arrowright" => KeyCode::Right,
        "up" | "arrowup" => KeyCode::Up,
        "down" | "arrowdown" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "null" => KeyCode::Null,
        "esc" | "escape" => KeyCode::Esc,
        "capslock" => KeyCode::CapsLock,
        "scrolllock" => KeyCode::ScrollLock,
        "numlock" => KeyCode::NumLock,
        "printscreen" => KeyCode::PrintScreen,
        "pause" => KeyCode::Pause,
        "menu" => KeyCode::Menu,
        "keypadbegin" => KeyCode::KeypadBegin,
        "space" => KeyCode::Char(' '),
        _ => {
            if let Some(number) = lower.strip_prefix('f') {
                let number = number
                    .parse::<u8>()
                    .map_err(|_| format!("invalid function key: {spec}"))?;
                return Ok(KeyCode::F(number));
            }
            return Err(format!("unknown key: {spec}"));
        }
    };
    Ok(code)
}

/// Map ASCII control codes to canonical printable characters.
fn ctrl_control_code(c: char) -> Option<char> {
    match c {
        '\u{0}' => Some('@'),
        '\u{1}'..='\u{1A}' => Some((c as u8 + b'@') as char),
        '\u{1B}' => Some('['),
        '\u{1C}' => Some('\\'),
        '\u{1D}' => Some(']'),
        '\u{1E}' => Some('^'),
        '\u{1F}' => Some('/'),
        '\u{7F}' => Some('?'),
        _ => None,
    }
}

/// Map Ctrl-modified printable aliases to canonical equivalents.
fn ctrl_alias_char(c: char) -> Option<char> {
    match c {
        '_' => Some('/'),
        '?' => Some('/'),
        '4' => Some('\\'),
        '5' => Some(']'),
        '6' => Some('^'),
        '7' => Some('/'),
        _ => None,
    }
}

impl PartialEq<char> for Key {
    /// An unmodified key matches the character it produces.
    fn eq(&self, c: &char) -> bool {
        self.mods == Empty && self.key == KeyCode::Char(*c)
    }
}

impl From<char> for Key {
    fn from(c: char) -> Self {
        Self {
            mods: Empty,
            key: KeyCode::Char(c),
        }
    }
}

impl From<KeyCode> for Key {
    fn from(c: KeyCode) -> Self {
        Self {
            mods: Empty,
            key: c,
        }
    }
}

impl fmt::Display for KeyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backspace => write!(f, "Backspace"),
            Self::Enter => write!(f, "Enter"),
            Self::Left => write!(f, "Left"),
            Self::Right => write!(f, "Right"),
            Self::Up => write!(f, "Up"),
            Self::Down => write!(f, "Down"),
            Self::Home => write!(f, "Home"),
            Self::End => write!(f, "End"),
            Self::PageUp => write!(f, "PageUp"),
            Self::PageDown => write!(f, "PageDown"),
            Self::Tab => write!(f, "Tab"),
            Self::BackTab => write!(f, "BackTab"),
            Self::Delete => write!(f, "Delete"),
            Self::Insert => write!(f, "Insert"),
            Self::Null => write!(f, "Null"),
            Self::Esc => write!(f, "Esc"),
            Self::CapsLock => write!(f, "CapsLock"),
            Self::ScrollLock => write!(f, "ScrollLock"),
            Self::NumLock => write!(f, "NumLock"),
            Self::PrintScreen => write!(f, "PrintScreen"),
            Self::Pause => write!(f, "Pause"),
            Self::Menu => write!(f, "Menu"),
            Self::KeypadBegin => write!(f, "KeypadBegin"),
            Self::F(n) => write!(f, "F{n}"),
            Self::Char(' ') => write!(f, "Space"),
            Self::Char(c) => write!(f, "{c}"),
            Self::Media(m) => write!(f, "Media({m:?})"),
            Self::Modifier(m) => write!(f, "Mod({m:?})"),
        }
    }
}

impl fmt::Display for Mods {
    /// Write the active modifiers as `Ctrl+Alt+Shift`, or nothing when none are
    /// set.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut separator = "";
        for (active, name) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
        ] {
            if active {
                write!(f, "{separator}{name}")?;
                separator = "+";
            }
        }
        Ok(())
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods == Empty {
            write!(f, "{}", self.key)
        } else {
            write!(f, "{}+{}", self.mods, self.key)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{error::Result, event::key::*};

    #[test]
    fn normalize() -> Result<()> {
        assert_eq!((Shift + 'A').normalize(), 'A',);
        assert_eq!((Shift + 'a').normalize(), 'A',);
        assert_eq!((Shift + ')').normalize(), ')',);
        assert_eq!((Shift + ' ').normalize(), Shift + ' ');
        assert_eq!((Shift + KeyCode::Enter).normalize(), Shift + KeyCode::Enter);

        assert_eq!((Shift + Alt + 'A').normalize(), Alt + 'A',);
        assert_eq!((Ctrl + '\u{1}').normalize(), Ctrl + 'A');
        assert_eq!((Ctrl + '\u{1A}').normalize(), Ctrl + 'Z');
        assert_eq!((Ctrl + '\u{1B}').normalize(), Ctrl + '[');
        assert_eq!((Ctrl + '\u{1C}').normalize(), Ctrl + '\\');
        assert_eq!((Ctrl + '\u{1D}').normalize(), Ctrl + ']');
        assert_eq!((Ctrl + '\u{1E}').normalize(), Ctrl + '^');
        assert_eq!((Ctrl + '\u{1F}').normalize(), Ctrl + '/');
        assert_eq!((Ctrl + '_').normalize(), Ctrl + '/');
        assert_eq!((Ctrl + '?').normalize(), Ctrl + '/');
        assert_eq!((Ctrl + '\u{7F}').normalize(), Ctrl + '/');
        assert_eq!((Ctrl + '4').normalize(), Ctrl + '\\');
        assert_eq!((Ctrl + '5').normalize(), Ctrl + ']');
        assert_eq!((Ctrl + '6').normalize(), Ctrl + '^');
        assert_eq!((Ctrl + '7').normalize(), Ctrl + '/');
        assert_eq!(Key::from('\u{1}').normalize(), Ctrl + 'A');
        assert_eq!(Key::from('\u{1F}').normalize(), Ctrl + '/');
        assert_eq!(Key::from('\u{7F}').normalize(), Ctrl + '/');
        Ok(())
    }

    #[test]
    fn parse_specs() -> Result<()> {
        assert_eq!(Key::parse_spec("ctrl-s"), Ok(Ctrl + 's'));
        assert_eq!(Key::parse_spec("PageDown"), Ok(KeyCode::PageDown.into()));
        assert_eq!(Key::parse_spec("ArrowUp"), Ok(KeyCode::Up.into()));
        assert_eq!(Key::parse_spec("A"), Ok('A'.into()));
        assert_eq!(Key::parse_spec("Space"), Ok(' '.into()));
        assert_eq!(Key::parse_spec("+"), Ok('+'.into()));
        assert_eq!(Key::parse_spec("-"), Ok('-'.into()));
        assert!(Key::parse_spec("ctrl-what").is_err());
        Ok(())
    }
}
