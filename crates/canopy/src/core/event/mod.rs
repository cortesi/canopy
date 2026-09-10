/// Keyboard event types.
pub mod key;
/// Mouse event types.
pub mod mouse;

use crate::geom::Size;

/// This enum represents all the event types that drive the application.
#[derive(Debug, Clone)]
pub enum Event {
    /// A keystroke
    Key(key::Key),
    /// A mouse action
    Mouse(mouse::MouseEvent),
    /// Terminal resize
    Resize(Size),
    /// Terminal has gained focus
    FocusGained,
    /// Terminal has lost focus
    FocusLost,
    /// Cut and paste
    Paste(String),
}
