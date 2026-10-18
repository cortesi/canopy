//! The shared document controller, publication policy, and fixed reader chrome.

use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    fmt::Display,
    rc::Rc,
    result::Result as StdResult,
    sync::Arc,
    time::{Duration, Instant},
};

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, Register, Setup, ViewContext,
    ViewContextExt, Widget,
    commands::{ArgValue, CommandStatus, CommandTarget},
    derive_commands,
    error::{Error, Result},
    geom::{Line, Point, Rect, Size},
    input::{
        BindingAction, BindingOptions, BindingPhase, BindingTier, Event, IntentName, IntentSpec,
        NavIntent, key::Key, mouse,
    },
    layout::{Direction, Edges, Layout, RevealAlign, ScrollOp},
    render::Render,
    runtime::{NodeWakeHandle, NoticePost, PollLifetime, WidgetSemantics},
};
use canopy_widgets::{
    Container, SearchBar, TableView,
    editor::{Editor, LineNumbers, WrapMode},
    highlight::palette_style_rules,
    mosaic::{Tier, tier_for},
    scrollbar::Scrollbar,
    table::{self, CellKey, PreparedTable, TableLayoutOptions},
};

use crate::{
    Follow, MarkdownOptions, Mode, ViewType,
    document::{
        BlockKind, DocumentSnapshot, LocalLink, ObjectKind, ReadingPosition, ReadingRange,
        ResolvedLink, heading_slug,
    },
    executor::{
        CopyInput, PreparedView, SearchOutput, WorkHandle, WorkRequest, WorkResult, WorkSearch,
    },
    host::{
        HostAction, HostOutcome, HostReply, HostRequest, Requests, Resource, ResourceKind,
        ResourceRequest,
    },
    inspector::{Inspector, InspectorContent, InspectorEvent, InspectorHandle, OutlineEntry},
    presentation::{GeometryKey, ImageInfo, ImagePixels, ImageSizes, style_rules},
    search::{SearchAddress, SearchCase, SearchLimits, SearchMode, SearchQuery, SearchResults},
    source_store::{DocumentInput, FinishOutcome, Lifecycle, SourceSnapshot, SourceStore},
    source_view::{MarkdownSource, SourceAnchor, SourceMatches, SourceOptions},
    surface::{RenderedSurface, SurfaceState, nav_scroll},
};

/// A view-independent persisted reading location within one document epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReaderPosition {
    /// Document epoch; replacement invalidates the position.
    pub epoch: u64,
    /// View that produced the position.
    pub view: ViewType,
    /// Exact or nearest source byte location.
    pub source_byte: usize,
    /// Precise rendered location, when captured from that representation.
    pub reading: Option<ReadingPosition>,
    /// Desired viewport row of the location.
    pub viewport_row: u32,
    /// Horizontal viewport offset for an unwrapped view.
    pub horizontal: u32,
}

/// Nodes mounted under one persistent reader root.
struct Nodes {
    /// Common allocation that hosts exactly one visible reading surface.
    body: NodeId,
    /// Persistent rendered surface, with optional cached prepared geometry.
    rendered: NodeId,
    /// Lazily mounted source surface, removed when its inactive cache is
    /// oversized.
    source: Option<NodeId>,
    /// Optional built-in query field and status row.
    search: NodeId,
    /// Fixed view control and lifecycle status row.
    _chrome: NodeId,
}
/// Pinned inspector provenance, independent of subsequent publications.
struct OpenInspector {
    /// Modal handle used for events, readiness, and dismissal.
    handle: InspectorHandle,
    /// Pinned inspected object, or None for an outline or full source.
    object: Option<u64>,
    /// Source identity captured when this inspector opened.
    epoch: u64,
    /// Displayed revision captured when this inspector opened.
    revision: u64,
}
/// One logical switch; the outgoing displayed revision is pinned until it
/// commits.
struct Switch {
    /// Requested representation awaiting an atomic prepared publication.
    target: ViewType,
    /// Outgoing revision that the target must reach before committing.
    minimum_revision: u64,
}
/// State painted by the fixed view control without reading the owner's widget
/// slot.
#[derive(Default)]
struct ChromeState {
    /// Current bounded-by-source lifecycle and action status label.
    text: String,
    /// Whether the view control currently admits switching.
    enabled: bool,
    /// Representation currently taking reader input.
    active: ViewType,
}

/// An asynchronous Markdown reader with sibling Rendered and Source surfaces.
pub struct Markdown {
    /// Host capabilities, shared work service, and presentation preferences.
    options: MarkdownOptions,
    /// Exact immutable accepted source and producer lifecycle.
    store: SourceStore,
    /// Representation currently displayed and taking input.
    active: ViewType,
    /// Optional fixed-minimum target awaiting preparation.
    switching: Option<Switch>,
    /// Current Rendered presentation and mutable reader overlays.
    shared: Rc<RefCell<SurfaceState>>,
    /// Mounted child identities, absent before mounting.
    nodes: Option<Nodes>,
    /// Non-owning wake for worker completion and allocation changes.
    wake: Option<NodeWakeHandle>,
    /// This reader's bounded shared-executor lane.
    lane: Option<WorkHandle>,
    /// Exact source snapshot represented by the active publication.
    displayed: Option<SourceSnapshot>,
    /// Last published Source snapshot, also retained while Source is inactive.
    source_snapshot: Option<SourceSnapshot>,
    /// Last published semantics, retained independently of cached geometry;
    /// any cached Rendered presentation is built from this document.
    last_document: Option<Arc<DocumentSnapshot>>,
    /// Byte endpoints preserved when inactive Source geometry is evicted.
    saved_source_selection: Option<(usize, usize)>,
    /// Whether current source, geometry, or query still needs submission.
    dirty: bool,
    /// Optional coalescing deadline for replaceable producer updates.
    deadline: Option<Instant>,
    /// Owner generation used to reject stale view and geometry results.
    generation: u64,
    /// Latest allocated body width observed during render.
    geometry_width: u32,
    /// Block mosaics of the terminal's glyphs, observed during render.
    mosaic: Tier,
    /// Identity of the most recent admitted replaceable request.
    last_submitted: Option<(u64, u64, ViewType, GeometryKey, u64, u64)>,
    /// Retained query and explicit matching policy.
    query: SearchQuery,
    /// Monotonic identity of the current query.
    query_generation: u64,
    /// Bounded Rendered matches for one displayed revision.
    results: Option<SearchResults>,
    /// Explicitly activated match, or None before navigation.
    search_index: Option<usize>,
    /// Exact Source match ranges for one displayed revision.
    source_results: Option<SourceMatches>,
    /// Current compile or scan failure, without discarding query text.
    query_error: Option<String>,
    /// At most one lazily mounted modal inspector.
    inspector: Option<OpenInspector>,
    /// Destination of the link the inspector shows, which its Open follows.
    inspected_link: Option<ResolvedLink>,
    /// What the reader has done about each image block's image, by
    /// destination: asked for it, or settled it.
    image_states: HashMap<String, ImageState>,
    /// Resource requests awaiting the host's reply, by id, with the
    /// destination each names.
    resource_requests: HashMap<u64, String>,
    /// The last resource request id; ids are never reused.
    resource_id: u64,
    /// What the reader knows of each supplied image, for layout.
    image_sizes: Arc<ImageSizes>,
    /// Bounded immutable effects awaiting host delivery or acknowledgement.
    requests: Requests,
    /// Latest status of an explicitly activated host effect.
    action_status: String,
    /// Current worker or publication failure shown outside document text.
    failure: Option<String>,
    /// Shared chrome label read by its sibling widget.
    chrome: Rc<RefCell<ChromeState>>,
    /// Vertical scrollbar interaction state owned by the reader.
    vertical: Scrollbar,
    /// Horizontal scrollbar interaction state owned by the reader.
    horizontal: Scrollbar,
    /// Whether a reader scrollbar owns pointer capture.
    bar_drag: bool,
    /// Per-view logical anchors tagged with the navigation generation.
    saved: [Option<(ReaderPosition, u64)>; 2],
    /// Accepted byte count last seen at the visible tail.
    acknowledged: usize,
    /// One return anchor for a fragment, footnote, or outline jump.
    return_position: Option<ReaderPosition>,
    /// The position before each recent jump, oldest first, for the host's
    /// own history.
    jumps: VecDeque<ReaderPosition>,
    /// Last painted active viewport offset for follow observation.
    last_scroll: Option<Point>,
    /// Last painted allocation used to distinguish resize from navigation.
    last_size: Option<Size>,
    /// Suppresses navigation detection for the next programmatic restoration.
    restoring: bool,
    /// Whether pointer capture postpones a completed surface swap.
    deferred_publication: bool,
}

/// Convert an application-facing error into the common Canopy error type.
fn invalid(error: impl Display) -> Error {
    Error::App(error.to_string().into())
}
/// Map the two representations to their fixed saved-state slots.
fn index(view: ViewType) -> usize {
    usize::from(view == ViewType::Source)
}

/// Vertical and horizontal scrollbar targets, in reader-local coordinates.
type ScrollbarTracks = (Vec<(NodeId, Rect)>, Vec<(NodeId, Rect)>);

/// Jumps the reader keeps for its host before it drops the oldest.
const JUMP_LIMIT: usize = 32;
/// Resource requests awaiting replies at once.
const RESOURCE_LIMIT: usize = 8;
/// Images one document asks for at most.
const IMAGE_LIMIT: usize = 64;

/// What the reader has done about one image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImageState {
    /// Asked the host for it.
    Requested,
    /// Holds its answer, or never asks for it, as for a remote image.
    Settled,
}
/// Conditional Rendered activation route that Source can decline.
const ACTIVATE_INTENT: &str = "markdown.activate";
/// Conditional forward Rendered object traversal route.
const NEXT_OBJECT_INTENT: &str = "markdown.object_next";
/// Conditional backward Rendered object traversal route.
const PREVIOUS_OBJECT_INTENT: &str = "markdown.object_previous";

