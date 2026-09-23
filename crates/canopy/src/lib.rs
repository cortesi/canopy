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
//! - [`input`] - Events, bindings, intents, modals, and route analysis
//! - [`tree`] - Slots, identities, and focus traversal
//! - [`layout`] - Layout, views, scrolling, and reveals
//! - [`render`] - The widget renderer, frame buffers, and backends
//! - [`runtime`] - Turns, published frames, polls, wakes, and notices
//! - [`script`] - Luau evaluation, automation, and fixtures
//! - [`geom`] - Geometry primitives (Rect, Point, Size, etc.)

// Allow derive macros to reference `canopy::` from within this crate
extern crate self as canopy;

// Internal core module - re-export specific items below
mod core;

// `canopy::geom` is the app-facing path to the geometry crate.
pub use canopy_geom as geom;
pub mod layout;
pub(crate) mod widget;

pub(crate) use core::backend;
#[cfg(any(test, feature = "testing"))]
pub use core::testing;
// The app-author root: the application, widgets, contexts, and node handles.
pub use core::{
    Canopy, CanopyBuilder, ChangeOutcome, Context, ContextExt, NodeId, Register, Setup, TypedId,
    ViewContext, ViewContextExt, path::NodeName,
};
// App-author modules used by widget implementations and derive output.
pub use core::{commands, error, path, script, style, text};

// Internal module paths used across the crate.
pub(crate) use crate::core::change::{ChangeSet, Invalidation};

/// Input: events, bindings and their tiers, intents, modals, and what the
/// route does with a key.
pub mod input {
    pub use crate::core::{
        canopy::{RouteTraceEntry, RouteTraceKind},
        event::{Event, key, mouse},
        help::{AvailableBinding, BindingCommand, BindingSnapshot, BindingTarget},
        inputmap::{
            BindingAction, BindingActionKind, BindingId, BindingOptions, BindingPhase, BindingTier,
            FrameworkBindingGroup, InputSpec, IntentName, IntentSpec, NavIntent,
        },
        keyroute::{
            KeyDispatchDivergence, KeyExpectation, KeyRouteExplanation, KeyRouteStep, RouteOutcome,
            RouteWinner, StepBinding,
        },
        world::modal::{ModalBindings, ModalOptions, ModalToken},
    };
}

/// Tree structure: slots, identities, and focus traversal.
pub mod tree {
    pub use crate::core::{
        context::{ChildSlot, FocusDirection, FocusScope},
        node::NodeIdentity,
    };
}

/// The runtime: turns, published frames, polls and wakes, and notices.
pub mod runtime {
    pub use crate::core::{
        canopy::{FrameId, TurnInput, TurnOutcome},
        notice::{Notice, NoticeSource},
        snapshot::{FrameSnapshot, NodeSnapshot, WidgetSemantics},
        wake::{NodeWakeHandle, PollLifetime, WakeOutcome, WakeSender, wake_channel},
    };
}

/// Rendering: the widget renderer, frame buffers, backends, and cursors.
pub mod render {
    pub use crate::core::{
        cursor,
        render::{NopBackend, Render, RenderBackend},
        termbuf::{Cell, RenderLimits, TermBuf},
    };
}

/// Crossterm terminal run-loop integration.
pub mod terminal {
    pub use crate::core::backend::crossterm::{InterruptPolicy, RunOptions, runloop};
}

// Re-export derive macros
pub use canopy_derive::{CommandArg, CommandEnum, command, derive_commands};
// Re-export widget trait and event outcome
pub use widget::{EventOutcome, Widget};
