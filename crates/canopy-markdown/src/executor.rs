//! Bounded Markdown work shared by the host's readers.
//!
//! Each reader owns a lane with one running request, one replaceable desired
//! request, and one completed result. Owners acknowledge accepted and rejected
//! results with the latest published base before the next preparation starts.
use std::{
    collections::{HashMap, VecDeque},
    error::Error as StdError,
    fmt, io,
    mem::{size_of, take},
    ops::Range,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    thread::{self, JoinHandle},
};

use canopy::runtime::NodeWakeHandle;

use crate::{
    ViewType,
    document::{DocumentSnapshot, ObjectKind, ParseLimits, ReadingRange, parse_document},
    presentation::{GeometryKey, PresentationSnapshot, Supplied},
    search::{CompiledQuery, SearchLimits, SearchQuery, SearchResults, search_document},
    source_store::SourceSnapshot,
    source_view::{MarkdownSource, PreparedSource, SourceMatches, SourceOptions},
};

/// Aggregate bounds for one host's Markdown work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutorOptions {
    /// Maximum concurrent jobs; zero is normalized to one.
    pub worker_count: usize,
    /// Maximum registered reader lanes.
    pub max_documents: usize,
    /// Maximum accepted copies, including running and undisposed results.
    pub max_copy_jobs: usize,
    /// Aggregate retained input payload/header estimates and maximum output
    /// bytes of accepted copies, excluding allocator overhead.
    pub max_copy_bytes: usize,
}

impl Default for ExecutorOptions {
    fn default() -> Self {
        Self {
            worker_count: 2,
            max_documents: 64,
            max_copy_jobs: 16,
            max_copy_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Immutable input. Reader interaction state is deliberately absent.
#[derive(Clone, Debug)]
pub struct WorkRequest {
    /// Exact accepted bytes and lifecycle to prepare.
    pub source: SourceSnapshot,
    /// Requested representation.
    pub view: ViewType,
    /// Allocated body geometry and layout preferences.
    pub geometry: GeometryKey,
    /// Source wrapping, numbering, and syntax preferences.
    pub source_options: SourceOptions,
    /// Previously published Rendered geometry available for layout reuse.
    pub previous: Option<Arc<PresentationSnapshot>>,
    /// Last published semantics, retained independently of cached geometry.
    pub previous_document: Option<Arc<DocumentSnapshot>>,
    /// Exact previously published Source bytes from the same epoch. Prefix
    /// verification happens on the worker before UI publication.
    pub previous_source: Option<Arc<str>>,
    /// Semantic construction bounds.
    pub limits: ParseLimits,
    /// Owner generation for view and geometry changes.
    pub generation: u64,
    /// Optional query over this request's exact projection.
    pub search: Option<WorkSearch>,
    /// What the host supplied for the document's embeds and links.
    pub supplied: Arc<Supplied>,
}

/// A query over the exact snapshot carried by its preparation request.
#[derive(Clone, Debug)]
pub struct WorkSearch {
    /// Retained query text and explicit matching behavior.
    pub query: SearchQuery,
    /// Owner query generation used to reject obsolete results.
    pub generation: u64,
    /// Bounds on query compilation, scan bytes, and stored matches.
    pub limits: SearchLimits,
}

#[derive(Debug)]
/// A query result addressed in its prepared representation.
pub enum SearchOutput {
    /// Exact raw-source byte ranges.
    Source(SourceMatches),
    /// Readable unit, line, or table-cell addresses.
    Rendered(SearchResults),
}

/// Identity checked by the owner immediately before publishing a result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkTags {
    /// Source identity, changed on replacement.
    pub epoch: u64,
    /// Accepted source revision, including lifecycle changes.
    pub revision: u64,
    /// Prepared representation.
    pub view: ViewType,
    /// Geometry used by this preparation.
    pub geometry: GeometryKey,
    /// Source preferences used by this preparation.
    pub source_options: SourceOptions,
    /// Semantic limits used by this preparation.
    pub limits: ParseLimits,
    /// Owner generation captured at submission.
    pub generation: u64,
    /// Optional query generation captured at submission.
    pub query_generation: Option<u64>,
}

impl WorkRequest {
    /// Capture the complete immutable identity of this request.
    pub fn tags(&self) -> WorkTags {
        WorkTags {
            epoch: self.source.epoch,
            revision: self.source.revision,
            view: self.view,
            geometry: self.geometry,
            source_options: self.source_options.clone(),
            limits: self.limits,
            generation: self.generation,
            query_generation: self.search.as_ref().map(|search| search.generation),
        }
    }
}

/// An owned representation ready for atomic UI publication.
pub enum PreparedView {
    /// Semantic model and Rendered geometry published together.
    Rendered(Arc<PresentationSnapshot>),
    /// Exact accepted Source bytes with prepared editor layout.
    Source(Box<PreparedSource>),
}

impl fmt::Debug for PreparedView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rendered(presentation) => f.debug_tuple("Rendered").field(presentation).finish(),
            Self::Source(_) => f.write_str("Source(PreparedSource)"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// A failed job that still completes the lane handshake.
pub enum WorkError {
    /// The owner was removed, the executor stopped, or held work was dropped.
    Cancelled,
    /// An explicitly reported preparation or copy failure.
    Failed(String),
    /// A worker panic caught at the job boundary.
    WorkerPanicked(String),
}

impl fmt::Display for WorkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("Markdown work cancelled"),
            Self::Failed(message) => f.write_str(message),
            Self::WorkerPanicked(message) => write!(f, "Markdown worker panicked: {message}"),
        }
    }
}

impl StdError for WorkError {}

#[derive(Debug)]
/// One completed preparation, including its exact request provenance.
pub struct WorkResult {
    /// The request snapshot, retained for exact publication provenance.
    pub source: SourceSnapshot,
    /// Identity checked by the owner before publication.
    pub tags: WorkTags,
    /// Prepared owned data or a recoverable worker failure.
    pub result: Result<PreparedView, WorkError>,
    /// Optional bounded query results, independently reporting query errors.
    pub search: Option<Result<SearchOutput, String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// An admission failure; the owner retains responsibility for dirty work.
pub enum SubmitError {
    /// The shared service no longer accepts work.
    Shutdown,
    /// A document, copy-count, or copy-byte bound was reached.
    Capacity,
    /// Admission was deliberately rejected by the controlled scheduler.
    Rejected,
}

impl fmt::Display for SubmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Shutdown => "Markdown executor is shut down",
            Self::Capacity => "Markdown executor document capacity reached",
            Self::Rejected => "Markdown work admission rejected",
        })
    }
}