#[derive_commands]
impl Markdown {
    /// Construct a complete document. Preparation begins after allocation.
    pub fn new(source: String, options: MarkdownOptions) -> Result<Self> {
        let store =
            SourceStore::static_with_limit(source, options.source_limit).map_err(invalid)?;
        Ok(Self::from_store(store, options, Follow::Paused))
    }
    /// Construct a receiving document that initially follows its tail.
    pub fn streaming(options: MarkdownOptions) -> Result<Self> {
        let store = SourceStore::with_limit(options.source_limit);
        Ok(Self::from_store(store, options, Follow::Following))
    }
    /// Initialize reader state without parsing or preparing document text.
    fn from_store(store: SourceStore, options: MarkdownOptions, follow: Follow) -> Self {
        let acknowledged = if matches!(store.snapshot().lifecycle, Lifecycle::Receiving) {
            0
        } else {
            store.snapshot().len()
        };
        let active = if options.source_only {
            ViewType::Source
        } else {
            options.initial_view
        };
        let shared = Rc::new(RefCell::new(SurfaceState::default()));
        shared.borrow_mut().follow = follow;
        Self {
            options,
            store,
            active,
            switching: None,
            shared,
            nodes: None,
            wake: None,
            lane: None,
            displayed: None,
            source_snapshot: None,
            last_document: None,
            saved_source_selection: None,
            dirty: true,
            deadline: None,
            generation: 1,
            geometry_width: 0,
            mosaic: Tier::Full,
            last_submitted: None,
            query: SearchQuery::default(),
            query_generation: 0,
            results: None,
            search_index: None,
            source_results: None,
            query_error: None,
            inspector: None,
            inspected_link: None,
            image_states: HashMap::new(),
            resource_requests: HashMap::new(),
            resource_id: 0,
            image_sizes: Arc::default(),
            requests: Requests::default(),
            action_status: String::new(),
            failure: None,
            chrome: Rc::new(RefCell::new(ChromeState::default())),
            vertical: Scrollbar::vertical("markdown/structure", '█'),
            horizontal: Scrollbar::horizontal("markdown/structure", '▄'),
            bar_drag: false,
            saved: [None, None],
            acknowledged,
            return_position: None,
            jumps: VecDeque::new(),
            last_scroll: None,
            last_size: None,
            restoring: false,
            deferred_publication: false,
        }
    }
    /// Accept a complete UTF-8 chunk or reject it without changing the
    /// document.
    pub fn append(&mut self, ctx: &mut dyn Context, text: &str) -> Result<u64> {
        if self.store.append(text).map_err(invalid)? {
            self.schedule(ctx, false)?;
        }
        Ok(self.store.snapshot().revision)
    }
    /// Apply final EOF interpretation, including an explicitly aborted outcome.
    pub fn finish(&mut self, ctx: &mut dyn Context, outcome: FinishOutcome) -> Result<u64> {
        if self.store.finish(outcome) {
            self.schedule(ctx, true)?;
        }
        Ok(self.store.snapshot().revision)
    }
    /// Replace the document epoch, retaining host presentation and query
    /// preferences.
    pub fn replace(&mut self, ctx: &mut dyn Context, input: DocumentInput) -> Result<u64> {
        let (text, receiving) = match input {
            DocumentInput::Complete(text) => (text, false),
            DocumentInput::Receiving(text) => (text, true),
        };
        self.store.replace(&text, receiving).map_err(invalid)?;
        self.dismiss_inspector(ctx)?;
        if let Some(nodes) = &self.nodes {
            ctx.with_widget_mut(nodes.rendered, |surface: &mut RenderedSurface, ctx| {
                surface.escape(ctx)
            })?;
        }
        if let Some(source) = self.nodes.as_ref().and_then(|n| n.source) {
            ctx.with_widget_mut(source, |source: &mut MarkdownSource, ctx| {
                source.cancel_drag(ctx)
            })?;
        }
        self.source_results = None;
        self.requests.replace();
        self.switching = None;
        self.displayed = None;
        self.source_snapshot = None;
        self.last_document = None;
        self.saved_source_selection = None;
        self.results = None;
        self.search_index = None;
        self.saved = [None, None];
        self.return_position = None;
        self.jumps.clear();
        // An action's status belongs to the document that took it.
        self.action_status.clear();
        // So do its images, which a new document asks for again.
        self.image_states.clear();
        self.resource_requests.clear();
        self.image_sizes = Arc::default();
        self.shared.borrow_mut().overlays.images = ImagePixels::default();
        self.acknowledged = if receiving {
            0
        } else {
            self.store.snapshot().len()
        };
        self.failure = None;
        self.last_scroll = None;
        self.generation += 1;
        self.last_submitted = None;
        {
            let mut state = self.shared.borrow_mut();
            state.presentation = None;
            state.document = None;
            state.overlays = Default::default();
            state.mode = Mode::Scroll;
            state.follow = if receiving {
                Follow::Following
            } else {
                Follow::Paused
            };
            state.dragging = false;
            state.pending_open = None;
            state.navigation += 1;
        }
        if let Some(nodes) = &self.nodes {
            ctx.set_hidden(nodes.rendered, self.active != ViewType::Rendered)?;
            if let Some(source) = nodes.source {
                ctx.set_hidden(source, true)?;
            }
        }
        self.schedule(ctx, true)?;
        Ok(self.store.snapshot().epoch)
    }
    /// The presentation currently taking input.
    #[command]
    pub fn view_type(&self) -> ViewType {
        self.active
    }
    /// The requested view while preparation is pending.
    #[command]
    pub fn requested_view_type(&self) -> ViewType {
        self.switching.as_ref().map_or(self.active, |s| s.target)
    }
    /// Switch representation when no modal or drag owns the interaction.
    #[command]
    pub fn set_view_type(&mut self, ctx: &mut dyn Context, view: ViewType) -> Result<()> {
        if !self.can_switch(ctx)? {
            return Err(invalid(
                "Close the inspector or finish the drag before switching",
            ));
        }
        if view == ViewType::Rendered && self.options.source_only {
            return Err(invalid(
                "Rendered is unavailable for a partial source preview",
            ));
        }
        if view == self.active {
            self.switching = None;
            self.generation += 1;
            self.last_submitted = None;
            return self.schedule(ctx, true);
        }
        let position = self.position(ctx)?;
        self.saved[index(self.active)] = Some((position, self.shared.borrow().navigation));
        self.switching = Some(Switch {
            target: view,
            minimum_revision: self.displayed_revision(),
        });
        self.generation += 1;
        self.last_submitted = None;
        self.failure = None;
        if self.use_cached_view(ctx, view, position)? {
            return Ok(());
        }
        self.schedule(ctx, true)
    }

    /// Reuse an already mounted current-revision surface without redoing
    /// layout.
    fn use_cached_view(
        &mut self,
        ctx: &mut dyn Context,
        view: ViewType,
        position: ReaderPosition,
    ) -> Result<bool> {
        let source = self.store.snapshot();
        let available = match view {
            ViewType::Rendered => self.shared.borrow().presentation.as_ref().is_some_and(|p| {
                p.document.source.epoch == source.epoch
                    && p.document.source.revision == source.revision
                    && p.geometry == self.geometry()
            }),
            ViewType::Source => {
                if let Some(node) = self.nodes.as_ref().and_then(|n| n.source) {
                    ctx.with_widget(node, |s: &MarkdownSource| {
                        Ok(s.epoch() == source.epoch
                            && s.revision() == source.revision
                            && s.prepared_width() == self.geometry_width
                            && s.options() == &self.source_options())
                    })?
                } else {
                    false
                }
            }
        };
        if !available {
            return Ok(false);
        }
        let focused = ctx.is_focused()
            || self
                .navigation_node()
                .is_some_and(|node| ctx.is_on_focus_path(node));
        self.active = view;
        self.switching = None;
        self.displayed = Some(source);
        self.results = None;
        self.search_index = None;
        self.source_results = None;
        self.shared.borrow_mut().overlays.search.clear();
        if let Some(nodes) = &self.nodes {
            ctx.set_hidden(nodes.rendered, view != ViewType::Rendered)?;
            if let Some(node) = nodes.source {
                ctx.set_hidden(node, view != ViewType::Source)?;
            }
        }
        if focused && let Some(node) = self.navigation_node() {
            ctx.set_focus(node)?;
        }
        if self.follow_state() == Follow::Following {
            self.reveal_tail(ctx)?;
        } else {
            let navigation = self.shared.borrow().navigation;
            let anchor = self.saved[index(view)]
                .filter(|(_, n)| *n == navigation)
                .map_or(position, |(p, _)| p);
            self.restore(ctx, anchor)?;
        }
        if self.query.text.is_empty() {
            self.dirty = false;
            self.deadline = None;
        } else {
            self.schedule(ctx, true)?;
        }
        self.trim_cache(ctx)?;
        self.update_chrome(ctx)?;
        self.wake_publication_host()?;
        Ok(true)
    }

