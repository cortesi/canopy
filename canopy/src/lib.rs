// Needed due to the semantics of the duplicate crate
#![allow(clippy::needless_arbitrary_self_type)]

pub use canopy_derive::{action, derive_actions};

mod canopy;
mod control;
mod global;
mod node;
mod outcome;
mod poll;
mod render;
mod state;
mod tutils;

pub mod actions;
pub mod backend;
pub mod cursor;
pub mod error;
pub mod event;
pub mod geom;
pub mod inspector;
pub mod style;
pub mod viewport;
pub mod widgets;

pub use crate::canopy::*;
pub use actions::{Action, Actions};
pub use control::BackendControl;
pub use error::{Error, Result};
pub use node::*;
pub use outcome::Outcome;
pub use render::Render;
pub use state::{NodeState, StatefulNode};
pub use viewport::ViewPort;
