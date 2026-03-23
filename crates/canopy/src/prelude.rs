//! Convenience re-exports for common Canopy types.

pub use crate::{
    Binder, Canopy, ChildKey, Context, Loader, NodeId, ReadContext, Slot, TypedId, Widget, error,
    event::{Event, key::Key, mouse},
    geom::{Point, Rect, Size},
    key,
    layout::{
        Align, Constraint, Direction, Display, Layout, MeasureConstraints, Measurement, Sizing,
    },
    render::Render,
    state::NodeName,
};

/// Common result alias for Canopy operations.
pub type Result<T> = error::Result<T>;
