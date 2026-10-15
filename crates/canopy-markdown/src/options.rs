//! Host choices and independent reader state values.

use std::time::Duration;

use canopy::{CommandEnum, runtime::NodeWakeHandle};
use canopy_widgets::editor::{LineNumbers, WrapMode};

use crate::{
    document::ParseLimits, executor::MarkdownExecutor, presentation::GlyphPreset,
    source_store::DEFAULT_SOURCE_LIMIT,
};

/// The document representation displayed by the reader.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CommandEnum)]
pub enum ViewType {
    /// Reading presentation with actionable cards and links.
    #[default]
    Rendered,
    /// Original Markdown text with exact source copying.
    Source,
}

/// Interaction within the rendered representation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CommandEnum)]
pub enum MarkdownMode {
    /// Navigation moves the document viewport.
    #[default]
    Scroll,
    /// Navigation moves an object cursor or text-selection endpoint.
    Interact,
}

/// Short Rust name for the reader's domain-qualified command mode.
pub use MarkdownMode as Mode;

/// Whether new content moves the viewport to the tail.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CommandEnum)]
pub enum Follow {
    /// New publications reveal the last readable row.
    Following,
    /// New publications preserve the reader's source anchor.
    #[default]
    Paused,
}

/// Resource bounds and presentation policy for one Markdown reader.
#[derive(Clone)]
pub struct MarkdownOptions {
    /// Shared preparation service. Clone this option across readers.
    pub executor: MarkdownExecutor,
    /// Optional host wake after an accepted publication changes displayed
    /// content, search, view, or preparation failure. Queries never wake it.
    pub publication_wake: Option<NodeWakeHandle>,
    /// Initial representation of a new document.
    pub initial_view: ViewType,
    /// Restrict an explicitly partial source preview to Source.
    pub source_only: bool,
    /// Host status outside document content, such as a retained file range.
    pub status: Option<String>,
    /// The host owns its search row instead of the built-in search field.
    pub external_search: bool,
    /// Wrapping in the Source surface.
    pub source_wrap: WrapMode,
    /// Optional absolute or relative source line numbers.
    pub line_numbers: LineNumbers,
    /// Unicode or ASCII document decoration.
    pub glyphs: GlyphPreset,
    /// Maximum prose width before wrapping, in terminal cells.
    pub prose_width: u32,
    /// Largest accepted source, in UTF-8 bytes.
    pub source_limit: usize,
    /// Bounds on semantic construction and rich table adaptation.
    pub parse_limits: ParseLimits,
    /// Maximum retained inactive surface layout bytes; active buffers and
    /// models are accounted separately.
    pub cache_limit: usize,
    /// Maximum automatic highlighting per source document.
    pub syntax_limit: usize,
    /// Maximum retained search results.
    pub search_limit: usize,
    /// Maximum query length, in bytes.
    pub query_limit: usize,
    /// Maximum host requests awaiting a reply.
    pub action_limit: usize,
    /// Maximum bytes of one copied payload.
    pub copy_limit: usize,
    /// Time for coalescing replaceable source and geometry requests.
    pub coalesce: Duration,
    /// Enable explicit Copy actions. The host must service requests.
    pub clipboard: bool,
    /// Enable explicit HTTP/HTTPS Open actions. The host services requests.
    pub browser: bool,
    /// Base URL for resolving relative destinations.
    pub base_url: Option<String>,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            executor: MarkdownExecutor::default(),
            publication_wake: None,
            initial_view: ViewType::Rendered,
            source_only: false,
            status: None,
            external_search: false,
            source_wrap: WrapMode::Soft,
            line_numbers: LineNumbers::None,
            glyphs: GlyphPreset::Unicode,
            prose_width: 88,
            source_limit: DEFAULT_SOURCE_LIMIT,
            parse_limits: ParseLimits::default(),
            cache_limit: 32 * 1024 * 1024,
            syntax_limit: 256 * 1024,
            search_limit: 10_000,
            query_limit: 4096,
            action_limit: 16,
            copy_limit: 4 * 1024 * 1024,
            coalesce: Duration::from_millis(33),
            clipboard: false,
            browser: false,
            base_url: None,
        }
    }
}
