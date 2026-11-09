//! Interactive Markdown reading and exact source viewing for Canopy.
//!
//! The host supplies text and performs explicit browser and clipboard requests.
//! Parsing, preparation, and publication never perform external actions.

/// The movement of the block cursor over the rows of edit units.
mod cursor;
/// Semantic documents, source mappings, and actionable objects.
pub mod document;
/// Bounded shared preparation and deterministic scheduling.
pub mod executor;
/// Host requests and replies.
pub mod host;
/// Read-only inspector overlays.
mod inspector;
/// Reader configuration and state values.
mod options;
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
/// Proptest strategies and invariant checks that other tests share.
#[cfg(any(test, feature = "testing"))]
pub mod testing;
/// The whole source lines of each block, and spoken replaces of their text.
pub mod units;
/// The document controller and view control.
mod widget;

use canopy::Setup;
use canopy_widgets::{highlight::palette_style_rules, table};
pub use options::{Follow, MarkdownOptions, Mode, ViewType};
pub use widget::{BlockEdit, Markdown, MatchPlace, ReaderPosition};

/// Source replacement with an explicit initial lifecycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentInput {
    /// A complete static document or closed source epoch.
    Complete(String),
    /// A source epoch that still accepts appends.
    Receiving(String),
}

/// The producer outcome applied with ordinary final EOF interpretation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinishOutcome {
    /// The producer completed normally.
    Completed,
    /// The producer stopped with a retained reason.
    Aborted(String),
}

/// Register the styles that prepared documents paint with: the Markdown roles,
/// the colors of syntax highlighting, and tables. A host that paints documents
/// itself, without a reader, calls this in place of `Markdown::register`.
pub fn register_styles(setup: &mut Setup) {
    setup.widget_styles(presentation::style_rules);
    setup.widget_styles(palette_style_rules);
    setup.widget_styles(table::style_rules);
}
