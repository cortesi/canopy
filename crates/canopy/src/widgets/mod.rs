//! Built-in widgets for canopy applications.

// Re-export the trait for derive macros
/// Button widget.
pub(crate) mod button;
/// Center widget.
pub(crate) mod center;
/// Dropdown widget.
pub(crate) mod dropdown;
/// Editor widget.
pub mod editor;
/// Image view widget.
pub(crate) mod image_view;
/// Input helpers.
pub(crate) mod input;
/// Modal widget.
pub(crate) mod modal;
/// Pane grid layout widget.
pub(crate) mod panes;
/// Selector widget.
pub(crate) mod selector;
/// Text widget.
pub(crate) mod text;
/// Vertical stack widget.
pub(crate) mod vstack;

/// Box widget.
pub(crate) mod boxed;
/// Frame widget.
pub(crate) mod frame;
/// Inspector widget.
pub mod inspector;
/// List widget.
pub(crate) mod list;
/// Root widget.
pub(crate) mod root;
/// Tabs widget.
pub mod tabs;
/// Terminal widget.
pub(crate) mod terminal;

pub use boxed::{Box, BoxGlyphs, DOUBLE, ROUND, ROUND_THICK, SINGLE, SINGLE_THICK};
pub use button::Button;
pub use center::Center;
pub use dropdown::{Dropdown, DropdownItem};
pub use frame::{Frame, SCROLL, ScrollGlyphs};
pub use image_view::ImageView;
pub use input::Input;
pub use list::{List, ListActivateConfig, Selectable};
pub use modal::Modal;
pub use panes::Panes;
pub use root::Root;
pub use selector::{Selector, SelectorItem};
pub use terminal::{Terminal, TerminalColors, TerminalConfig};
pub use text::{CanvasWidth, Text};
pub use vstack::VStack;
