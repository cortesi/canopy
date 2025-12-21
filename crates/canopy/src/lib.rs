//! Canopy: A terminal UI library.
// The canopy library.

pub mod core;
pub mod geom;
pub mod widgets;

// Re-export core types to the root for convenience, matching previous behavior.
pub use core::*;
pub use widgets::Root;
