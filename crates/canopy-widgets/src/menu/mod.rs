//! A menu of keyed rows, chosen with the keyboard.
//!
//! Each row names one action: its key, a label, and a value at the right,
//! such as a setting's current state. In a selectable menu, the arrows, `j`
//! and `k`, the page keys, and Home and End move the selection, Enter or Space
//! activates the selected row, a row's key activates that row, and Esc or `q`
//! closes.
//! A menu without selection only lists its rows, as the menu of a transient
//! mode does while the mode waits for its key.
//!
//! [`Menu`](crate::Menu) is the modal: it centres a titled frame over
//! whatever it covers and holds a [`MenuList`](crate::MenuList) of rows. A
//! host opens it with Canopy's modal, as it opens a
//! [`Picker`](crate::Picker). A panel that keeps rows of its own, such as an
//! options panel with levels and inline edits, draws them with
//! [`render_row`] and the same styles, so every menu reads the same.
//!
//! The rows draw under the `menu` style layer: `row`, `row/key`,
//! `row/value`, `row/muted`, `row/mark`, `selection`, and `edit`.
//! [`style_rules`] derives them from the palette.

/// The menu widgets, and the drawing of their rows.
pub(crate) mod widget;

pub use widget::{RowLayout, render_row, style_rules};
