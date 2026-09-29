//! Built-in widgets for canopy applications.
//!
//! This crate provides a collection of reusable widgets for building terminal
//! user interfaces with canopy.

/// Large text in a built-in pixel font.
mod big_text;
/// Border widget with customizable glyphs.
mod border;
/// Button widget with command dispatch.
mod button;
/// Chart primitives: scales, stacked bars and columns, and a braille canvas.
pub mod chart;
/// Multi-click tracker shared by editor and terminal.
mod click;
/// Stacked columns above and below an axis, with a cursor.
mod column_chart;
/// Panes side by side with scroll position dividers.
mod columns;
/// Modal yes or no question.
mod confirm;
/// Layout-only container.
mod container;
/// Framed panel centred over the view.
mod dialog;
/// Line diffs of two full texts, and DiffView's supporting types.
pub mod diff;
/// Diff view widget over the line-diff row model.
mod diff_view;
/// Dropdown selection widget.
mod dropdown;
/// Multiline text editor.
pub mod editor;
/// ASCII font rasterization helpers.
#[cfg(feature = "graphics")]
pub mod font;
/// Banner widget that renders ASCII fonts.
#[cfg(feature = "graphics")]
mod font_banner;
/// Scrollable frame container.
mod frame;
/// Fuzzy label ranking for lists that jump or filter as the operator types.
pub mod fuzzy;
/// Contextual key-binding help widgets.
mod help;
/// Syntax highlighting shared by the Editor and DiffView.
pub mod highlight;
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
mod list_view;
/// One-row gauge over a track.
mod meter;
/// Modal filtered list of items.
mod picker;
/// Regular expressions that match as ripgrep does, and their ranges in text.
pub mod regex_search;
/// Application root widget.
mod root;
/// Shared row cursor and label helpers for Selector, Dropdown, and PickerList.
mod row_cursor;
/// Shared line painter for Editor and DiffView.
mod run_paint;
/// Scrolling container.
mod scroll;
pub mod scrollbar;
/// One-row search field with what the search found.
mod search_bar;
/// One row under search results that says what the search found.
mod search_progress;
/// Selection widget.
mod selector;
/// Small series of values as bars or a braille line.
mod sparkline;
/// Frames for a busy indicator.
mod spinner;
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
mod text_buffer;

pub use big_text::BigText;
pub use border::{Border, BoxGlyphs};
pub use button::Button;
pub use column_chart::ColumnChart;
pub use columns::Columns;
pub use confirm::{Answer, Confirm, ConfirmRequest};
pub use container::Container;
pub use dialog::Dialog;
pub use diff_view::DiffView;
pub use dropdown::Dropdown;
#[cfg(feature = "graphics")]
pub use font_banner::FontBanner;
pub use frame::Frame;
#[cfg(feature = "graphics")]
pub use image_view::ImageView;
pub use input::{CLEAR_INTENT, Input, ValueExposure, register_clear_intent};
pub use keyed::KeyedChildren;
pub use label::ItemLabel;
pub use list_view::{List, Selectable};
pub use meter::Meter;
pub use picker::{Picker, PickerList};
pub use root::Root;
pub use scroll::Scroll;
pub use search_bar::SearchBar;
pub use search_progress::SearchProgress;
pub use selector::Selector;
pub use sparkline::Sparkline;
pub use spinner::Spinner;
pub use status_bar::{KeyHint, StatusBar};
pub use tabs::Tabs;
pub use text::{CanvasWidth, Text};

/// List's supporting types.
pub mod list {
    pub use crate::list_view::AutoKey;
}

#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod scrolling_tests;
