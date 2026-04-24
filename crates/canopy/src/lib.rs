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
//! - [`Widget`] - The trait implemented by all widgets
//! - [`Context`] - The mutation API available to widgets
//!
//! # Module Organization
//!
//! - [`geom`] - Geometry primitives (Rect, Point, Size, etc.)

#![warn(missing_docs)]

// Allow derive macros to reference `canopy::` from within this crate
extern crate self as canopy;

// Internal core module - re-export specific items below
mod core;

// Public modules - re-export canopy-geom as geom for backwards compatibility
pub use canopy_geom as geom;
pub mod layout;
pub mod prelude;
pub(crate) mod widget;

pub use core::termbuf::TermBuf;
#[cfg(any(test, feature = "testing"))]
pub use core::testing;
// Stable app-author surface.
pub use core::{
    AutomationCallback, AutomationHandle, BindingId, Canopy, ChildKey, CommandContext, Context,
    Fixture, FixtureInfo, FocusContext, KeyedChildren, LayoutContext, Loader, NodeId, Path,
    PathFilter, ReadContext, RemovePolicy, ScrollContext, Slot, StyleContext, TreeContext, TypedId,
};
// Lower-level runtime exports retained for internal crates and diagnostics.
#[doc(hidden)]
pub use core::{Core, InputMap, InputSpec, Preorder, RoutePhase, RouteTraceEntry};
#[doc(hidden)]
pub use core::{
    backend, commands, cursor, error, event, help, inputmap, path, render, script, state, style,
    text, view,
};

// Re-export derive macros
pub use canopy_derive::{CommandArg, CommandEnum, command, derive_commands};
// Re-export widget trait and event outcome
pub use widget::{EventOutcome, Widget};
