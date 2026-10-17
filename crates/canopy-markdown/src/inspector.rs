//! Private modal readers. Host actions leave through an owned bounded mailbox.

use std::{
    cell::RefCell, collections::VecDeque, io::Error as IoError, mem::size_of, ops::Range, rc::Rc,
    sync::Arc, time::Duration,
};

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, Register, Setup, TypedId, ViewContext,
    ViewContextExt, Widget,
    commands::{CommandStatus, CommandTarget},
    derive_commands,
    error::{Error, Result},
    geom::{Line, Rect, Size},
    input::{
        BindingAction, BindingOptions, BindingPhase, BindingTier, Event, FrameworkBindingGroup,
        InputSpec, ModalBindings, ModalOptions, ModalToken,
        key::{Key, KeyCode},
    },
    layout::{CanvasContext, Direction, Layout, RevealAlign},
    render::Render,
    runtime::{NodeWakeHandle, PollLifetime, WakeOutcome},
    tree::{FocusDirection, FocusScope},
};
use canopy_widgets::{
    Button, Container, Frame, TableView, Text,
    editor::{LineNumbers, WrapMode},
    table::{CellKey, PreparedTable, TableEvent, TableViewOptions},
};

use crate::{
    ViewType,
    document::ParseLimits,
    executor::{MarkdownExecutor, PreparedView, SearchOutput, WorkHandle, WorkRequest, WorkSearch},
    presentation::GeometryKey,
    search::{SearchLimits, SearchQuery},
    source_store::{SourceSnapshot, SourceStore},
    source_view::{MarkdownSource, SourceAnchor, SourceOptions},
};

/// Maximum host actions retained before the controller drains the mailbox.
const EVENT_COUNT: usize = 32;
/// Maximum aggregate bytes retained by inspector host actions. Frozen source
/// requests charge their entire shared source, even when selecting a small
/// range.
const EVENT_BYTES: usize = 16 * 1024 * 1024;
/// Default output budget for standalone inspector tests.
const DEFAULT_COPY_BYTES: usize = 4 * 1024 * 1024;

/// One keyed heading offered by the document outline.
#[derive(Clone, Debug)]
pub struct OutlineEntry {
    /// Stable heading identity within the current document epoch.
    pub id: u64,
    /// Markdown heading depth.
    pub level: u8,
    /// Full heading label without Markdown delimiters.
    pub text: String,
    /// Exact source byte used by an explicit heading jump.
    pub source_byte: usize,
}

/// Pinned reader content or the live keyed outline navigation index.
pub enum InspectorContent {
    /// A complete code-block payload with its fence language.
    Code {
        /// The original code-fence information string.
        info: String,
        /// Exact normalized parser payload, including its final newline.
        text: Arc<str>,
    },
    /// The exact accepted Markdown source revision.
    Source {
        /// The immutable accepted source revision pinned at inspector entry.
        snapshot: SourceSnapshot,
    },
    /// A keyed table that may receive newer prepared cell publications.
    Table {
        /// Prepared keyed table geometry and its immutable data snapshot.
        table: Arc<PreparedTable>,
    },
    /// A fixed full label and original and resolved destinations.
    Link {
        /// The complete link label.
        label: String,
        /// The destination written in the Markdown source.
        original: String,
        /// The host-resolved destination, when resolution succeeded.
        resolved: Option<String>,
        /// Whether this explicit destination may produce a host Open request.
        allow_open: bool,
    },
    /// The document navigation index, updated by keyed heading publication.
    Outline {
        /// Headings in document order.
        entries: Vec<OutlineEntry>,
    },
}

/// An explicit action for the owning Markdown controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InspectorEvent {
    /// Dismiss the inspector and restore document focus.
    Close,
    /// Copy these complete immutable bytes through the host.
    CopyText(String),
    /// Serialize this exact frozen selection on the owning document's lane.
    CopySource {
        /// Immutable source retained until the controller admits the copy.
        text: Arc<str>,
        /// Exact UTF-8 byte interval selected at activation.
        range: Range<usize>,
    },
    /// Open this pinned immutable destination through the host.
    OpenUrl(String),
    /// Jump to this explicit source byte in the owning document.
    Navigate(usize),
}

/// Bounded immutable actions awaiting the owner poll.
#[derive(Default)]
struct Mailbox {
    /// Complete host actions retained in FIFO order.
    events: VecDeque<InspectorEvent>,
    /// Aggregate immutable text bytes charged to pending actions.
    bytes: usize,
}
impl Mailbox {
    /// Whether one complete action fits without displacing queued actions.
    fn has_capacity(&self, bytes: usize) -> bool {
        self.events.len() < EVENT_COUNT && self.bytes.saturating_add(bytes) <= EVENT_BYTES
    }
    /// Queue one complete action only when both mailbox bounds permit it.
    fn push(&mut self, event: InspectorEvent) -> Result<()> {
        let bytes = event_bytes(&event);
        if !self.has_capacity(bytes) {
            return Err(Error::App(
                IoError::other("inspector request queue is full").into(),
            ));
        }
        self.bytes += bytes;
        self.events.push_back(event);
        Ok(())
    }
    /// Drain at most the requested actions and release their byte accounting.
    fn take(&mut self, limit: usize) -> Vec<InspectorEvent> {
        let events: Vec<_> = self.events.drain(..limit.min(self.events.len())).collect();
        for event in &events {
            self.bytes -= event_bytes(event);
        }
        events
    }
}
/// Return the immutable text bytes charged to one pending host action.
fn event_bytes(event: &InspectorEvent) -> usize {
    match event {
        InspectorEvent::CopyText(text) | InspectorEvent::OpenUrl(text) => text.len(),
        InspectorEvent::CopySource { text, .. } => text.len(),
        _ => 0,
    }
}

/// Controller access to one modal inspector and its action mailbox.
#[derive(Clone)]
pub struct InspectorHandle {
    /// The root node of this inspector modal subtree.
    pub node: TypedId<Inspector>,
    /// The framework modal scope retired during dismissal.
    token: ModalToken,
    /// An incarnation-aware wake for this inspector overlay.
    wake: NodeWakeHandle,
    /// Shared bounded actions consumed by the Markdown controller.
    mailbox: Rc<RefCell<Mailbox>>,
}
impl InspectorHandle {
    /// Signal from the owner's unmount hook. The later poll observes commit or
    /// rollback.
    pub fn wake(&self) -> Result<WakeOutcome> {
        self.wake.wake()
    }
    /// The visible reader has prepared content and no pending search or detail.
    pub fn ready(&self, ctx: &dyn ViewContext) -> Result<bool> {
        ctx.with_widget(self.node, |inspector: &Inspector| {
            Ok(inspector.pending_source.is_none()
                && !inspector.detail_loading
                && inspector.work_search.is_none()
                && matches!(
                    inspector.reader,
                    Some(Reader::Source(_) | Reader::Table(_) | Reader::Outline(_))
                ))
        })
    }
    /// Conservative retained storage, including shared snapshots. This does not
    /// scan document text or table cells and excludes executor-owned jobs.
    pub fn resource_bytes(&self, ctx: &dyn ViewContext) -> Result<usize> {
        ctx.with_widget(self.node, |inspector: &Inspector| {
            let mut bytes = size_of::<Inspector>()
                .saturating_add(inspector.mailbox.borrow().bytes)
                .saturating_add(inspector.pending_copy.as_ref().map_or(0, String::capacity))
                .saturating_add(
                    inspector
                        .pending_source
                        .as_ref()
                        .map_or(0, SourceSnapshot::len),
                )
                .saturating_add(
                    inspector
                        .display_source
                        .as_ref()
                        .map_or(0, SourceSnapshot::len),
                );
            if inspector.pending_source.is_none() && inspector.display_source.is_none() {
                bytes =
                    bytes.saturating_add(inspector.full_copy.as_ref().map_or(0, |text| text.len()));
            }
            bytes = bytes
                .saturating_add(inspector.open_url.as_ref().map_or(0, String::capacity))
                .saturating_add(
                    inspector
                        .prompt
                        .as_ref()
                        .map_or(0, |prompt| prompt.value.capacity()),
                );
            if let Some(reader) = inspector.reader {
                bytes = bytes.saturating_add(match reader {
                    Reader::Source(source) => {
                        ctx.with_widget(source, |source: &MarkdownSource| {
                            Ok(source.derived_bytes())
                        })?
                    }
                    Reader::Table(table) => ctx.with_widget(table, |table: &TableView| {
                        Ok(table.table().derived_bytes())
                    })?,
                    Reader::Outline(outline) => {
                        ctx.with_widget(outline, |outline: &OutlineBody| Ok(outline.storage_bytes))?
                    }
                    Reader::Text(_) => 0,
                });
            }
            if let Some(detail) = inspector.detail {
                bytes =
                    bytes.saturating_add(ctx.with_widget(detail, |source: &MarkdownSource| {
                        Ok(source.derived_bytes())
                    })?);
            }
            Ok(bytes)
        })
    }
    /// Drain a bounded action batch for the owning controller.
    pub fn take_events(&self, limit: usize) -> Vec<InspectorEvent> {
        let events = self.mailbox.borrow_mut().take(limit);
        if !events.is_empty() {
            // Released capacity lets a retained copy continue without a timer.
            drop(self.wake.wake());
        }
        events
    }
    /// Transfer accepted actions before normal dismissal reclaims the modal.
    /// Deferred table copies follow the already queued actions; table
    /// activation is superseded by dismissal.
    pub(crate) fn take_closing_events(&self, ctx: &mut dyn Context) -> Result<Vec<InspectorEvent>> {
        let mut events = self.take_events(EVENT_COUNT);
        ctx.with_widget_mut(self.node, |inspector: &mut Inspector, ctx| {
            if let Some(text) = inspector.pending_copy.take() {
                events.push(InspectorEvent::CopyText(text));
            }
            if let Some(Reader::Table(table)) = inspector.reader {
                let pending = ctx.with_widget_mut(table, |table: &mut TableView, _| {
                    Ok(table.take_events(EVENT_COUNT))
                })?;
                for event in pending {
                    if let TableEvent::Copy { text, .. } = event
                        && inspector.allow_copy
                        && text.len() <= inspector.copy_limit
                    {
                        events.push(InspectorEvent::CopyText(text));
                    }
                }
            }
            Ok(())
        })?;
        Ok(events)
    }
    /// Replace outline headings while preserving its filter and selected
    /// heading key.
    pub fn publish_outline(
        &self,
        ctx: &mut dyn Context,
        entries: impl FnOnce() -> Vec<OutlineEntry>,
    ) -> Result<()> {
        ctx.with_widget_mut(self.node, |inspector: &mut Inspector, ctx| {
            if let Some(Reader::Outline(outline)) = inspector.reader {
                ctx.with_widget_mut(outline, |outline: &mut OutlineBody, _| {
                    outline.publish(entries());
                    Ok(())
                })?;
            }
            Ok(())
        })
    }
    /// Publish a newer prepared table without replacing the surrounding modal.
    pub fn publish_table(&self, ctx: &mut dyn Context, table: Arc<PreparedTable>) -> Result<()> {
        ctx.with_widget_mut(self.node, |inspector: &mut Inspector, ctx| {
            inspector.publish_table(ctx, table)
        })
    }
    #[cfg(test)]
    pub fn set_capabilities(
        &self,
        ctx: &mut dyn Context,
        allow_copy: bool,
        copy_limit: usize,
    ) -> Result<()> {
        ctx.with_widget_mut(self.node, |inspector: &mut Inspector, ctx| {
            if let Some(Reader::Table(table)) = inspector.reader {
                ctx.with_widget_mut(table, |table: &mut TableView, _| {
                    table.set_copy_enabled(allow_copy);
                    table.set_copy_limit(copy_limit.min(EVENT_BYTES));
                    Ok(())
                })?;
            }
            inspector.allow_copy = allow_copy;
            inspector.copy_limit = copy_limit.min(EVENT_BYTES);
            Ok(())
        })
    }
    /// Reveal a one-based source line now or after its reader becomes ready.
    pub fn reveal_line(&self, ctx: &mut dyn Context, line: usize) -> Result<()> {
        ctx.with_widget_mut(self.node, |inspector: &mut Inspector, ctx| {
            inspector.reveal = Some(InspectorReveal::Line(line));
            inspector.apply_reveal(ctx)
        })
    }
    /// Reveal an exact source byte now or after its reader becomes ready.
    pub fn reveal_source_byte(&self, ctx: &mut dyn Context, byte: usize) -> Result<()> {
        ctx.with_widget_mut(self.node, |inspector: &mut Inspector, ctx| {
            inspector.reveal = Some(InspectorReveal::Byte(byte));
            inspector.apply_reveal(ctx)
        })
    }
    /// Select and reveal a keyed cell in the mounted table reader.
    pub fn reveal_cell(&self, ctx: &mut dyn Context, cell: &CellKey) -> Result<()> {
        ctx.with_widget_mut(self.node, |inspector: &mut Inspector, ctx| {
            if let Some(Reader::Table(table)) = inspector.reader {
                ctx.with_widget_mut(table, |table: &mut TableView, ctx| {
                    table.set_cursor(ctx, cell)
                })?;
            }
            Ok(())
        })
    }
    /// Defer modal closure and subtree reclamation until the dispatch finishes.
    pub fn close(&self, ctx: &mut dyn Context) -> Result<()> {
        ctx.close_modal(self.token)?;
        ctx.remove_after_dispatch(self.node.into())
    }
}

