#![allow(clippy::new_without_default)]
//! A library for building terminal UIs.

pub use canopy_derive::{command, derive_commands};

mod canopy;
mod error;
mod inputmap;
mod node;
mod poll;
mod render;
mod root;
mod state;
mod viewport;

pub mod backend;
mod binder;
pub mod commands;
pub mod cursor;
pub mod event;
pub mod geom;
pub mod inspector;
pub mod layout;
pub mod path;
pub mod script;
pub mod style;
pub mod tree;
pub mod widgets;

pub use crate::canopy::*;
pub use binder::*;
pub use error::{Error, Result};
pub use node::*;
pub use root::*;

pub use layout::Layout;
pub use render::Render;
pub use state::{NodeId, NodeName, NodeState, StatefulNode};
pub use viewport::ViewPort;

// Hide the test utils from docs. We need to expose it for integration tests, but it's not for external use.
#[doc(hidden)]
pub mod tutils;
