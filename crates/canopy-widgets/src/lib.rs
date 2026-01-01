//! Built-in widgets for canopy applications.
//!
//! This crate provides a collection of reusable widgets for building terminal
//! user interfaces with canopy.

#![warn(missing_docs)]

/// Box border widget with customizable glyphs.
mod boxed;
/// Button widget with command dispatch.
mod button;
/// Content centering container.
mod center;
/// Dropdown selection widget.
mod dropdown;
/// Editor widget with syntax highlighting and vi mode.
pub mod editor;
/// Scrollable frame container.
mod frame;
/// Image rendering widget.
mod image_view;
/// Text input widget.
mod input;
/// Inspector overlay widget.
pub mod inspector;
/// Typed list container with selection.
mod list;
/// Modal overlay container.
mod modal;
/// 2D grid layout of panes.
mod panes;
/// Application root widget.
mod root;
/// Selection widget.
mod selector;
/// Tab container widget.
pub mod tabs;
/// Terminal emulation widget.
mod terminal;
/// Multiline text widget.
mod text;
/// Vertical stack container.
mod vstack;

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
