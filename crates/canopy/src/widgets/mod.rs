//! Built-in widgets for canopy applications.

// Re-export the trait for derive macros
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

pub use center::Center;
pub use dropdown::{Dropdown, DropdownItem};
pub use input::{Input, TextBuf};
pub use modal::Modal;
pub use panes::Panes;
pub use root::Root;
pub use selector::{Selector, SelectorItem};
pub use terminal::{Terminal, TerminalColors, TerminalConfig};
pub use text::Text;
