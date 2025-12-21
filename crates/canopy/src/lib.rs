//! Canopy: A terminal UI library.
//!
//! Canopy is a terminal UI library for building interactive applications.
//! It provides a tree-based widget system with focus management, styling,
//! and event handling.
//!
//! # Quick Start
//!
//! The main entry points are:
//! - [`Canopy`] - The core application state
//! - [`Node`] - The trait implemented by all widgets
//! - [`Root`] - The recommended root widget for applications
//!
//! # Module Organization
//!
//! - [`geom`] - Geometry primitives (Rect, Point, Expanse, etc.)
//! - [`widgets`] - Built-in widget implementations

// Allow derive macros to reference `canopy::` from within this crate
extern crate self as canopy;

// Internal core module - re-export specific items below
mod core;

// Public modules
pub mod geom;
pub mod widgets;

// Re-export submodules that users may need to access directly
#[cfg(any(test, feature = "testing"))]
pub use core::testing;
// Re-export core application types
pub use core::{Binder, Canopy, Context, DefaultBindings, Layout, Loader, Poller};
// Re-export terminal buffer and text buffer
pub use core::termbuf::TermBuf;
pub use widgets::input::TextBuf;
// Re-export focus utilities
pub use core::{FocusableNode, collect_focusable_nodes, find_focus_target, find_focused_node};
// Re-export input mapping
pub use core::{InputMap, InputMode, InputSpec};
pub use core::{
    backend, commands, cursor, error, event, node, path, render, script, state, style, tree,
};

// Re-export derive macros
pub use canopy_derive::{StatefulNode, command, derive_commands};

// Macros - these are exported at crate root by #[macro_export]
// buf! is defined in testing::buf
// rgb! is defined in style
