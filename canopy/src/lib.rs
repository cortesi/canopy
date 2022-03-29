// Needed due to the semantics of the duplicate crate
#![allow(clippy::needless_arbitrary_self_type)]

mod actions;
mod canopy;
mod control;
mod global;
mod graft;
mod node;
mod outcome;
mod render;
mod state;
mod tutils;

pub mod backend;
pub mod cursor;
pub mod error;
pub mod event;
pub mod geom;
pub mod inspector;
pub mod style;
pub mod viewport;
pub mod widgets;

pub use crate::canopy::Canopy;
pub use actions::Actions;
pub use control::BackendControl;
pub use error::{Error, Result};
pub use graft::Graft;
pub use node::Node;
pub use outcome::Outcome;
pub use render::Render;
pub use state::{NodeState, StatefulNode};
pub use viewport::ViewPort;