impl StdError for SubmitError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A bounded snapshot of shared queue occupancy.
pub struct ExecutorStats {
    /// Number of registered reader lanes.
    pub documents: usize,
    /// Jobs currently occupying workers.
    pub running: usize,
    /// Lanes retaining a latest desired preparation.
    pub desired: usize,
    /// Prepared results awaiting owner disposition.
    pub completed: usize,
    /// Reader lanes queued for fair worker selection.
    pub ready: usize,
    /// Maximum jobs that may run concurrently.
    pub worker_limit: usize,
    /// Whether admission and queued execution have stopped.
    pub stopped: bool,
    /// Accepted copies, including queued, running, and undisposed results.
    pub copy_jobs: usize,
    /// Reserved retained-input and maximum-output bytes for accepted copies.
    pub copy_bytes: usize,
}

/// Immutable payload retained until its FIFO copy request completes.
#[derive(Clone, Debug)]
pub enum CopyInput {
    /// Exact source bytes and a UTF-8 selection interval.
    Source {
        /// Shared exact bytes of the displayed revision.
        text: Arc<str>,
        /// Selected UTF-8 byte interval.
        range: Range<usize>,
    },
    /// A complete readable projection or a selected reading interval.
    Reading {
        /// Pinned immutable semantics used by this copy.
        document: Arc<DocumentSnapshot>,
        /// Optional readable selection; None copies every reading unit.
        selection: Option<ReadingRange>,
    },
    /// A link destination or one object's complete readable payload.
    Object {
        /// Pinned immutable semantics used by this copy.
        document: Arc<DocumentSnapshot>,
        /// Pinned object identity within the document.
        object_id: u64,
    },
    /// A small already-owned immutable payload, such as a host-provided URL.
    Text(Arc<str>),
}

impl CopyInput {
    /// Charge shared retained payloads and Arc headers without scanning text.
    /// Allocator bookkeeping and rounding are excluded.
    fn retained_bytes(&self) -> usize {
        let arc_header = 2 * size_of::<usize>();
        match self {
            Self::Source { text, .. } | Self::Text(text) => text.len().saturating_add(arc_header),
            Self::Reading { document, .. } | Self::Object { document, .. } => document
                .source
                .retained_bytes()
                .saturating_add(document.derived_bytes())
                .saturating_add(arc_header),
        }
    }
}

#[derive(Debug)]
/// One FIFO copy completion awaiting owner disposition.
pub struct CopyResult {
    /// Host request identity captured before copy work was submitted.
    pub id: u64,
    /// Complete copied payload or an explicit failure; never partial output.
    pub result: Result<String, WorkError>,
}

/// One accepted FIFO copy with its aggregate capacity reservation.
struct CopyRequest {
    /// Host activation identity allocated before serialization.
    id: u64,
    /// Pinned immutable payload to serialize.
    input: CopyInput,
    /// Maximum complete output size.
    max_bytes: usize,
    /// Retained-input plus maximum-output capacity reservation.
    reserved: usize,
}

/// A completed copy that still owns its capacity reservation.
struct CopyCompletion {
    /// Complete FIFO result awaiting owner disposition.
    result: CopyResult,
    /// Capacity released when this completion is taken or removed.
    reserved: usize,
}

/// Concrete work selected from one reader lane.
enum Job {
    /// Preparation whose latest desired input has been admitted.
    Prepare(Box<WorkRequest>),
    /// One FIFO copy that must not be coalesced away.
    Copy(CopyRequest),
}

#[derive(Default)]
/// One reader's replaceable preparation and independent FIFO copies.
struct Lane {
    /// Latest replaceable preparation not yet started.
    desired: Option<WorkRequest>,
    /// Whether this lane currently occupies a worker.
    running: bool,
    /// Whether the running job is a FIFO copy.
    running_copy: bool,
    /// Whether a taken preparation still awaits publication disposition.
    awaiting_ack: bool,
    /// At most one prepared result awaiting the owner.
    completed: Option<WorkResult>,
    /// Whether this lane already appears in the ready queue.
    queued: bool,
    /// Non-owning runtime wake for the registered node.
    wake: Option<NodeWakeHandle>,
    /// Accepted copies in activation order.
    copies: VecDeque<CopyRequest>,
    /// Completed copies in activation order.
    copy_completed: VecDeque<CopyCompletion>,
    /// Alternates copy and preparation when both can start.
    prefer_copy: bool,
}

/// Shared queue bookkeeping, mutated only while holding the service mutex.
struct State {
    /// Registered reader lanes, keyed by never-reused identities.
    lanes: HashMap<u64, Lane>,
    /// Fair round-robin reader identities eligible for execution.
    ready: VecDeque<u64>,
    /// Next reader identity, never reused after removal.
    next_id: u64,
    /// Aggregate workers occupied, including removed owners' jobs.
    running: usize,
    /// Whether further admission and queued execution are forbidden.
    stopped: bool,
    /// Controlled scheduler's remaining injected admission failures.
    reject_submissions: usize,
    /// Accepted copies still retaining a capacity reservation.
    copy_jobs: usize,
    /// Aggregate retained-input plus maximum-output reservation.
    copy_bytes: usize,
}

/// Worker-visible bounds, queue state, and readiness signal.
struct Shared {
    /// Immutable aggregate worker and storage bounds.
    options: ExecutorOptions,
    /// Queue state shared by owners, workers, and the controlled driver.
    state: Mutex<State>,
    /// Signals newly executable work, completion, or shutdown.
    ready: Condvar,
}

impl Shared {
    /// Recover queue bookkeeping after poisoning so shutdown remains possible.
    fn lock(&self) -> MutexGuard<'_, State> {
        // Work and owner callbacks never run under this mutex. Recovering the
        // bookkeeping still allows shutdown if another internal operation
        // panics.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Append one eligible lane only once, preserving round-robin order.
    fn queue(state: &mut State, id: u64) {
        if state.stopped {
            return;
        }
        if let Some(lane) = state.lanes.get_mut(&id)
            && !lane.running
            && !lane.queued
            && ((!lane.copies.is_empty())
                || (!lane.awaiting_ack && lane.completed.is_none() && lane.desired.is_some()))
        {
            lane.queued = true;
            state.ready.push_back(id);
        }
    }

    /// Reserve one worker slot and move a fair lane's next job out.
    fn take_ready(self: &Arc<Self>, state: &mut State) -> Option<HeldWork> {
        if state.stopped || state.running >= self.options.worker_count {
            return None;
        }
        while let Some(id) = state.ready.pop_front() {
            let Some(lane) = state.lanes.get_mut(&id) else {
                continue;
            };
            lane.queued = false;
            if lane.running {
                continue;
            }
            let can_prepare =
                !lane.awaiting_ack && lane.completed.is_none() && lane.desired.is_some();
            let job = if !lane.copies.is_empty() && (lane.prefer_copy || !can_prepare) {
                lane.prefer_copy = false;
                Job::Copy(lane.copies.pop_front().expect("queued copy"))
            } else if can_prepare {
                lane.prefer_copy = true;
                Job::Prepare(Box::new(lane.desired.take().expect("desired work")))
            } else {
                continue;
            };
            lane.running = true;
            lane.running_copy = matches!(&job, Job::Copy(_));
            state.running += 1;
            return Some(HeldWork {
                shared: self.clone(),
                id,
                job: Some(job),
            });
        }
        None
    }

