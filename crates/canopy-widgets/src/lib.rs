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
mod click;
/// Panes side by side with scroll position dividers.
mod columns;
/// Modal yes or no question.
mod confirm;
/// Layout-only container.
mod container;
/// Line diffs of two full texts.
mod diff;
/// Diff view widget over the line-diff row model.
mod diff_view;
/// Dropdown selection widget.
mod dropdown;
/// Multiline text editor and syntax highlighting.
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
mod inspector;
/// Keyed child reconciler used by List.
mod keyed;
mod label;
/// Typed list container with selection.
mod list;
/// Padding container widget.
mod pad;
/// Modal filtered list of items.
mod picker;
/// Application root widget.
mod root;
/// Shared row cursor and label helpers for Selector, Dropdown, and PickerList.
mod row_cursor;
/// Shared line painter for Editor and DiffView.
mod run_paint;
/// Scrolling container.
mod scroll;
pub mod scrollbar;
/// Selection widget.
mod selector;
/// Single-line status bar.
mod status_bar;
/// Tabbed pages beneath a tab bar.
mod tabs;
/// Terminal emulation widget.
#[cfg(feature = "terminal-widget")]
pub mod terminal;
/// Multiline text widget.
mod text;
/// Shared text editing machinery for Input and Editor.
pub mod text_buffer;
/// Wrapping an existing node in a container widget.
mod wrap;

pub use boxed::{Border, BoxGlyphs, DOUBLE, ROUND, SINGLE, SINGLE_THICK};
pub use button::Button;
pub use center::Center;
pub use columns::Columns;
pub use confirm::{Answer, Confirm};
pub use container::Container;
pub use diff::{Diff, DiffRow, Scope};
pub use diff_view::{DiffView, PreparedDiff, Strategy};
pub use dropdown::Dropdown;
pub use frame::Frame;
#[cfg(feature = "graphics")]
pub use image_view::ImageView;
pub use input::{CLEAR_INTENT, Input, ValueExposure, register_clear_intent};
pub use keyed::KeyedChildren;
pub use label::Label;
pub use list::{AutoKey, List, Selectable};
pub use pad::Pad;
pub use picker::{Picker, PickerFilter, PickerList, Truncate};
pub use root::Root;
pub use scroll::Scroll;
pub use scrollbar::{Scrollbar, ScrollbarGlyphs, THIN};
pub use selector::Selector;
pub use status_bar::{KeyHint, StatusBar};
pub use tabs::Tabs;
pub use text::{CanvasWidth, Text};
pub use wrap::wrap;

#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod scrolling_tests;