    /// Retain only inactive geometry that fits the host's cache budget.
    fn trim_cache(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.active == ViewType::Source {
            let oversized = self
                .shared
                .borrow()
                .presentation
                .as_ref()
                .is_some_and(|p| p.derived_bytes() > self.options.cache_limit);
            if oversized {
                self.shared.borrow_mut().presentation = None;
            }
        } else if let Some(node) = self.nodes.as_ref().and_then(|n| n.source) {
            let (bytes, selection) = ctx.with_widget(node, |source: &MarkdownSource| {
                Ok((source.derived_bytes(), source.selection_bytes()))
            })?;
            if bytes > self.options.cache_limit {
                self.saved_source_selection = Some(selection);
                self.nodes.as_mut().expect("mounted source").source = None;
                ctx.remove_after_dispatch(node)?;
            }
        }
        Ok(())
    }
    /// Toggle Source and Rendered; a second invocation cancels a pending
    /// switch.
    #[command(enabled = "toggle_status")]
    pub fn toggle_view_type(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let target = if self.switching.is_some() {
            self.active
        } else if self.active == ViewType::Rendered {
            ViewType::Source
        } else {
            ViewType::Rendered
        };
        self.set_view_type(ctx, target)
    }
    /// Whether the view toggle is currently available.
    pub fn can_toggle_view_type(&self, ctx: &dyn ViewContext) -> Result<bool> {
        Ok(self.can_switch(ctx)? && !self.options.source_only)
    }
    /// Use the same admission policy for view commands and contextual help.
    fn toggle_status(&self, ctx: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(if self.can_toggle_view_type(ctx)? {
            CommandStatus::Enabled
        } else {
            CommandStatus::Disabled(
                "View switching is unavailable during an inspector, drag, or partial preview"
                    .into(),
            )
        })
    }
    /// Reject switches while a modal or any reader drag owns interaction.
    fn can_switch(&self, ctx: &dyn ViewContext) -> Result<bool> {
        Ok(self.inspector.is_none() && !self.drag_active(ctx)?)
    }
    /// Whether any Rendered, scrollbar, or mounted Source drag owns the
    /// pointer.
    fn drag_active(&self, ctx: &dyn ViewContext) -> Result<bool> {
        if self.shared.borrow().dragging || self.bar_drag {
            return Ok(true);
        }
        match self.nodes.as_ref().and_then(|n| n.source) {
            Some(node) => ctx.with_widget(node, |source: &MarkdownSource| {
                Ok(source.selection_drag_active())
            }),
            None => Ok(false),
        }
    }
    /// Whether the active Source surface holds or is dragging a selection.
    fn source_selecting(&self, ctx: &dyn ViewContext) -> Result<bool> {
        if self.active != ViewType::Source {
            return Ok(false);
        }
        match self.navigation_node() {
            Some(node) => ctx.with_widget(node, |source: &MarkdownSource| {
                Ok(!source.selection_range().is_empty() || source.selection_drag_active())
            }),
            None => Ok(false),
        }
    }
    /// Restrict a partial document to its exact retained source range.
    pub fn set_source_only(
        &mut self,
        ctx: &mut dyn Context,
        source_only: bool,
        status: Option<String>,
    ) -> Result<()> {
        if source_only
            && (self.active != ViewType::Source || self.requested_view_type() != ViewType::Source)
        {
            self.set_view_type(ctx, ViewType::Source)?;
        }
        self.options.source_only = source_only;
        self.options.status = status;
        self.update_chrome(ctx)
    }
    /// Change source wrap and number preferences without resetting the
    /// document.
    pub fn set_source_options(
        &mut self,
        ctx: &mut dyn Context,
        wrap: WrapMode,
        numbers: LineNumbers,
    ) -> Result<()> {
        self.options.source_wrap = wrap;
        self.options.line_numbers = numbers;
        self.generation += 1;
        self.last_submitted = None;
        self.schedule(ctx, true)
    }
    /// Enter or leave the rendered object-navigation mode.
    #[command]
    pub fn set_mode(&mut self, ctx: &mut dyn Context, mode: Mode) -> Result<()> {
        if mode == Mode::Interact {
            if let Some(nodes) = &self.nodes {
                ctx.with_widget_mut(nodes.rendered, |surface: &mut RenderedSurface, ctx| {
                    surface.enter_interact(ctx)
                })?;
            }
        } else {
            self.shared.borrow_mut().mode = mode;
        }
        self.update_chrome(ctx)
    }
    /// Enter object navigation without pausing follow until the cursor moves.
    #[command]
    pub fn interact(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.set_mode(ctx, Mode::Interact)
    }
    /// Extend or start rendered text selection at the reading position.
    #[command]
    pub fn select_text(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.active != ViewType::Rendered {
            return Err(invalid("Text selection command belongs to Rendered"));
        }
        if let Some(nodes) = &self.nodes {
            ctx.with_widget_mut(nodes.rendered, |surface: &mut RenderedSurface, ctx| {
                surface.start_selection(ctx)
            })?;
        }
        Ok(())
    }
    /// End the innermost document selection, otherwise return to scrolling.
    #[command]
    pub fn escape(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.bar_drag {
            ctx.release_mouse()?;
            self.bar_drag = false;
        }
        if let Some(nodes) = &self.nodes {
            if self.active == ViewType::Rendered {
                ctx.with_widget_mut(nodes.rendered, |surface: &mut RenderedSurface, ctx| {
                    surface.escape(ctx)
                })?;
            } else if let Some(source) = nodes.source {
                ctx.with_widget_mut(source, |source: &mut MarkdownSource, ctx| {
                    source.cancel_drag(ctx)
                })?;
            }
        }
        self.update_chrome(ctx)
    }
    /// Visit the next actionable object or leave through normal focus
    /// traversal.
    #[command(enabled = "rendered_action_status")]
    pub fn next_object(&mut self, ctx: &mut dyn Context, delta: i32) -> Result<()> {
        if self.active != ViewType::Rendered {
            return Ok(());
        }
        if let Some(nodes) = &self.nodes {
            ctx.with_widget_mut(nodes.rendered, |surface: &mut RenderedSurface, ctx| {
                surface.object_next(ctx, delta, true)
            })?;
        }
        Ok(())
    }
    /// Resume tail following explicitly.
    #[command]
    pub fn follow(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.shared.borrow_mut().follow = Follow::Following;
        self.reveal_tail(ctx)?;
        self.update_chrome(ctx)
    }
    /// Current follow policy.
    #[command]
    pub fn follow_state(&self) -> Follow {
        self.shared.borrow().follow
    }
    /// Accepted source revision, including a Finish transition.
    #[command]
    pub fn accepted_revision(&self) -> u64 {
        self.store.snapshot().revision
    }
    /// Revision painted by the active representation.
    #[command]
    pub fn displayed_revision(&self) -> u64 {
        self.displayed.as_ref().map_or(0, |s| s.revision)
    }
    /// Current document epoch.
    #[command]
    pub fn epoch(&self) -> u64 {
        self.store.snapshot().epoch
    }
    /// Whether the active view has caught up with accepted input and requested
    /// geometry.
    #[command]
    pub fn ready(&self) -> bool {
        self.displayed
            .as_ref()
            .is_some_and(|s| s.epoch == self.epoch() && s.revision == self.accepted_revision())
            && !self.dirty
            && !self.lane.as_ref().is_some_and(|lane| lane.is_pending())
            && self.switching.is_none()
            && self.failure.is_none()
    }
    /// The actual scrolling surface, when it is mounted.
    pub fn navigation_node(&self) -> Option<NodeId> {
        self.nodes.as_ref().and_then(|n| {
            if self.active == ViewType::Source {
                n.source
            } else {
                Some(n.rendered)
            }
        })
    }
    /// Current vertical viewport offset in terminal rows.
    pub fn view_offset(&self, ctx: &dyn ViewContext) -> u32 {
        self.navigation_node()
            .and_then(|n| ctx.view_of(n))
            .map_or(0, |v| v.scroll.y)
    }
    /// Scroll the active view while preserving object cursor and follow policy.
    pub fn scroll_view(&mut self, ctx: &mut dyn Context, op: ScrollOp) -> Result<()> {
        let Some(node) = self.navigation_node() else {
            return Ok(());
        };
        let before = ctx.view_of(node).map(|v| v.scroll);
        ctx.scroll_node(node, op)?;
        if let Some(view) = ctx.view_of(node)
            && before != Some(view.scroll)
        {
            self.shared.borrow_mut().navigation += 1;
            if view.scroll.y.saturating_add(view.content.h) < view.canvas.h {
                self.shared.borrow_mut().follow = Follow::Paused;
            }
        }
        self.update_chrome(ctx)
    }
    /// Capture the current logical viewport anchor.
    pub fn position(&self, ctx: &dyn ViewContext) -> Result<ReaderPosition> {
        if self.displayed.is_none() {
            return Ok(ReaderPosition {
                epoch: self.epoch(),
                view: self.active,
                source_byte: 0,
                reading: None,
                viewport_row: 0,
                horizontal: 0,
            });
        }
        let scroll = self
            .navigation_node()
            .and_then(|n| ctx.view_of(n))
            .map_or(Point { x: 0, y: 0 }, |v| v.scroll);
        let source_byte = if self.active == ViewType::Source {
            if let Some(node) = self.navigation_node() {
                ctx.with_widget(node, |source: &MarkdownSource| {
                    Ok(source.source_anchor_at(scroll).byte)
                })?
            } else {
                0
            }
        } else {
            self.shared
                .borrow()
                .presentation
                .as_ref()
                .and_then(|p| p.source_at_row(scroll.y))
                .unwrap_or(0)
        };
        let reading = if self.active == ViewType::Rendered {
            if let Some(node) = self.navigation_node() {
                ctx.with_widget(node, |surface: &RenderedSurface| {
                    Ok(surface.viewport_anchor(scroll))
                })?
            } else {
                None
            }
        } else {
            None
        };
        // The reading anchor's own byte, so the byte and the viewport row
        // describe one row, and a position restores exactly without its
        // reading identity, as it must after a reload.
        let source_byte = reading
            .and_then(|(position, _)| {
                self.shared
                    .borrow()
                    .presentation
                    .as_ref()?
                    .source_at_reading(position)
            })
            .unwrap_or(source_byte);
        Ok(ReaderPosition {
            epoch: self.epoch(),
            view: self.active,
            source_byte,
            reading: reading.map(|(p, _)| p),
            viewport_row: reading.map_or(0, |(_, row)| row),
            horizontal: scroll.x,
        })
    }
    /// Restore a same-epoch logical source location in the active view.
    pub fn restore_position(
        &mut self,
        ctx: &mut dyn Context,
        position: ReaderPosition,
    ) -> Result<()> {
        if position.epoch != self.epoch() {
            return Err(invalid("Reading position belongs to a replaced document"));
        }
        self.restore(ctx, position)
    }
    /// Restore a logical anchor through the target surface's prepared geometry.
    fn restore(&mut self, ctx: &mut dyn Context, position: ReaderPosition) -> Result<()> {
        if let Some(node) = self.navigation_node() {
            if self.active == ViewType::Source {
                let anchor = SourceAnchor {
                    epoch: self.epoch(),
                    byte: position.source_byte,
                };
                let point = ctx.with_widget(node, |source: &MarkdownSource| {
                    source.point_for_anchor(anchor).map_err(invalid)
                })?;
                let width = self
                    .nodes
                    .as_ref()
                    .and_then(|nodes| ctx.view_of(nodes.body))
                    .map_or(1, |view| view.content.w.max(1));
                ctx.with_widget_mut(node, |_: &mut MarkdownSource, ctx| {
                    ctx.reveal_area(
                        Rect::new(position.horizontal, point.y, width, 1),
                        RevealAlign::Top(position.viewport_row),
                    );
                    Ok(())
                })?;
            } else {
                ctx.with_widget_mut(node, |surface: &mut RenderedSurface, ctx| {
                    if let Some(reading) = position.reading
                        && surface.restore_viewport_anchor(ctx, reading, position.viewport_row)?
                    {
                        return Ok(());
                    }
                    surface.reveal_source(ctx, position.source_byte, position.viewport_row)
                })?;
            }
        }
        self.restoring = true;
        Ok(())
    }
    /// Retain a literal or fh-compatible smart-case regex query without moving
    /// the reader.
    pub fn set_search(&mut self, ctx: &mut dyn Context, query: String, regex: bool) -> Result<()> {
        if query.len() > self.options.query_limit {
            return Err(invalid("Search query exceeds the configured limit"));
        }
        self.query = SearchQuery {
            text: query,
            mode: if regex {
                SearchMode::Regex
            } else {
                SearchMode::Literal
            },
            case: if regex {
                SearchCase::Smart
            } else {
                SearchCase::Sensitive
            },
        };
        self.source_results = None;
        self.query_generation += 1;
        self.query_error = None;
        self.results = None;
        self.search_index = None;
        self.shared.borrow_mut().overlays.search.clear();
        if let Some(node) = self.nodes.as_ref().and_then(|nodes| nodes.source) {
            ctx.with_widget_mut(node, |source: &mut MarkdownSource, ctx| {
                source.clear_search(ctx);
                Ok(())
            })?;
        }
        self.last_submitted = None;
        self.schedule(ctx, true)
    }
    /// Edit the built-in search field using literal matching.
    #[command]
    pub fn search_changed(&mut self, ctx: &mut dyn Context, query: String) -> Result<()> {
        self.set_search(ctx, query, false)
    }
    /// Show the built-in search field, retaining the current query.
    #[command]
    pub fn start_search(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.shared.borrow_mut().follow = Follow::Paused;
        if let Some(nodes) = &self.nodes {
            let query = self.query.text.clone();
            ctx.with_widget_mut(nodes.search, |bar: &mut SearchBar, ctx| {
                bar.open(ctx, &query)
            })?;
        }
        Ok(())
    }
    /// Finish typing while retaining the query and its highlights.
    #[command]
    pub fn submit_search(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if let Some(node) = self.navigation_node() {
            ctx.set_focus(node)?;
        }
        Ok(())
    }
    /// Drop the query and close the built-in field.
    #[command]
    pub fn clear_search(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.set_search(ctx, String::new(), false)?;
        if let Some(nodes) = &self.nodes {
            ctx.with_widget_mut(nodes.search, |bar: &mut SearchBar, ctx| bar.close(ctx))?;
        }
        Ok(())
    }
    /// Count of retained matches in the active projection.
    #[command]
    pub fn search_matches(&self) -> usize {
        if self.active == ViewType::Rendered {
            self.results.as_ref().map_or(0, |r| r.matches.len())
        } else {
            self.source_results.as_ref().map_or(0, |r| r.ranges.len())
        }
    }
    /// One-based current match, or zero before explicit navigation.
    #[command]
    pub fn search_position(&self) -> usize {
        self.search_index.map_or(0, |i| i + 1)
    }
    /// The current query error without changing the retained query.
    pub fn search_error(&self) -> Option<&str> {
        self.query_error.as_deref()
    }
    /// Visit a retained match in the active representation.
    #[command]
    pub fn search_next(&mut self, ctx: &mut dyn Context, delta: i32) -> Result<()> {
        let count = self.search_matches();
        if count == 0 {
            return Ok(());
        }
        {
            let mut state = self.shared.borrow_mut();
            state.follow = Follow::Paused;
            state.navigation += 1;
        }
        let current = self
            .search_index
            .map(|i| i as i64)
            .unwrap_or(if delta < 0 { 0 } else { -1 });
        let next = (current + i64::from(delta)).rem_euclid(count as i64) as usize;
        self.search_index = Some(next);
        if self.active == ViewType::Source {
            if let Some(node) = self.navigation_node() {
                ctx.with_widget_mut(node, |source: &mut MarkdownSource, ctx| {
                    source.activate_match(ctx, next);
                    Ok(())
                })?;
            }
        } else if let Some(found) = self
            .results
            .as_ref()
            .and_then(|r| r.matches.get(next))
            .cloned()
        {
            let unit = found.address.unit_id();
            let presentation = self.shared.borrow().presentation.clone();
            if let Some(p) = presentation {
                self.shared.borrow_mut().overlays.active_object = match found.address {
                    SearchAddress::Unit { .. } => p.document.unit(unit).and_then(|unit| {
                        unit.spans
                            .iter()
                            .filter(|span| {
                                span.range.start < found.range.end
                                    && found.range.start < span.range.end
                            })
                            .find_map(|span| span.object)
                    }),
                    _ => p
                        .document
                        .objects
                        .iter()
                        .find(|o| o.unit_id == unit)
                        .map(|o| o.id),
                };
                if let Some(node) = self.navigation_node() {
                    let row = p.row_for_unit(unit, found.range.start);
                    ctx.with_widget_mut(node, |_: &mut RenderedSurface, ctx| {
                        ctx.reveal_area(Rect::new(0, row, 1, 1), RevealAlign::Nearest);
                        Ok(())
                    })?;
                }
            }
        }
        self.update_chrome(ctx)
    }
    /// Open the selected link, code, table, or source inspector.
    #[command(enabled = "rendered_action_status")]
    pub fn activate(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.active != ViewType::Rendered {
            return Ok(());
        }
        let p = self.shared.borrow().presentation.clone();
        let Some(p) = p else { return Ok(()) };
        let id = self.shared.borrow().overlays.active_object.or_else(|| {
            if self.search_index.is_some() {
                None
            } else {
                p.document.objects.first().map(|o| o.id)
            }
        });
        let Some(object) = id
            .and_then(|id| p.document.objects.iter().find(|o| o.id == id))
            .cloned()
        else {
            return Ok(());
        };
        let block = p
            .document
            .blocks
            .iter()
            .find(|b| b.unit_id == object.unit_id);
        let content = match &object.kind {
            ObjectKind::Code => {
                let Some(BlockKind::Code(code)) = block.map(|b| &b.kind) else {
                    return Ok(());
                };
                InspectorContent::Code {
                    info: code.info.clone(),
                    text: code.payload.clone(),
                }
            }
            ObjectKind::Table => {
                let Some(table) = p
                    .prepared_table(object.unit_id)
                    .and_then(|table| full_table(&table))
                else {
                    return Ok(());
                };
                InspectorContent::Table { table }
            }
            ObjectKind::Source => InspectorContent::Source {
                snapshot: p.document.source.clone(),
            },
            ObjectKind::Footnote { label } => {
                let byte = p.document.blocks.iter().find_map(|b| {
                    if let BlockKind::Footnote { label: other } = &b.kind {
                        (other == label).then_some(b.source.start)
                    } else {
                        None
                    }
                });
                if let Some(byte) = byte {
                    self.jump_source(ctx, byte)?;
                }
                return Ok(());
            }
            ObjectKind::Link(link) => {
                let resolved = link.resolve(self.base_url().as_ref());
                if let ResolvedLink::Fragment(anchor) = &resolved {
                    self.reveal_anchor(ctx, anchor)?;
                    return Ok(());
                }
                if self.options.follow_links
                    && self.follow_link(&resolved, p.document.source.revision, Some(object.id))?
                {
                    return self.update_chrome(ctx);
                }
                let (shown, allow_open) = match &resolved {
                    ResolvedLink::External(url) => (Some(url.to_string()), self.options.browser),
                    ResolvedLink::Local(local) => {
                        (Some(local_display(local)), self.options.local_links)
                    }
                    ResolvedLink::Unavailable(reason) => {
                        self.action_status.clone_from(reason);
                        (None, false)
                    }
                    ResolvedLink::Fragment(_) => unreachable!("fragments jump above"),
                };
                let content = InspectorContent::Link {
                    label: link.label.clone(),
                    original: link.destination.clone(),
                    resolved: shown,
                    allow_open,
                };
                self.inspected_link = Some(resolved);
                content
            }
        };
        self.open_inspector(ctx, content, Some(object.id), p.document.source.revision)?;
        if let Some(found) = self
            .search_index
            .and_then(|i| self.results.as_ref()?.matches.get(i))
            && let Some(open) = &self.inspector
        {
            match found.address {
                SearchAddress::CodeLine { unit_id, line } if unit_id == object.unit_id => {
                    open.handle.reveal_line(ctx, line + 1)?
                }
                SearchAddress::SourceCardLine { unit_id, .. } if unit_id == object.unit_id => {
                    if let Some(range) = &found.source_range {
                        open.handle.reveal_source_byte(ctx, range.start)?;
                    }
                }
                SearchAddress::TableCell {
                    unit_id,
                    row_key: Some(row),
                    column_key,
                } if unit_id == object.unit_id => open.handle.reveal_cell(
                    ctx,
                    &CellKey {
                        row: row.to_string().into(),
                        column: column_key.to_string().into(),
                    },
                )?,
                _ => {}
            }
        }
        Ok(())
    }
    /// Inspect a pinned snapshot of the displayed source.
    #[command]
    pub fn inspect_source(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let Some(snapshot) = self.displayed.clone() else {
            return Ok(());
        };
        let revision = snapshot.revision;
        self.open_inspector(ctx, InspectorContent::Source { snapshot }, None, revision)
    }
    /// Open the current rendered heading outline.
    #[command]
    pub fn outline(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let Some(p) = self.shared.borrow().presentation.clone() else {
            return Ok(());
        };
        self.open_inspector(
            ctx,
            InspectorContent::Outline {
                entries: outline_entries(&p.document),
            },
            None,
            p.document.source.revision,
        )
    }
    /// Lazily mount one pinned modal and pause following on explicit
    /// inspection.
    fn open_inspector(
        &mut self,
        ctx: &mut dyn Context,
        content: InspectorContent,
        object: Option<u64>,
        revision: u64,
    ) -> Result<()> {
        if self.inspector.is_some() {
            return Err(invalid("Close the current inspector first"));
        }
        self.shared.borrow_mut().follow = Follow::Paused;
        let body = self
            .nodes
            .as_ref()
            .map(|nodes| nodes.body)
            .ok_or_else(|| invalid("The reader is not mounted"))?;
        let handle = Inspector::open_configured(
            ctx,
            body,
            self.options.executor.clone(),
            content,
            self.options.clipboard,
            self.options.copy_limit,
            self.options.syntax_limit,
        )?;
        self.inspector = Some(OpenInspector {
            handle,
            object,
            epoch: self.epoch(),
            revision,
        });
        self.update_chrome(ctx)
    }
    /// Close the active inspector and restore the document's focus.
    #[command]
    pub fn dismiss_inspector(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if let Some(open) = self.inspector.take() {
            let mut failure = None;
            for event in open.handle.take_closing_events(ctx)? {
                if let Err(error) =
                    self.apply_inspector_event(ctx, event, open.epoch, open.revision)
                {
                    failure = Some(format!("Failed: {error}"));
                }
            }
            open.handle.close(ctx)?;
            if let Some(failure) = failure {
                self.action_status = failure;
            }
        }
        // Events still queued in the inspector have followed its link.
        self.inspected_link = None;
        self.update_chrome(ctx)
    }
    /// Admit explicit inspector effects on the document's ordered action lane.
    fn apply_inspector_event(
        &mut self,
        ctx: &mut dyn Context,
        event: InspectorEvent,
        epoch: u64,
        revision: u64,
    ) -> Result<()> {
        if epoch != self.epoch() {
            return Ok(());
        }
        match event {
            InspectorEvent::Close => self.dismiss_inspector(ctx)?,
            InspectorEvent::Navigate(byte) => {
                self.dismiss_inspector(ctx)?;
                self.jump_source(ctx, byte)?;
            }
            InspectorEvent::CopyText(text) => {
                self.queue_copy(ctx, CopyInput::Text(text.into()), None, revision)?;
            }
            InspectorEvent::CopySource { text, range } => {
                self.queue_copy(ctx, CopyInput::Source { text, range }, None, revision)?;
            }
            InspectorEvent::OpenUrl(_) => {
                if let Some(link) = self.inspected_link.clone() {
                    self.follow_link(&link, revision, None)?;
                }
            }
        }
        Ok(())
    }
    /// Where the selected link leads, so it can be checked before it is
    /// followed.
    fn selected_destination(&self) -> Option<String> {
        let shared = self.shared.borrow();
        let id = shared.overlays.active_object?;
        let p = shared.presentation.as_ref()?;
        let ObjectKind::Link(link) = &p.document.objects.iter().find(|o| o.id == id)?.kind else {
            return None;
        };
        Some(match link.resolve(self.base_url().as_ref()) {
            ResolvedLink::Fragment(anchor) => format!("#{anchor}"),
            ResolvedLink::External(url) => url.to_string(),
            ResolvedLink::Local(local) => local_display(&local),
            ResolvedLink::Unavailable(reason) => reason,
        })
    }
    /// Ask the host to follow a link it can service: a URL through its
    /// browser, a document of its own through its local links. Returns
    /// whether a request went out.
    fn follow_link(
        &mut self,
        link: &ResolvedLink,
        revision: u64,
        object: Option<u64>,
    ) -> Result<bool> {
        let (action, status) = match link {
            ResolvedLink::External(url) if self.options.browser => {
                (HostAction::OpenUrl(url.to_string()), "Opening")
            }
            ResolvedLink::Local(local) if self.options.local_links => {
                (HostAction::OpenLocal(local.clone()), "Following")
            }
            _ => return Ok(false),
        };
        let reserved = self
            .requests
            .reserve(self.epoch(), revision, object, self.options.action_limit)
            .ok_or_else(|| invalid("The host request queue is full"))?;
        self.requests.publish(reserved.id, action);
        self.action_status = status.into();
        Ok(true)
    }
    /// Reveal what a link's `#` anchor names: a block by its `^id`, or a
    /// heading by its fragment slug or by its text without regard to case.
    /// Returns whether one matched.
    pub fn reveal_anchor(&mut self, ctx: &mut dyn Context, anchor: &str) -> Result<bool> {
        let Some(p) = self.shared.borrow().presentation.clone() else {
            return Ok(false);
        };
        let anchor = anchor.trim();
        let start = if let Some(id) = anchor.strip_prefix('^') {
            p.document
                .block_anchors
                .iter()
                .find(|block| block.id == id)
                .map(|block| block.source.start)
        } else {
            let slug = heading_slug(anchor);
            p.document
                .outline
                .iter()
                .find(|e| {
                    e.slug == anchor || e.slug == slug || e.text.trim().eq_ignore_ascii_case(anchor)
                })
                .map(|e| e.source.start)
        };
        let Some(start) = start else {
            return Ok(false);
        };
        self.jump_source(ctx, start)?;
        Ok(true)
    }
    /// Save one return location and navigate to an accepted source byte.
    fn jump_source(&mut self, ctx: &mut dyn Context, byte: usize) -> Result<()> {
        let before = self.position(ctx)?;
        if self.jumps.len() == JUMP_LIMIT {
            self.jumps.pop_front();
        }
        self.jumps.push_back(before);
        self.return_position = Some(before);
        self.shared.borrow_mut().follow = Follow::Paused;
        self.shared.borrow_mut().navigation += 1;
        self.restore(
            ctx,
            ReaderPosition {
                epoch: self.epoch(),
                view: self.active,
                source_byte: byte,
                reading: None,
                viewport_row: 0,
                horizontal: 0,
            },
        )
    }
    /// Drain requests for the images the displayed document's image blocks
    /// show, each once per document. Remote images are never fetched; they
    /// keep their alt text. Without the `images` option, nothing is asked.
    pub fn take_resource_requests(&mut self) -> Vec<ResourceRequest> {
        let mut requests = Vec::new();
        if !self.options.images {
            return requests;
        }
        let Some(p) = self.shared.borrow().presentation.clone() else {
            return requests;
        };
        if p.document.source.epoch != self.epoch() {
            return requests;
        }
        for block in &p.document.blocks {
            let BlockKind::Image(image) = &block.kind else {
                continue;
            };
            let destination = &image.link.destination;
            if self.image_states.contains_key(destination) {
                continue;
            }
            if self.resource_requests.len() >= RESOURCE_LIMIT
                || self.image_states.len() >= IMAGE_LIMIT
            {
                break;
            }
            let ResolvedLink::Local(target) = image.link.resolve(self.base_url().as_ref()) else {
                self.image_states
                    .insert(destination.clone(), ImageState::Settled);
                continue;
            };
            self.resource_id += 1;
            self.image_states
                .insert(destination.clone(), ImageState::Requested);
            self.resource_requests
                .insert(self.resource_id, destination.clone());
            requests.push(ResourceRequest {
                id: self.resource_id,
                epoch: self.epoch(),
                target,
                kind: ResourceKind::Image,
            });
        }
        requests
    }
    /// Whether an image block of the displayed document still waits for its
    /// image: not yet asked for, or asked for and unanswered.
    #[command]
    pub fn images_pending(&self) -> bool {
        if !self.options.images {
            return false;
        }
        let shared = self.shared.borrow();
        let Some(p) = shared.presentation.as_ref() else {
            return false;
        };
        p.document.blocks.iter().any(|block| {
            matches!(&block.kind, BlockKind::Image(image)
                if self.image_states.get(&image.link.destination) != Some(&ImageState::Settled))
        })
    }
    /// Take the host's answer to a resource request. An answer for another
    /// document, or for an unknown request, changes nothing. An image lays
    /// out again at its own aspect; a failure shows its reason.
    pub fn supply_resource(
        &mut self,
        ctx: &mut dyn Context,
        id: u64,
        resource: Resource,
    ) -> Result<()> {
        let Some(destination) = self.resource_requests.remove(&id) else {
            return Ok(());
        };
        let info = match resource {
            Resource::Image(image) => {
                let info = ImageInfo::Size {
                    width: image.width(),
                    height: image.height(),
                };
                self.shared
                    .borrow_mut()
                    .overlays
                    .images
                    .images
                    .insert(destination.clone(), image);
                info
            }
            Resource::Missing => ImageInfo::Failed("not found".into()),
            Resource::Failed(reason) => ImageInfo::Failed(reason),
        };
        self.image_states
            .insert(destination.clone(), ImageState::Settled);
        Arc::make_mut(&mut self.image_sizes).insert(destination, info);
        self.generation += 1;
        self.schedule(ctx, true)
    }
    /// Drain the positions the reader left by its own jumps (fragments,
    /// block and heading anchors, footnotes, and the outline), oldest first.
    /// The reader keeps at most a few; a host with a history drains them
    /// each turn.
    pub fn take_jumps(&mut self) -> Vec<ReaderPosition> {
        self.jumps.drain(..).collect()
    }
    /// Return once from a fragment, footnote, or outline jump.
    #[command]
    pub fn return_from_jump(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if let Some(position) = self.return_position.take() {
            self.restore(ctx, position)?;
        }
        Ok(())
    }
    /// Copy the active selection or selected object's default payload.
    #[command(enabled = "copy_status")]
    pub fn copy(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.displayed.is_none() {
            return Ok(());
        }
        if self.active == ViewType::Source {
            let Some(node) = self.navigation_node() else {
                return Ok(());
            };
            let (text, range) = ctx.with_widget(node, |source: &MarkdownSource| {
                Ok((source.source_text(), source.selection_range()))
            })?;
            return self.queue_copy(
                ctx,
                CopyInput::Source { text, range },
                None,
                self.displayed_revision(),
            );
        }
        let Some(p) = self.shared.borrow().presentation.clone() else {
            return Ok(());
        };
        let selection = self.shared.borrow().overlays.selection.clone();
        if let Some(selection) = selection {
            return self.queue_copy(
                ctx,
                CopyInput::Reading {
                    document: p.document.clone(),
                    selection: Some(selection),
                },
                None,
                p.document.source.revision,
            );
        }
        let selected = self.shared.borrow().overlays.active_object;
        if let Some(object) = selected.and_then(|id| p.document.objects.iter().find(|o| o.id == id))
        {
            self.queue_copy(
                ctx,
                CopyInput::Object {
                    document: p.document.clone(),
                    object_id: object.id,
                },
                Some(object.id),
                p.document.source.revision,
            )?;
        }
        Ok(())
    }
    /// Copy the exact displayed source, including its final newline.
    #[command(enabled = "copy_status")]
    pub fn copy_source(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let Some(snapshot) = self.displayed.clone() else {
            return Ok(());
        };
        let text = snapshot.text();
        let len = text.len();
        self.queue_copy(
            ctx,
            CopyInput::Source {
                text,
                range: 0..len,
            },
            None,
            snapshot.revision,
        )
    }
    /// Admit copying only with a host service and a displayed source revision.
    fn copy_status(&self, _: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(if !self.options.clipboard {
            CommandStatus::Disabled("The host has no clipboard service".into())
        } else if self.displayed.is_none() {
            CommandStatus::Disabled("No document revision has been displayed".into())
        } else {
            CommandStatus::Enabled
        })
    }
    /// Expose active-surface admission for explicit Rendered commands.
    fn rendered_action_status(&self, _: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(if self.rendered_actions_available() {
            CommandStatus::Enabled
        } else {
            CommandStatus::Disabled("Rendered actions require a prepared Rendered view".into())
        })
    }
    /// Admit Rendered actions only while prepared Rendered content is active.
    fn rendered_actions_available(&self) -> bool {
        self.active == ViewType::Rendered
            && self.displayed.is_some()
            && self.shared.borrow().presentation.is_some()
    }
    /// Reserve a host action before submitting bounded immutable copy work.
    fn queue_copy(
        &mut self,
        ctx: &mut dyn Context,
        input: CopyInput,
        object: Option<u64>,
        revision: u64,
    ) -> Result<()> {
        if !self.options.clipboard {
            return Err(invalid("The host has no clipboard service"));
        }
        let lane = self
            .lane
            .as_ref()
            .ok_or_else(|| invalid("The reader is not mounted"))?;
        let context = self
            .requests
            .reserve(self.epoch(), revision, object, self.options.action_limit)
            .ok_or_else(|| invalid("The host request queue is full"))?;
        let result = lane.submit_copy(context.id, input, self.options.copy_limit);
        if let Err(error) = result {
            self.requests.cancel(context.id);
            return Err(invalid(error));
        }
        self.action_status = "Copying".into();
        self.update_chrome(ctx)
    }
    /// Drain complete immutable requests once. The host performs each action
    /// and replies.
    pub fn take_requests(&mut self, limit: usize) -> Vec<HostRequest> {
        self.requests.take(limit)
    }
    /// Complete one host effect; duplicate and replaced-document replies are
    /// ignored.
    pub fn complete_request(&mut self, ctx: &mut dyn Context, reply: HostReply) -> Result<()> {
        if let Some(context) = self.requests.complete(reply.id)
            && context.epoch == self.epoch()
        {
            self.action_status = match reply.outcome {
                HostOutcome::Success => "Done".into(),
                HostOutcome::Rejected(reason) => format!("Rejected: {reason}"),
                HostOutcome::Failed(reason) => format!("Failed: {reason}"),
            };
        }
        self.update_chrome(ctx)
    }
    /// Latest truthful status of a requested host action.
    #[command]
    pub fn action_status(&self) -> String {
        self.action_status.clone()
    }
    /// Whether an inspector is open over the reader.
    pub fn inspector_open(&self) -> bool {
        self.inspector.is_some()
    }
    /// Whether an open inspector has published all requested preparation.
    pub fn inspector_ready(&self, ctx: &dyn ViewContext) -> Result<bool> {
        self.inspector
            .as_ref()
            .map_or(Ok(false), |open| open.handle.ready(ctx))
    }
    /// Read bounded reader state without exposing the document payload.
    #[command]
    pub fn state(&self, ctx: &mut dyn Context) -> Result<ArgValue> {
        queries::state(self, ctx)
    }
    /// Read a bounded page of rendered objects with its snapshot revision.
    #[command]
    pub fn object_window(&self, offset: usize, limit: usize) -> ArgValue {
        queries::objects(self, offset, limit)
    }
    /// Read a bounded page of headings with its snapshot revision.
    #[command]
    pub fn outline_window(&self, offset: usize, limit: usize) -> ArgValue {
        queries::outline(self, offset, limit)
    }
    /// Read a bounded page of matches in the active search projection.
    #[command]
    pub fn search_window(&self, offset: usize, limit: usize) -> ArgValue {
        queries::search(self, offset, limit)
    }
    /// Retry the accepted source after a worker failure.
    #[command]
    pub fn retry(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.failure = None;
        self.last_submitted = None;
        self.schedule(ctx, true)
    }

