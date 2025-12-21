#![allow(clippy::new_without_default)]
//! Core types and traits for the Canopy terminal UI library.

// Core modules - public
/// Backend implementations.
pub mod backend;
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
/// Node traits and helpers.
pub mod node;
/// Path and traversal helpers.
pub mod path;
/// Rendering interfaces.
pub mod render;
/// Scripting support.
pub mod script;
/// Shared node state types.
pub mod state;
/// Styling and color helpers.
pub mod style;
/// Testing utilities.
#[cfg(any(test, feature = "testing"))]
pub mod testing;
/// Tree traversal utilities.
pub mod tree;

// Core modules - internal
/// Binding utilities.
pub mod binder;
/// Core Canopy application implementation.
pub mod canopy;
/// Context trait and helpers.
pub mod context;
/// Focus traversal helpers.
pub mod focus;
/// Input mapping.
pub mod inputmap;
/// Layout helpers.
pub mod layout;
/// Polling utilities.
pub mod poll;
/// Terminal buffer types.
pub mod termbuf;
/// Viewport management.
pub mod viewport;
/// View stack utilities.
pub mod viewstack;

// Public exports from internal modules
pub use binder::{Binder, DefaultBindings};
pub use canopy::{Canopy, Loader};
pub use context::Context;
pub use focus::{FocusableNode, collect_focusable_nodes, find_focus_target, find_focused_node};
pub use inputmap::{Input, InputMap, InputMode};
pub use layout::Layout;
pub use poll::Poller;
