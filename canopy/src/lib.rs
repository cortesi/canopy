// Needed due to the semantics of the duplicate crate
#![allow(clippy::needless_arbitrary_self_type)]

pub use canopy_derive::{command, derive_commands};

mod canopy;
mod control;
mod global;
mod keybinding;
mod node;
mod outcome;
mod poll;
mod render;
mod state;
mod tutils;

pub mod backend;
pub mod commands;
pub mod cursor;
pub mod error;
pub mod event;
pub mod geom;
pub mod inspector;
pub mod style;
pub mod viewport;
pub mod widgets;

pub use crate::canopy::*;
pub use commands::{Command, Commands};
pub use control::BackendControl;
pub use error::{Error, Result};
pub use keybinding::Keybindings;
pub use node::*;
pub use outcome::Outcome;
pub use render::Render;
pub use state::{NodeState, StatefulNode};
pub use viewport::ViewPort;
