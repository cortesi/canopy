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

pub use buffer::{LineChange, TextBuffer};
pub use position::{TextPosition, TextRange};
pub use selection::Selection;
pub use util::single_line;
