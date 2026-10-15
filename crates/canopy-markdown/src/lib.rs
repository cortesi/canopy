//! Interactive Markdown reading and exact source viewing for Canopy.
//!
//! The host supplies text and performs explicit browser and clipboard requests.
//! Parsing, preparation, and publication never perform external actions.

/// Semantic documents, source mappings, and actionable objects.
pub mod document;
/// Bounded shared preparation and deterministic scheduling.
pub mod executor;
/// Host requests and replies.
pub mod host;
/// Read-only inspector overlays.
mod inspector;
/// Reader configuration and state values.
pub mod options;
/// Prepared terminal presentation.
pub mod presentation;
/// Bounded literal and regex queries.
pub mod search;
/// Accepted source and streaming lifecycle.
pub mod source_store;
/// Exact source text viewing.
pub mod source_view;
/// The rendered reading surface.
mod surface;
/// The document controller and view control.
mod widget;

pub use options::{Follow, MarkdownOptions, Mode, ViewType};
pub use source_store::{DocumentInput, FinishOutcome};
pub use source_view::MarkdownSource;
pub use widget::{Markdown, ReaderPosition};