    /// The host base for relative links, ignoring an unparsable preference.
    fn base_url(&self) -> Option<url::Url> {
        self.options
            .base_url
            .as_deref()
            .and_then(|base| url::Url::parse(base).ok())
    }
    /// Capture Source layout and syntax preferences for worker preparation.
    fn source_options(&self) -> SourceOptions {
        SourceOptions {
            wrap: self.options.source_wrap,
            line_numbers: self.options.line_numbers,
            syntax_limit: self.options.syntax_limit,
            ..Default::default()
        }
    }
    /// Capture all preferences that influence Rendered layout geometry.
    fn geometry(&self) -> GeometryKey {
        GeometryKey {
            width: self.geometry_width,
            glyphs: self.options.glyphs,
            prose_width: self.options.prose_width,
            syntax_limit: self.options.syntax_limit,
            mosaic: self.mosaic,
            ..Default::default()
        }
    }
    /// Mark replaceable work dirty and request immediate or coalesced owner
    /// polling.
    fn schedule(&mut self, ctx: &mut dyn Context, immediate: bool) -> Result<()> {
        self.dirty = true;
        if immediate {
            self.deadline = None;
        } else if self.deadline.is_none() {
            self.deadline = Some(ctx.now() + self.options.coalesce);
        }
        ctx.request_poll()
    }
    /// Admit the latest source, geometry, query, and published semantic bases.
    fn submit_work(&mut self, ctx: &dyn ViewContext) -> Result<()> {
        if self.geometry_width == 0 {
            return Ok(());
        }
        let source = self.store.snapshot();
        let target = self.requested_view_type();
        let geometry = self.geometry();
        let key = (
            source.epoch,
            source.revision,
            target,
            geometry,
            self.generation,
            self.query_generation,
        );
        if self.last_submitted == Some(key) {
            self.dirty = false;
            return Ok(());
        }
        let previous = self.shared.borrow().presentation.clone();
        let search = Some(WorkSearch {
            query: self.query.clone(),
            generation: self.query_generation,
            limits: SearchLimits {
                max_query_bytes: self.options.query_limit,
                max_matches: self.options.search_limit,
                max_scan_bytes: self.options.source_limit,
            },
        });
        let previous_source = self
            .source_snapshot
            .as_ref()
            .filter(|old| old.epoch == source.epoch)
            .map(SourceSnapshot::text);
        let request = WorkRequest {
            previous_source,
            source,
            view: target,
            geometry,
            source_options: self.source_options(),
            previous,
            previous_document: self.last_document.clone(),
            limits: self.options.parse_limits,
            generation: self.generation,
            search,
            images: self.image_sizes.clone(),
        };
        let Some(lane) = &self.lane else {
            return Ok(());
        };
        match lane.submit(request) {
            Ok(()) => {
                self.last_submitted = Some(key);
                self.dirty = false;
                self.deadline = None;
            }
            Err(error) => {
                self.failure = Some(error.to_string());
                self.dirty = true;
                self.deadline =
                    Some(ctx.now() + self.options.coalesce.max(Duration::from_millis(1)));
            }
        }
        Ok(())
    }
    /// Validate a completion and reconcile current reader state before atomic
    /// publication.
    fn receive(&mut self, ctx: &mut dyn Context, result: WorkResult) -> Result<()> {
        let tags = &result.tags;
        if tags.epoch != self.epoch()
            || tags.view != self.requested_view_type()
            || tags.geometry != self.geometry()
            || tags.source_options != self.source_options()
            || tags.generation != self.generation
            || tags.revision < self.displayed_revision()
        {
            self.dirty = true;
            self.last_submitted = None;
            return Ok(());
        }
        if self
            .switching
            .as_ref()
            .is_some_and(|s| tags.revision < s.minimum_revision)
        {
            self.dirty = true;
            self.last_submitted = None;
            return Ok(());
        }
        let prepared = match result.result {
            Ok(prepared) => prepared,
            Err(error) => {
                self.failure = Some(error.to_string());
                self.switching = None;
                self.dirty = false;
                self.wake_publication_host()?;
                return Ok(());
            }
        };
        let position = self.position(ctx)?;
        let was_focused = ctx.is_focused()
            || self
                .navigation_node()
                .is_some_and(|node| ctx.is_on_focus_path(node));
        let Some(nodes) = &mut self.nodes else {
            return Ok(());
        };
        match prepared {
            PreparedView::Rendered(p) => {
                self.last_document = Some(p.document.clone());
                let clear = ctx
                    .with_widget_mut(nodes.rendered, |surface: &mut RenderedSurface, ctx| {
                        surface.publish(ctx, p.clone())
                    })?;
                if clear {
                    ctx.notify(NoticePost::info(
                        "Selection cleared because its displayed text changed",
                    ))?;
                }
                if let Some(open) = &self.inspector {
                    open.handle
                        .publish_outline(ctx, || outline_entries(&p.document))?;
                    if let Some(id) = open.object {
                        match p.document.objects.iter().find(|o| o.id == id) {
                            Some(object) if matches!(object.kind, ObjectKind::Table) => {
                                if let Some(table) = p
                                    .prepared_table(object.unit_id)
                                    .and_then(|table| full_table(&table))
                                {
                                    open.handle.publish_table(ctx, table)?;
                                }
                            }
                            None => {
                                self.dismiss_inspector(ctx)?;
                                ctx.notify(NoticePost::info("The inspected object changed"))?;
                            }
                            _ => {}
                        }
                    }
                }
            }
            PreparedView::Source(prepared) => {
                if let Some(source) = nodes.source {
                    if let Err(error) = ctx
                        .with_widget_mut(source, |source: &mut MarkdownSource, _| {
                            source.publish(*prepared).map_err(invalid)
                        })
                    {
                        self.failure = Some(error.to_string());
                        self.last_submitted = None;
                        self.dirty = true;
                        self.wake_publication_host()?;
                        return Ok(());
                    }
                } else {
                    let mut source = MarkdownSource::from_prepared(*prepared);
                    if let Some((anchor, head)) = self.saved_source_selection.take() {
                        source.set_selection(anchor, head).map_err(invalid)?;
                    }
                    let source = ctx.add_child(nodes.body, source)?;
                    nodes.source = Some(source.into());
                }
                self.source_snapshot = Some(result.source.clone());
            }
        }
        let switched = self.switching.take().is_some();
        self.active = tags.view;
        self.displayed = Some(result.source);
        self.failure = None;
        if let Some(nodes) = &self.nodes {
            ctx.set_hidden(nodes.rendered, self.active != ViewType::Rendered)?;
            if let Some(source) = nodes.source {
                ctx.set_hidden(source, self.active != ViewType::Source)?;
            }
        }
        if switched {
            self.results = None;
            self.search_index = None;
            self.source_results = None;
            if was_focused && let Some(node) = self.navigation_node() {
                ctx.set_focus(node)?;
            }
        }
        if self.shared.borrow().follow == Follow::Following {
            self.reveal_tail(ctx)?;
        } else if switched || self.active == ViewType::Source {
            let navigation = self.shared.borrow().navigation;
            let restore = if switched {
                self.saved[index(self.active)]
                    .filter(|(_, n)| *n == navigation)
                    .map_or(position, |p| p.0)
            } else {
                position
            };
            self.restore(ctx, restore)?;
        }
        if tags.query_generation == Some(self.query_generation) {
            if let Some(search) = result.search {
                self.apply_search(ctx, search)?;
            }
        } else {
            self.dirty = true;
            self.last_submitted = None;
        }
        if self.accepted_revision() > self.displayed_revision() {
            self.dirty = true;
            self.last_submitted = None;
        }
        self.trim_cache(ctx)?;
        self.update_chrome(ctx)?;
        self.wake_publication_host()
    }
    /// Signal a host-owned search/status surface only after publication has
    /// accepted a result or changed the displayed view or failure state.
    fn wake_publication_host(&self) -> Result<()> {
        if let Some(wake) = &self.options.publication_wake {
            wake.wake()?;
        }
        Ok(())
    }
    /// Install bounded matches or a query error in place of all prior match
    /// state, without moving the reader or stealing prompt focus.
    fn apply_search(
        &mut self,
        ctx: &mut dyn Context,
        search: StdResult<SearchOutput, String>,
    ) -> Result<()> {
        let output = match search {
            Ok(output) => output,
            Err(error) => {
                self.query_error = Some(error);
                self.results = None;
                self.source_results = None;
                self.search_index = None;
                self.shared.borrow_mut().overlays.search.clear();
                return Ok(());
            }
        };
        self.query_error = None;
        match output {
            SearchOutput::Source(matches) => {
                let current = self
                    .search_index
                    .and_then(|i| self.source_results.as_ref()?.ranges.get(i));
                self.search_index =
                    current.and_then(|range| matches.ranges.iter().position(|next| next == range));
                self.source_results = Some(matches.clone());
                if let Some(node) = self.nodes.as_ref().and_then(|n| n.source) {
                    ctx.with_widget_mut(node, |source: &mut MarkdownSource, _| {
                        source.publish_matches(matches).map_err(invalid)
                    })?;
                }
            }
            SearchOutput::Rendered(results) => {
                let current = self
                    .search_index
                    .and_then(|i| self.results.as_ref()?.matches.get(i));
                self.search_index = current
                    .and_then(|current| results.matches.iter().position(|found| found == current));
                self.shared.borrow_mut().overlays.search = results
                    .matches
                    .iter()
                    .filter_map(|found| {
                        let unit_id = found.address.unit_id();
                        let range = match found.address {
                            SearchAddress::Unit { .. } => found.range.clone(),
                            _ => 0..self.last_document.as_ref()?.unit(unit_id)?.text.len(),
                        };
                        Some(ReadingRange {
                            start: ReadingPosition {
                                unit_id,
                                offset: range.start,
                            },
                            end: ReadingPosition {
                                unit_id,
                                offset: range.end,
                            },
                        })
                    })
                    .collect();
                self.results = Some(results);
            }
        }
        Ok(())
    }
    /// Reveal the last readable row using the active surface's prepared
    /// geometry.
    fn reveal_tail(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if let Some(node) = self.navigation_node() {
            let row = if self.active == ViewType::Source {
                ctx.with_widget(node, |source: &MarkdownSource| {
                    source
                        .point_for_anchor(SourceAnchor {
                            epoch: source.epoch(),
                            byte: source.source_text().len(),
                        })
                        .map(|p| p.y)
                        .map_err(invalid)
                })?
            } else {
                self.shared
                    .borrow()
                    .presentation
                    .as_ref()
                    .map_or(0, |p| p.height().saturating_sub(1))
            };
            if self.active == ViewType::Source {
                ctx.with_widget_mut(node, |_: &mut MarkdownSource, ctx| {
                    ctx.reveal_area(Rect::new(0, row, 1, 1), RevealAlign::Nearest);
                    Ok(())
                })?;
            } else {
                ctx.with_widget_mut(node, |_: &mut RenderedSurface, ctx| {
                    ctx.reveal_area(Rect::new(0, row, 1, 1), RevealAlign::Nearest);
                    Ok(())
                })?;
            }
        }
        self.restoring = true;
        Ok(())
    }
    /// Whether completed input still awaits its final displayed revision.
    fn finishing(&self, accepted: &SourceSnapshot) -> bool {
        matches!(
            accepted.lifecycle,
            Lifecycle::Complete(FinishOutcome::Completed)
        ) && self.displayed_revision() < accepted.revision
    }
    /// Refresh fixed status and query chrome without changing document state.
    fn update_chrome(&self, ctx: &mut dyn Context) -> Result<()> {
        let snapshot = self.store.snapshot();
        let lifecycle = match &snapshot.lifecycle {
            Lifecycle::Receiving => "Receiving".to_owned(),
            Lifecycle::Complete(FinishOutcome::Completed) if self.finishing(&snapshot) => {
                "Finishing".into()
            }
            Lifecycle::Complete(FinishOutcome::Completed) => "Complete".into(),
            Lifecycle::Complete(FinishOutcome::Aborted(reason)) => format!("Aborted: {reason}"),
        };
        let label = if self.active == ViewType::Source {
            "Rendered | [Source]"
        } else {
            "[Rendered] | Source"
        };
        let update = if self.switching.is_some() {
            " · Switching"
        } else if self.displayed_revision() < snapshot.revision || self.dirty {
            " · Updating"
        } else {
            ""
        };
        let follow = if self.shared.borrow().follow == Follow::Following {
            "Following"
        } else {
            "Paused"
        };
        let mode = if self.active == ViewType::Source {
            "Source"
        } else if self.shared.borrow().mode == Mode::Interact {
            "Interact"
        } else {
            "Scroll"
        };
        let new_bytes = snapshot.len().saturating_sub(self.acknowledged);
        let new = if new_bytes > 0 && self.shared.borrow().follow == Follow::Paused {
            format!(" · {new_bytes} new bytes")
        } else {
            String::new()
        };
        let text = format!(
            "{label}  {mode} · {lifecycle} · {follow}{update}{new}{}{}{}",
            self.options
                .status
                .as_ref()
                .map_or(String::new(), |s| format!(" · {s}")),
            self.failure
                .as_ref()
                .map_or(String::new(), |s| format!(" · {s}")),
            if self.action_status.is_empty() {
                self.selected_destination()
                    .map_or(String::new(), |shown| format!(" · → {shown}"))
            } else {
                format!(" · {}", self.action_status)
            }
        );
        let enabled = self.can_toggle_view_type(ctx)?;
        *self.chrome.borrow_mut() = ChromeState {
            text,
            enabled,
            active: self.active,
        };
        if !self.options.external_search
            && let Some(nodes) = &self.nodes
        {
            let count = self.search_matches();
            let status = self.query_error.clone().unwrap_or_else(|| {
                format!(
                    "{} of {} · {:?}",
                    self.search_position(),
                    count,
                    self.active
                )
            });
            ctx.with_widget_mut(nodes.search, |bar: &mut SearchBar, ctx| {
                bar.set_status(ctx, status, count > 0)
            })?;
        }
        Ok(())
    }
    /// Compute scrollbar tracks from the current active child allocation.
    fn tracks(&self, ctx: &dyn ViewContext) -> ScrollbarTracks {
        // An open inspector covers the document, whose position it hides.
        if self.inspector.is_some() {
            return (vec![], vec![]);
        }
        let Some(node) = self.navigation_node() else {
            return (vec![], vec![]);
        };
        let Some(view) = ctx.view_of(node) else {
            return (vec![], vec![]);
        };
        let owner = ctx.view().outer;
        let x = (view.content.tl.x - owner.tl.x).max(0) as u32;
        let y = (view.content.tl.y - owner.tl.y).max(0) as u32;
        (
            vec![(node, Rect::new(x + view.content.w, y, 1, view.content.h))],
            vec![(node, Rect::new(x, y + view.content.h, view.content.w, 1))],
        )
    }
}

