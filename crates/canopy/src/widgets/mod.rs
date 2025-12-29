//! Built-in widgets for canopy applications.

// Re-export the trait for derive macros
/// Button widget.
pub mod button;
/// Center widget.
pub mod center;
/// Dropdown widget.
pub mod dropdown;
/// Editor widget.
pub mod editor;
/// Input helpers.
pub(crate) mod input;
/// Modal widget.
pub mod modal;
/// Pane grid layout widget.
pub(crate) mod panes;
/// Selector widget.
pub mod selector;
/// Text widget.
pub(crate) mod text;
/// Vertical stack widget.
pub(crate) mod vstack;

/// Box widget.
pub mod boxed;
/// Frame widget.
pub mod frame;
/// Inspector widget.
pub mod inspector;
/// List widget.
pub mod list;
/// Root widget.
pub(crate) mod root;
/// Tabs widget.
pub mod tabs;
/// Terminal widget.
pub mod terminal;

pub use boxed::{Box, BoxGlyphs};
pub use button::Button;
pub use center::Center;
pub use dropdown::{Dropdown, DropdownItem};
pub use input::{Input, TextBuf};
pub use list::{List, Selectable};
pub use modal::Modal;
pub use panes::Panes;
pub use root::Root;
pub use selector::{Selector, SelectorItem};
pub use terminal::{Terminal, TerminalColors, TerminalConfig};
pub use text::{CanvasWidth, Text};
pub use vstack::VStack;
