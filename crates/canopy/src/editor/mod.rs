//! Text editing primitives used by widgets.

/// Text buffer implementation backed by a rope.
pub mod buffer;
/// Undo/redo edit definitions.
pub mod edit;
/// Text position and range types.
pub mod position;
/// Selection types and helpers.
pub mod selection;
/// Shared editor helpers.
pub mod util;

pub use buffer::{LineChange, TextBuffer};
pub use position::{TextPosition, TextRange};
pub use selection::Selection;
pub use util::tab_width;
