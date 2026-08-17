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
        let spec = spec.trim();
        if spec.is_empty() {
            return Err("mouse specification cannot be empty".into());
        }

        let parts = spec
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let Some((body, modifier_parts)) = parts.split_last() else {
            return Err("mouse specification cannot be empty".into());
        };

        let mut modifiers = key::Empty;
        for part in modifier_parts {
            if part.eq_ignore_ascii_case("ctrl") || part.eq_ignore_ascii_case("control") {
                modifiers.ctrl = true;
            } else if part.eq_ignore_ascii_case("alt") {
                modifiers.alt = true;
            } else if part.eq_ignore_ascii_case("shift") {
                modifiers.shift = true;
            } else {
                return Err(format!("unknown mouse modifier: {part}"));
            }
        }

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
    /// Cursor location in local coordinates relative to the node view. To map
    /// back to screen coordinates, add the node view's outer top-left.
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
