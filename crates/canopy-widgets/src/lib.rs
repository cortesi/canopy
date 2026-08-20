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
/// Dropdown selection widget.
mod dropdown;
/// Experimental editor API with syntax highlighting and vi mode.
pub mod editor;
/// ASCII font rasterization helpers.
mod font;
/// Banner widget that renders ASCII fonts.
mod font_banner;
/// Scrollable frame container.
mod frame;
/// Contextual key-binding help widgets.
mod help;
/// Image rendering widget.
mod image_view;
/// Text input widget.
mod input;
/// Experimental inspector overlay internals.
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
mod terminal;
/// Multiline text widget.
mod text;
/// Vertical stack container.
mod vstack;
/// Wrapping an existing node in a container widget.
mod wrap;

pub use boxed::{Border, BoxGlyphs, DOUBLE, ROUND, SINGLE, SINGLE_THICK};
pub use button::Button;
pub use center::Center;
pub use dropdown::Dropdown;
/// Experimental ASCII font rendering API.
pub use font::{Font, FontEffects, FontRenderer, LayoutOptions};
pub use font_banner::FontBanner;
pub use frame::Frame;
pub use image_view::ImageView;
pub use input::Input;
pub use label::Label;
pub use list::{List, Selectable};
pub use pad::Pad;
pub use panes::Panes;
pub use root::Root;
pub use selector::Selector;
pub use terminal::{Terminal, TerminalConfig};
pub use text::{CanvasWidth, Text};
pub use vstack::VStack;

#[cfg(test)]
mod render_tests;