/// Lay out a previewed table for its inspector, which shows every value in
/// full: values wrap within their columns, and the grid scrolls sideways when
/// the columns are wider than the inspector.
fn full_table(preview: &PreparedTable) -> Option<Arc<PreparedTable>> {
    table::prepare(
        preview.data().clone(),
        TableLayoutOptions {
            glyphs: preview.glyphs(),
            wrap: true,
            ..TableLayoutOptions::default()
        },
    )
    .ok()
    .map(Arc::new)
}

impl Widget for Markdown {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }
    fn accept_focus(&self, _: &dyn ViewContext) -> bool {
        true
    }
    fn owns_scrollbars(&self) -> bool {
        true
    }
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let owner = ctx.node_id();
        let wake = ctx.wake_handle(PollLifetime::Node)?;
        self.wake = Some(wake.clone());
        self.shared.borrow_mut().wake = Some(wake.clone());
        self.lane = Some(
            self.options
                .executor
                .register(Some(wake))
                .map_err(invalid)?,
        );
        let target = CommandTarget::Exact(owner);
        let search = ctx.add_child(
            owner,
            SearchBar::new()
                .with_on_change(Self::spec_search_changed().call().with_target(target))
                .with_on_submit(Self::call_submit_search().with_target(target))
                .with_on_cancel(Self::call_clear_search().with_target(target)),
        )?;
        ctx.set_hidden(search.into(), true)?;
        let body = ctx.add_child(owner, Container::stack().with_name("markdown_body"))?;
        // The override replaces the container's own layout, so it names the
        // stack again: an open inspector lies over the reading surface.
        ctx.set_layout_override(
            body.into(),
            Layout::fill()
                .direction(Direction::Stack)
                .padding(Edges {
                    left: 0,
                    right: 1,
                    top: 0,
                    bottom: 1,
                })
                .into(),
        )?;
        let rendered = ctx.add_child(body, RenderedSurface::new(self.shared.clone()))?;
        let chrome = ctx.add_child(
            owner,
            Chrome {
                state: self.chrome.clone(),
                owner,
            },
        )?;
        self.nodes = Some(Nodes {
            body: body.into(),
            rendered: rendered.into(),
            source: None,
            search: search.into(),
            _chrome: chrome.into(),
        });
        self.update_chrome(ctx)?;
        ctx.request_poll()
    }
    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        render.fill("markdown", ctx.view().outer_rect_local(), ' ')?;
        if let Some(nodes) = &self.nodes {
            let width = ctx.view_of(nodes.body).map_or(0, |v| v.content.w);
            let mosaic = tier_for(render.glyphs());
            if width != self.geometry_width || mosaic != self.mosaic {
                self.geometry_width = width;
                self.mosaic = mosaic;
                self.generation += 1;
                self.dirty = true;
                self.last_submitted = None;
                if let Some(wake) = &self.wake {
                    let _ = wake.wake()?;
                }
            }
        }
        if self.follow_state() == Follow::Following && self.source_selecting(ctx)? {
            self.shared.borrow_mut().follow = Follow::Paused;
            if let Some(wake) = &self.wake {
                let _ = wake.wake()?;
            }
        }
        if self.deferred_publication
            && let Some(wake) = &self.wake
        {
            let _ = wake.wake()?;
        }
        let (vertical, horizontal) = self.tracks(ctx);
        self.vertical.render(render, ctx, &vertical)?;
        self.horizontal.render(render, ctx, &horizontal)?;
        if let Some(view) = self.navigation_node().and_then(|node| ctx.view_of(node)) {
            if self
                .last_scroll
                .is_some_and(|previous| previous != view.scroll)
                && self.last_size == Some(view.content_size())
                && !self.restoring
            {
                self.shared.borrow_mut().navigation += 1;
                if view.scroll.y.saturating_add(view.content.h) < view.canvas.h {
                    self.shared.borrow_mut().follow = Follow::Paused;
                }
                if let Some(wake) = &self.wake {
                    let _ = wake.wake()?;
                }
            }
            self.last_scroll = Some(view.scroll);
            self.last_size = Some(view.content_size());
            self.restoring = false;
            if view.scroll.y.saturating_add(view.content.h) >= view.canvas.h
                && let Some(displayed) = &self.displayed
            {
                self.acknowledged = displayed.len();
            }
        }
        Ok(())
    }
    fn poll(&mut self, ctx: &mut dyn Context) -> Result<Option<Duration>> {
        if self.lane.is_none() {
            self.lane = Some(
                self.options
                    .executor
                    .register(self.wake.clone())
                    .map_err(invalid)?,
            );
        }
        if self.displayed.is_some() && self.source_selecting(ctx)? {
            self.shared.borrow_mut().follow = Follow::Paused;
        }
        let mut action_failure = None;
        if let Some(open) = &self.inspector {
            let events = open.handle.take_events(32);
            let epoch = open.epoch;
            let revision = open.revision;
            for event in events {
                if let Err(error) = self.apply_inspector_event(ctx, event, epoch, revision) {
                    action_failure = Some(format!("Failed: {error}"));
                }
            }
        }
        let pending = self.shared.borrow_mut().pending_open.take();
        if let Some(object) = pending {
            // A failed pointer activation is a status, not a reason to skip
            // this turn's worker completions.
            self.shared.borrow_mut().overlays.active_object = Some(object);
            if let Err(error) = self.activate(ctx) {
                action_failure = Some(format!("Failed: {error}"));
            }
        }
        self.deferred_publication = self.drag_active(ctx)?;
        let completed = if self.deferred_publication {
            None
        } else {
            self.lane.as_ref().and_then(WorkHandle::take_completed)
        };
        if let Some(result) = completed {
            let outcome = self.receive(ctx, result);
            if let Some(lane) = &self.lane {
                lane.acknowledge_document(
                    self.shared.borrow().presentation.clone(),
                    self.source_snapshot.as_ref().map(SourceSnapshot::text),
                    self.last_document.clone(),
                );
            }
            outcome?;
        }
        while let Some(copy) = self.lane.as_ref().and_then(WorkHandle::take_copy_completed) {
            match copy.result {
                Ok(text) => {
                    if self.requests.publish(copy.id, HostAction::CopyText(text)) {
                        self.action_status = "Copy ready".into();
                    }
                }
                Err(error) => {
                    self.requests.cancel(copy.id);
                    self.action_status = error.to_string();
                }
            }
        }
        if let Some(failure) = action_failure {
            self.action_status = failure;
        }
        if self.dirty {
            if let Some(deadline) = self.deadline
                && deadline > ctx.now()
            {
                self.update_chrome(ctx)?;
                return Ok(Some(deadline.duration_since(ctx.now())));
            }
            self.submit_work(ctx)?;
        }
        self.update_chrome(ctx)?;
        Ok(if self.dirty && self.geometry_width > 0 {
            Some(self.options.coalesce.max(Duration::from_millis(1)))
        } else {
            None
        })
    }
    fn on_unmount(&mut self, _: &mut dyn Context) {
        if let Some(open) = &self.inspector {
            drop(open.handle.wake());
        }
        self.lane = None;
        self.last_submitted = None;
        self.dirty = true;
    }
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(event) = event {
            let (vertical, horizontal) = self.tracks(ctx);
            let before = self.view_offset(ctx);
            let handled = self.vertical.handle_mouse(ctx, event, &vertical)?
                == EventOutcome::Handle
                || self.horizontal.handle_mouse(ctx, event, &horizontal)? == EventOutcome::Handle;
            if handled {
                self.bar_drag = ctx.has_mouse_capture();
                if before != self.view_offset(ctx) {
                    if self
                        .navigation_node()
                        .and_then(|node| ctx.view_of(node))
                        .is_some_and(|view| {
                            view.scroll.y.saturating_add(view.content.h) < view.canvas.h
                        })
                    {
                        self.shared.borrow_mut().follow = Follow::Paused;
                    }
                    self.shared.borrow_mut().navigation += 1;
                }
                return Ok(EventOutcome::Handle);
            }
        }
        if self.displayed.is_some()
            && self.active == ViewType::Source
            && ctx.is_focused()
            && let Some(source) = self.navigation_node()
        {
            return ctx.with_widget_mut(source, |source: &mut MarkdownSource, ctx| {
                source.on_event(event, ctx)
            });
        }
        Ok(EventOutcome::Ignore)
    }
    fn key_outcome(&self, key: Key, ctx: &dyn ViewContext) -> EventOutcome {
        if self.displayed.is_some()
            && self.active == ViewType::Source
            && ctx.is_focused()
            && let Some(source) = self.navigation_node()
        {
            return ctx
                .with_widget(source, |source: &MarkdownSource| {
                    Ok(source.key_outcome(key, ctx))
                })
                .unwrap_or(EventOutcome::Ignore);
        }
        EventOutcome::Ignore
    }
    fn accepts_intent(&self, intent: &str, ctx: &dyn ViewContext) -> bool {
        // A prompt is a sibling of the reading surface. Reader navigation must
        // not claim its ordinary text keys when bubbling through the
        // controller.
        if !ctx.is_focused()
            && !self
                .navigation_node()
                .is_some_and(|node| ctx.is_on_focus_path(node))
        {
            return false;
        }
        if matches!(
            intent,
            ACTIVATE_INTENT | NEXT_OBJECT_INTENT | PREVIOUS_OBJECT_INTENT
        ) {
            return self.rendered_actions_available();
        }
        let Some(nav) = NavIntent::from_name(intent) else {
            return false;
        };
        // Source's vi editor owns line and edge motions, but it has no
        // paging, so the reader pages its view.
        if self.active == ViewType::Source {
            return matches!(nav, NavIntent::PageUp | NavIntent::PageDown);
        }
        if self.shared.borrow().claims(nav) {
            return true;
        }
        self.navigation_node()
            .and_then(|node| ctx.scroll_outcome(node, nav_scroll(nav)))
            .is_some_and(|outcome| outcome == canopy::ChangeOutcome::Changed)
    }
    fn on_intent(&mut self, intent: &str, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if matches!(
            intent,
            ACTIVATE_INTENT | NEXT_OBJECT_INTENT | PREVIOUS_OBJECT_INTENT
        ) {
            if !self.rendered_actions_available() {
                return Ok(EventOutcome::Ignore);
            }
            match intent {
                ACTIVATE_INTENT => self.activate(ctx)?,
                NEXT_OBJECT_INTENT => self.next_object(ctx, 1)?,
                PREVIOUS_OBJECT_INTENT => self.next_object(ctx, -1)?,
                _ => unreachable!(),
            }
            return Ok(EventOutcome::Handle);
        }
        let Some(nav) = NavIntent::from_name(intent) else {
            return Ok(EventOutcome::Ignore);
        };
        if self.active == ViewType::Rendered {
            if let Some(nodes) = &self.nodes {
                ctx.with_widget_mut(nodes.rendered, |surface: &mut RenderedSurface, ctx| {
                    surface.navigate(ctx, nav)
                })?;
            }
        } else {
            self.scroll_view(ctx, nav_scroll(nav))?;
        }
        Ok(EventOutcome::Handle)
    }
    fn semantics(&self, _: &dyn ViewContext) -> Result<WidgetSemantics> {
        Ok(WidgetSemantics {
            role: Some("markdown-reader".into()),
            label: Some("Markdown".into()),
            value: Some(format!(
                "{:?} {} / {} · {:?}",
                self.active,
                self.displayed_revision(),
                self.accepted_revision(),
                self.follow_state()
            )),
            selected: Some(self.shared.borrow().overlays.selection.is_some()),
            ..Default::default()
        })
    }
    fn name(&self) -> NodeName {
        NodeName::convert("markdown")
    }
}

