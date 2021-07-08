use crate::event::key;
use crate::geom::Point;
use crossterm::event::{self as cevent};
use std::ops::Add;

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy)]
pub enum Button {
    Left,
    Right,
    Middle,
}

impl Add<key::Mods> for Button {
    type Output = Mouse;

    fn add(self, other: key::Mods) -> Self::Output {
        Mouse {
            action: None,
            button: Some(self),
            modifiers: Some(other),
            loc: Point { x: 0, y: 0 },
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

impl From<cevent::MouseButton> for Button {
    fn from(e: cevent::MouseButton) -> Self {
        match e {
            cevent::MouseButton::Left => Button::Left,
            cevent::MouseButton::Right => Button::Right,
            cevent::MouseButton::Middle => Button::Middle,
        }
    }
}

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy)]
pub enum Action {
    Down,
    Up,
    Drag,
    Moved,
    ScrollDown,
    ScrollUp,
}

impl Add<key::Mods> for Action {
    type Output = Mouse;

    fn add(self, other: key::Mods) -> Self::Output {
        Mouse {
            action: Some(self),
            button: None,
            modifiers: Some(other),
            loc: Point { x: 0, y: 0 },
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
            action: Some(self),
            button: Some(other),
            modifiers: None,
            loc: Point { x: 0, y: 0 },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Mouse {
    pub action: Option<Action>,
    pub button: Option<Button>,
    pub modifiers: Option<key::Mods>,
    pub loc: Point,
}

impl From<cevent::MouseEvent> for Mouse {
    fn from(e: cevent::MouseEvent) -> Self {
        let mut button: Option<Button> = None;
        let action = match e.kind {
            cevent::MouseEventKind::Down(b) => {
                button = Some(b.into());
                Action::Down
            }
            cevent::MouseEventKind::Up(b) => {
                button = Some(b.into());
                Action::Up
            }
            cevent::MouseEventKind::Drag(b) => {
                button = Some(b.into());
                Action::Drag
            }
            cevent::MouseEventKind::Moved => Action::Moved,
            cevent::MouseEventKind::ScrollDown => Action::ScrollDown,
            cevent::MouseEventKind::ScrollUp => Action::ScrollUp,
        };
        Mouse {
            button,
            action: Some(action),
            loc: Point {
                x: e.column,
                y: e.row,
            },
            modifiers: Some(e.modifiers.into()),
        }
    }
}

impl std::cmp::PartialEq<Mouse> for Mouse {
    fn eq(&self, other: &Mouse) -> bool {
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
        if let (Some(m1), Some(m2)) = (self.modifiers, other.modifiers) {
            if m1 != m2 {
                return false;
            }
        }
        true
    }
}

impl From<Button> for Mouse {
    fn from(e: Button) -> Self {
        Mouse {
            action: None,
            modifiers: None,
            button: Some(e),
            loc: Point { x: 0, y: 0 },
        }
    }
}

impl From<Action> for Mouse {
    fn from(e: Action) -> Self {
        Mouse {
            action: Some(e),
            modifiers: None,
            button: None,
            loc: Point { x: 0, y: 0 },
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

impl Add<Mouse> for Mouse {
    type Output = Mouse;

    fn add(self, other: Mouse) -> Self::Output {
        let mut r = self;
        if let Some(b) = other.button {
            r.button = Some(b);
        }
        if let Some(a) = other.action {
            r.action = Some(a);
        }
        if let Some(m) = other.modifiers {
            r.modifiers = Some(m);
        }
        r
    }
}

impl Add<Button> for Mouse {
    type Output = Mouse;

    fn add(self, other: Button) -> Self::Output {
        let mut r = self;
        r.button = Some(other);
        r
    }
}

impl Add<Action> for Mouse {
    type Output = Mouse;

    fn add(self, other: Action) -> Self::Output {
        let mut r = self;
        r.action = Some(other);
        r
    }
}

impl Add<key::Mods> for Mouse {
    type Output = Mouse;

    fn add(self, other: key::Mods) -> Self::Output {
        let mut r = self;
        r.modifiers = Some(other);
        r
    }
}

#[cfg(test)]
mod tests {
    use crate::error::Error;
    use crate::event::mouse::*;

    #[test]
    fn tmouse() -> Result<(), Error> {
        assert!(
            Mouse {
                button: Some(Button::Left),
                action: Some(Action::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: None,
            } == Button::Left
        );
        assert!(
            Mouse {
                button: Some(Button::Left),
                action: Some(Action::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: None,
            } == Button::Left + Action::Down
        );
        assert!(
            Mouse {
                button: Some(Button::Left),
                action: Some(Action::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: None,
            } == Action::Down
        );
        assert!(
            Mouse {
                button: Some(Button::Left),
                action: Some(Action::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: None,
            } != Action::Down + Button::Right
        );
        assert!(
            Mouse {
                button: Some(Button::Left),
                action: Some(Action::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: None,
            } != Button::Right
        );
        assert!(
            Mouse {
                button: Some(Button::Left),
                action: Some(Action::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: None,
            } != key::Alt + Button::Right
        );
        assert!(
            Mouse {
                button: Some(Button::Left),
                action: Some(Action::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: Some(key::Alt),
            } == key::Alt + Button::Left
        );
        assert!(
            Mouse {
                button: Some(Button::Left),
                action: Some(Action::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: Some(key::Alt),
            } == key::Alt + Action::Down + Button::Left
        );
        assert!(
            Mouse {
                button: Some(Button::Left),
                action: Some(Action::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: Some(key::Alt),
            } != key::Alt + Action::Up + Button::Left
        );
        assert!(
            Mouse {
                button: Some(Button::Left),
                action: Some(Action::Down),
                modifiers: Some(key::Alt),
                loc: Point { x: 0, y: 0 }
            } == Action::Down
        );
        Ok(())
    }
}
