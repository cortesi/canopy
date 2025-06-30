#![allow(clippy::new_without_default)]
//! A library for building terminal UIs.

// Re-export everything from canopy-core
pub use canopy_core::*;

// Modules that remain in canopy
mod canopy;
mod inputmap;
mod poll;
mod root;

pub mod backend;
mod binder;
pub mod inspector;
pub mod script;
// Re-export widgets from canopy-widgets
pub use canopy_widgets as widgets;

// Re-export canopy-specific types
pub use crate::canopy::{Canopy, Loader};
pub use binder::*;
pub use root::*;

// Hide the test utils from docs. We need to expose it for integration tests, but it's not for external use.
#[doc(hidden)]
pub mod tutils;
