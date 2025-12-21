//! Built-in widgets for canopy applications.

// Re-export the trait for derive macros
/// Editor widget.
pub mod editor;
/// Input helpers.
mod input;
/// Pane grid layout widget.
mod panes;
/// Text widget.
mod text;

/// Frame widget.
pub mod frame;
/// Inspector widget.
pub mod inspector;
/// List widget.
pub mod list;
/// Root widget.
mod root;
/// Tabs widget.
pub mod tabs;

pub use editor::Editor;
pub use input::Input;
pub use panes::Panes;
pub use root::Root;
pub use text::Text;
