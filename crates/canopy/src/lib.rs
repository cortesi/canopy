// The canopy library.

pub mod core;
pub mod geom;
pub mod widgets;

// Re-export core types to the root for convenience, matching previous behavior.
pub use core::*;
pub use widgets::Root;

// Allow internal references to `canopy::` to resolve to this crate.
// This is often needed for proc-macros that assume the crate is named `canopy`.
use crate as canopy;