/// The mounted content subtree beneath the inspector chrome.
#[derive(Clone, Copy)]
enum Reader {
    /// The exact accepted Markdown source revision.
    Source(TypedId<MarkdownSource>),
    /// A keyed table that may receive newer prepared cell publications.
    Table(TypedId<TableView>),
    /// The document navigation index, updated by keyed heading publication.
    Outline(TypedId<OutlineBody>),
    /// A mounted loading message while source preparation is pending.
    Text(NodeId),
}
/// The operation requested by the temporary input prompt.
#[derive(Clone, Copy)]
enum PromptKind {
    /// Search prepared source or filter the outline.
    Search,
    /// Reveal a one-based logical source line.
    Line,
}
/// A source location to apply after asynchronous preparation.
#[derive(Clone, Copy)]
enum InspectorReveal {
    /// Reveal a one-based logical source line.
    Line(usize),
    /// Reveal an exact accepted source byte.
    Byte(usize),
}
/// Input for one bounded search or line-jump request.
struct Prompt {
    /// The operation performed when the prompt is submitted.
    kind: PromptKind,
    /// Current bounded prompt input.
    value: String,
}

/// Short controller-facing name for the command-owning overlay type.
pub type Inspector = MarkdownInspector;

/// A modal subtree with fixed payloads and explicitly routed host actions.
pub struct MarkdownInspector {
    /// Initial immutable content, consumed once during mounting.
    content: Option<InspectorContent>,
    /// Shared host preparation service used by the private inspector lane.
    executor: MarkdownExecutor,
    /// Node-lifetime wake used to detect owner removal and signal host actions.
    owner: NodeWakeHandle,
    /// Shared bounded actions consumed by the Markdown controller.
    mailbox: Rc<RefCell<Mailbox>>,
    /// An incarnation-aware wake for this inspector overlay.
    wake: Option<NodeWakeHandle>,
    /// Mounted primary content subtree.
    reader: Option<Reader>,
    /// Content container that also hosts nested cell details.
    reader_parent: Option<NodeId>,
    /// The fixed status and key-hint row.
    status: Option<TypedId<Text>>,
    /// Bounded source preparation lane owned by this overlay.
    lane: Option<WorkHandle>,
    /// Immutable source awaiting worker preparation and publication.
    pending_source: Option<SourceSnapshot>,
    /// Geometry, language, and syntax bounds for source preparation.
    source_options: SourceOptions,
    /// Temporary search or line-jump input, when active.
    prompt: Option<Prompt>,
    /// Pinned source reader for the activated table cell.
    detail: Option<TypedId<MarkdownSource>>,
    /// Original complete code payload or pinned URL for whole-content copying.
    full_copy: Option<Arc<str>>,
    /// Pinned destination admitted for an explicit host Open action.
    open_url: Option<String>,
    /// Latest preparation generation used to reject superseded results.
    generation: u64,
    /// Immutable source retained by the currently displayed source reader.
    display_source: Option<SourceSnapshot>,
    /// The query awaiting worker matching over the fixed source.
    work_search: Option<WorkSearch>,
    /// The current query's first forward match has not yet been revealed.
    search_fresh: bool,
    /// Whether an explicitly activated cell is awaiting preparation.
    detail_loading: bool,
    /// Whether the host admits clipboard actions from this inspector.
    allow_copy: bool,
    /// Maximum bytes in one accepted complete copy payload.
    copy_limit: usize,
    /// One accepted table copy awaiting capacity in the owner's action mailbox.
    pending_copy: Option<String>,
    /// Source location retained until its asynchronous reader is ready.
    reveal: Option<InspectorReveal>,
}

#[derive_commands]
impl MarkdownInspector {
    /// The framework bindings admitted by this inspector modal.
    pub const BINDINGS: FrameworkBindingGroup = FrameworkBindingGroup::new("markdown_inspector");

    #[cfg(test)]
    pub fn open(
        ctx: &mut dyn Context,
        executor: MarkdownExecutor,
        content: InspectorContent,
    ) -> Result<InspectorHandle> {
        let parent = ctx.node_id();
        Self::open_configured(
            ctx,
            parent,
            executor,
            content,
            true,
            DEFAULT_COPY_BYTES,
            256 * 1024,
        )
    }
    /// Mount and open a modal with its host capabilities and preparation
    /// bounds.
    ///
    /// The inspector mounts in `parent`, a stack inside the calling owner,
    /// and fills it, so it covers the reader it inspects and nothing else.
    /// The modal still takes focus and the keys while it is open.
    pub fn open_configured(
        ctx: &mut dyn Context,
        parent: NodeId,
        executor: MarkdownExecutor,
        content: InspectorContent,
        allow_copy: bool,
        copy_limit: usize,
        syntax_limit: usize,
    ) -> Result<InspectorHandle> {
        let owner_id = ctx.node_id();
        let owner = ctx.wake_handle(PollLifetime::Node)?;
        let mailbox = Rc::new(RefCell::new(Mailbox::default()));
        let mut inspector = Self::new(executor, owner, mailbox.clone(), content);
        inspector.allow_copy = allow_copy;
        inspector.copy_limit = copy_limit.min(EVENT_BYTES);
        inspector.source_options.syntax_limit = syntax_limit;
        let node = ctx.add_child(parent, inspector)?;
        let view: &dyn ViewContext = ctx;
        let (focus, wake) = view.with_widget(node, |widget: &Self| {
            Ok((
                widget.focus_target(view)?,
                widget.wake.clone().expect("mounted inspector wake"),
            ))
        })?;
        let token = match ctx.open_modal(ModalOptions {
            owner: owner_id,
            modal: node.into(),
            initial_focus: focus.unwrap_or(node.into()),
            // The inspector covers the reader, so nothing beside it dims.
            dim_target: None,
            bindings: ModalBindings::Framework {
                groups: &[Self::BINDINGS],
                intents: &[],
            },
        }) {
            Ok(token) => token,
            Err(error) => {
                ctx.remove_subtree(node.into())?;
                return Err(error);
            }
        };
        Ok(InspectorHandle {
            node,
            token,
            wake,
            mailbox,
        })
    }