    /// Deliver a preparation or discard it after owner removal, freeing its
    /// worker.
    fn complete(&self, id: u64, result: WorkResult) {
        let wake = {
            let mut state = self.lock();
            state.running -= 1;
            if let Some(lane) = state.lanes.get_mut(&id) {
                lane.running = false;
                lane.completed = Some(result);
                let wake = lane.wake.clone();
                Self::queue(&mut state, id);
                wake
            } else {
                None
            }
        };
        self.ready.notify_all();
        if let Some(wake) = wake {
            drop(wake.wake());
        }
    }

    /// Deliver a FIFO copy and release the worker even if its owner was
    /// removed.
    fn complete_copy(&self, id: u64, result: CopyResult, reserved: usize) {
        let wake = {
            let mut state = self.lock();
            state.running -= 1;
            let stopped = state.stopped;
            if let Some(lane) = state.lanes.get_mut(&id) {
                lane.running = false;
                lane.running_copy = false;
                lane.copy_completed
                    .push_back(CopyCompletion { result, reserved });
                if stopped {
                    Self::cancel_copies(lane);
                }
                let wake = lane.wake.clone();
                Self::queue(&mut state, id);
                wake
            } else {
                state.copy_jobs -= 1;
                state.copy_bytes -= reserved;
                None
            }
        };
        self.ready.notify_all();
        if let Some(wake) = wake {
            drop(wake.wake());
        }
    }

    /// Check cooperative cancellation between expensive preparation passes.
    fn cancelled(&self, id: u64) -> bool {
        let state = self.lock();
        state.stopped || !state.lanes.contains_key(&id)
    }

    /// Append queued cancellations in FIFO order without releasing result
    /// reservations.
    fn cancel_copies(lane: &mut Lane) {
        for request in lane.copies.drain(..) {
            lane.copy_completed.push_back(CopyCompletion {
                result: CopyResult {
                    id: request.id,
                    result: Err(WorkError::Cancelled),
                },
                reserved: request.reserved,
            });
        }
    }

    /// Stop queued execution, cancel pending copies, and wake every owner.
    fn stop(&self) {
        let wakes = {
            let mut state = self.lock();
            state.stopped = true;
            state.ready.clear();
            state
                .lanes
                .values_mut()
                .filter_map(|lane| {
                    lane.desired = None;
                    lane.queued = false;
                    if !lane.running_copy {
                        Self::cancel_copies(lane);
                    }
                    lane.wake.clone()
                })
                .collect::<Vec<_>>()
        };
        self.ready.notify_all();
        for wake in wakes {
            drop(wake.wake());
        }
    }
}

/// Host-owned worker handles and worker-visible shared state.
struct Service {
    /// Worker-visible state that can outlive the host handle.
    shared: Arc<Shared>,
    /// Join handles reserved for explicit host teardown.
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl Drop for Service {
    fn drop(&mut self) {
        self.shared.stop();
        // Dropping JoinHandles detaches them. Neither reader removal nor the
        // last service handle waits for a non-interruptible parser pass.
    }
}

/// Host-owned service. Clones share workers, queues, and all bounds.
#[derive(Clone)]
pub struct MarkdownExecutor {
    /// Host-owned worker pool shared by every clone.
    service: Arc<Service>,
}

impl fmt::Debug for MarkdownExecutor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MarkdownExecutor")
            .field("stats", &self.stats())
            .finish()
    }
}

impl Default for MarkdownExecutor {
    fn default() -> Self {
        Self::new(ExecutorOptions::default())
    }
}

impl MarkdownExecutor {
    /// Start a host-owned worker pool, panicking if a thread cannot start.
    pub fn new(options: ExecutorOptions) -> Self {
        Self::try_new(options).expect("start Markdown worker threads")
    }

    /// Start a host-owned worker pool and report thread creation failures.
    pub fn try_new(options: ExecutorOptions) -> io::Result<Self> {
        let executor = Self::without_workers(options);
        for index in 0..executor.service.shared.options.worker_count {
            let shared = executor.service.shared.clone();
            let worker = thread::Builder::new()
                .name(format!("markdown-{index}"))
                .spawn(move || {
                    loop {
                        let work = {
                            let mut state = shared.lock();
                            loop {
                                if state.stopped {
                                    return;
                                }
                                if let Some(work) = shared.take_ready(&mut state) {
                                    break work;
                                }
                                state = shared
                                    .ready
                                    .wait(state)
                                    .unwrap_or_else(PoisonError::into_inner);
                            }
                        };
                        work.finish();
                    }
                })?;
            executor
                .service
                .workers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(worker);
        }
        Ok(executor)
    }