/// Project a document's headings into inspector outline rows.
fn outline_entries(document: &DocumentSnapshot) -> Vec<OutlineEntry> {
    document
        .outline
        .iter()
        .map(|entry| OutlineEntry {
            id: entry.id,
            level: entry.level,
            text: entry.text.clone(),
            source_byte: entry.source.start,
        })
        .collect()
}

impl Register for Markdown {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        Editor::register(setup)?;
        Inspector::register(setup)?;
        TableView::register(setup)?;
        setup.widget_styles(style_rules);
        setup.widget_styles(palette_style_rules);
        for (key, description, call) in [
            ("i", "Interact", Self::call_interact()),
            ("v", "Select text", Self::call_select_text()),
            ("Escape", "End selection", Self::call_escape()),
            (
                "ctrl-r",
                "Toggle Rendered / Source",
                Self::call_toggle_view_type(),
            ),
            ("/", "Search document", Self::call_start_search()),
            ("y", "Copy", Self::call_copy()),
            ("ctrl-y", "Copy exact source", Self::call_copy_source()),
            ("o", "Outline", Self::call_outline()),
            (
                "ctrl-s",
                "Inspect source snapshot",
                Self::call_inspect_source(),
            ),
            ("f", "Follow tail", Self::call_follow()),
            ("ctrl-o", "Return from jump", Self::call_return_from_jump()),
            ("n", "Next match", Self::call_search_next(1i32)),
            ("N", "Previous match", Self::call_search_next(-1i32)),
        ] {
            setup.bind(
                Key::parse_spec(key)?,
                BindingOptions {
                    show_in_help: true,
                    path: Some("**/markdown/**/".parse()?),
                    tier: BindingTier::Default,
                    description: description.into(),
                    source: None,
                    phase: key
                        .starts_with("ctrl-")
                        .then_some(BindingPhase::BeforeWidget),
                },
                BindingAction::Command(call),
            )?;
        }
        // Dormant intents allow Source to retain normal host focus traversal.
        // Disabled command bindings would consume Tab even when ineligible.
        for (key, name, description) in [
            (
                "Enter",
                ACTIVATE_INTENT,
                "Inspect the selected Rendered object",
            ),
            (
                "Tab",
                NEXT_OBJECT_INTENT,
                "Next Rendered object or focus target",
            ),
            (
                "shift-Tab",
                PREVIOUS_OBJECT_INTENT,
                "Previous Rendered object or focus target",
            ),
        ] {
            setup.register_intent(IntentSpec::new(name, description)?)?;
            setup.bind(
                Key::parse_spec(key)?,
                BindingOptions {
                    show_in_help: true,
                    path: Some("**/markdown/**/".parse()?),
                    tier: BindingTier::Default,
                    description: description.into(),
                    source: None,
                    phase: None,
                },
                BindingAction::Intent(IntentName::new(name)?),
            )?;
        }
        for (key, nav) in [
            ("Up", NavIntent::Up),
            ("k", NavIntent::Up),
            ("Down", NavIntent::Down),
            ("j", NavIntent::Down),
            ("Left", NavIntent::Left),
            ("h", NavIntent::Left),
            ("Right", NavIntent::Right),
            ("l", NavIntent::Right),
            ("PageUp", NavIntent::PageUp),
            ("PageDown", NavIntent::PageDown),
            ("Space", NavIntent::PageDown),
            ("Home", NavIntent::First),
            ("g", NavIntent::First),
            ("End", NavIntent::Last),
            ("G", NavIntent::Last),
        ] {
            for path in ["**/markdown/", "**/markdown_rendered/"] {
                setup.bind(
                    Key::parse_spec(key)?,
                    BindingOptions {
                        show_in_help: true,
                        path: Some(path.parse()?),
                        tier: BindingTier::Default,
                        description: format!("{nav:?}"),
                        source: None,
                        phase: None,
                    },
                    BindingAction::Intent(IntentName::new(nav.name())?),
                )?;
            }
        }
        Ok(())
    }
}

