/// Editor core logic and state façade.
pub mod core;
/// Editor node implementation details.
mod editor_impl;
/// Undo/redo effect definitions.
mod effect;
/// Editor cursor and line primitives.
mod primitives;
/// Editor state and internal utilities.
mod state;

pub use editor_impl::Editor;
pub use primitives::{CharPos, InsertPos, Pos, Window};
