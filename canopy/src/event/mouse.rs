use crate::{event::key, geom::Point};
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

impl Add<MouseAction> for Button {
    type Output = Mouse;

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

impl Add<MouseAction> for key::Mods {
    type Output = Mouse;

    fn add(self, other: MouseAction) -> Self::Output {
        other + self
    }
}

impl Add<Button> for MouseAction {
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
    pub action: Option<MouseAction>,
    pub button: Option<Button>,
    pub modifiers: Option<key::Mods>,
    pub loc: Point,
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

impl From<MouseAction> for Mouse {
    fn from(e: MouseAction) -> Self {
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

impl std::cmp::PartialEq<MouseAction> for Mouse {
    fn eq(&self, k: &MouseAction) -> bool {
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

impl Add<MouseAction> for Mouse {
    type Output = Mouse;

    fn add(self, other: MouseAction) -> Self::Output {
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
    use crate::error::Result;
    use crate::event::mouse::*;

    #[test]
    fn tmouse() -> Result<()> {
        assert_eq!(
            Mouse {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: None,
            },
            Button::Left
        );
        assert_eq!(
            Mouse {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: None,
            },
            Button::Left + MouseAction::Down
        );
        assert_eq!(
            Mouse {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: None,
            },
            MouseAction::Down
        );
        assert_ne!(
            Mouse {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: None,
            },
            MouseAction::Down + Button::Right
        );
        assert_ne!(
            Mouse {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: None,
            },
            Button::Right
        );
        assert_ne!(
            Mouse {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: None,
            },
            key::Alt + Button::Right
        );
        assert_eq!(
            Mouse {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: Some(key::Alt),
            },
            key::Alt + Button::Left
        );
        assert_eq!(
            Mouse {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: Some(key::Alt),
            },
            key::Alt + MouseAction::Down + Button::Left
        );
        assert_ne!(
            Mouse {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                loc: Point { x: 0, y: 0 },
                modifiers: Some(key::Alt),
            },
            key::Alt + MouseAction::Up + Button::Left
        );
        assert_eq!(
            Mouse {
                button: Some(Button::Left),
                action: Some(MouseAction::Down),
                modifiers: Some(key::Alt),
                loc: Point { x: 0, y: 0 }
            },
            MouseAction::Down
        );
        Ok(())
    }
}