    /// Construct unmounted inspector state around one immutable content
    /// request.
    fn new(
        executor: MarkdownExecutor,
        owner: NodeWakeHandle,
        mailbox: Rc<RefCell<Mailbox>>,
        content: InspectorContent,
    ) -> Self {
        Self {
            content: Some(content),
            executor,
            owner,
            mailbox,
            wake: None,
            reader: None,
            reader_parent: None,
            status: None,
            lane: None,
            pending_source: None,
            source_options: SourceOptions::default(),
            prompt: None,
            detail: None,
            full_copy: None,
            open_url: None,
            generation: 0,
            display_source: None,
            work_search: None,
            search_fresh: false,
            detail_loading: false,
            allow_copy: true,
            copy_limit: DEFAULT_COPY_BYTES,
            pending_copy: None,
            reveal: None,
        }
    }
    /// Apply a queued source location once the source reader has mounted.
    fn apply_reveal(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let (Some(source), Some(reveal)) = (self.source_reader(), self.reveal) else {
            return Ok(());
        };
        ctx.with_widget_mut(source, |source: &mut MarkdownSource, ctx| {
            match reveal {
                InspectorReveal::Line(line) => source.jump_line(ctx, line.max(1)),
                InspectorReveal::Byte(byte) => source
                    .restore_anchor(
                        ctx,
                        SourceAnchor {
                            epoch: source.epoch(),
                            byte,
                        },
                    )
                    .map_err(|e| Error::App(Box::new(e)))?,
            }
            Ok(())
        })?;
        self.reveal = None;
        Ok(())
    }
    /// Return the active source reader, preferring nested table cell detail.
    fn source_reader(&self) -> Option<TypedId<MarkdownSource>> {
        match self.reader {
            Some(Reader::Source(id)) => self.detail.or(Some(id)),
            _ => self.detail,
        }
    }
    /// Return the content node that receives reader focus, preferring nested
    /// cell detail and the table's own focus target.
    fn focus_target(&self, ctx: &dyn ViewContext) -> Result<Option<NodeId>> {
        if let Some(detail) = self.detail {
            return Ok(Some(detail.into()));
        }
        Ok(match self.reader {
            Some(Reader::Table(table)) => {
                ctx.with_widget(table, |table: &TableView| Ok(table.initial_focus()))?
            }
            Some(Reader::Source(id)) => Some(id.into()),
            Some(Reader::Outline(id)) => Some(id.into()),
            Some(Reader::Text(id)) => Some(id),
            None => None,
        })
    }
    /// Queue an immutable action and wake its owning controller.
    fn emit(&self, event: InspectorEvent) -> Result<()> {
        self.mailbox.borrow_mut().push(event)?;
        let _ = self.owner.wake()?;
        Ok(())
    }
    /// Retain accepted copy output until the owner drains enough mailbox space.
    fn emit_copy(&mut self, text: String) -> Result<bool> {
        if !self.mailbox.borrow().has_capacity(text.len()) {
            self.pending_copy = Some(text);
            return Ok(false);
        }
        self.emit(InspectorEvent::CopyText(text))?;
        Ok(true)
    }
    /// Replace the fixed status row without rebuilding the content subtree.
    fn show_status(&self, ctx: &mut dyn Context, value: impl Into<String>) -> Result<()> {
        if let Some(status) = self.status {
            let value = value.into();
            ctx.with_widget_mut(status, |text: &mut Text, _| {
                text.set_text(value);
                Ok(())
            })?;
        }
        Ok(())
    }
    /// Restore focus to the current reader after dismissing a nested prompt or
    /// detail.
    fn restore_focus(&self, ctx: &mut dyn Context) -> Result<()> {
        if let Some(focus) = self.focus_target(ctx)? {
            ctx.set_focus(focus)?;
        }
        Ok(())
    }
    /// Escape unwinds a prompt or cell detail before dismissing the modal.
    #[command]
    pub fn dismiss(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.prompt.take().is_some() {
            self.show_status(ctx, "Search canceled")?;
            return self.restore_focus(ctx);
        }
        if let Some(source) = self.source_reader() {
            let matches =
                ctx.with_widget(
                    source,
                    |source: &MarkdownSource| Ok(source.search_matches()),
                )?;
            if matches > 0 {
                ctx.with_widget_mut(source, |source: &mut MarkdownSource, ctx| {
                    source.clear_search(ctx);
                    Ok(())
                })?;
                self.work_search = None;
                self.search_fresh = false;
                self.generation += 1;
                self.show_status(ctx, "Search cleared")?;
                return Ok(());
            }
        }
        if self.detail_loading {
            self.detail_loading = false;
            self.pending_source = None;
            self.generation += 1;
            self.show_status(ctx, "Cell detail canceled")?;
            return self.restore_focus(ctx);
        }
        if let Some(detail) = self.detail.take() {
            self.generation += 1;
            self.work_search = None;
            self.search_fresh = false;
            self.display_source = None;
            ctx.remove_after_dispatch(detail.into())?;
            if let Some(Reader::Table(table)) = self.reader {
                ctx.set_hidden(table.into(), false)?;
            }
            return self.restore_focus(ctx);
        }
        self.emit(InspectorEvent::Close)
    }
    /// Queue frozen selection bytes in activation order. Serialization belongs
    /// to the document so accepted copies survive normal modal dismissal.
    fn request_source_copy(&self, text: Arc<str>, range: Range<usize>) -> Result<()> {
        if !self.allow_copy {
            return Err(Error::App(
                IoError::other("clipboard actions are disabled").into(),
            ));
        }
        let selected = text
            .get(range.clone())
            .ok_or_else(|| Error::Invalid("Invalid source selection".into()))?;
        if selected.len() > self.copy_limit {
            return Err(Error::Invalid(format!(
                "Copy exceeds {} bytes",
                self.copy_limit
            )));
        }
        self.emit(InspectorEvent::CopySource { text, range })
    }
    /// Describe whether this reader offers host clipboard actions.
    fn copy_status(&self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(
            if self.allow_copy
                && (self.full_copy.is_some()
                    || self.source_reader().is_some()
                    || matches!(self.reader, Some(Reader::Table(_))))
            {
                CommandStatus::Enabled
            } else {
                CommandStatus::Disabled("clipboard actions are disabled".into())
            },
        )
    }
    /// Describe whether a prepared source or outline can accept a search query.
    fn search_status(&self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(
            if self.source_reader().is_some() || matches!(self.reader, Some(Reader::Outline(_))) {
                CommandStatus::Enabled
            } else {
                CommandStatus::Disabled("no searchable reader is ready".into())
            },
        )
    }
    /// Describe whether a source reader can accept line or match navigation.
    fn line_status(&self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(if self.source_reader().is_some() {
            CommandStatus::Enabled
        } else {
            CommandStatus::Disabled("no source reader is ready".into())
        })
    }
    /// Describe whether the pinned link destination permits a host Open action.
    fn open_status(&self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(if self.open_url.is_some() {
            CommandStatus::Enabled
        } else {
            CommandStatus::Disabled("this destination cannot be opened".into())
        })
    }
    /// Copy the current selection or keyed table cell.
    #[command(enabled = "copy_status")]
    pub fn copy_selection(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let source = self.source_reader();
        if let Some(source) = source {
            let input = ctx.with_widget(source, |source: &MarkdownSource| {
                let text = source.source_text();
                let selected = source.selection_range();
                let range = if selected.is_empty() {
                    0..text.len()
                } else {
                    selected
                };
                Ok((text, range))
            })?;
            return self.request_source_copy(input.0, input.1);
        }
        if let Some(Reader::Table(table)) = self.reader {
            ctx.with_widget_mut(table, |table: &mut TableView, ctx| {
                if table.selection().is_empty() {
                    table.copy_cell(ctx)
                } else {
                    table.copy_selection(ctx, false)
                }
            })?;
            return self.drain_table(ctx);
        }
        self.copy_all(ctx)
    }
    /// Copy complete immutable content, preserving its bytes.
    #[command(enabled = "copy_status")]
    pub fn copy_all(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if let Some(text) = &self.full_copy {
            return self.request_source_copy(text.clone(), 0..text.len());
        }
        let source = self.source_reader();
        if let Some(source) = source {
            let text =
                ctx.with_widget(source, |source: &MarkdownSource| Ok(source.source_text()))?;
            let range = 0..text.len();
            return self.request_source_copy(text, range);
        }
        if let Some(Reader::Table(id)) = self.reader {
            ctx.with_widget_mut(id, |table: &mut TableView, ctx| table.copy_all(ctx))?;
            return self.drain_table(ctx);
        }
        Ok(())
    }
    /// Request the pinned resolved destination. Only an explicit action reaches
    /// the host.
    #[command(enabled = "open_status")]
    pub fn open_url(&self) -> Result<()> {
        if let Some(url) = &self.open_url {
            self.emit(InspectorEvent::OpenUrl(url.clone()))?;
        }
        Ok(())
    }
    /// Focus a bounded prompt for source search or outline filtering.
    #[command(enabled = "search_status")]
    pub fn search(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.prompt = Some(Prompt {
            kind: PromptKind::Search,
            value: String::new(),
        });
        ctx.set_focus(ctx.node_id())?;
        self.show_status(ctx, "Find: ")
    }
    /// Focus the one-based source line prompt.
    #[command(enabled = "line_status")]
    pub fn line(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.prompt = Some(Prompt {
            kind: PromptKind::Line,
            value: String::new(),
        });
        ctx.set_focus(ctx.node_id())?;
        self.show_status(ctx, "Line: ")
    }
    /// Reveal the next or previous prepared source match.
    #[command(enabled = "line_status")]
    pub fn next_match(&mut self, ctx: &mut dyn Context, delta: i32) -> Result<()> {
        let source = self.source_reader();
        if let Some(source) = source {
            let delta = if self.search_fresh && delta > 0 {
                delta - 1
            } else {
                delta
            };
            ctx.with_widget_mut(source, |source: &mut MarkdownSource, ctx| {
                source.search_next(ctx, delta);
                Ok(())
            })?;
            self.search_fresh = false;
        }
        Ok(())
    }
    /// Move between controls within this inspector subtree alone.
    #[command]
    pub fn focus(&self, ctx: &mut dyn Context, direction: FocusDirection) -> Result<()> {
        ctx.focus_move(FocusScope::Node(ctx.node_id()), direction)?;
        Ok(())
    }
    /// Republish a matching table identity while retaining the modal and keyed
    /// selection.
    pub fn publish_table(&self, ctx: &mut dyn Context, prepared: Arc<PreparedTable>) -> Result<()> {
        if let Some(Reader::Table(table)) = self.reader {
            ctx.with_widget_mut(table, |table: &mut TableView, ctx| {
                if table.table().data().dataset() != prepared.data().dataset() {
                    return Err(Error::Invalid("table inspector identity changed".into()));
                }
                table.publish(ctx, prepared)
            })?;
        }
        Ok(())
    }
    /// Forward immutable table copies and prepare explicitly activated cell
    /// details.
    fn drain_table(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.pending_copy.is_some() {
            return Ok(());
        }
        let Some(Reader::Table(table)) = self.reader else {
            return Ok(());
        };
        for _ in 0..EVENT_COUNT {
            let event = ctx.with_widget_mut(table, |table: &mut TableView, _| {
                Ok(table.take_events(1).pop())
            })?;
            let Some(event) = event else { break };
            match event {
                TableEvent::Copy { text, .. } => {
                    if self.allow_copy && text.len() <= self.copy_limit && !self.emit_copy(text)? {
                        break;
                    }
                }
                TableEvent::Activate { cell, .. } => {
                    let text = ctx.with_widget(table, |table: &TableView| {
                        Ok(table
                            .table()
                            .data()
                            .cell(&cell)
                            .map(|cell| cell.shared_text()))
                    })?;
                    if let Some(text) = text {
                        self.open_detail(ctx, text)?;
                    }
                }
            }
        }
        Ok(())
    }
    /// Submit a pinned cell value for asynchronous source-reader preparation.
    fn open_detail(&mut self, ctx: &mut dyn Context, text: Arc<str>) -> Result<()> {
        // Preparation is submitted through the same lane. The table remains
        // mounted.
        let store = SourceStore::static_text(text).map_err(|e| Error::Invalid(e.to_string()))?;
        self.pending_source = Some(store.snapshot());
        self.detail_loading = true;
        self.source_options.language = None;
        self.submit(ctx)?;
        self.show_status(ctx, "Preparing cell detail…")
    }
    /// Submit the latest fixed source and optional query through the inspector
    /// lane.
    fn submit(&mut self, ctx: &dyn ViewContext) -> Result<()> {
        let Some(source) = self
            .pending_source
            .clone()
            .or_else(|| self.display_source.clone())
        else {
            return Ok(());
        };
        let Some(lane) = &self.lane else {
            return Ok(());
        };
        self.generation += 1;
        let width = ctx.view().content.w.saturating_sub(4).max(1);
        lane.submit(WorkRequest {
            source,
            view: ViewType::Source,
            geometry: GeometryKey {
                width,
                ..GeometryKey::default()
            },
            source_options: self.source_options.clone(),
            previous: None,
            previous_document: None,
            previous_source: None,
            limits: ParseLimits::default(),
            generation: self.generation,
            search: self.work_search.clone(),
        })
        .map_err(|e| Error::App(Box::new(e)))
    }
}

