#![allow(clippy::new_without_default)]
//! Core types and traits for the Canopy terminal UI library.

// Re-export derive macros
pub use canopy_derive::StatefulNode;
pub use canopy_derive::{StatefulNode as DeriveStatefulNode, command, derive_commands};

// Core modules
pub mod commands;
mod context;
pub mod error;
mod layout;
pub mod node;
pub mod render;
pub mod state;
mod termbuf;
mod viewport;
mod viewstack;

pub mod cursor;
pub mod dump;
pub mod event;
pub use geom;
pub mod backend;
mod binder;
mod canopy;
mod focus;
mod inputmap;
pub mod path;
mod poll;
pub mod script;
pub mod style;
pub mod tree;
pub mod tutils;

// Public exports
pub use commands::*;
pub use context::Context;
pub use error::{Error, Result};
pub use node::{EventOutcome, Node};
pub use state::{NodeId, NodeName, NodeState};
// Re-export the trait as both names for compatibility
pub use layout::*;
pub use render::{Render, RenderBackend};
pub use state::StatefulNode;
pub use state::StatefulNode as StatefulNodeTrait;
pub use termbuf::TermBuf;
pub use viewport::ViewPort;
pub use viewstack::ViewStack;

// Application-level types
pub use binder::{Binder, DefaultBindings};
pub use canopy::{Canopy, Loader};
pub use focus::{FocusableNode, collect_focusable_nodes, find_focus_target, find_focused_node};
pub use inputmap::{Input, InputMap, InputMode};
pub use poll::Poller;

// Export commonly used geometry types at the root
pub use geom::{Direction, Expanse, Point, Rect};
