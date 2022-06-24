use crate::{event::key, geom::Point};
use std::ops::Add;

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy)]
pub enum Button {
    Left,
    Right,
    Middle,
}

impl Add<key::Mods> for Button {
    type Output = MouseEvent;

    fn add(self, other: key::Mods) -> Self::Output {
        MouseEvent {
            action: None,
            button: Some(self),
            modifiers: other,
            loc: Point { x: 0, y: 0 },
        }
    }
}

impl Add<Button> for key::Mods {
    type Output = MouseEvent;

    fn add(self, other: Button) -> Self::Output {
        other + self
    }
}

impl Add<MouseAction> for Button {
    type Output = MouseEvent;

    fn add(self, other: MouseAction) -> Self::Output {
        other + self
    }
}

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy)]
pub enum MouseAction {
    Down,
    Up,
    Drag,
    Moved,
    ScrollDown,
    ScrollUp,
}

impl Add<key::Mods> for MouseAction {
    type Output = MouseEvent;

    fn add(self, other: key::Mods) -> Self::Output {
        MouseEvent {
            action: Some(self),
            button: None,
            modifiers: other,
            loc: Point { x: 0, y: 0 },
        }
    }
}

impl Add<MouseAction> for key::Mods {
    type Output = MouseEvent;

    fn add(self, other: MouseAction) -> Self::Output {
        other + self
    }
}

impl Add<Button> for MouseAction {
    type Output = MouseEvent;

    fn add(self, other: Button) -> Self::Output {
        MouseEvent {
            action: Some(self),
            button: Some(other),
            modifiers: key::Empty,
            loc: Point { x: 0, y: 0 },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    pub action: Option<MouseAction>,
    pub button: Option<Button>,
    pub modifiers: key::Mods,
    pub loc: Point,
}

impl std::cmp::PartialEq<MouseEvent> for MouseEvent {
    fn eq(&self, other: &MouseEvent) -> bool {
        if let (Some(b1), Some(b2)) = (self.button, other.button) {
            if b1 != b2 {
                return false;
            }
        }
        if let (Some(a1), Some(a2)) = (self.action, other.action) {
            if a1 != a2 {
                return false;
            }
        }
        if self.modifiers != other.modifiers {
            return false;
        }
        true
    }
}

impl From<Button> for MouseEvent {
    fn from(e: Button) -> Self {
        MouseEvent {
            action: None,
            modifiers: key::Empty,
            button: Some(e),
            loc: Point { x: 0, y: 0 },
        }
    }
}

impl From<MouseAction> for MouseEvent {
    fn from(e: MouseAction) -> Self {
        MouseEvent {
            action: Some(e),
            modifiers: key::Empty,
            button: None,
            loc: Point { x: 0, y: 0 },
        }
    }
}

impl std::cmp::PartialEq<Button> for MouseEvent {
    fn eq(&self, k: &Button) -> bool {
        let m: MouseEvent = (*k).into();
        *self == m
    }
}

impl std::cmp::PartialEq<MouseAction> for MouseEvent {
    fn eq(&self, k: &MouseAction) -> bool {
        let m: MouseEvent = (*k).into();
        *self == m
    }
}

impl Add<MouseEvent> for MouseEvent {
    type Output = MouseEvent;

    fn add(self, other: MouseEvent) -> Self::Output {
        let mut r = self;
        if let Some(b) = other.button {
            r.button = Some(b);
        }
        if let Some(a) = other.action {
            r.action = Some(a);
        }
        r.modifiers = other.modifiers;
        r
    }
}

impl Add<Button> for MouseEvent {
    type Output = MouseEvent;

    fn add(self, other: Button) -> Self::Output {
        let mut r = self;
        r.button = Some(other);
        r
    }
}

impl Add<MouseAction> for MouseEvent {
    type Output = MouseEvent;

    fn add(self, other: MouseAction) -> Self::Output {
        let mut r = self;
        r.action = Some(other);
        r
    }
}

impl Add<key::Mods> for MouseEvent {
    type Output = MouseEvent;

    fn add(self, other: key::Mods) -> Self::Output {
        let mut r = self;
        r.modifiers = other;
        r
    }
}

#[cfg(test)]
mod tests {
    use crate::error::Result;
    use crate::event::mouse::*;

    #[test]
    fn tmouse() -> Result<()> {
        assert_eq!(
            MouseEvent {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: key::Empty,
            },
            Button::Left
        );
        assert_eq!(
            MouseEvent {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: key::Empty,
            },
            Button::Left + MouseAction::Down
        );
        assert_eq!(
            MouseEvent {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: key::Empty,
            },
            MouseAction::Down
        );
        assert_ne!(
            MouseEvent {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: key::Empty,
            },
            MouseAction::Down + Button::Right
        );
        assert_ne!(
            MouseEvent {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: key::Empty,
            },
            Button::Right
        );
        assert_ne!(
            MouseEvent {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: key::Empty,
            },
            key::Alt + Button::Right
        );
        assert_eq!(
            MouseEvent {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: key::Alt,
            },
            key::Alt + Button::Left
        );
        assert_eq!(
            MouseEvent {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: key::Alt,
            },
            key::Alt + MouseAction::Down + Button::Left
        );
        assert_ne!(
            MouseEvent {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: key::Alt,
            },
            key::Alt + MouseAction::Up + Button::Left
        );
        assert_eq!(
            MouseEvent {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                modifiers: key::Empty,
                loc: Point { x: 0, y: 0 }
            },
            MouseAction::Down
        );
        Ok(())
    }
}