impl Register for Inspector {
    fn register(setup: &mut Setup) -> Result<()> {
        MarkdownSource::register(setup)?;
        Button::register(setup)?;
        TableView::register(setup)?;
        setup.add_commands::<Self>()?;
        for (key, description, call, phase) in [
            (
                "/",
                "Find text or filter outline",
                Self::call_search(),
                BindingPhase::AfterWidget,
            ),
            (
                "?",
                "Find text or filter outline",
                Self::call_search(),
                BindingPhase::AfterWidget,
            ),
            (
                "n",
                "Next match",
                Self::call_next_match(1),
                BindingPhase::AfterWidget,
            ),
            (
                "N",
                "Previous match",
                Self::call_next_match(-1),
                BindingPhase::AfterWidget,
            ),
            (
                "y",
                "Copy selection",
                Self::call_copy_selection(),
                BindingPhase::AfterWidget,
            ),
            (
                "Esc",
                "Close detail or inspector",
                Self::call_dismiss(),
                BindingPhase::AfterWidget,
            ),
            (
                "ctrl-c",
                "Copy selection",
                Self::call_copy_selection(),
                BindingPhase::BeforeWidget,
            ),
            (
                "ctrl-y",
                "Copy complete content",
                Self::call_copy_all(),
                BindingPhase::BeforeWidget,
            ),
            (
                "ctrl-o",
                "Open URL",
                Self::call_open_url(),
                BindingPhase::BeforeWidget,
            ),
            (
                "ctrl-f",
                "Find text or filter outline",
                Self::call_search(),
                BindingPhase::BeforeWidget,
            ),
            (
                ":",
                "Jump to line",
                Self::call_line(),
                BindingPhase::AfterWidget,
            ),
            (
                "F3",
                "Next match",
                Self::call_next_match(1),
                BindingPhase::BeforeWidget,
            ),
            (
                "shift-F3",
                "Previous match",
                Self::call_next_match(-1),
                BindingPhase::BeforeWidget,
            ),
            (
                "Tab",
                "Next control",
                Self::call_focus(FocusDirection::Next),
                BindingPhase::AfterWidget,
            ),
            (
                "BackTab",
                "Previous control",
                Self::call_focus(FocusDirection::Prev),
                BindingPhase::AfterWidget,
            ),
            (
                "Enter",
                "Activate button",
                Button::call_press(),
                BindingPhase::AfterWidget,
            ),
        ] {
            setup.bind(
                InputSpec::Key(Key::parse_spec(key)?),
                BindingOptions {
                    tier: BindingTier::Framework(Self::BINDINGS),
                    description: description.into(),
                    path: Some("**/markdown_inspector/**/".parse()?),
                    phase: Some(phase),
                    show_in_help: true,
                    source: None,
                },
                BindingAction::Command(call),
            )?;
        }
        Ok(())
    }
}

impl Widget for Inspector {
    fn name(&self) -> NodeName {
        NodeName::convert("markdown_inspector")
    }
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.wake = Some(ctx.wake_handle(PollLifetime::Node)?);
        self.lane = Some(
            self.executor
                .register(self.wake.clone())
                .map_err(|e| Error::Invalid(e.to_string()))?,
        );
        let content = self
            .content
            .take()
            .ok_or_else(|| Error::Invalid("inspector already mounted".into()))?;
        let copy_label = match &content {
            InspectorContent::Code { .. } => "Copy Block",
            InspectorContent::Source { .. } => "Copy Source",
            InspectorContent::Table { .. } => "Copy Table",
            InspectorContent::Link { .. } => "Copy URL",
            InspectorContent::Outline { .. } => "Copy",
        };
        let title = match &content {
            InspectorContent::Code { info, .. } => format!(
                "Code · {}",
                info.chars()
                    .take(160)
                    .map(|ch| if ch.is_control() { ' ' } else { ch })
                    .collect::<String>()
            ),
            InspectorContent::Source { snapshot } => {
                format!("Source · revision {} (pinned)", snapshot.revision)
            }
            InspectorContent::Table { .. } => "Table".into(),
            InspectorContent::Link { .. } => "Link".into(),
            InspectorContent::Outline { .. } => "Outline".into(),
        };
        let frame = ctx.add_child(ctx.node_id(), Frame::new().with_title(title))?;
        let body = ctx.add_child(
            frame,
            Container::new(Layout::fill().direction(Direction::Column)).with_fill(),
        )?;
        let reader_parent = ctx.add_child(body, Container::new(Layout::fill()))?;
        self.reader_parent = Some(reader_parent.into());
        self.source_options.wrap = WrapMode::None;
        self.source_options.line_numbers = LineNumbers::Absolute;
        let ready = match content {
            InspectorContent::Code { info, text } => {
                self.full_copy = Some(text.clone());
                self.source_options.language = Some(Arc::from(info));
                self.pending_source = Some(
                    SourceStore::static_text(text)
                        .map_err(|e| Error::Invalid(e.to_string()))?
                        .snapshot(),
                );
                false
            }
            InspectorContent::Source { snapshot } => {
                self.pending_source = Some(snapshot);
                false
            }
            InspectorContent::Table { table } => {
                let table = ctx.add_child(
                    reader_parent,
                    TableView::new(
                        table,
                        TableViewOptions {
                            copy_bytes: self.copy_limit,
                            // The default queue bound would reject any
                            // admitted copy larger than itself.
                            event_bytes: EVENT_BYTES,
                            ..TableViewOptions::default()
                        },
                    ),
                )?;
                ctx.with_widget_mut(table, |table: &mut TableView, _| {
                    table.set_event_wake(self.wake.clone().expect("mounted wake"));
                    table.set_copy_enabled(self.allow_copy);
                    Ok(())
                })?;
                self.reader = Some(Reader::Table(table));
                true
            }
            InspectorContent::Link {
                label,
                original,
                resolved,
                allow_open,
            } => {
                self.full_copy = Some(Arc::from(resolved.as_deref().unwrap_or(&original)));
                if allow_open {
                    self.open_url = resolved.clone();
                }
                let text = format!(
                    "Label:\n{label}\n\nOriginal URL:\n{original}\n\nResolved URL:\n{}",
                    resolved.as_deref().unwrap_or("Unresolved")
                );
                self.pending_source = Some(
                    SourceStore::static_text(text)
                        .map_err(|e| Error::Invalid(e.to_string()))?
                        .snapshot(),
                );
                self.source_options.language = None;
                false
            }
            InspectorContent::Outline { entries } => {
                let outline = ctx.add_child(
                    reader_parent,
                    OutlineBody::new(entries, self.owner.clone(), self.mailbox.clone()),
                )?;
                self.reader = Some(Reader::Outline(outline));
                true
            }
        };
        if !ready {
            self.reader = Some(Reader::Text(
                ctx.add_child(reader_parent, Text::new("Preparing…").with_focusable(true))?
                    .into(),
            ));
            self.submit(ctx)?;
        }
        let status = ctx.add_child(body, Text::new(""))?;
        ctx.set_layout_override(
            status.into(),
            Layout::column().flex_horizontal(1).fixed_height(1).into(),
        )?;
        self.status = Some(status);
        let buttons = ctx.add_child(
            body,
            Container::new(Layout::row().flex_horizontal(1).fixed_height(1).gap(1)),
        )?;
        let target = CommandTarget::Exact(ctx.node_id());
        ctx.add_child(
            buttons,
            Button::new(copy_label).with_command(Self::call_copy_all().with_target(target)),
        )?;
        if self.open_url.is_some() {
            ctx.add_child(
                buttons,
                Button::new("Open").with_command(Self::call_open_url().with_target(target)),
            )?;
        }
        ctx.add_child(
            buttons,
            Button::new("Close").with_command(Self::call_dismiss().with_target(target)),
        )?;
        Ok(())
    }
    fn poll(&mut self, ctx: &mut dyn Context) -> Result<Option<Duration>> {
        if self.owner.wake()? == WakeOutcome::Expired {
            self.lane = None;
            ctx.remove_after_dispatch(ctx.node_id())?;
            return Ok(None);
        }
        // A retained copy keeps later table events queued in order, but it
        // does not hold back source preparation.
        if let Some(text) = self.pending_copy.take() {
            self.emit_copy(text)?;
        }
        self.drain_table(ctx)?;
        if let Some(result) = self.lane.as_ref().and_then(WorkHandle::take_completed) {
            if let Some(lane) = &self.lane {
                lane.acknowledge(None);
            }
            if result.tags.generation != self.generation {
                return Ok(None);
            }
            let search_result = result.search;
            if self.work_search.is_some() {
                if let (Some(source), Some(matches)) = (self.source_reader(), search_result) {
                    match matches {
                        Ok(SearchOutput::Source(matches)) => {
                            let count = matches.ranges.len();
                            let complete = matches.complete;
                            ctx.with_widget_mut(source, |source: &mut MarkdownSource, _| {
                                source
                                    .publish_matches(matches)
                                    .map_err(|e| Error::Invalid(e.to_string()))
                            })?;
                            self.search_fresh = true;
                            self.show_status(
                                ctx,
                                format!(
                                    "{count} matches{}",
                                    if complete { "" } else { " (partial coverage)" }
                                ),
                            )?;
                        }
                        Err(error) => self.show_status(ctx, error)?,
                        _ => {}
                    }
                } else if let Err(error) = &result.result {
                    self.show_status(ctx, format!("Search failed: {error}"))?;
                }
                self.work_search = None;
                return Ok(None);
            }
            match result.result {
                Ok(PreparedView::Source(prepared)) => {
                    let was_focused = self
                        .focus_target(ctx)?
                        .is_some_and(|node| ctx.is_on_focus_path(node));
                    let source = MarkdownSource::from_prepared(*prepared);
                    let limited = source.syntax_limited();
                    let parent = self.reader_parent.expect("mounted reader parent");
                    let id = ctx.add_child(parent, source)?;
                    match self.reader {
                        Some(Reader::Table(table)) => {
                            ctx.set_hidden(table.into(), true)?;
                            self.detail = Some(id);
                        }
                        reader => {
                            if let Some(Reader::Text(old)) = reader {
                                ctx.remove_subtree(old)?;
                            }
                            self.reader = Some(Reader::Source(id));
                        }
                    }
                    if was_focused {
                        ctx.set_focus(id.into())?;
                    }
                    self.display_source = self.pending_source.take();
                    self.detail_loading = false;
                    self.apply_reveal(ctx)?;
                    self.show_status(
                        ctx,
                        if limited {
                            "Plain text · syntax limit"
                        } else {
                            ""
                        },
                    )?;
                }
                Ok(PreparedView::Rendered(_)) => {}
                Err(error) => self.show_status(ctx, format!("Preparation failed: {error}"))?,
            }
        }
        Ok(None)
    }
    fn on_unmount(&mut self, _ctx: &mut dyn Context) {
        self.lane = None;
    }
    fn key_outcome(&self, key: Key, _ctx: &dyn ViewContext) -> EventOutcome {
        if self.prompt.is_some()
            && !key.mods.ctrl
            && !key.mods.alt
            && matches!(
                key.key,
                KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Enter | KeyCode::Esc
            )
        {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        }
    }
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Key(key) = event else {
            return Ok(if matches!(event, Event::Mouse(_)) {
                EventOutcome::Handle
            } else {
                EventOutcome::Ignore
            });
        };
        if self.key_outcome(*key, ctx) == EventOutcome::Ignore {
            return Ok(EventOutcome::Ignore);
        }
        if key.key == KeyCode::Esc {
            self.dismiss(ctx)?;
            return Ok(EventOutcome::Handle);
        }
        let prompt = self.prompt.as_mut().expect("prompt key");
        match key.key {
            KeyCode::Backspace => {
                prompt.value.pop();
            }
            KeyCode::Char(ch) if prompt.value.len() + ch.len_utf8() <= 4096 => {
                prompt.value.push(ch)
            }
            KeyCode::Enter => {
                let prompt = self.prompt.take().expect("prompt");
                let status = match prompt.kind {
                    PromptKind::Line => {
                        let line = match prompt.value.parse::<usize>() {
                            Ok(line) if line > 0 => line,
                            _ => {
                                self.prompt = Some(prompt);
                                self.show_status(ctx, "Enter a one-based line number")?;
                                return Ok(EventOutcome::Handle);
                            }
                        };
                        if let Some(source) = self.source_reader() {
                            ctx.with_widget_mut(source, |source: &mut MarkdownSource, ctx| {
                                source.jump_line(ctx, line);
                                Ok(())
                            })?;
                        }
                        format!("Line {line}")
                    }
                    PromptKind::Search => {
                        if let Some(Reader::Outline(id)) = self.reader {
                            ctx.with_widget_mut(id, |outline: &mut OutlineBody, ctx| {
                                outline.filter(&prompt.value);
                                ctx.reveal_anchor(RevealAlign::Nearest);
                                Ok(())
                            })?;
                            format!("Filter: {}", prompt.value)
                        } else {
                            if self.display_source.is_some() {
                                self.work_search = Some(WorkSearch {
                                    query: SearchQuery {
                                        text: prompt.value.clone(),
                                        ..SearchQuery::default()
                                    },
                                    generation: self.generation + 1,
                                    limits: SearchLimits::default(),
                                });
                                self.submit(ctx)?;
                            }
                            format!("Find: {}", prompt.value)
                        }
                    }
                };
                self.restore_focus(ctx)?;
                self.show_status(ctx, status)?;
                return Ok(EventOutcome::Handle);
            }
            _ => {}
        }
        let prompt = self.prompt.as_ref().expect("prompt retained");
        self.show_status(
            ctx,
            format!(
                "{}: {}",
                if matches!(prompt.kind, PromptKind::Search) {
                    "Find"
                } else {
                    "Line"
                },
                prompt.value
            ),
        )?;
        Ok(EventOutcome::Handle)
    }
}

