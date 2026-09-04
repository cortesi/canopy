use std::fmt;

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

impl From<MouseEvent> for Mouse {
    fn from(o: MouseEvent) -> Self {
        Self {
            action: o.action,
            modifiers: o.modifiers,
            button: o.button,
        }
    }
}

impl Mouse {
    /// Parse a mouse specification such as `ScrollUp` or `ctrl-LeftDown`.
    pub fn parse_spec(spec: &str) -> Result<Self, String> {
        let (modifiers, body) = key::parse_spec_parts(spec, &['-'])?;

        let lower = body.to_ascii_lowercase();
        let action = [
            ("scrollright", Action::ScrollRight),
            ("scrollleft", Action::ScrollLeft),
            ("scrolldown", Action::ScrollDown),
            ("scrollup", Action::ScrollUp),
            ("moved", Action::Moved),
            ("drag", Action::Drag),
            ("down", Action::Down),
            ("up", Action::Up),
        ]
        .into_iter()
        .find_map(|(suffix, action)| lower.ends_with(suffix).then_some((suffix, action)))
        .ok_or_else(|| format!("unknown mouse action: {spec}"))?;

        let button = match &body[..body.len() - action.0.len()] {
            "" => {
                if action.1.is_button() {
                    Button::Left
                } else {
                    Button::None
                }
            }
            prefix if prefix.eq_ignore_ascii_case("left") => Button::Left,
            prefix if prefix.eq_ignore_ascii_case("right") => Button::Right,
            prefix if prefix.eq_ignore_ascii_case("middle") => Button::Middle,
            other => return Err(format!("unknown mouse button: {other}")),
        };

        Ok(Self {
            action: action.1,
            button,
            modifiers,
        })
    }
}

impl fmt::Display for Mouse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers != key::Empty {
            write!(f, "{}+", self.modifiers)?;
        }
        if matches!(self.button, Button::None) {
            write!(f, "{:?}", self.action)
        } else {
            write!(f, "{:?} {:?}", self.button, self.action)
        }
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
    /// Cursor location in screen coordinates for incoming events, and relative
    /// to the node's content origin for events delivered to widgets.
    /// Coordinates before the content origin saturate to zero, including
    /// captured events and events in padding, so the conversion is not
    /// always reversible.
    pub location: Point,
}

#[cfg(test)]
mod tests {
    use crate::{error::Result, event::mouse::*};

    fn spec(action: Action, button: Button, modifiers: key::Mods) -> Mouse {
        Mouse {
            action,
            button,
            modifiers,
        }
    }

    #[test]
    fn mouse_event_converts_to_a_spec() {
        let event = MouseEvent {
            action: Action::Drag,
            button: Button::Middle,
            modifiers: key::Shift,
            location: Point { x: 3, y: 4 },
        };
        assert_eq!(
            Mouse::from(event),
            spec(Action::Drag, Button::Middle, key::Shift)
        );
    }

    #[test]
    fn parse_specs() -> Result<()> {
        assert_eq!(
            Mouse::parse_spec("ScrollUp"),
            Ok(spec(Action::ScrollUp, Button::None, key::Empty))
        );
        assert_eq!(
            Mouse::parse_spec("ctrl-LeftDown"),
            Ok(spec(Action::Down, Button::Left, key::Ctrl))
        );
        assert_eq!(
            Mouse::parse_spec("shift-MiddleDrag"),
            Ok(spec(Action::Drag, Button::Middle, key::Shift))
        );
        assert!(Mouse::parse_spec("ctrl-nope").is_err());
        Ok(())
    }
}