/// A fixed, pointer-activated view control that does not steal listing focus.
struct Chrome {
    /// Reader-owned label and toggle state, read without borrowing the owner.
    state: Rc<RefCell<ChromeState>>,
    /// Exact Markdown command target for pointer view selection.
    owner: NodeId,
}
impl Widget for Chrome {
    fn layout(&self) -> Layout {
        Layout::row().flex_horizontal(1).fixed_height(1)
    }
    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let size = ctx.view().content_size();
        render.fill("markdown/status", ctx.view().outer_rect_local(), ' ')?;
        render.text("markdown/status", Line::new(0, 0, size.w), &{
            let state = self.state.borrow();
            if size.w < 22 {
                if state.active == ViewType::Source {
                    "Source".to_string()
                } else {
                    "Rendered".to_string()
                }
            } else {
                state.text.clone()
            }
        })
    }
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(event) = event
            && event.action == mouse::Action::Down
            && event.button == mouse::Button::Left
            && event.location.x < 21
            && self.state.borrow().enabled
        {
            ctx.post(
                &Markdown::call_toggle_view_type().with_target(CommandTarget::Exact(self.owner)),
            )?;
            return Ok(EventOutcome::Handle);
        }
        Ok(EventOutcome::Ignore)
    }
    fn semantics(&self, _: &dyn ViewContext) -> Result<WidgetSemantics> {
        Ok(WidgetSemantics {
            role: Some("view-toggle".into()),
            label: Some("Rendered / Source".into()),
            value: Some(self.state.borrow().text.clone()),
            ..Default::default()
        })
    }
    fn name(&self) -> NodeName {
        NodeName::convert("markdown_status")
    }
}

/// The destination a local link names, as the inspector shows it.
fn local_display(link: &LocalLink) -> String {
    let anchor = link
        .fragment
        .as_ref()
        .map_or(String::new(), |fragment| format!("#{fragment}"));
    if link.wiki {
        format!("[[{}{anchor}]]", link.path)
    } else {
        format!("{}{anchor}", link.path)
    }
}

#[cfg(test)]
mod tests;

mod queries;