/// Estimate retained outline entries and their visible index during
/// publication.
fn outline_storage(entries: &[OutlineEntry], capacity: usize) -> usize {
    size_of::<OutlineBody>()
        .saturating_add(capacity.saturating_mul(size_of::<OutlineEntry>()))
        .saturating_add(entries.iter().fold(0usize, |bytes, entry| {
            bytes.saturating_add(entry.text.capacity())
        }))
        .saturating_add(entries.len().saturating_mul(size_of::<usize>()))
}

/// A filtered navigation list that preserves keyed heading selection.
struct OutlineBody {
    /// Headings in document order.
    entries: Vec<OutlineEntry>,
    /// Filtered indices into the source-ordered heading entries.
    visible: Vec<usize>,
    /// Ordinal of the selected entry within the visible index.
    selected: usize,
    /// Applied lowercase filter, retained across heading publications.
    filter: String,
    /// Cached retained-entry and visible-index storage estimate.
    storage_bytes: usize,
    /// Node-lifetime wake used to detect owner removal and signal host actions.
    owner: NodeWakeHandle,
    /// Shared bounded actions consumed by the Markdown controller.
    mailbox: Rc<RefCell<Mailbox>>,
}
impl OutlineBody {
    /// Construct the filtered outline with its owner notification channel.
    fn new(
        entries: Vec<OutlineEntry>,
        owner: NodeWakeHandle,
        mailbox: Rc<RefCell<Mailbox>>,
    ) -> Self {
        let storage_bytes = outline_storage(&entries, entries.capacity());
        Self {
            visible: (0..entries.len()).collect(),
            entries,
            selected: 0,
            filter: String::new(),
            storage_bytes,
            owner,
            mailbox,
        }
    }
    /// Return the selected visible heading identity, if one exists.
    fn selected_id(&self) -> Option<u64> {
        self.visible
            .get(self.selected)
            .map(|index| self.entries[*index].id)
    }
    /// Rebuild the filtered index while retaining a heading key or nearby
    /// ordinal.
    fn rebuild(&mut self, selected: Option<u64>, fallback: usize) {
        self.visible = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                entry
                    .text
                    .to_lowercase()
                    .contains(&self.filter)
                    .then_some(index)
            })
            .collect();
        self.selected = selected
            .and_then(|id| {
                self.visible
                    .iter()
                    .position(|index| self.entries[*index].id == id)
            })
            .unwrap_or(fallback.min(self.visible.len().saturating_sub(1)));
    }
    /// Apply a heading filter while preserving the selected heading key.
    fn filter(&mut self, query: &str) {
        let selected = self.selected_id();
        self.filter = query.to_lowercase();
        self.rebuild(selected, 0);
    }
    /// Replace headings without changing the applied filter or taking focus.
    fn publish(&mut self, entries: Vec<OutlineEntry>) {
        let selected = self.selected_id();
        let fallback = self.selected;
        self.storage_bytes = outline_storage(&entries, entries.capacity());
        self.entries = entries;
        self.rebuild(selected, fallback);
    }
    /// Classify the raw keys consumed by outline navigation.
    fn handles(key: Key) -> bool {
        !key.mods.ctrl
            && !key.mods.alt
            && matches!(
                key.key,
                KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Home
                    | KeyCode::End
                    | KeyCode::Enter
                    | KeyCode::Char('j' | 'k' | 'g' | 'G')
            )
    }
}
impl Widget for OutlineBody {
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn name(&self) -> NodeName {
        NodeName::convert("markdown_outline")
    }
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }
    fn canvas(&self, content: Size, _ctx: &CanvasContext) -> Size {
        Size::new(content.w, self.visible.len().min(u32::MAX as usize) as u32)
    }
    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        for (row, index) in self
            .visible
            .iter()
            .enumerate()
            .skip(view.scroll.y as usize)
            .take(view.content.h as usize)
        {
            let entry = &self.entries[*index];
            let label = format!(
                "{}{}",
                "  ".repeat(entry.level.saturating_sub(1).min(5) as usize),
                entry
                    .text
                    .chars()
                    .take((view.content.w as usize).saturating_mul(4))
                    .map(|ch| if ch.is_control() { ' ' } else { ch })
                    .collect::<String>()
            );
            render.text(
                if row == self.selected {
                    "list/selected"
                } else {
                    "text"
                },
                Line::new(0, row as u32, view.content.w),
                &label,
            )?;
        }
        Ok(())
    }
    fn reveal_anchor(&self, _content: Size) -> Option<Rect> {
        Some(Rect::new(0, self.selected as u32, 1, 1))
    }
    fn key_outcome(&self, key: Key, _ctx: &dyn ViewContext) -> EventOutcome {
        if Self::handles(key) {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        }
    }
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Key(key) = event else {
            return Ok(EventOutcome::Ignore);
        };
        if !Self::handles(*key) {
            return Ok(EventOutcome::Ignore);
        }
        match key.key {
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.visible.len().saturating_sub(1))
            }
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => {
                self.selected = self.visible.len().saturating_sub(1)
            }
            KeyCode::Enter => {
                if let Some(index) = self.visible.get(self.selected) {
                    self.mailbox
                        .borrow_mut()
                        .push(InspectorEvent::Navigate(self.entries[*index].source_byte))?;
                    let _ = self.owner.wake()?;
                }
            }
            _ => {}
        }
        ctx.reveal_anchor(RevealAlign::Nearest);
        Ok(EventOutcome::Handle)
    }
}

#[cfg(test)]
mod tests {
    use canopy::{runtime::TurnInput, testing::harness::Harness};

    use super::*;
    use crate::executor::{ExecutorOptions, ManualExecutor};

