use std::fmt;

use crate::{error::ParseError, event::key, geom::PointI32};

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

/// Cells one wheel step scrolls.
const WHEEL_STEP: i32 = 3;

impl Action {
    /// Return the scroll offset one wheel step requests, or `None` for
    /// actions that do not scroll.
    ///
    /// Positive values move the view down or right.
    pub fn scroll_delta(self) -> Option<PointI32> {
        let (x, y) = match self {
            Self::ScrollUp => (0, -WHEEL_STEP),
            Self::ScrollDown => (0, WHEEL_STEP),
            Self::ScrollLeft => (-WHEEL_STEP, 0),
            Self::ScrollRight => (WHEEL_STEP, 0),
            Self::Down | Self::Up | Self::Drag | Self::Moved => return None,
        };
        Some(PointI32 { x, y })
    }

    /// Is this a button-driven action?
    fn is_button(&self) -> bool {
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
    pub fn parse_spec(spec: &str) -> Result<Self, ParseError> {
        let (modifiers, body) = key::parse_spec_parts(spec, &['-']).map_err(ParseError::new)?;

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
        .ok_or_else(|| ParseError::new(format!("unknown mouse action: {spec}")))?;

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
            other => return Err(ParseError::new(format!("unknown mouse button: {other}"))),
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
    /// Cursor location.
    ///
    /// Incoming events carry screen coordinates. Events delivered to a widget
    /// are relative to its content origin, before scroll. The location is
    /// signed: a point in padding above or left of the content is negative, and
    /// a captured event may lie anywhere. Use [`View::outer_point`],
    /// [`View::viewport_point`], or [`View::content_point`] to find the cell
    /// under the pointer in the space the widget paints.
    ///
    /// [`View::outer_point`]: crate::View::outer_point
    /// [`View::viewport_point`]: crate::View::viewport_point
    /// [`View::content_point`]: crate::View::content_point
    pub location: PointI32,
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
            location: PointI32 { x: 3, y: 4 },
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
