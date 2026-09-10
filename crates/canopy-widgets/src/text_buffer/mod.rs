//! Feature-independent rope storage, editing, and selection helpers.

/// Rope-backed storage and edit history.
mod buffer;
/// Undo and redo operations.
mod edit;
/// Logical text coordinates.
mod position;
/// Text selection state.
mod selection;
/// Grapheme and display-width helpers.
mod util;

pub(crate) use buffer::LineChange;
pub use buffer::{TextBuffer, TextTransaction};
pub use position::{TextPosition, TextRange};
pub use selection::Selection;
pub(crate) use util::{display_width, single_line};