    struct Owner {
        inspector: Option<InspectorHandle>,
    }
    impl Widget for Owner {
        // The inspector mounts in its owner, over the owner's own drawing.
        fn layout(&self) -> Layout {
            Layout::fill().direction(Direction::Stack)
        }
        fn name(&self) -> NodeName {
            NodeName::convert("inspector_owner")
        }
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }
        fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
            render.fill("text", ctx.view().outer_rect_local(), '~')
        }
        fn on_unmount(&mut self, _ctx: &mut dyn Context) {
            if let Some(inspector) = &self.inspector {
                inspector.wake().expect("wake overlay during owner unmount");
            }
        }
    }
    struct TestRoot;
    impl Widget for TestRoot {
        fn layout(&self) -> Layout {
            Layout::fill().direction(Direction::Stack)
        }
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            ctx.add_child(ctx.node_id(), Owner { inspector: None })?;
            Ok(())
        }
    }
    pub(super) fn fixture(
        content: InspectorContent,
    ) -> Result<(Harness, NodeId, InspectorHandle, ManualExecutor)> {
        let (executor, manual) = MarkdownExecutor::manual(ExecutorOptions::default());
        let mut harness = Harness::builder(TestRoot)
            .register::<Inspector>()
            .size(80, 24)
            .build()?;
        let owner = harness.find_nodes("**/inspector_owner/")?[0];
        let handle = harness.canopy.with_context(owner, |ctx| {
            ctx.with_widget_mut(owner, |owner: &mut Owner, ctx| {
                let handle = Inspector::open(ctx, executor, content)?;
                owner.inspector = Some(handle.clone());
                Ok(handle)
            })
        })?;
        harness.canopy.turn(TurnInput::Prepare)?;
        Ok((harness, owner, handle, manual))
    }
    pub(super) fn outline() -> InspectorContent {
        InspectorContent::Outline {
            entries: vec![
                OutlineEntry {
                    id: 1,
                    level: 1,
                    text: "First".into(),
                    source_byte: 2,
                },
                OutlineEntry {
                    id: 2,
                    level: 2,
                    text: "Second".into(),
                    source_byte: 20,
                },
            ],
        }
    }
    #[test]
    fn the_inspector_covers_its_owner_between_controls_and_in_empty_reader_rows() -> Result<()> {
        let (mut harness, owner, handle, _) = fixture(outline())?;
        harness.render()?;
        let (owner, inspector) = harness.canopy.with_root_context(|ctx| {
            Ok((
                ctx.view_of(owner).unwrap().outer,
                ctx.view_of(handle.node.into()).unwrap().outer,
            ))
        })?;
        assert_eq!(
            inspector, owner,
            "the inspector fills its owner, and only it"
        );
        for row in harness.tbuf().lines() {
            assert!(
                !row.contains('~'),
                "the owner's drawing leaked through the inspector: {row}"
            );
        }
        Ok(())
    }

    #[test]
    fn owner_removal_reclaims_the_inspector_without_a_timer() -> Result<()> {
        let (mut harness, owner, handle, _) = fixture(outline())?;
        harness
            .canopy
            .with_root_context(|ctx| ctx.remove_subtree(owner))?;
        harness.canopy.turn(TurnInput::Prepare)?;
        assert!(harness.find_nodes("**/markdown_inspector/")?.is_empty());
        assert_eq!(handle.wake()?, WakeOutcome::Expired);
        Ok(())
    }
    #[test]
    fn rolled_back_owner_removal_keeps_the_modal_alive() -> Result<()> {
        let (mut harness, owner, handle, _) = fixture(outline())?;
        let result = harness.canopy.with_root_context(|ctx| {
            ctx.edit_structure(&mut |ctx| {
                ctx.remove_subtree(owner)?;
                Err(Error::Invalid("rollback fixture".into()))
            })
        });
        assert!(result.is_err());
        harness.canopy.turn(TurnInput::Prepare)?;
        assert_eq!(harness.find_nodes("**/markdown_inspector/")?.len(), 1);
        assert_ne!(handle.wake()?, WakeOutcome::Expired);
        harness.key(KeyCode::Down)?;
        harness.key(KeyCode::Enter)?;
        assert_eq!(handle.take_events(2), vec![InspectorEvent::Navigate(20)]);
        Ok(())
    }
    #[test]
    fn hiding_and_detaching_owner_preserve_node_lifetime_inspector() -> Result<()> {
        let (mut harness, owner, handle, _) = fixture(outline())?;
        harness.canopy.with_root_context(|ctx| {
            ctx.set_hidden(owner, true)?;
            ctx.detach(owner)
        })?;
        handle.wake()?;
        harness.canopy.turn(TurnInput::Prepare)?;
        // The inspector travels with its detached owner, and lives on.
        assert_ne!(handle.wake()?, WakeOutcome::Expired);
        harness.canopy.with_root_context(|ctx| {
            let root = ctx.root_id();
            ctx.attach(root, owner)?;
            ctx.set_hidden(owner, false)
        })?;
        harness.canopy.turn(TurnInput::Prepare)?;
        assert_eq!(harness.find_nodes("**/markdown_inspector/")?.len(), 1);
        Ok(())
    }
    #[test]
    fn loading_overlay_closes_and_releases_queued_work() -> Result<()> {
        let (mut harness, _, handle, manual) = fixture(InspectorContent::Code {
            info: "rust".into(),
            text: Arc::from("let x = 1;\n"),
        })?;
        harness.key(KeyCode::Esc)?;
        assert_eq!(handle.take_events(1), vec![InspectorEvent::Close]);
        harness.canopy.with_root_context(|ctx| handle.close(ctx))?;
        assert!(!manual.run_next());
        assert!(harness.find_nodes("**/markdown_inspector/")?.is_empty());
        Ok(())
    }
    #[test]
    fn pinned_source_copy_is_exact_and_worker_prepared() -> Result<()> {
        let mut store = SourceStore::stream();
        store.append("é\r\n\tend\n").unwrap();
        let pinned = store.snapshot();
        let (mut harness, _, handle, manual) =
            fixture(InspectorContent::Source { snapshot: pinned })?;
        store.append("new").unwrap();
        assert!(manual.run_next());
        harness.canopy.turn(TurnInput::Prepare)?;
        harness.canopy.with_context(handle.node, |ctx| {
            ctx.with_widget_mut(handle.node, |inspector: &mut Inspector, ctx| {
                inspector.copy_all(ctx)
            })
        })?;
        assert_eq!(
            handle.take_events(8),
            vec![InspectorEvent::CopySource {
                text: Arc::from("é\r\n\tend\n"),
                range: 0..9,
            }]
        );
        assert!(
            !manual.run_next(),
            "the owner serializes the frozen copy once"
        );
        Ok(())
    }
    #[test]
    fn source_copy_admission_preserves_requests_and_rejects_mailbox_overflow() -> Result<()> {
        let (mut harness, _, handle, _) = fixture(InspectorContent::Code {
            info: String::new(),
            text: Arc::from(""),
        })?;
        harness.canopy.with_context(handle.node, |ctx| {
            ctx.with_widget_mut(handle.node, |inspector: &mut Inspector, ctx| {
                for index in 0..4 {
                    inspector.full_copy = Some(Arc::from(
                        char::from(b'a' + index)
                            .to_string()
                            .repeat(DEFAULT_COPY_BYTES),
                    ));
                    inspector.copy_all(ctx)?;
                }
                inspector.full_copy = Some(Arc::from("e".repeat(DEFAULT_COPY_BYTES)));
                assert!(
                    inspector
                        .copy_all(ctx)
                        .unwrap_err()
                        .to_string()
                        .contains("queue is full")
                );
                Ok(())
            })
        })?;
        let first = handle.take_events(EVENT_COUNT);
        assert_eq!(first.len(), 4);
        for (index, event) in first.into_iter().enumerate() {
            let InspectorEvent::CopySource { text, range } = event else {
                panic!("copy event")
            };
            assert_eq!(range, 0..DEFAULT_COPY_BYTES);
            assert_eq!(text.len(), DEFAULT_COPY_BYTES);
            assert!(text.bytes().all(|byte| byte == b'a' + index as u8));
        }
        // A rejected request can be retried after the owner releases capacity.
        harness.canopy.with_context(handle.node, |ctx| {
            ctx.with_widget_mut(handle.node, |inspector: &mut Inspector, ctx| {
                inspector.copy_all(ctx)
            })
        })?;
        let last = handle.take_events(EVENT_COUNT);
        assert_eq!(last.len(), 1);
        let InspectorEvent::CopySource { text, .. } = &last[0] else {
            panic!("copy event")
        };
        assert_eq!(text.len(), DEFAULT_COPY_BYTES);
        assert!(text.bytes().all(|byte| byte == b'e'));
        Ok(())
    }
    #[test]
    fn configured_copy_limit_never_admits_output_larger_than_the_mailbox() -> Result<()> {
        let (executor, _) = MarkdownExecutor::manual(ExecutorOptions::default());
        let mut harness = Harness::builder(TestRoot)
            .register::<Inspector>()
            .size(80, 24)
            .build()?;
        let owner = harness.find_nodes("**/inspector_owner/")?[0];
        let handle = harness.canopy.with_context(owner, |ctx| {
            Inspector::open_configured(
                ctx,
                owner,
                executor,
                InspectorContent::Code {
                    info: String::new(),
                    text: Arc::from(""),
                },
                true,
                usize::MAX,
                0,
            )
        })?;
        harness.canopy.turn(TurnInput::Prepare)?;
        for bytes in [EVENT_BYTES, EVENT_BYTES + 1] {
            let admission = harness.canopy.with_context(handle.node, |ctx| {
                ctx.with_widget_mut(handle.node, |inspector: &mut Inspector, ctx| {
                    assert_eq!(inspector.copy_limit, EVENT_BYTES);
                    inspector.full_copy = Some(Arc::from("x".repeat(bytes)));
                    inspector.copy_all(ctx)
                })
            });
            let events = handle.take_events(EVENT_COUNT);
            if bytes == EVENT_BYTES {
                admission?;
                assert_eq!(events.len(), 1);
                let InspectorEvent::CopySource { text, .. } = &events[0] else {
                    panic!("copy event")
                };
                assert_eq!(text.len(), EVENT_BYTES);
            } else {
                assert!(admission.unwrap_err().to_string().contains("Copy exceeds"));
                assert!(events.is_empty());
                harness.canopy.with_root_context(|ctx| {
                    ctx.with_widget(handle.node, |inspector: &Inspector| {
                        assert!(
                            inspector
                                .request_source_copy(inspector.full_copy.clone().unwrap(), 0..1)
                                .unwrap_err()
                                .to_string()
                                .contains("queue is full")
                        );
                        Ok(())
                    })
                })?;
            }
            harness.canopy.with_root_context(|ctx| {
                ctx.with_widget(handle.node, |inspector: &Inspector| {
                    assert!(inspector.pending_copy.is_none());
                    Ok(())
                })
            })?;
        }
        Ok(())
    }
    #[test]
    fn prompt_escape_does_not_dismiss_and_admission_matches() -> Result<()> {
        let (mut harness, _, handle, _) = fixture(outline())?;
        harness.key(Key::parse_spec("ctrl-f")?)?;
        harness.type_text("Second")?;
        harness.key(KeyCode::Esc)?;
        assert!(handle.take_events(4).is_empty());
        harness.key(KeyCode::Esc)?;
        assert_eq!(handle.take_events(4), vec![InspectorEvent::Close]);
        Ok(())
    }
    #[test]
    fn mailbox_limits_preserve_complete_requests() {
        let mut mailbox = Mailbox::default();
        assert!(
            mailbox
                .push(InspectorEvent::CopyText("x".repeat(EVENT_BYTES + 1)))
                .is_err()
        );
        assert!(mailbox.events.is_empty());
        for _ in 0..EVENT_COUNT {
            mailbox.push(InspectorEvent::Close).unwrap();
        }
        assert!(mailbox.push(InspectorEvent::Close).is_err());
        assert_eq!(mailbox.take(1), vec![InspectorEvent::Close]);
        mailbox.push(InspectorEvent::Close).unwrap();
    }
}

