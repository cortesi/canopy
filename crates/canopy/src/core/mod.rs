#![allow(clippy::new_without_default)]
//! Core types and traits for the Canopy terminal UI library.

/// Backend implementations.
pub mod backend;
/// Keyed child collection helpers.
/// Command definition and dispatch.
pub mod commands;
/// Cursor and position helpers.
pub mod cursor;
/// Debug dump utilities.
pub mod dump;
/// Core error types.
pub mod error;
/// Input event types.
pub mod event;
/// Named, reproducible application fixtures.
pub mod fixture;
/// Node data and arena structures.
pub mod node;
/// Recoverable failures reported while the application keeps running.
pub mod notice;
/// Path and traversal helpers.
pub mod path;
/// Rendering interfaces.
pub mod render;
/// Scripting support.
pub mod script;
/// Immutable frame observations.
pub mod snapshot;
/// Shared node name types.
pub mod state;
/// Styling and color helpers.
pub mod style;
/// Testing utilities.
#[cfg(any(test, feature = "testing"))]
pub mod testing;
/// View management.
pub mod view;

/// Core Canopy application implementation.
pub mod canopy;
/// Accepted mutation outcomes.
pub mod change;
/// Core context traits and implementations.
pub mod context;
/// Help snapshot API.
pub mod help;
/// Node ID types.
pub mod id;
/// Input mapping.
pub mod inputmap;
pub mod keyroute;
/// Polling utilities.
pub mod poll;
/// Consumer-aware binding selection.
mod select;
/// Terminal buffer types.
pub mod termbuf;
/// Text utilities.
pub mod text;
/// Lifetime-bound worker notifications.
pub mod wake;
/// Widget slot borrowing and extraction guards.
mod widget_access;
/// World state and layout integration.
pub mod world;

// Public exports from internal modules
pub use canopy::{
    AutomationCallback, AutomationHandle, Canopy, CanopyBuilder, EvalId, EvalOutcome, EvalRequest,
    EvalTicket, FrameId, Register, RouteTraceEntry, RouteTraceKind, ScriptJournalEntry,
    ScriptOrigin, ScriptTrust, Setup, TurnInput, TurnOutcome,
};
pub use change::ChangeOutcome;
pub use context::{
    ChildSlot, Context, ContextExt, FocusDirection, FocusScope, RevealAlign, ScrollDirection,
    ScrollOp, ViewContext, ViewContextExt,
};
pub use fixture::{Fixture, FixtureInfo};
pub use id::{NodeId, TypedId};
pub use inputmap::{
    BindingAction, BindingActionKind, BindingId, BindingOptions, BindingPhase, BindingTier,
    FrameworkBindingGroup, InputSpec, IntentName, IntentSpec,
};
pub use node::NodeIdentity;
pub use notice::{Notice, NoticeSource};
pub use snapshot::{FrameSnapshot, NodeSnapshot, WidgetSemantics};
pub use wake::{NodeWakeHandle, PollLifetime, WakeOutcome, WakeSender, wake_channel};
pub use world::{
    Core,
    modal::{ModalBindings, ModalOptions, ModalToken},
};
