#![allow(clippy::new_without_default)]
//! Core types and traits for the Canopy terminal UI library.

// Re-export derive macros
pub use canopy_derive::{
    StatefulNode, StatefulNode as DeriveStatefulNode, command, derive_commands,
};

// Core modules
/// Command definition and dispatch.
pub mod commands;
/// Context trait and helpers.
mod context;
/// Core error types.
pub mod error;
/// Layout helpers.
mod layout;
/// Node traits and helpers.
pub mod node;
/// Rendering interfaces.
pub mod render;
/// Shared node state types.
pub mod state;
/// Terminal buffer types.
mod termbuf;
/// Viewport management.
mod viewport;
/// View stack utilities.
mod viewstack;

/// Cursor and position helpers.
pub mod cursor;
/// Debug dump utilities.
pub mod dump;
/// Input event types.
pub mod event;
pub use geom;
/// Backend implementations.
pub mod backend;
/// Binding utilities.
mod binder;
/// Core Canopy application implementation.
mod canopy;
/// Focus traversal helpers.
mod focus;
/// Input mapping.
mod inputmap;
/// Path and traversal helpers.
pub mod path;
/// Polling utilities.
mod poll;
/// Scripting support.
pub mod script;
/// Styling and color helpers.
pub mod style;
/// Tree traversal utilities.
pub mod tree;
/// Testing utilities.
pub mod tutils;

// Public exports
// Application-level types
pub use binder::{Binder, DefaultBindings};
pub use canopy::{Canopy, Loader};
pub use commands::*;
pub use context::Context;
pub use error::{Error, Result};
pub use focus::{FocusableNode, collect_focusable_nodes, find_focus_target, find_focused_node};
// Export commonly used geometry types at the root
pub use geom::{Direction, Expanse, Point, Rect};
pub use inputmap::{Input, InputMap, InputMode};
// Re-export the trait as both names for compatibility
pub use layout::*;
pub use node::{EventOutcome, Node};
pub use poll::Poller;
pub use render::{Render, RenderBackend};
pub use state::{NodeId, NodeName, NodeState, StatefulNode, StatefulNode as StatefulNodeTrait};
pub use termbuf::TermBuf;
pub use viewport::ViewPort;
pub use viewstack::ViewStack;