    /// Create the production bookkeeping without starting physical threads.
    fn without_workers(mut options: ExecutorOptions) -> Self {
        options.worker_count = options.worker_count.max(1);
        let shared = Arc::new(Shared {
            options,
            state: Mutex::new(State {
                lanes: HashMap::new(),
                ready: VecDeque::new(),
                next_id: 1,
                running: 0,
                stopped: false,
                reject_submissions: 0,
                copy_jobs: 0,
                copy_bytes: 0,
            }),
            ready: Condvar::new(),
        });
        Self {
            service: Arc::new(Service {
                shared,
                workers: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Creates the production queue with explicitly driven work completion.
    pub fn manual(options: ExecutorOptions) -> (Self, ManualExecutor) {
        let executor = Self::without_workers(options);
        let manual = ManualExecutor {
            executor: executor.clone(),
        };
        (executor, manual)
    }

    /// Reserve one reader lane, optionally attaching its runtime wake.
    pub fn register(&self, wake: Option<NodeWakeHandle>) -> Result<WorkHandle, SubmitError> {
        let shared = &self.service.shared;
        let mut state = shared.lock();
        if state.stopped {
            return Err(SubmitError::Shutdown);
        }
        if state.lanes.len() >= shared.options.max_documents {
            return Err(SubmitError::Capacity);
        }
        let id = state.next_id;
        state.next_id = state
            .next_id
            .checked_add(1)
            .expect("Markdown document IDs exhausted");
        state.lanes.insert(
            id,
            Lane {
                wake,
                ..Lane::default()
            },
        );
        Ok(WorkHandle {
            shared: shared.clone(),
            id,
        })
    }

    /// Inspect aggregate occupancy without retaining any work payloads.
    pub fn stats(&self) -> ExecutorStats {
        let state = self.service.shared.lock();
        ExecutorStats {
            documents: state.lanes.len(),
            running: state.running,
            desired: state
                .lanes
                .values()
                .filter(|lane| lane.desired.is_some())
                .count(),
            completed: state
                .lanes
                .values()
                .filter(|lane| lane.completed.is_some())
                .count(),
            ready: state.ready.len(),
            worker_limit: self.service.shared.options.worker_count,
            stopped: state.stopped,
            copy_jobs: state.copy_jobs,
            copy_bytes: state.copy_bytes,
        }
    }

    /// Stops admission and queued work; already running passes finish in the
    /// background. This method never joins a worker on the calling UI thread.
    pub fn shutdown(&self) {
        self.service.shared.stop();
    }

    /// Waits for worker exit after explicit shutdown. Call from host teardown,
    /// never from removal of a Markdown node.
    pub fn join(&self) {
        self.shutdown();
        let workers = take(
            &mut *self
                .service
                .workers
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for worker in workers {
            drop(worker.join());
        }
    }
}

/// One reader's bounded lane. Dropping it releases queued and completed data.
pub struct WorkHandle {
    /// Shared service state retained independently of the owner node.
    shared: Arc<Shared>,
    /// This reader's never-reused lane identity.
    id: u64,
}

impl fmt::Debug for WorkHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkHandle").field("id", &self.id).finish()
    }
}

impl WorkHandle {
    /// Replace the latest desired preparation without interrupting running
    /// work.
    pub fn submit(&self, request: WorkRequest) -> Result<(), SubmitError> {
        {
            let mut state = self.shared.lock();
            if state.stopped {
                return Err(SubmitError::Shutdown);
            }
            if state.reject_submissions > 0 {
                state.reject_submissions -= 1;
                return Err(SubmitError::Rejected);
            }
            let Some(lane) = state.lanes.get_mut(&self.id) else {
                return Err(SubmitError::Shutdown);
            };
            lane.desired = Some(request);
            Shared::queue(&mut state, self.id);
        }
        self.shared.ready.notify_one();
        Ok(())
    }

    /// Takes a result for validation and publication. Call `acknowledge` after
    /// either accepting or rejecting it, supplying the latest published base.
    pub fn take_completed(&self) -> Option<WorkResult> {
        // Taking a result makes nothing runnable: the lane awaits its
        // acknowledgement, which signals workers instead.
        let mut state = self.shared.lock();
        let lane = state.lanes.get_mut(&self.id)?;
        let result = lane.completed.take();
        if result.is_some() {
            lane.awaiting_ack = true;
        }
        result
    }

    /// Releases the result handshake after publication or rejection. Any queued
    /// request builds on this actually published semantic base.
    pub fn acknowledge(&self, previous: Option<Arc<PresentationSnapshot>>) {
        self.acknowledge_with_source(previous, None);
    }

    /// Rebase both prepared surfaces before releasing queued work. `source`
    /// must be the exact published Source bytes from the request's epoch.
    pub fn acknowledge_with_source(
        &self,
        previous: Option<Arc<PresentationSnapshot>>,
        source: Option<Arc<str>>,
    ) {
        let document = previous
            .as_ref()
            .map(|presentation| presentation.document.clone());
        self.acknowledge_document(previous, source, document);
    }

    /// Rebase queued work on the last published semantics even when its cached
    /// Rendered geometry has been evicted. Every base belongs to this epoch.
    pub fn acknowledge_document(
        &self,
        previous: Option<Arc<PresentationSnapshot>>,
        source: Option<Arc<str>>,
        document: Option<Arc<DocumentSnapshot>>,
    ) {
        {
            let mut state = self.shared.lock();
            let Some(lane) = state.lanes.get_mut(&self.id) else {
                return;
            };
            lane.awaiting_ack = false;
            if let Some(desired) = &mut lane.desired {
                desired.previous = previous;
                desired.previous_source = source;
                desired.previous_document = document;
            }
            Shared::queue(&mut state, self.id);
        }
        self.shared.ready.notify_one();
    }

    /// Admits an immutable copy without replacing earlier copy requests. The
    /// input and maximum output are reserved against aggregate byte bounds.
    pub fn submit_copy(
        &self,
        id: u64,
        input: CopyInput,
        max_bytes: usize,
    ) -> Result<(), SubmitError> {
        let reserved = input
            .retained_bytes()
            .checked_add(max_bytes)
            .ok_or(SubmitError::Capacity)?;
        {
            let mut state = self.shared.lock();
            if state.stopped {
                return Err(SubmitError::Shutdown);
            }
            if state.copy_jobs >= self.shared.options.max_copy_jobs
                || reserved
                    > self
                        .shared
                        .options
                        .max_copy_bytes
                        .saturating_sub(state.copy_bytes)
            {
                return Err(SubmitError::Capacity);
            }
            let Some(lane) = state.lanes.get_mut(&self.id) else {
                return Err(SubmitError::Shutdown);
            };
            lane.copies.push_back(CopyRequest {
                id,
                input,
                max_bytes,
                reserved,
            });
            state.copy_jobs += 1;
            state.copy_bytes += reserved;
            Shared::queue(&mut state, self.id);
        }
        self.shared.ready.notify_one();
        Ok(())
    }

    /// Take the oldest completed copy and release its capacity reservation.
    pub fn take_copy_completed(&self) -> Option<CopyResult> {
        let mut state = self.shared.lock();
        let completion = state.lanes.get_mut(&self.id)?.copy_completed.pop_front()?;
        state.copy_jobs -= 1;
        state.copy_bytes -= completion.reserved;
        Some(completion.result)
    }

    /// Attach or refresh the owner wake; signal any already completed result.
    pub fn set_wake(&self, wake: NodeWakeHandle) {
        let to_wake = {
            let mut state = self.shared.lock();
            let stopped = state.stopped;
            let Some(lane) = state.lanes.get_mut(&self.id) else {
                return;
            };
            lane.wake = Some(wake);
            if lane.completed.is_some() || !lane.copy_completed.is_empty() || stopped {
                lane.wake.clone()
            } else {
                None
            }
        };
        if let Some(wake) = to_wake {
            drop(wake.wake());
        }
    }

    /// Whether this lane retains work, results, or a publication handshake.
    pub fn is_pending(&self) -> bool {
        self.shared.lock().lanes.get(&self.id).is_some_and(|lane| {
            lane.running
                || lane.awaiting_ack
                || lane.desired.is_some()
                || lane.completed.is_some()
                || !lane.copies.is_empty()
                || !lane.copy_completed.is_empty()
        })
    }
}

impl Drop for WorkHandle {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        if let Some(lane) = state.lanes.remove(&self.id) {
            let queued_jobs = lane.copies.len() + lane.copy_completed.len();
            let queued_bytes = lane.copies.iter().map(|copy| copy.reserved).sum::<usize>()
                + lane
                    .copy_completed
                    .iter()
                    .map(|copy| copy.reserved)
                    .sum::<usize>();
            state.copy_jobs -= queued_jobs;
            state.copy_bytes -= queued_bytes;
        }
        // Removal frees no worker slot; a running job releases its own on
        // completion.
        state.ready.retain(|id| *id != self.id);
    }
}

/// Deterministic driver for the same admission, queue, and completion
/// machinery.
#[derive(Clone, Debug)]
pub struct ManualExecutor {
    /// Service whose production queue this driver controls.
    executor: MarkdownExecutor,
}

impl ManualExecutor {
    /// Start the next fair queued job while respecting the worker bound.
    pub fn start_next(&self) -> Option<HeldWork> {
        let shared = &self.executor.service.shared;
        let mut state = shared.lock();
        shared.take_ready(&mut state)
    }

    /// Run and deliver one job; return false when none can start.
    pub fn run_next(&self) -> bool {
        if let Some(work) = self.start_next() {
            work.finish();
            true
        } else {
            false
        }
    }

    /// Deliver jobs until all remaining lanes await input or disposition.
    pub fn run_until_idle(&self) {
        while self.run_next() {}
    }

    /// Reject one subsequent preparation admission for deterministic tests.
    pub fn reject_next_submission(&self) {
        self.executor.service.shared.lock().reject_submissions += 1;
    }
}

/// A running job held by the controlled scheduler. Dropping it completes the
/// handshake with cancellation, so a test cannot accidentally strand a lane.
pub struct HeldWork {
    /// Queue that receives this job's completion.
    shared: Arc<Shared>,
    /// Owning lane identity, possibly removed before completion.
    id: u64,
    /// One running job; taking it prevents duplicate completion.
    job: Option<Job>,
}

impl HeldWork {
    /// Inspect a held preparation request; copy jobs return None.
    pub fn request(&self) -> Option<&WorkRequest> {
        match self.job.as_ref()? {
            Job::Prepare(request) => Some(request),
            Job::Copy(_) => None,
        }
    }

    /// Inspect the host request identity of a held copy job.
    pub fn copy_id(&self) -> Option<u64> {
        match self.job.as_ref()? {
            Job::Copy(request) => Some(request.id),
            Job::Prepare(_) => None,
        }
    }

    /// Execute held work with panic containment and deliver its completion.
    pub fn finish(mut self) {
        let job = self.job.take().expect("running Markdown job");
        match job {
            Job::Prepare(request) => {
                let tags = request.tags();
                let source = request.source.clone();
                let result = guarded(|| prepare(&self.shared, self.id, *request));
                let (result, search) = match result {
                    Ok((view, search)) => (Ok(view), search),
                    Err(error) => (Err(error), None),
                };
                self.shared.complete(
                    self.id,
                    WorkResult {
                        source,
                        tags,
                        result,
                        search,
                    },
                );
            }
            Job::Copy(request) => {
                let result = guarded(|| {
                    if self.shared.cancelled(self.id) {
                        return Err(WorkError::Cancelled);
                    }
                    let output = serialize_copy(request.input, request.max_bytes)?;
                    if self.shared.cancelled(self.id) {
                        return Err(WorkError::Cancelled);
                    }
                    Ok(output)
                });
                self.shared.complete_copy(
                    self.id,
                    CopyResult {
                        id: request.id,
                        result,
                    },
                    request.reserved,
                );
            }
        }
    }

    /// Deliver a controlled failure through the normal completion path.
    pub fn fail(mut self, message: impl Into<String>) {
        let error = WorkError::Failed(message.into());
        self.complete_error(error);
    }

    /// Complete either concrete job kind exactly once with an explicit error.
    fn complete_error(&mut self, error: WorkError) {
        match self.job.take().expect("running Markdown job") {
            Job::Prepare(request) => self.shared.complete(
                self.id,
                WorkResult {
                    source: request.source.clone(),
                    tags: request.tags(),
                    result: Err(error),
                    search: None,
                },
            ),
            Job::Copy(request) => self.shared.complete_copy(
                self.id,
                CopyResult {
                    id: request.id,
                    result: Err(error),
                },
                request.reserved,
            ),
        }
    }
}

impl Drop for HeldWork {
    fn drop(&mut self) {
        if self.job.is_some() {
            self.complete_error(WorkError::Cancelled);
        }
    }
}

/// Translate worker panics into ordinary failure completions.
fn guarded<T>(work: impl FnOnce() -> Result<T, WorkError>) -> Result<T, WorkError> {
    catch_unwind(AssertUnwindSafe(work)).unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_owned());
        Err(WorkError::WorkerPanicked(message))
    })
}

