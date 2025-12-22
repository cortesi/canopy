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
/// Node data and arena structures.
pub mod node;
/// Path and traversal helpers.
pub mod path;
/// Rendering interfaces.
pub mod render;
/// Scripting support.
pub mod script;
/// Shared node name types.
pub mod state;
/// Styling and color helpers.
pub mod style;
/// Testing utilities.
#[cfg(any(test, feature = "testing"))]
pub mod testing;
/// Viewport management.
pub mod viewport;
/// View stack utilities.
pub mod viewstack;

// Core modules - internal
/// Binding utilities.
pub mod binder;
/// Builder API for nodes.
pub mod builder;
/// Core Canopy application implementation.
pub mod canopy;
/// Core context traits and implementations.
pub mod context;
/// Node ID types.
pub mod id;
/// Input mapping.
pub mod inputmap;
/// Polling utilities.
pub mod poll;
/// Terminal buffer types.
pub mod termbuf;
/// World state and layout integration.
pub mod world;

// Public exports from internal modules
pub use binder::{Binder, DefaultBindings};
pub use canopy::{Canopy, Loader};
pub use context::{Context, ViewContext};
pub use id::NodeId;
pub use inputmap::{InputMap, InputMode, InputSpec};
pub use poll::Poller;
pub use world::Core;
