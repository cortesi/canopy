//! Convenience re-exports for common Canopy types.

pub use crate::{
    Canopy, ChangeOutcome, ChildKey, CommandArg, CommandEnum, Context, EventOutcome, FocusScope,
    Loader, NodeId, Path, PathFilter, RenderLimits, ScriptApiState, Slot, TypedId, ViewContext,
    Widget, command, derive_commands, error,
    event::{Event, key::Key, mouse},
    geom::{Point, Rect, Size},
    key,
    layout::{
        Align, Constraint, Direction, Display, Layout, MeasureConstraints, Measurement, Sizing,
    },
    render::Render,
    state::NodeName,
    style::{StyleBuilder, StyleMap},
};

/// Common result alias for Canopy operations.
pub type Result<T> = error::Result<T>;
