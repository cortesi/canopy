//! Canopy: A terminal UI library.
//!
//! Canopy is a terminal UI library for building interactive applications.
//! It provides a tree-based widget system with focus management, styling,
//! and event handling.
//!
//! # Quick Start
//!
//! The main entry points are:
//! - [`Canopy`] - The core application state
//! - [`Node`] - The trait implemented by all widgets
//! - [`Root`] - The recommended root widget for applications
//!
//! # Module Organization
//!
//! - [`geom`] - Geometry primitives (Rect, Point, Expanse, etc.)
//! - [`widgets`] - Built-in widget implementations

// Allow derive macros to reference `canopy::` from within this crate
extern crate self as canopy;

// Internal core module - re-export specific items below
mod core;

// Public modules
pub mod geom;
pub mod widgets;

// Re-export submodules that users may need to access directly
pub use core::backend;
pub use core::commands;
pub use core::cursor;
pub use core::dump;
pub use core::error;
pub use core::event;
pub use core::node;
pub use core::path;
pub use core::render;
pub use core::script;
pub use core::state;
pub use core::style;
pub use core::tree;
pub use core::tutils;

// Re-export derive macros
pub use canopy_derive::{StatefulNode, command, derive_commands};

// Re-export core application types
pub use core::{Binder, Canopy, Context, DefaultBindings, Loader};

// Re-export command-related types
pub use core::commands::{
    ArgTypes, Args, CommandInvocation, CommandNode, CommandSet, CommandSpec, ReturnSpec,
    ReturnTypes, ReturnValue, dispatch,
};

// Re-export error types
pub use core::error::{Error, Result};

// Re-export node types
pub use core::node::{EventOutcome, Node};

// Re-export render types
pub use core::render::{Render, RenderBackend};

// Re-export state types
pub use core::state::{NodeId, NodeName, NodeState, StatefulNode};

// Re-export commonly used geometry types at root for convenience
pub use geom::{Direction, Expanse, Point, Rect};

// Re-export layout
pub use core::Layout;

// Re-export focus utilities
pub use core::{FocusableNode, collect_focusable_nodes, find_focus_target, find_focused_node};

// Re-export input mapping
pub use core::{Input, InputMap, InputMode};

// Re-export polling
pub use core::Poller;

// Re-export the main widget
pub use widgets::Root;

// Macros - these are exported at crate root by #[macro_export]
// buf! is defined in tutils::buf
// rgb! is defined in style
