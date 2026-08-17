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

pub(crate) use core::backend;
pub use core::termbuf::{RenderLimits, TermBuf};
#[cfg(any(test, feature = "testing"))]
pub use core::testing;
// Stable app-author surface.
pub use core::{
    AutomationCallback, AutomationHandle, BindingId, Canopy, ChangeOutcome, ChildKey, Context,
    Fixture, FixtureInfo, FocusScope, InputSpec, KeyedChildren, Loader, NodeId, Path, PathFilter,
    RemovePolicy, RoutePhase, RouteTraceEntry, ScriptApiState, ScriptJournalEntry,
    ScriptModuleRoots, Slot, TypedId, ViewContext,
};
// App-author modules used by widget implementations and derive output.
pub use core::{
    commands, cursor, error, event, help, path, render, script, state, style, text, view,
};

/// Crossterm terminal run-loop integration.
pub mod terminal {
    pub use crate::core::backend::crossterm::runloop;
}

// Re-export derive macros
pub use canopy_derive::{CommandArg, CommandEnum, command, derive_commands};
// Re-export widget trait and event outcome
pub use widget::{EventOutcome, Widget};
