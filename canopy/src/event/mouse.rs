use crate::{event::key, geom::Point};
use std::ops::Add;

/// An abstract specification for a mouse action
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Mouse {
    pub action: Action,
    pub button: Button,
    pub modifiers: key::Mods,
}

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Button {
    Left,
    Right,
    Middle,
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

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Action {
    Down,
    Up,
    Drag,
    Moved,
    ScrollDown,
    ScrollUp,
    ScrollLeft,
    ScrollRight,
}

impl Action {
    /// Is this a button-driven action?
    pub fn is_button(&self) -> bool {
        match self {
            Action::Down => true,
            Action::Up => true,
            Action::Drag => true,
            Action::Moved => false,
            Action::ScrollUp => false,
            Action::ScrollDown => false,
            Action::ScrollLeft => false,
            Action::ScrollRight => false,
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
        Mouse {
            action: o.action,
            modifiers: o.modifiers,
            button: o.button,
        }
    }
}

impl From<Button> for Mouse {
    fn from(e: Button) -> Self {
        Mouse {
            action: Action::Down,
            modifiers: key::Empty,
            button: e,
        }
    }
}

impl From<Action> for Mouse {
    fn from(e: Action) -> Self {
        Mouse {
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

impl std::cmp::PartialEq<Button> for Mouse {
    fn eq(&self, k: &Button) -> bool {
        let m: Mouse = (*k).into();
        *self == m
    }
}

impl std::cmp::PartialEq<Action> for Mouse {
    fn eq(&self, k: &Action) -> bool {
        let m: Mouse = (*k).into();
        *self == m
    }
}

impl Add<Button> for Mouse {
    type Output = Mouse;

    fn add(self, other: Button) -> Self::Output {
        let mut r = self;
        r.button = other;
        r
    }
}

impl Add<Action> for Mouse {
    type Output = Mouse;

    fn add(self, other: Action) -> Self::Output {
        let mut r = self;
        r.action = other;
        r
    }
}

impl Add<key::Mods> for Mouse {
    type Output = Mouse;

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
    pub action: Action,
    pub button: Button,
    pub modifiers: key::Mods,
    pub location: Point,
}

impl std::cmp::PartialEq<Mouse> for MouseEvent {
    fn eq(&self, o: &Mouse) -> bool {
        self.action == o.action && self.button == o.button && self.modifiers == o.modifiers
    }
}

impl std::cmp::PartialEq<Mouse> for &MouseEvent {
    fn eq(&self, o: &Mouse) -> bool {
        self.action == o.action && self.button == o.button && self.modifiers == o.modifiers
    }
}

impl std::cmp::PartialEq<Action> for MouseEvent {
    fn eq(&self, o: &Action) -> bool {
        let m: Mouse = (*o).into();
        self == m
    }
}

#[cfg(test)]
mod tests {
    use crate::error::Result;
    use crate::event::mouse::*;

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
