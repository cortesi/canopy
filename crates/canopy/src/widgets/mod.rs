//! Built-in widgets for canopy applications.

// Re-export the trait for derive macros
/// Editor widget.
pub mod editor;
/// Input helpers.
pub(crate) mod input;
/// Pane grid layout widget.
pub(crate) mod panes;
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

pub use editor::Editor;
pub use input::Input;
pub use panes::Panes;
pub use root::Root;
pub use text::Text;
