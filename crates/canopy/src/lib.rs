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

// Allow derive macros to reference `canopy::` from within this crate
extern crate self as canopy;

// Internal core module - re-export specific items below
mod core;

// `canopy::geom` is the app-facing path to the geometry crate.
pub use canopy_geom as geom;
pub mod layout;
pub mod prelude;
pub(crate) mod widget;

pub(crate) use core::backend;
pub use core::termbuf::{Cell, RenderLimits, TermBuf};
#[cfg(any(test, feature = "testing"))]
pub use core::testing;
// Stable app-author surface.
pub use core::{
    AutomationCallback, AutomationHandle, BindingId, BindingOptions, BindingOwner, BindingPhase,
    BindingScope, Canopy, CanopyBuilder, ChangeOutcome, ChangeSet, ChildBuilder, ChildConfig,
    ChildSlot, Context, ContextExt, EvalId, EvalOutcome, EvalRequest, EvalTicket,
    ExclusiveFrameToken, Fixture, FixtureInfo, FocusDirection, FocusScope, FrameId, FrameSnapshot,
    FrameworkBindingGroup, InputSpec, InteractionToken, Invalidation, KeyedChildren, Loader,
    ModalBindings, ModalOptions, NodeId, NodeSnapshot, NodeWakeHandle, RoutePhase, RouteTraceEntry,
    ScriptJournalEntry, ScriptOrigin, ScriptTrust, SemanticIdentity, TurnOutcome, TypedId,
    ViewContext, ViewContextExt, WakeOutcome, WidgetSemantics, Work, WorkLifetime,
};
// App-author modules used by widget implementations and derive output.
pub use core::{
    commands, cursor, error, event, help, path, render, script, state, style, text, view,
};

/// Crossterm terminal run-loop integration.
pub mod terminal {
    pub use crate::core::backend::crossterm::{
        InterruptPolicy, RunOptions, runloop, runloop_with_options,
    };
}

// Re-export derive macros
pub use canopy_derive::{CommandArg, CommandEnum, command, derive_commands};
// Re-export widget trait and event outcome
pub use widget::{EventOutcome, Widget};
