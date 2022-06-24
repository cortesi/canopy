use std::ops::Add;

#[derive(Default, Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl Add<KeyCode> for Mods {
    type Output = Key;

    fn add(self, other: KeyCode) -> Self::Output {
        Key(self, other)
    }
}

impl Add<char> for Mods {
    type Output = Key;

    fn add(self, other: char) -> Self::Output {
        Key(self, other.into())
    }
}

impl Add<Mods> for Mods {
    type Output = Mods;

    fn add(self, other: Mods) -> Self::Output {
        Mods {
            shift: self.shift || other.shift,
            ctrl: self.ctrl || other.ctrl,
            alt: self.alt || other.alt,
        }
    }
}

#[allow(non_upper_case_globals)]
pub const Empty: Mods = Mods {
    shift: false,
    ctrl: false,
    alt: false,
};

#[allow(non_upper_case_globals)]
pub const Shift: Mods = Mods {
    shift: true,
    ctrl: false,
    alt: false,
};

#[allow(non_upper_case_globals)]
pub const Ctrl: Mods = Mods {
    shift: false,
    ctrl: true,
    alt: false,
};

#[allow(non_upper_case_globals)]
pub const Alt: Mods = Mods {
    shift: false,
    ctrl: false,
    alt: true,
};

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum KeyCode {
    Backspace,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    /// Shift + Tab key.
    BackTab,
    Delete,
    Insert,
    /// F key.
    ///
    /// `KeyEvent::F(1)` represents F1 key, etc.
    F(u8),
    /// A character.
    ///
    /// `KeyEvent::Char('c')` represents `c` character, etc.
    Char(char),
    Null,
    Esc,
}

impl KeyCode {
    fn upper(&self) -> Self {
        if let KeyCode::Char(c) = self {
            KeyCode::Char(c.to_ascii_uppercase())
        } else {
            *self
        }
    }
}

impl From<char> for KeyCode {
    fn from(c: char) -> Self {
        KeyCode::Char(c)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Key(pub Mods, pub KeyCode);

impl std::cmp::PartialEq<KeyCode> for Key {
    fn eq(&self, c: &KeyCode) -> bool {
        let mut shift = false;
        if self.0 != Empty && self.0 != Shift {
            return false;
        }
        if self.0 == Shift {
            shift = true
        }
        let sc = if shift { self.1.upper() } else { self.1 };
        *c == sc
    }
}

impl std::cmp::PartialEq<char> for Key {
    fn eq(&self, c: &char) -> bool {
        *self == KeyCode::Char(*c)
    }
}

impl std::cmp::PartialEq<Key> for char {
    fn eq(&self, k: &Key) -> bool {
        *k == KeyCode::Char(*self)
    }
}

impl From<char> for Key {
    fn from(c: char) -> Self {
        let modifiers = if c.is_uppercase() { Shift } else { Empty };
        // FIXME: For the moment, we panic if we get a char that has a complex
        // lowercase conversion. We should do something better here.
        Key(modifiers, KeyCode::Char(c.to_lowercase().next().unwrap()))
    }
}

impl From<KeyCode> for Key {
    fn from(c: KeyCode) -> Self {
        Key(Empty, c)
    }
}

#[cfg(test)]
mod tests {
    use crate::{event::key::*, Result};

    #[test]
    fn tkey() -> Result<()> {
        assert_eq!(Shift + 'c', Key(Shift, KeyCode::Char('c')));
        assert!(Alt + 'c' != Shift + 'c');
        assert!('c' != Shift + 'c');
        assert!('C' == Shift + 'C');
        assert!('C' == 'C');
        assert_eq!(Shift + Alt + 'c', Key(Shift + Alt, KeyCode::Char('c')));
        assert!(Key(Empty, KeyCode::Char('c')) == 'c');
        assert!('c' == Key(Empty, KeyCode::Char('c')));

        match Shift + 'c' {
            x if x == Shift + 'c' => println!("matched"),
            _ => println!("none"),
        }
        Ok(())
    }
}