/// Prepared surface plus independently fallible optional search results.
type PreparedWork = (PreparedView, Option<Result<SearchOutput, String>>);

/// Prepare immutable input off the UI thread, checking cancellation between
/// passes.
fn prepare(shared: &Shared, id: u64, request: WorkRequest) -> Result<PreparedWork, WorkError> {
    if shared.cancelled(id) {
        return Err(WorkError::Cancelled);
    }
    match request.view {
        ViewType::Source => {
            // Exact accepted source remains usable even if Markdown parsing
            // exceeds its independent semantic bounds.
            let mut source = PreparedSource::new(
                request.source.epoch,
                request.source.revision,
                request.source.text(),
                request.geometry.width,
                request.source_options,
            );
            if let Some(previous) = request.previous_source {
                source = source
                    .verify_prefix(previous)
                    .map_err(|error| WorkError::Failed(error.to_string()))?;
            }
            if shared.cancelled(id) {
                return Err(WorkError::Cancelled);
            }
            let search = request.search.map(|search| {
                MarkdownSource::find_matches(
                    request.source.epoch,
                    request.source.revision,
                    &request.source.text(),
                    &search.query,
                    &search.limits,
                )
                .map(SearchOutput::Source)
                .map_err(|error| error.to_string())
            });
            if shared.cancelled(id) {
                return Err(WorkError::Cancelled);
            }
            Ok((PreparedView::Source(Box::new(source)), search))
        }
        ViewType::Rendered => {
            let previous_document = request.previous_document.or_else(|| {
                request
                    .previous
                    .as_ref()
                    .map(|presentation| presentation.document.clone())
            });
            let document = previous_document
                .as_ref()
                .filter(|previous| {
                    previous.source.epoch == request.source.epoch
                        && previous.source.revision == request.source.revision
                        && previous.limits == request.limits
                })
                .cloned()
                .unwrap_or_else(|| {
                    Arc::new(parse_document(
                        &request.source,
                        &request.limits,
                        previous_document.as_deref(),
                    ))
                });
            if shared.cancelled(id) {
                return Err(WorkError::Cancelled);
            }
            let presentation = PresentationSnapshot::prepare(
                document.clone(),
                request.geometry,
                request.previous.as_deref(),
                &request.supplied,
            );
            if shared.cancelled(id) {
                return Err(WorkError::Cancelled);
            }
            let search = request.search.map(|search| {
                CompiledQuery::new(&search.query, &search.limits)
                    .map(|query| {
                        SearchOutput::Rendered(search_document(
                            &document,
                            &query,
                            search.generation,
                            &search.limits,
                        ))
                    })
                    .map_err(|error| error.to_string())
            });
            if shared.cancelled(id) {
                return Err(WorkError::Cancelled);
            }
            Ok((PreparedView::Rendered(Arc::new(presentation)), search))
        }
    }
}

