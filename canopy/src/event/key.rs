//! This module contains the core primitives to represent keyboard input.
use std::ops::Add;

#[derive(Default, Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
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

#[derive(Debug, PartialOrd, PartialEq, Hash, Eq, Clone, Copy)]
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
    Null,
    Esc,
    /// F key.
    ///
    /// `KeyEvent::F(1)` represents F1 key, etc.
    F(u8),
    /// A character.
    ///
    /// `KeyEvent::Char('c')` represents `c` character, etc.
    Char(char),
}

impl From<char> for KeyCode {
    fn from(c: char) -> Self {
        KeyCode::Char(c)
    }
}

const LEAVE_INTACT: &'static [KeyCode] = &[KeyCode::Enter, KeyCode::Char(' ')];

/// A keystroke along with modifiers.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Key {
    pub mods: Mods,
    pub key: KeyCode,
}

impl Key {
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
    ///     - If the key is one of a special class of characters that commonly
    ///       don't have a shift conversion (space, enter), leave shift intact
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
    /// `normalize` must be called explicitly when needed - all comparison and
    /// conversion methods are literal and stright-forward, and don't perform
    /// normalization automatically.
    pub fn normalize(&self) -> Key {
        if self.mods.shift {
            if let KeyCode::Char(c) = self.key {
                if c.is_ascii_lowercase() {
                    Key {
                        mods: Mods {
                            shift: false,
                            alt: self.mods.alt,
                            ctrl: self.mods.ctrl,
                        },
                        key: KeyCode::Char(c.to_ascii_uppercase()),
                    }
                } else if LEAVE_INTACT.contains(&self.key) {
                    self.clone()
                } else {
                    Key {
                        mods: Mods {
                            shift: false,
                            alt: self.mods.alt,
                            ctrl: self.mods.ctrl,
                        },
                        key: self.key,
                    }
                }
            } else {
                self.clone()
            }
        } else {
            self.clone()
        }
    }
}

impl std::cmp::PartialEq<KeyCode> for Key {
    fn eq(&self, c: &KeyCode) -> bool {
        // If there are modifiers, we never match.
        if self.mods != Empty {
            return false;
        }
        *c == self.key
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
        Key {
            mods: Empty,
            key: KeyCode::Char(c),
        }
    }
}

impl From<KeyCode> for Key {
    fn from(c: KeyCode) -> Self {
        Key {
            mods: Empty,
            key: c,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{event::key::*, Result};

    #[test]
    fn normalize() -> Result<()> {
        assert_eq!((Shift + 'A').normalize(), 'A',);
        assert_eq!((Shift + 'a').normalize(), 'A',);
        assert_eq!((Shift + ')').normalize(), ')',);
        assert_eq!((Shift + ' ').normalize(), Shift + ' ');
        assert_eq!((Shift + KeyCode::Enter).normalize(), Shift + KeyCode::Enter);

        assert_eq!((Shift + Alt + 'A').normalize(), Alt + 'A',);
        assert_eq!(
            Key {
                mods: Mods {
                    shift: false,
                    alt: false,
                    ctrl: false
                },
                key: KeyCode::Char('c')
            },
            Key {
                mods: Mods {
                    shift: false,
                    alt: false,
                    ctrl: false
                },
                key: KeyCode::Char('c')
            }
        );
        Ok(())
    }
}
