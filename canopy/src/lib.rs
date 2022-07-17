#![allow(clippy::new_without_default)]

pub use canopy_derive::{command, derive_commands};

mod binder;
mod canopy;
mod control;
mod inputmap;
mod node;
mod poll;
mod render;
mod root;
mod state;
pub mod tutils;

pub mod backend;
pub mod commands;
pub mod cursor;
pub mod error;
pub mod event;
pub mod geom;
pub mod inspector;
pub mod path;
pub mod script;
pub mod style;
pub mod viewport;
pub mod widgets;

pub use crate::canopy::*;
pub use binder::*;
pub use commands::{CommandNode, CommandSpec};
pub use control::BackendControl;
pub use error::{Error, Result};
pub use node::*;
pub use render::Render;
pub use root::*;
pub use state::{NodeId, NodeName, NodeState, StatefulNode};
pub use viewport::ViewPort;