/// Build only complete output that fits the caller's byte budget.
fn serialize_copy(input: CopyInput, max_bytes: usize) -> Result<String, WorkError> {
    let too_large = || WorkError::Failed(format!("Copy exceeds {max_bytes} bytes"));
    match input {
        CopyInput::Text(text) => {
            if text.len() > max_bytes {
                return Err(too_large());
            }
            Ok(text.to_string())
        }
        CopyInput::Source { text, range } => {
            let selected = text
                .get(range)
                .ok_or_else(|| WorkError::Failed("Invalid source selection".into()))?;
            if selected.len() > max_bytes {
                return Err(too_large());
            }
            Ok(selected.to_owned())
        }
        CopyInput::Object {
            document,
            object_id,
        } => {
            let object = document
                .objects
                .iter()
                .find(|object| object.id == object_id)
                .ok_or_else(|| WorkError::Failed("Missing copy object".into()))?;
            let text = match &object.kind {
                ObjectKind::Link(link) => link.destination.as_str(),
                _ => document
                    .unit(object.unit_id)
                    .map(|unit| unit.text.as_str())
                    .ok_or_else(|| WorkError::Failed("Missing copy object payload".into()))?,
            };
            if text.len() > max_bytes {
                return Err(too_large());
            }
            Ok(text.to_owned())
        }
        CopyInput::Reading {
            document,
            selection,
        } => {
            let mut output = String::new();
            let (start, end) = if let Some(selection) = selection {
                let a = document
                    .unit_position(selection.start.unit_id)
                    .ok_or_else(|| WorkError::Failed("Missing copy selection start".into()))?;
                let b = document
                    .unit_position(selection.end.unit_id)
                    .ok_or_else(|| WorkError::Failed("Missing copy selection end".into()))?;
                let from = (a, selection.start.offset);
                let to = (b, selection.end.offset);
                if from <= to { (from, to) } else { (to, from) }
            } else {
                let Some(last) = document.units.last() else {
                    return Ok(output);
                };
                ((0, 0), (document.units.len() - 1, last.text.len()))
            };
            for index in start.0..=end.0 {
                let unit = &document.units[index];
                let from = if index == start.0 && !unit.atomic {
                    start.1.min(unit.text.len())
                } else {
                    0
                };
                let to = if index == end.0 && !unit.atomic {
                    end.1.min(unit.text.len())
                } else {
                    unit.text.len()
                };
                let part = unit
                    .text
                    .get(from..to)
                    .ok_or_else(|| WorkError::Failed("Invalid readable selection".into()))?;
                let separator = if index == start.0 {
                    ""
                } else {
                    document.separator(index)
                };
                if separator.len().saturating_add(part.len())
                    > max_bytes.saturating_sub(output.len())
                {
                    return Err(too_large());
                }
                output.push_str(separator);
                output.push_str(part);
            }
            Ok(output)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::source_store::SourceStore;

    fn request(generation: u64) -> WorkRequest {
        WorkRequest {
            source: SourceStore::static_text("# A\r\n\té\n").unwrap().snapshot(),
            view: ViewType::Source,
            geometry: GeometryKey::default(),
            source_options: SourceOptions::default(),
            previous: None,
            previous_document: None,
            previous_source: None,
            limits: ParseLimits::default(),
            generation,
            search: None,
            supplied: Arc::default(),
        }
    }

    fn manual(workers: usize, documents: usize) -> (MarkdownExecutor, ManualExecutor) {
        MarkdownExecutor::manual(ExecutorOptions {
            worker_count: workers,
            max_documents: documents,
            ..ExecutorOptions::default()
        })
    }

    #[test]
    fn desired_requests_coalesce_but_running_and_completed_are_not_replaced() {
        let (executor, driver) = manual(1, 1);
        let handle = executor.register(None).unwrap();
        handle.submit(request(1)).unwrap();
        let running = driver.start_next().unwrap();
        handle.submit(request(2)).unwrap();
        handle.submit(request(3)).unwrap();
        assert!(driver.start_next().is_none());
        running.finish();
        assert!(driver.start_next().is_none());
        assert_eq!(handle.take_completed().unwrap().tags.generation, 1);
        assert!(driver.start_next().is_none());
        handle.acknowledge(None);
        let latest = driver.start_next().unwrap();
        assert_eq!(latest.request().unwrap().generation, 3);
        latest.finish();
        assert_eq!(handle.take_completed().unwrap().tags.generation, 3);
        handle.acknowledge(None);
        assert!(!handle.is_pending());
        assert_eq!(executor.stats().running, 0);
    }

    #[test]
    fn lanes_are_fair_and_cancelled_or_failed_work_releases_the_lane() {
        let (executor, driver) = manual(1, 2);
        let first = executor.register(None).unwrap();
        let second = executor.register(None).unwrap();
        first.submit(request(1)).unwrap();
        let held = driver.start_next().unwrap();
        first.submit(request(3)).unwrap();
        second.submit(request(2)).unwrap();
        drop(held);
        assert_eq!(
            first.take_completed().unwrap().result.unwrap_err(),
            WorkError::Cancelled
        );
        first.acknowledge(None);
        let next = driver.start_next().unwrap();
        assert_eq!(next.request().unwrap().generation, 2);
        next.fail("injected worker failure");
        assert!(matches!(
            second.take_completed().unwrap().result,
            Err(WorkError::Failed(_))
        ));
        assert!(driver.run_next());
        assert_eq!(first.take_completed().unwrap().tags.generation, 3);
        assert_eq!(executor.stats().running, 0);
    }

    #[test]
    fn document_admission_and_removal_bound_ready_queue() {
        let (executor, driver) = manual(1, 1);
        let first = executor.register(None).unwrap();
        assert_eq!(executor.register(None).unwrap_err(), SubmitError::Capacity);
        first.submit(request(1)).unwrap();
        let held = driver.start_next().unwrap();
        drop(first);
        assert_eq!(executor.stats().documents, 0);
        let replacement = executor.register(None).unwrap();
        replacement.submit(request(2)).unwrap();
        assert!(driver.start_next().is_none());
        held.finish();
        assert!(driver.run_next());
        replacement.take_completed().unwrap();
        drop(replacement);
        for _ in 0..20 {
            let transient = executor.register(None).unwrap();
            transient.submit(request(3)).unwrap();
            drop(transient);
            assert_eq!(executor.stats().ready, 0);
        }
    }

    #[test]
    fn rejection_leaves_no_phantom_running_request() {
        let (executor, driver) = manual(1, 1);
        let handle = executor.register(None).unwrap();
        driver.reject_next_submission();
        assert_eq!(handle.submit(request(1)), Err(SubmitError::Rejected));
        assert!(!handle.is_pending());
        assert!(!driver.run_next());
        handle.submit(request(2)).unwrap();
        driver.run_until_idle();
        assert_eq!(handle.take_completed().unwrap().tags.generation, 2);
    }

    #[test]
    fn copies_are_fifo_immutable_bounded_and_survive_prepare_replacement() {
        let (executor, driver) = MarkdownExecutor::manual(ExecutorOptions {
            worker_count: 1,
            max_documents: 1,
            max_copy_jobs: 2,
            max_copy_bytes: 64,
        });
        let handle = executor.register(None).unwrap();
        let text: Arc<str> = "é\r\n\t".into();
        handle
            .submit_copy(
                7,
                CopyInput::Source {
                    text: text.clone(),
                    range: 0..text.len(),
                },
                5,
            )
            .unwrap();
        handle
            .submit_copy(8, CopyInput::Text("second".into()), 6)
            .unwrap();
        assert_eq!(
            handle.submit_copy(9, CopyInput::Text("third".into()), 5),
            Err(SubmitError::Capacity)
        );
        handle.submit(request(1)).unwrap();
        handle.submit(request(2)).unwrap();
        driver.run_until_idle();
        assert_eq!(handle.take_completed().unwrap().tags.generation, 2);
        let first = handle.take_copy_completed().unwrap();
        let second = handle.take_copy_completed().unwrap();
        assert_eq!((first.id, first.result.unwrap()), (7, text.to_string()));
        assert_eq!((second.id, second.result.unwrap()), (8, "second".into()));
        assert_eq!(executor.stats().copy_jobs, 0);
        assert_eq!(executor.stats().copy_bytes, 0);
    }

    #[test]
    fn copy_admission_charges_many_small_source_chunk_headers() {
        let text = "x".repeat(128);
        let single = SourceStore::static_text(text.clone()).unwrap();
        let mut chunks = SourceStore::stream();
        for _ in 0..text.len() {
            chunks.append("x").unwrap();
        }
        chunks.finish(crate::FinishOutcome::Completed);
        let input = |store: &SourceStore| CopyInput::Reading {
            document: Arc::new(parse_document(
                &store.snapshot(),
                &ParseLimits::default(),
                None,
            )),
            selection: None,
        };
        let single_input = input(&single);
        let chunked_input = input(&chunks);
        let single_bytes = single_input.retained_bytes();
        assert!(chunked_input.retained_bytes() > single_bytes + text.len());
        let (executor, driver) = MarkdownExecutor::manual(ExecutorOptions {
            max_copy_bytes: single_bytes + text.len(),
            ..ExecutorOptions::default()
        });
        let handle = executor.register(None).unwrap();
        assert_eq!(
            handle.submit_copy(1, chunked_input, text.len()),
            Err(SubmitError::Capacity)
        );
        assert_eq!(executor.stats().copy_bytes, 0);
        handle.submit_copy(2, single_input, text.len()).unwrap();
        driver.run_until_idle();
        assert_eq!(handle.take_copy_completed().unwrap().result.unwrap(), text);
        assert_eq!(executor.stats().copy_bytes, 0);
    }

    #[test]
    fn shutdown_cancels_queued_copies_in_fifo_order_and_releases_reservations() {
        let (executor, driver) = manual(1, 1);
        let handle = executor.register(None).unwrap();
        for id in 1..=3 {
            handle
                .submit_copy(id, CopyInput::Text("copy".into()), 4)
                .unwrap();
        }
        let held = driver.start_next().unwrap();
        assert_eq!(held.copy_id(), Some(1));
        executor.shutdown();
        held.finish();
        for id in 1..=3 {
            let copy = handle.take_copy_completed().unwrap();
            assert_eq!(copy.id, id);
            assert_eq!(copy.result, Err(WorkError::Cancelled));
        }
        assert_eq!(executor.stats().copy_jobs, 0);
        assert_eq!(executor.stats().copy_bytes, 0);
        assert!(!driver.run_next());
    }

    #[test]
    fn acknowledgement_rebases_queued_work_and_resize_reuses_semantics() {
        let (executor, driver) = manual(1, 1);
        let handle = executor.register(None).unwrap();
        let mut work = request(1);
        work.view = ViewType::Rendered;
        handle.submit(work.clone()).unwrap();
        let held = driver.start_next().unwrap();
        work.generation = 2;
        work.geometry.width = 37;
        handle.submit(work).unwrap();
        held.finish();
        let PreparedView::Rendered(first) = handle.take_completed().unwrap().result.unwrap() else {
            panic!("rendered work expected")
        };
        handle.acknowledge(Some(first.clone()));
        let next = driver.start_next().unwrap();
        assert!(Arc::ptr_eq(
            next.request().unwrap().previous.as_ref().unwrap(),
            &first
        ));
        next.finish();
        let PreparedView::Rendered(resized) = handle.take_completed().unwrap().result.unwrap()
        else {
            panic!("rendered work expected")
        };
        assert!(Arc::ptr_eq(&first.document, &resized.document));
        assert_eq!(resized.geometry.width, 37);
    }

    #[test]
    fn source_acknowledgement_rebases_prefix_provenance_before_next_job_starts() {
        let (executor, driver) = manual(1, 1);
        let handle = executor.register(None).unwrap();
        let mut store = SourceStore::stream();
        store.append("prefix").unwrap();
        let mut first = request(1);
        first.source = store.snapshot();
        handle.submit(first.clone()).unwrap();
        let held = driver.start_next().unwrap();
        store.append(" appended").unwrap();
        let mut latest = first;
        latest.source = store.snapshot();
        latest.previous_source = Some("older prefix".into());
        handle.submit(latest).unwrap();
        held.finish();
        let published = handle.take_completed().unwrap().source.text();
        assert!(driver.start_next().is_none());
        handle.acknowledge_with_source(None, Some(published.clone()));
        let next = driver.start_next().unwrap();
        assert!(Arc::ptr_eq(
            next.request().unwrap().previous_source.as_ref().unwrap(),
            &published
        ));
        next.finish();
        assert!(handle.take_completed().unwrap().result.is_ok());
    }

    #[test]
    fn source_prefix_failure_is_returned_by_worker_and_does_not_strand_lane() {
        let (executor, driver) = manual(1, 1);
        let handle = executor.register(None).unwrap();
        let mut work = request(1);
        work.previous_source = Some("different text".into());
        handle.submit(work).unwrap();
        driver.run_until_idle();
        assert!(matches!(
            handle.take_completed().unwrap().result,
            Err(WorkError::Failed(_))
        ));
        handle.acknowledge(None);
        handle.submit(request(2)).unwrap();
        assert!(driver.run_next());
        assert!(handle.take_completed().unwrap().result.is_ok());
    }

    #[test]
    fn source_search_is_tagged_and_preserves_exact_byte_addresses() {
        let (executor, driver) = manual(1, 1);
        let handle = executor.register(None).unwrap();
        let mut work = request(1);
        work.search = Some(WorkSearch {
            query: SearchQuery {
                text: "é".into(),
                ..SearchQuery::default()
            },
            generation: 9,
            limits: SearchLimits::default(),
        });
        handle.submit(work).unwrap();
        driver.run_until_idle();
        let result = handle.take_completed().unwrap();
        assert_eq!(result.tags.query_generation, Some(9));
        let SearchOutput::Source(matches) = result.search.unwrap().unwrap() else {
            panic!("source matches expected")
        };
        assert_eq!(matches.ranges, vec![6..8]);
    }

    #[test]
    fn object_copy_checks_original_payload_on_worker() {
        let source = SourceStore::static_text(
            "[label](https://example.test/path)\n\n```txt\npayload\n```\n",
        )
        .unwrap()
        .snapshot();
        let document = Arc::new(parse_document(&source, &ParseLimits::default(), None));
        let link = document
            .objects
            .iter()
            .find(|object| matches!(object.kind, ObjectKind::Link(_)))
            .unwrap()
            .id;
        let code = document
            .objects
            .iter()
            .find(|object| matches!(object.kind, ObjectKind::Code))
            .unwrap()
            .id;
        assert_eq!(
            serialize_copy(
                CopyInput::Object {
                    document: document.clone(),
                    object_id: link
                },
                64
            )
            .unwrap(),
            "https://example.test/path"
        );
        assert!(
            serialize_copy(
                CopyInput::Object {
                    document: document.clone(),
                    object_id: code
                },
                2
            )
            .is_err()
        );
        assert_eq!(
            serialize_copy(
                CopyInput::Object {
                    document,
                    object_id: code
                },
                64
            )
            .unwrap(),
            "payload\n"
        );
    }

    #[test]
    fn evicted_layout_reuses_semantic_base_and_preserves_queued_append_identity() {
        let (executor, driver) = manual(1, 1);
        let handle = executor.register(None).unwrap();
        let mut store = SourceStore::stream();
        store.append("tail").unwrap();
        let previous = Arc::new(parse_document(
            &store.snapshot(),
            &ParseLimits::default(),
            None,
        ));
        let tail_id = previous.units[0].id;
        let mut work = request(1);
        work.view = ViewType::Rendered;
        work.source = store.snapshot();
        work.previous_document = Some(previous.clone());
        handle.submit(work.clone()).unwrap();
        driver.run_until_idle();
        let PreparedView::Rendered(result) = handle.take_completed().unwrap().result.unwrap()
        else {
            panic!("rendered work expected")
        };
        assert!(Arc::ptr_eq(&result.document, &previous));
        store.append(" grows").unwrap();
        work.source = store.snapshot();
        work.previous_document = None;
        handle.submit(work).unwrap();
        handle.acknowledge_document(None, None, Some(previous.clone()));
        let next = driver.start_next().unwrap();
        assert!(next.request().unwrap().previous.is_none());
        assert!(Arc::ptr_eq(
            next.request().unwrap().previous_document.as_ref().unwrap(),
            &previous
        ));
        next.finish();
        let PreparedView::Rendered(result) = handle.take_completed().unwrap().result.unwrap()
        else {
            panic!("rendered work expected")
        };
        assert_eq!(result.document.units[0].id, tail_id);
    }

    #[test]
    fn queued_append_uses_newly_published_tail_identity() {
        let (executor, driver) = manual(1, 1);
        let handle = executor.register(None).unwrap();
        let mut store = SourceStore::stream();
        store.append("# Head\n\nnew tail").unwrap();
        let mut work = request(1);
        work.source = store.snapshot();
        work.view = ViewType::Rendered;
        handle.submit(work.clone()).unwrap();
        let held = driver.start_next().unwrap();
        store.append(" grows").unwrap();
        work.source = store.snapshot();
        work.generation = 2;
        handle.submit(work).unwrap();
        held.finish();
        let PreparedView::Rendered(first) = handle.take_completed().unwrap().result.unwrap() else {
            panic!("rendered work expected")
        };
        let tail_id = first.document.units.last().unwrap().id;
        handle.acknowledge(Some(first));
        driver.run_until_idle();
        let PreparedView::Rendered(second) = handle.take_completed().unwrap().result.unwrap()
        else {
            panic!("rendered work expected")
        };
        assert_eq!(second.document.units.last().unwrap().id, tail_id);
    }

    #[test]
    fn oversized_readable_copy_fails_before_building_its_output() {
        let source = SourceStore::static_text("one\n\ntwo").unwrap().snapshot();
        let document = Arc::new(parse_document(&source, &ParseLimits::default(), None));
        let expected = document.readable_text();
        assert_eq!(
            serialize_copy(
                CopyInput::Reading {
                    document: document.clone(),
                    selection: None
                },
                expected.len()
            )
            .unwrap(),
            expected
        );
        assert!(
            serialize_copy(
                CopyInput::Reading {
                    document,
                    selection: None
                },
                2
            )
            .is_err()
        );
        assert!(
            serialize_copy(
                CopyInput::Source {
                    text: "é".into(),
                    range: 1..2
                },
                2
            )
            .is_err()
        );
    }

    #[test]
    fn source_bypasses_semantic_limits_and_exact_snapshot_survives() {
        let (executor, driver) = manual(1, 1);
        let handle = executor.register(None).unwrap();
        let mut work = request(1);
        let expected = work.source.text();
        work.limits.max_nodes = 0;
        handle.submit(work).unwrap();
        driver.run_until_idle();
        let result = handle.take_completed().unwrap();
        assert_eq!(result.tags.view, ViewType::Source);
        assert!(matches!(result.result, Ok(PreparedView::Source(_))));
        assert_eq!(&*expected, "# A\r\n\té\n");
    }

    #[test]
    fn real_worker_completes_and_shutdown_joins_without_polling_sleeps() {
        let executor = MarkdownExecutor::new(ExecutorOptions {
            worker_count: 1,
            ..ExecutorOptions::default()
        });
        let handle = executor.register(None).unwrap();
        handle.submit(request(1)).unwrap();
        let shared = &executor.service.shared;
        let state = shared.lock();
        let (state, timeout) = shared
            .ready
            .wait_timeout_while(state, Duration::from_secs(5), |state| {
                state.lanes[&handle.id].completed.is_none()
            })
            .unwrap();
        assert!(!timeout.timed_out(), "worker did not complete");
        drop(state);
        assert!(handle.take_completed().unwrap().result.is_ok());
        executor.join();
        assert_eq!(handle.submit(request(2)), Err(SubmitError::Shutdown));
        assert!(executor.stats().stopped);
    }

    #[test]
    fn panic_is_a_failed_result() {
        let result: Result<(), WorkError> = guarded(|| panic!("injected parser panic"));
        assert_eq!(
            result,
            Err(WorkError::WorkerPanicked("injected parser panic".into()))
        );
    }
}
