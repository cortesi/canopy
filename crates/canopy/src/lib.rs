//! Canopy: A terminal UI library.
//!
//! Canopy is a terminal UI library for building interactive applications.
//! It provides an arena-based widget system with focus management, styling,
//! and event handling.
//!
//! # Quick Start
//!
//! The main entry points are:
//! - [`Canopy`] - The core application state
//! - [`Core`] - The arena and layout engine
//! - [`Widget`] - The trait implemented by all widgets
//!
//! # Module Organization
//!
//! - [`geom`] - Geometry primitives (Rect, Point, Expanse, etc.)
//! - [`widgets`] - Built-in widget implementations

#![warn(missing_docs)]

// Allow derive macros to reference `canopy::` from within this crate
extern crate self as canopy;

// Internal core module - re-export specific items below
mod core;

// Public modules
pub mod geom;
pub mod layout;
pub mod widget;
pub mod widgets;

// Re-export submodules that users may need to access directly
// Re-export terminal buffer and text buffer
pub use core::termbuf::TermBuf;
#[cfg(any(test, feature = "testing"))]
pub use core::testing;
// Re-export core application types
pub use core::{
    Binder, Canopy, Context, Core, DefaultBindings, InputMap, InputMode, InputSpec, Loader, NodeId,
    Poller, TypedId, ViewContext,
};
// Re-export input mapping
pub use core::{backend, commands, cursor, error, event, path, render, script, state, style};

// Re-export derive macros
pub use canopy_derive::{command, derive_commands};
// Re-export widget trait and event outcome
pub use widget::{EventOutcome, Widget};
pub use widgets::input::TextBuf;