#[cfg(test)]
mod interaction_tests {
    #[cfg(feature = "syntax")]
    use canopy::testing::harness::Harness;
    use canopy::{input::key, runtime::TurnInput};

    use super::{
        tests::{fixture, outline},
        *,
    };

    #[test]
    fn outline_publication_keeps_filter_and_selected_heading_identity() -> Result<()> {
        let (mut harness, _, handle, _) = fixture(outline())?;
        harness.key(Key::parse_spec("ctrl-f")?)?;
        harness.type_text("Second")?;
        harness.key(key::KeyCode::Enter)?;
        harness.canopy.with_root_context(|ctx| {
            handle.publish_outline(ctx, || {
                vec![
                    OutlineEntry {
                        id: 3,
                        level: 1,
                        text: "Second new".into(),
                        source_byte: 30,
                    },
                    OutlineEntry {
                        id: 2,
                        level: 2,
                        text: "Second".into(),
                        source_byte: 22,
                    },
                    OutlineEntry {
                        id: 4,
                        level: 1,
                        text: "Unmatched".into(),
                        source_byte: 40,
                    },
                ]
            })
        })?;
        harness.with_unique(|outline: &mut OutlineBody, _| {
            assert_eq!(outline.filter, "second");
            assert_eq!(outline.visible.len(), 2);
            assert_eq!(outline.selected_id(), Some(2));
            Ok(())
        })?;
        harness.key(key::KeyCode::Enter)?;
        assert_eq!(handle.take_events(4), vec![InspectorEvent::Navigate(22)]);
        Ok(())
    }

    #[test]
    #[cfg(feature = "syntax")]
    fn code_inspector_uses_shared_fence_grammar_resolution() -> Result<()> {
        use canopy::style::PartialStyle;

        // The initial caret restyles byte zero; inspect a different keyword.
        for (info, text, token, highlighted) in [
            (
                "rust,ignore",
                "fn main() { let value = 42; }\n",
                "let",
                true,
            ),
            (
                "luau",
                "local value: number = 42\nreturn value\n",
                "return",
                true,
            ),
            (
                "Python extra words",
                "def main(): return 42\n",
                "return",
                true,
            ),
            ("C++", "int main() { return 0; }\n", "return", true),
            (
                "nosuchlang",
                "fn main() { let value = 42; }\n",
                "let",
                false,
            ),
        ] {
            let (mut harness, _, handle, manual) = fixture(InspectorContent::Code {
                info: info.into(),
                text: Arc::from(text),
            })?;
            assert!(manual.run_next());
            harness.canopy.turn(TurnInput::Prepare)?;
            harness.render()?;
            let keyword = harness.canopy.style().resolve("syntax/keyword/plain");
            assert_eq!(
                harness
                    .tbuf()
                    .contains_text_style(token, &PartialStyle::new().fg(keyword.fg)),
                highlighted,
                "fence info {info:?}"
            );
            harness.with_unique(|source: &mut MarkdownSource, _| {
                assert_eq!(source.options().language.as_deref(), Some(info));
                assert_eq!(source.source_text().as_ref(), text);
                Ok(())
            })?;
            assert!(harness.canopy.with_root_context(|ctx| handle.ready(ctx))?);
            assert!(!manual.run_next());
        }
        Ok(())
    }

    #[test]
    #[cfg(feature = "syntax")]
    fn code_inspector_recolors_across_palettes_without_repreparation() -> Result<()> {
        use canopy::style::{PartialStyle, themes};

        let (mut harness, _, handle, manual) = fixture(InspectorContent::Code {
            info: "rust".into(),
            text: Arc::from("fn main() { let answer = 42; }\n"),
        })?;
        assert!(manual.run_next());
        harness.canopy.turn(TurnInput::Prepare)?;
        let identity = |harness: &mut Harness| {
            harness.with_unique(|source: &mut MarkdownSource, ctx| {
                Ok((
                    ctx.node_id(),
                    source.source_text().as_ptr(),
                    source.epoch(),
                    source.revision(),
                    source.prepared_width(),
                    source.derived_bytes(),
                    ctx.view().canvas,
                ))
            })
        };
        let prepared_identity = identity(&mut harness)?;
        let mut foregrounds = Vec::new();
        for palette in [
            themes::default_dark(),
            themes::solarized_light(),
            themes::default_dark(),
        ] {
            harness.canopy.set_theme(palette);
            harness.render()?;
            let keyword = harness.canopy.style().resolve("syntax/keyword/plain");
            foregrounds.push(keyword.fg.clone());
            // Keep the syntax oracle outside the initial caret cell.
            assert!(
                harness
                    .tbuf()
                    .contains_text_style("let", &PartialStyle::new().fg(keyword.fg))
            );
            assert_eq!(identity(&mut harness)?, prepared_identity);
            assert!(harness.canopy.with_root_context(|ctx| handle.ready(ctx))?);
            assert!(
                !manual.run_next(),
                "a palette switch must not schedule source preparation"
            );
        }
        assert_ne!(foregrounds[0], foregrounds[1]);
        assert_eq!(foregrounds[0], foregrounds[2]);
        Ok(())
    }

