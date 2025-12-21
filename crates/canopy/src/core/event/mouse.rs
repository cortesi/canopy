use std::ops::Add;

use crate::{event::key, geom::Point};

/// An abstract specification for a mouse action.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Mouse {
    /// Mouse action type.
    pub action: Action,
    /// Mouse button.
    pub button: Button,
    /// Keyboard modifiers.
    pub modifiers: key::Mods,
}

/// Mouse button codes.
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Button {
    /// Left mouse button.
    Left,
    /// Right mouse button.
    Right,
    /// Middle mouse button.
    Middle,
    /// No button (for move/scroll).
    None,
}

/// Synthesize a Mouse specification - the action is assumed to be
/// `Action::Down`.
impl Add<key::Mods> for Button {
    type Output = Mouse;

    fn add(self, other: key::Mods) -> Self::Output {
        Mouse {
            action: Action::Down,
            button: self,
            modifiers: other,
        }
    }
}

impl Add<Button> for key::Mods {
    type Output = Mouse;

    fn add(self, other: Button) -> Self::Output {
        other + self
    }
}

impl Add<Action> for Button {
    type Output = Mouse;

    fn add(self, other: Action) -> Self::Output {
        other + self
    }
}

/// Mouse action kinds.
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Action {
    /// Button press.
    Down,
    /// Button release.
    Up,
    /// Mouse drag with button held.
    Drag,
    /// Mouse moved without button.
    Moved,
    /// Scroll wheel down.
    ScrollDown,
    /// Scroll wheel up.
    ScrollUp,
    /// Horizontal scroll left.
    ScrollLeft,
    /// Horizontal scroll right.
    ScrollRight,
}

impl Action {
    /// Is this a button-driven action?
    pub fn is_button(&self) -> bool {
        match self {
            Self::Down => true,
            Self::Up => true,
            Self::Drag => true,
            Self::Moved => false,
            Self::ScrollUp => false,
            Self::ScrollDown => false,
            Self::ScrollLeft => false,
            Self::ScrollRight => false,
        }
    }
}

/// Synthesize a `Mouse` input specification by adding modifiers to an action.
/// Assume that the button is `Button::None`.
impl Add<key::Mods> for Action {
    type Output = Mouse;

    fn add(self, other: key::Mods) -> Self::Output {
        Mouse {
            action: self,
            button: Button::None,
            modifiers: other,
        }
    }
}

impl Add<Action> for key::Mods {
    type Output = Mouse;

    fn add(self, other: Action) -> Self::Output {
        other + self
    }
}

impl Add<Button> for Action {
    type Output = Mouse;

    fn add(self, other: Button) -> Self::Output {
        Mouse {
            action: self,
            button: other,
            modifiers: key::Empty,
        }
    }
}

impl From<MouseEvent> for Mouse {
    fn from(o: MouseEvent) -> Self {
        Self {
            action: o.action,
            modifiers: o.modifiers,
            button: o.button,
        }
    }
}

impl From<Button> for Mouse {
    fn from(e: Button) -> Self {
        Self {
            action: Action::Down,
            modifiers: key::Empty,
            button: e,
        }
    }
}

impl From<Action> for Mouse {
    fn from(e: Action) -> Self {
        Self {
            action: e,
            modifiers: key::Empty,
            button: if e.is_button() {
                Button::Left
            } else {
                Button::None
            },
        }
    }
}

impl PartialEq<Button> for Mouse {
    fn eq(&self, k: &Button) -> bool {
        let m: Self = (*k).into();
        *self == m
    }
}

impl PartialEq<Action> for Mouse {
    fn eq(&self, k: &Action) -> bool {
        let m: Self = (*k).into();
        *self == m
    }
}

impl Add<Button> for Mouse {
    type Output = Self;

    fn add(self, other: Button) -> Self::Output {
        let mut r = self;
        r.button = other;
        r
    }
}

impl Add<Action> for Mouse {
    type Output = Self;

    fn add(self, other: Action) -> Self::Output {
        let mut r = self;
        r.action = other;
        r
    }
}

impl Add<key::Mods> for Mouse {
    type Output = Self;

    fn add(self, other: key::Mods) -> Self::Output {
        let mut r = self;
        r.modifiers = other;
        r
    }
}

/// A mouse input event. This has the same fields as the `Mouse` event
/// specification, but also includes a location.
#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    /// Mouse action type.
    pub action: Action,
    /// Mouse button.
    pub button: Button,
    /// Keyboard modifiers.
    pub modifiers: key::Mods,
    /// Cursor location in screen space.
    pub location: Point,
}

impl PartialEq<Mouse> for MouseEvent {
    fn eq(&self, o: &Mouse) -> bool {
        self.action == o.action && self.button == o.button && self.modifiers == o.modifiers
    }
}

impl PartialEq<Mouse> for &MouseEvent {
    fn eq(&self, o: &Mouse) -> bool {
        self.action == o.action && self.button == o.button && self.modifiers == o.modifiers
    }
}

impl PartialEq<Action> for MouseEvent {
    fn eq(&self, o: &Action) -> bool {
        let m: Mouse = (*o).into();
        self == m
    }
}

#[cfg(test)]
mod tests {
    use crate::{error::Result, event::mouse::*};

    #[test]
    fn tmouse() -> Result<()> {
        assert_eq!(
            Mouse {
                button: Button::Left,
                action: Action::Down,
                modifiers: key::Empty,
            },
            Button::Left
        );
        assert_eq!(
            Mouse {
                button: Button::Left,
                action: Action::Down,
                modifiers: key::Empty,
            },
            Button::Left + Action::Down
        );
        assert_eq!(
            Mouse {
                button: Button::Left,
                action: Action::Down,
                modifiers: key::Empty,
            },
            Action::Down
        );
        assert_ne!(
            Mouse {
                button: Button::Left,
                action: Action::Down,
                modifiers: key::Empty,
            },
            Action::Down + Button::Right
        );
        assert_ne!(
            Mouse {
                button: Button::Left,
                action: Action::Down,
                modifiers: key::Empty,
            },
            Button::Right
        );
        assert_ne!(
            Mouse {
                button: Button::Left,
                action: Action::Down,
                modifiers: key::Empty,
            },
            key::Alt + Button::Right
        );
        assert_eq!(
            Mouse {
                button: Button::Left,
                action: Action::Down,
                modifiers: key::Alt,
            },
            key::Alt + Button::Left
        );
        assert_eq!(
            Mouse {
                button: Button::Left,
                action: Action::Down,
                modifiers: key::Alt,
            },
            key::Alt + Action::Down + Button::Left
        );
        assert_ne!(
            Mouse {
                button: Button::Left,
                action: Action::Down,
                modifiers: key::Alt,
            },
            key::Alt + Action::Up + Button::Left
        );
        assert_eq!(
            Mouse {
                button: Button::Left,
                action: Action::Down,
                modifiers: key::Empty,
            },
            Action::Down
        );
        Ok(())
    }
}
