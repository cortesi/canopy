#![allow(clippy::new_without_default)]
//! Core types and traits for the Canopy terminal UI library.

// Re-export derive macros
pub use canopy_derive::{command, derive_commands, StatefulNode as DeriveStatefulNode};
pub use canopy_derive::StatefulNode;

// Core modules
mod context;
pub mod error;
pub mod node;
pub mod commands;
pub mod state;
mod viewport;
mod layout;
pub mod render;
mod termbuf;

pub mod cursor;
pub mod event;
pub mod geom;
pub mod style;
pub mod path;
pub mod tree;

// Public exports
pub use context::Context;
pub use error::{Error, Result};
pub use node::{Node, EventOutcome};
pub use commands::*;
pub use state::{NodeId, NodeName, NodeState};
// Re-export the trait as both names for compatibility
pub use state::StatefulNode;
pub use state::StatefulNode as StatefulNodeTrait;
pub use viewport::ViewPort;
pub use layout::*;
pub use render::{Render, RenderBackend};
pub use termbuf::TermBuf;

// Export commonly used geometry types at the root
pub use geom::{Expanse, Point, Rect, Direction};