    #[test]
    fn readiness_tracks_worker_publication_without_polling_timers() -> Result<()> {
        let snapshot = SourceStore::static_text("source text").unwrap().snapshot();
        let (mut harness, _, handle, manual) = fixture(InspectorContent::Source { snapshot })?;
        harness.canopy.with_root_context(|ctx| {
            assert!(!handle.ready(ctx)?);
            Ok(())
        })?;
        assert!(manual.run_next());
        harness.canopy.turn(TurnInput::Prepare)?;
        harness.canopy.with_root_context(|ctx| {
            assert!(handle.ready(ctx)?);
            assert!(handle.resource_bytes(ctx)? >= "source text".len());
            Ok(())
        })?;
        harness.key(Key::parse_spec("ctrl-f")?)?;
        harness.type_text("text")?;
        harness.key(key::KeyCode::Enter)?;
        harness.canopy.with_root_context(|ctx| {
            assert!(!handle.ready(ctx)?);
            Ok(())
        })?;
        assert!(manual.run_next());
        harness.canopy.turn(TurnInput::Prepare)?;
        harness.canopy.with_root_context(|ctx| {
            assert!(handle.ready(ctx)?);
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn queued_line_reveal_applies_after_source_preparation() -> Result<()> {
        let snapshot = SourceStore::static_text("a\r\nneedle\n")
            .unwrap()
            .snapshot();
        let (mut harness, _, handle, manual) = fixture(InspectorContent::Source { snapshot })?;
        harness
            .canopy
            .with_root_context(|ctx| handle.reveal_line(ctx, 2))?;
        assert!(manual.run_next());
        harness.canopy.turn(TurnInput::Prepare)?;
        harness.with_unique(|source: &mut MarkdownSource, _| {
            assert_eq!(source.source_anchor().byte, 3);
            Ok(())
        })?;
        harness
            .canopy
            .with_root_context(|ctx| handle.reveal_source_byte(ctx, 5))?;
        harness.with_unique(|source: &mut MarkdownSource, _| {
            assert_eq!(source.source_anchor().byte, 5);
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn prompt_submission_reports_its_own_operation() -> Result<()> {
        let snapshot = SourceStore::static_text("a\nb\nc\n").unwrap().snapshot();
        let (mut harness, _, _, manual) = fixture(InspectorContent::Source { snapshot })?;
        assert!(manual.run_next());
        harness.canopy.turn(TurnInput::Prepare)?;
        harness.key(':')?;
        harness.type_text("2")?;
        harness.key(key::KeyCode::Enter)?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Line 2"));
        assert!(!harness.tbuf().contains_text("Find: 2"));

        let (mut harness, _, _, _) = fixture(outline())?;
        harness.key(Key::parse_spec("ctrl-f")?)?;
        harness.type_text("Sec")?;
        harness.key(key::KeyCode::Enter)?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Filter: Sec"));
        Ok(())
    }

    #[test]
    fn outline_filter_retains_key_and_explicit_jump() -> Result<()> {
        let (mut harness, _, handle, _) = fixture(outline())?;
        harness.key(key::KeyCode::Down)?;
        harness.key(key::Key::parse_spec("ctrl-f")?)?;
        harness.type_text("Second")?;
        harness.key(key::KeyCode::Enter)?;
        assert!(handle.take_events(4).is_empty());
        harness.key(key::KeyCode::Enter)?;
        assert_eq!(handle.take_events(4), vec![InspectorEvent::Navigate(20)]);
        Ok(())
    }

    #[test]
    fn outline_navigates_with_vi_letters() -> Result<()> {
        let (mut harness, _, handle, _) = fixture(outline())?;
        for (key, byte) in [('j', 20), ('k', 2), ('G', 20), ('g', 2)] {
            harness.key(key)?;
            harness.key(KeyCode::Enter)?;
            assert_eq!(handle.take_events(4), vec![InspectorEvent::Navigate(byte)]);
        }
        Ok(())
    }

    #[test]
    fn outline_filter_reveals_fallback_selection_from_a_scrolled_view() -> Result<()> {
        let entries = (0..200)
            .map(|index| OutlineEntry {
                id: index,
                level: 1,
                text: format!("{} {index}", if index < 100 { "keep" } else { "drop" }),
                source_byte: index as usize,
            })
            .collect();
        let (mut harness, _, _, _) = fixture(InspectorContent::Outline { entries })?;
        harness.key(KeyCode::End)?;
        harness.render()?;
        harness.with_unique(|_: &mut OutlineBody, ctx| {
            assert!(ctx.view().scroll.y > 0);
            Ok(())
        })?;
        harness.key(Key::parse_spec("ctrl-f")?)?;
        harness.type_text("keep")?;
        harness.key(KeyCode::Enter)?;
        harness.render()?;
        harness.with_unique(|outline: &mut OutlineBody, ctx| {
            assert_eq!(outline.selected_id(), Some(0));
            assert_eq!(outline.visible.len(), 100);
            assert_eq!(ctx.view().scroll.y, 0);
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn source_search_is_worker_prepared_and_escape_clears_it_first() -> Result<()> {
        let snapshot = SourceStore::static_text("first\nneedle\nlast\nneedle\n")
            .unwrap()
            .snapshot();
        let (mut harness, _, handle, manual) = fixture(InspectorContent::Source { snapshot })?;
        assert!(manual.run_next());
        harness.canopy.turn(TurnInput::Prepare)?;
        harness.key(key::Key::parse_spec("ctrl-f")?)?;
        harness.type_text("needle")?;
        harness.key(key::KeyCode::Enter)?;
        assert!(manual.run_next());
        harness.canopy.turn(TurnInput::Prepare)?;
        harness.with_unique(|source: &mut MarkdownSource, _| {
            assert_eq!(source.search_matches(), 2);
            assert_eq!(source.source_anchor().byte, 0);
            Ok(())
        })?;
        harness.key(key::KeyCode::F(3))?;
        harness.with_unique(|source: &mut MarkdownSource, _| {
            assert_eq!(source.source_anchor().byte, 6);
            Ok(())
        })?;
        harness.key(key::KeyCode::F(3))?;
        harness.with_unique(|source: &mut MarkdownSource, _| {
            assert_eq!(source.source_anchor().byte, 18);
            Ok(())
        })?;
        harness.key(key::KeyCode::Esc)?;
        assert!(handle.take_events(4).is_empty());
        harness.key(key::KeyCode::Esc)?;
        assert_eq!(handle.take_events(4), vec![InspectorEvent::Close]);
        Ok(())
    }

    #[test]
    fn link_open_requires_explicit_action_and_uses_pinned_url() -> Result<()> {
        let (mut harness, _, handle, manual) = fixture(InspectorContent::Link {
            label: "A full label".into(),
            original: "../target".into(),
            resolved: Some("https://example.test/target".into()),
            allow_open: true,
        })?;
        assert!(handle.take_events(4).is_empty());
        assert!(manual.run_next());
        harness.canopy.turn(TurnInput::Prepare)?;
        assert!(handle.take_events(4).is_empty());
        harness.key(key::Key::parse_spec("ctrl-o")?)?;
        assert_eq!(
            handle.take_events(4),
            vec![InspectorEvent::OpenUrl(
                "https://example.test/target".into()
            )]
        );
        Ok(())
    }

    #[test]
    fn disabled_clipboard_disables_command_admission() -> Result<()> {
        let (mut harness, _, handle, _) = fixture(InspectorContent::Code {
            info: "".into(),
            text: Arc::from("code"),
        })?;
        harness
            .canopy
            .with_root_context(|ctx| handle.set_capabilities(ctx, false, 10))?;
        harness.canopy.with_context(handle.node, |ctx| {
            assert_eq!(
                ctx.command_status(
                    &Inspector::call_copy_all()
                        .with_target(CommandTarget::Exact(handle.node.into()))
                )?
                .label(),
                "disabled"
            );
            Ok(())
        })?;
        assert!(handle.take_events(4).is_empty());
        Ok(())
    }
}

#[cfg(test)]
mod table_tests {
    use canopy::{input::key::KeyCode, runtime::TurnInput};
    use canopy_widgets::table::{
        self, TableCell, TableColumn, TableData, TableLayoutOptions, TableRow,
    };

    use super::{tests::fixture, *};

    fn prepared(dataset: &str, revision: u64, value: &str) -> Arc<PreparedTable> {
        let data = TableData::new(
            dataset,
            revision,
            vec![TableColumn::new("column", "Header")],
            vec![TableRow::new("row", vec![TableCell::plain(value)])],
        )
        .unwrap();
        Arc::new(table::prepare(Arc::new(data), TableLayoutOptions::default()).unwrap())
    }
    #[test]
    fn modal_reader_uses_remaining_height_and_chrome_stays_at_bottom() -> Result<()> {
        let (mut harness, _, handle, _) = fixture(InspectorContent::Table {
            table: prepared("table", 1, "cell"),
        })?;
        harness.canopy.with_root_context(|ctx| {
            let (reader, status) = ctx.with_widget(handle.node, |inspector: &Inspector| {
                Ok((
                    inspector.reader_parent.unwrap(),
                    NodeId::from(inspector.status.unwrap()),
                ))
            })?;
            let body = ctx.parent_of(reader).unwrap();
            let buttons = *ctx.children_of(body).last().unwrap();
            let body = ctx.view_of(body).unwrap().content;
            let reader = ctx.view_of(reader).unwrap().outer;
            let status = ctx.view_of(status).unwrap().outer;
            let buttons = ctx.view_of(buttons).unwrap().outer;
            assert_eq!(status.h, 1);
            assert_eq!(buttons.h, 1);
            assert_eq!(reader.h, body.h - 2);
            assert_eq!(reader.tl.y, body.tl.y);
            assert_eq!(
                i64::from(status.tl.y),
                i64::from(reader.tl.y) + i64::from(reader.h)
            );
            assert_eq!(buttons.tl.y, status.tl.y + 1);
            assert_eq!(
                i64::from(buttons.tl.y) + i64::from(buttons.h),
                i64::from(body.tl.y) + i64::from(body.h)
            );
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn routed_copy_preserves_table_rectangle_selection() -> Result<()> {
        let data = TableData::new(
            "table",
            1,
            vec![
                TableColumn::new("stage", "Stage"),
                TableColumn::new("state", "State"),
            ],
            vec![
                TableRow::new(
                    "source",
                    vec![TableCell::plain("Source"), TableCell::plain("ready")],
                ),
                TableRow::new(
                    "parse",
                    vec![TableCell::plain("Parse"), TableCell::plain("ready")],
                ),
            ],
        )
        .unwrap();
        let table =
            Arc::new(table::prepare(Arc::new(data), TableLayoutOptions::default()).unwrap());
        let (mut harness, _, handle, _) = fixture(InspectorContent::Table { table })?;
        harness.key(Key::parse_spec("shift-Right")?)?;
        harness.key(Key::parse_spec("shift-Down")?)?;
        harness.key('y')?;
        assert_eq!(
            handle.take_events(4),
            vec![InspectorEvent::CopyText(
                "Source\tready\nParse\tready\n".into()
            )]
        );
        Ok(())
    }

    #[test]
    fn table_copy_above_the_default_table_queue_bound_reaches_the_owner() -> Result<()> {
        let value = "x".repeat(2 * 1024 * 1024);
        let (mut harness, _, handle, _) = fixture(InspectorContent::Table {
            table: prepared("table", 1, &value),
        })?;
        harness.key('y')?;
        assert_eq!(handle.take_events(4), vec![InspectorEvent::CopyText(value)]);
        Ok(())
    }

    #[test]
    fn table_copies_wait_for_mailbox_capacity_without_dropping_events() -> Result<()> {
        let (mut harness, _, handle, _) = fixture(InspectorContent::Table {
            table: prepared("table", 1, "cell"),
        })?;
        harness.canopy.with_context(handle.node, |ctx| {
            ctx.with_widget_mut(handle.node, |inspector: &mut Inspector, ctx| {
                for _ in 0..EVENT_COUNT {
                    inspector.mailbox.borrow_mut().push(InspectorEvent::Close)?;
                }
                let Some(Reader::Table(table)) = inspector.reader else {
                    panic!("table reader")
                };
                ctx.with_widget_mut(table, |table: &mut TableView, ctx| {
                    table.copy_cell(ctx)?;
                    table.copy_cell(ctx)
                })?;
                inspector.drain_table(ctx)?;
                assert_eq!(inspector.pending_copy.as_deref(), Some("cell"));
                ctx.with_widget(table, |table: &TableView| {
                    assert_eq!(table.pending_events().0, 1);
                    Ok(())
                })
            })
        })?;
        assert_eq!(handle.take_events(EVENT_COUNT).len(), EVENT_COUNT);
        harness.canopy.turn(TurnInput::Prepare)?;
        assert_eq!(
            handle.take_events(EVENT_COUNT),
            vec![
                InspectorEvent::CopyText("cell".into()),
                InspectorEvent::CopyText("cell".into()),
            ]
        );
        Ok(())
    }

    #[test]
    fn cell_detail_is_pinned_and_escape_restores_updated_keyed_table() -> Result<()> {
        let (mut harness, _, handle, manual) = fixture(InspectorContent::Table {
            table: prepared("table", 1, "old\r\ncell"),
        })?;
        harness.key(KeyCode::Enter)?;
        harness.canopy.turn(TurnInput::Prepare)?;
        assert!(manual.run_next());
        harness.canopy.turn(TurnInput::Prepare)?;
        harness
            .canopy
            .with_root_context(|ctx| handle.publish_table(ctx, prepared("table", 2, "new")))?;
        harness.with_unique(|source: &mut MarkdownSource, _| {
            assert_eq!(&*source.source_text(), "old\r\ncell");
            Ok(())
        })?;
        harness.key(KeyCode::Esc)?;
        assert!(handle.take_events(4).is_empty());
        harness.with_unique(|table: &mut TableView, _| {
            assert_eq!(table.table().data().revision(), 2);
            assert_eq!(table.cursor().unwrap().row.as_ref(), "row");
            Ok(())
        })?;
        assert!(harness.find_nodes("**/markdown_source/")?.is_empty());
        harness.key(KeyCode::Esc)?;
        assert_eq!(handle.take_events(4), vec![InspectorEvent::Close]);
        Ok(())
    }
    #[test]
    fn escape_cancels_pending_cell_preparation_without_closing_modal() -> Result<()> {
        let (mut harness, _, handle, manual) = fixture(InspectorContent::Table {
            table: prepared("table", 1, "cell"),
        })?;
        harness.key(KeyCode::Enter)?;
        harness.canopy.turn(TurnInput::Prepare)?;
        harness.key(KeyCode::Esc)?;
        assert!(handle.take_events(4).is_empty());
        manual.run_until_idle();
        harness.canopy.turn(TurnInput::Prepare)?;
        assert!(harness.find_nodes("**/markdown_source/")?.is_empty());
        harness.key(KeyCode::Esc)?;
        assert_eq!(handle.take_events(4), vec![InspectorEvent::Close]);
        Ok(())
    }
    #[test]
    fn table_republication_rejects_a_different_dataset() -> Result<()> {
        let (mut harness, _, handle, _) = fixture(InspectorContent::Table {
            table: prepared("table", 1, "cell"),
        })?;
        assert!(
            harness
                .canopy
                .with_root_context(
                    |ctx| handle.publish_table(ctx, prepared("other", 2, "different"))
                )
                .is_err()
        );
        harness.with_unique(|table: &mut TableView, _| {
            assert_eq!(table.table().data().dataset().as_ref(), "table");
            Ok(())
        })?;
        Ok(())
    }
}
