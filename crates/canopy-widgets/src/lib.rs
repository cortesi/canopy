//! Built-in widgets for canopy applications.
//!
//! This crate provides a collection of reusable widgets for building terminal
//! user interfaces with canopy.

/// Border widget with customizable glyphs.
mod boxed;
/// Button widget with command dispatch.
mod button;
/// Content centering container.
mod center;
/// Multi-click tracker shared by editor and terminal.
#[cfg(any(feature = "editor", feature = "terminal-widget"))]
mod click;
/// Dropdown selection widget.
mod dropdown;
#[cfg(feature = "editor")]
pub mod editor;
/// ASCII font rasterization helpers.
#[cfg(feature = "graphics")]
pub mod font;
/// Banner widget that renders ASCII fonts.
#[cfg(feature = "graphics")]
mod font_banner;
/// Scrollable frame container.
mod frame;
/// Contextual key-binding help widgets.
mod help;
/// Image rendering widget.
#[cfg(feature = "graphics")]
mod image_view;
/// Text input widget.
mod input;
/// Experimental inspector overlay internals.
#[cfg(feature = "devtools")]
pub mod inspector;
mod label;
/// Typed list container with selection.
mod list;
/// Padding container widget.
mod pad;
/// 2D grid layout of panes.
mod panes;
/// Application root widget.
mod root;
/// Selection widget.
mod selector;
/// Terminal emulation widget.
#[cfg(feature = "terminal-widget")]
pub mod terminal;
/// Multiline text widget.
mod text;
/// Shared text editing machinery for Input and Editor.
pub mod text_buffer;
/// Vertical stack container.
mod vstack;
/// Wrapping an existing node in a container widget.
mod wrap;

pub use boxed::{Border, BoxGlyphs, DOUBLE, ROUND, SINGLE, SINGLE_THICK};
pub use button::Button;
pub use center::Center;
pub use dropdown::Dropdown;
pub use frame::Frame;
pub use input::{Input, ValueExposure};
pub use label::Label;
pub use list::{AutoKey, List, Selectable};
pub use pad::Pad;
pub use panes::Panes;
pub use root::Root;
pub use selector::Selector;
pub use text::{CanvasWidth, Text};
pub use vstack::VStack;
pub use wrap::wrap;

#[cfg(test)]
mod render_tests;
