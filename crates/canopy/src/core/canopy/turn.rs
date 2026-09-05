//! Adapter-independent runtime turns and frame publication.

use std::{
    collections::HashMap,
    mem,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    task::{Context, Poll, Wake, Waker},
    thread,
    time::{Duration, Instant},
};

use futures::{
    channel::{mpsc::UnboundedSender, oneshot},
    executor,
    future::{pending, poll_fn},
};
use tokio::{
    runtime::{Builder as RuntimeBuilder, Handle},
    time::sleep,
};

use super::Canopy;
use crate::{
    NodeId,
    commands::ArgValue,
    error::{Error, Result, ScriptErrorKind},
    event::Event,
    script,
};

/// Identifier of an immutable publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameId(pub u64);
/// Globally unique evaluation identifier, never reused by another application.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EvalId(pub u64);
impl EvalId {
    /// Allocate an identity without borrowing the UI thread.
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}
/// Completion receiver that is awaited outside the UI thread.
pub struct EvalTicket {
    /// Accepted queue identity, including failed admission results.
    pub id: EvalId,
    /// Evaluation completion after publication.
    pub completion: oneshot::Receiver<EvalOutcome>,
}
/// Typed automation work delivered to the driver.
pub(super) enum AutomationMessage {
    /// Native callback, bounded by the driver's service budget.
    Callback(super::AutomationCallback),
    /// Evaluation submission, retaining no application borrow.
    Eval(EvalId, EvalRequest, oneshot::Sender<EvalOutcome>),
    /// Cancellation request with a synchronous acknowledgement.
    Cancel(EvalId, mpsc::Sender<Result<crate::ChangeOutcome>>),
}

/// One top-level script request.
#[derive(Clone, Debug)]
pub struct EvalRequest {
    /// Owned Luau source.
    pub source: String,
    /// Absolute execution budget, including parked time.
    pub timeout: Option<Duration>,
    /// Anchor retained across invocation segments.
    pub anchor: NodeId,
}
/// One runtime input.
pub enum Work {
    /// Deliver an input event.
    Input(Event),
    /// Service ready background work.
    Wake,
    /// Start a top-level evaluation.
    StartEval(EvalRequest),
    /// Cancel an evaluation owned by this runtime.
    CancelEval(EvalId),
    /// Prepare changes made through native access.
    Prepare,
}
/// Completed script value or failure.
#[derive(Clone)]
pub struct EvalOutcome {
    /// Evaluation that completed.
    pub id: EvalId,
    /// Shared completion, also delivered to an automation ticket.
    pub result: Arc<Result<ArgValue>>,
    /// Output isolated to this evaluation.
    pub logs: Vec<String>,
    /// Assertions isolated to this evaluation.
    pub assertions: Vec<script::ScriptAssertion>,
}
/// Observable effects of one turn.
#[derive(Default)]
pub struct TurnOutcome {
    /// Frame published by this turn, if any.
    pub frame: Option<FrameId>,
    /// Evaluation accepted by this turn.
    pub started: Option<EvalId>,
    /// Evaluations completed after publication.
    pub completed: Vec<EvalOutcome>,
    /// Requested application exit status.
    pub exit_code: Option<i32>,
}

/// Shared state for publication subscriptions.
#[derive(Default)]
struct PublicationState {
    /// Last published generation.
    generation: u64,
    /// Driver time, including deterministic test advances.
    now: Option<Instant>,
    /// Pending subscriber wakers and deadlines.
    waiters: Vec<(Waker, Option<Instant>)>,
}
/// A borrow-free subscription to frame changes and driver deadlines.
#[derive(Clone, Default)]
pub struct PublicationWatch(Arc<Mutex<PublicationState>>);
impl PublicationWatch {
    /// Read the current generation.
    pub(crate) fn generation(&self) -> u64 {
        self.0.lock().expect("publication lock poisoned").generation
    }
    /// Register and recheck atomically before parking.
    pub(crate) fn poll_changed(
        &self,
        since: u64,
        deadline: Option<Instant>,
        cx: &Context<'_>,
    ) -> Poll<u64> {
        let mut state = self.0.lock().expect("publication lock poisoned");
        if state.generation != since
            || deadline.is_some_and(|d| state.now.is_some_and(|now| now >= d))
        {
            return Poll::Ready(state.generation);
        }
        if let Some(waiter) = state
            .waiters
            .iter_mut()
            .find(|(w, _)| w.will_wake(cx.waker()))
        {
            waiter.1 = deadline;
        } else {
            state.waiters.push((cx.waker().clone(), deadline));
        }
        Poll::Pending
    }
    /// Publish after all frame construction succeeds.
    pub(super) fn publish(&self) {
        let waiters = {
            let mut state = self.0.lock().expect("publication lock poisoned");
            state.generation += 1;
            mem::take(&mut state.waiters)
        };
        for (waker, _) in waiters {
            waker.wake();
        }
    }
    /// Wake deadline subscribers using the driver's clock.
    fn advance(&self, now: Instant) {
        let mut state = self.0.lock().expect("publication lock poisoned");
        state.now = Some(now);
        let mut ready = Vec::new();
        state.waiters.retain(|(w, d)| {
            if d.is_some_and(|d| d <= now) {
                ready.push(w.clone());
                false
            } else {
                true
            }
        });
        drop(state);
        for w in ready {
            w.wake();
        }
    }
    /// Release all subscriptions when their only evaluation completes.
    fn clear_waiters(&self) {
        self.0
            .lock()
            .expect("publication lock poisoned")
            .waiters
            .clear();
    }
    /// Earliest subscribed timeout.
    fn next_deadline(&self) -> Option<Instant> {
        self.0
            .lock()
            .expect("publication lock poisoned")
            .waiters
            .iter()
            .filter_map(|(_, d)| *d)
            .min()
    }
}
/// Coalesced notification for a suspended VM.
struct VmWake {
    /// Whether a poll is due.
    ready: AtomicBool,
    /// Adapter notification channel.
    tx: UnboundedSender<Event>,
}
impl Wake for VmWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        if !self.ready.swap(true, Ordering::AcqRel) {
            let _closed = self.tx.unbounded_send(Event::Wake);
        }
    }
}
/// Owned state of an active evaluation.
struct ActiveEval {
    /// Stable evaluation identity.
    id: EvalId,
    /// Original request, retained for diagnostics.
    request: EvalRequest,
    /// Absolute expiration.
    deadline: Option<Instant>,
    /// Remaining total VM gas.
    gas: u64,
    /// Detached retained invocation.
    invocation: script::ScriptInvocation,
    /// Journal origin time.
    started: Instant,
}
/// Runtime scheduling state, held independently of adapters.
pub(super) struct Driver {
    /// Current evaluation, if parked.
    active: Option<ActiveEval>,
    /// VM notification shared with host futures.
    wake: Arc<VmWake>,
    /// Snapshot publication subscribers.
    pub(super) publication: PublicationWatch,
    /// Reject nested synchronous driver entry.
    pub(super) in_turn: bool,
    /// Whether automatic startup has already been attempted.
    pub(super) startup_attempted: bool,
    /// Live completion receivers awaiting a terminal evaluation outcome.
    tickets: HashMap<EvalId, oneshot::Sender<EvalOutcome>>,
    /// Cancellation requested by typed automation.
    cancel: Option<EvalId>,
}
impl Driver {
    /// Construct an idle driver.
    pub(super) fn new(tx: UnboundedSender<Event>) -> Self {
        Self {
            active: None,
            wake: Arc::new(VmWake {
                ready: AtomicBool::new(false),
                tx,
            }),
            publication: PublicationWatch::default(),
            in_turn: false,
            startup_attempted: false,
            tickets: Default::default(),
            cancel: None,
        }
    }
}
impl Canopy {
    /// Admit an evaluation before changing retained runtime state.
    fn start_eval(&mut self, id: EvalId, request: EvalRequest) -> Result<()> {
        if self.driver.active.is_some() || self.script_host.is_eval_active() {
            return Err(busy());
        }
        let baseline = self.begin_script_journal();
        self.script_host.set_diagnostics(Vec::new(), Vec::new());
        let prepared = (|| {
            self.ensure_finalized()?;
            self.prepare_frame(false)?;
            self.core.validate_attached_node(request.anchor)?;
            let deadline = request
                .timeout
                .map(|d| {
                    self.now().checked_add(d).ok_or_else(|| {
                        Error::InvalidOperation("evaluation deadline overflow".into())
                    })
                })
                .transpose()?;
            let script = self.script_host.compile(&request.source)?;
            let mut invocation = self.script_host.start_invocation(request.anchor, script)?;
            invocation.set_reporting_timeout(request.timeout);
            invocation.set_anchor_incarnation(self.core.nodes[request.anchor].incarnation);
            Ok((deadline, invocation))
        })();
        let (deadline, invocation) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let result: Result<()> = Err(error);
                self.record_script_journal("eval", &request.source, baseline, &result);
                return result;
            }
        };
        self.driver.active = Some(ActiveEval {
            id,
            request,
            deadline,
            gas: 500_000_000,
            invocation,
            started: self.now(),
        });
        self.driver.wake.ready.store(true, Ordering::Release);
        Ok(())
    }
    /// Service one typed automation request without starting a nested turn.
    pub(super) fn service_message(&mut self, message: AutomationMessage) {
        match message {
            AutomationMessage::Callback(callback) => callback(self),
            AutomationMessage::Eval(id, request, sender) => match self.start_eval(id, request) {
                Ok(()) => {
                    self.driver.tickets.insert(id, sender);
                }
                Err(error) => {
                    let _closed = sender.send(EvalOutcome {
                        id,
                        result: Arc::new(Err(error)),
                        logs: Vec::new(),
                        assertions: Vec::new(),
                    });
                }
            },
            AutomationMessage::Cancel(id, sender) => {
                let changed = self.driver.active.as_ref().is_some_and(|a| a.id == id);
                if changed {
                    self.driver.cancel = Some(id);
                }
                let _closed = sender.send(Ok(if changed {
                    crate::ChangeOutcome::Changed
                } else {
                    crate::ChangeOutcome::Unchanged
                }));
            }
        }
    }
    /// Return the current driver clock.
    pub(crate) fn now(&self) -> Instant {
        self.poller.now()
    }
    /// Subscribe without borrowing the application across suspension.
    pub(crate) fn publication_watch(&self) -> PublicationWatch {
        self.driver.publication.clone()
    }
    /// Register an adapter for coalesced node-worker notifications.
    pub(crate) fn poll_runtime_wake(&self, cx: &Context<'_>) -> Poll<Result<()>> {
        if self.driver.active.is_some() && self.driver.wake.ready.load(Ordering::Acquire) {
            return Poll::Ready(Ok(()));
        }
        self.core.wake_registry.poll_notified(cx)
    }
    /// Return the next time at which the adapter must deliver a wake.
    pub fn next_deadline(&mut self) -> Option<Instant> {
        [
            self.poller.next_deadline(),
            self.driver.active.as_ref().and_then(|a| a.deadline),
            self.driver.publication.next_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }
    /// Advance one bounded runtime turn.
    pub fn turn(&mut self, work: Work) -> Result<TurnOutcome> {
        if self.driver.in_turn {
            return Err(busy());
        }
        self.driver.in_turn = true;
        let result = self.turn_inner(work);
        self.driver.in_turn = false;
        result
    }
    /// Dispatch, poll and publish one turn after admission.
    fn turn_inner(&mut self, work: Work) -> Result<TurnOutcome> {
        let mut outcome = TurnOutcome::default();
        let mut completed = None;
        let before = self.driver.publication.generation();
        let mut dispatch_error = None;
        self.driver.publication.advance(self.now());
        match work {
            Work::Input(event) => {
                dispatch_error = self.event(event).err();
            }
            Work::StartEval(request) => {
                let id = EvalId::next();
                self.start_eval(id, request)?;
                outcome.started = Some(id);
            }
            Work::CancelEval(id) => {
                if self.driver.active.as_ref().is_some_and(|a| a.id == id) {
                    let mut active = self.driver.active.take().expect("active evaluation exists");
                    self.script_host.abort_invocation(&mut active.invocation)?;
                    completed = Some((active, Err(Error::ScriptCancelled)));
                }
            }
            Work::Wake | Work::Prepare => {}
        }
        self.service_automation();
        if let Some(id) = self.driver.cancel.take()
            && self.driver.active.as_ref().is_some_and(|a| a.id == id)
        {
            let mut active = self.driver.active.take().expect("active evaluation exists");
            self.script_host.abort_invocation(&mut active.invocation)?;
            completed = Some((active, Err(Error::ScriptCancelled)));
        }
        self.poller
            .retain(|stamp| self.core.work_stamp_valid(stamp));
        let mut due = self.poller.collect_due();
        due.extend(self.core.wake_registry.drain()?);
        due.sort_unstable();
        due.dedup_by_key(|stamp| (stamp.node, stamp.incarnation));
        for stamp in due {
            if self.core.work_stamp_valid(stamp) {
                self.poll_node(stamp.node)?;
            }
        }
        if let Some(mut active) = self.driver.active.take() {
            let now = self.now();
            if active.deadline.is_some_and(|d| now >= d) {
                self.script_host.abort_invocation(&mut active.invocation)?;
                let timeout_ms = active
                    .request
                    .timeout
                    .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
                completed = Some((active, Err(Error::ScriptTimeout { timeout_ms })));
            } else if self.driver.wake.ready.swap(false, Ordering::AcqRel) {
                let waker = Waker::from(Arc::clone(&self.driver.wake));
                let mut cx = Context::from_waker(&waker);
                let host = self.script_host.clone();
                let step = host.poll_invocation(
                    self,
                    &mut active.invocation,
                    &mut cx,
                    active.gas,
                    active.deadline.map(|d| d.saturating_duration_since(now)),
                );
                active.gas = active.gas.saturating_sub(step.gas_spent);
                match step.poll {
                    Poll::Ready(result) => completed = Some((active, result)),
                    Poll::Pending => self.driver.active = Some(active),
                }
            } else {
                self.driver.active = Some(active);
            }
        }
        let prepared = self.prepare_frame(false);
        let published = prepared.as_ref().is_ok_and(|p| *p);
        if let Some((mut active, result)) = completed {
            let result = match prepared {
                Ok(_) => result,
                Err(error) => Err(error),
            };
            self.driver.publication.clear_waiters();
            self.driver.wake.ready.store(false, Ordering::Release);
            let (logs, assertions) = active.invocation.take_diagnostics();
            self.script_host
                .set_diagnostics(logs.clone(), assertions.clone());
            let id = self.script_journal_next_id;
            self.script_journal_next_id += 1;
            self.script_journal.push(super::ScriptJournalEntry {
                id,
                origin: "eval".into(),
                source: active.request.source,
                ok: result.is_ok(),
                error: result.as_ref().err().map(ToString::to_string),
                logs: logs.clone(),
                assertions: assertions.clone(),
                duration_ms: u64::try_from(
                    self.now()
                        .saturating_duration_since(active.started)
                        .as_millis(),
                )
                .unwrap_or(u64::MAX),
            });
            self.enforce_script_journal_limit();
            let completion = EvalOutcome {
                id: active.id,
                result: Arc::new(result),
                logs,
                assertions,
            };
            if let Some(sender) = self.driver.tickets.remove(&active.id) {
                let _closed = sender.send(completion.clone());
            }
            outcome.completed.push(completion);
        } else {
            prepared?;
        }
        if published || self.driver.publication.generation() != before {
            outcome.frame = Some(FrameId(self.driver.publication.generation()));
        }
        if let Some(error) = dispatch_error {
            return Err(error);
        }
        outcome.exit_code = self.core.take_exit_request();
        Ok(outcome)
    }
}
/// Structured admission error shared by driver entry points.
fn busy() -> Error {
    Error::ScriptStructured {
        kind: ScriptErrorKind::ScriptBusy,
        command: None,
        owner: None,
        message: "another runtime turn or evaluation is active".into(),
    }
}

impl super::AutomationHandle {
    /// Submit evaluation work without blocking the UI thread while it runs.
    pub fn submit_eval(&self, request: EvalRequest) -> Result<EvalTicket> {
        let id = EvalId::next();
        let (sender, completion) = oneshot::channel();
        self.submit_message(AutomationMessage::Eval(id, request, sender))?;
        Ok(EvalTicket { id, completion })
    }
    /// Request cancellation and wait only for driver admission.
    pub fn cancel_eval(&self, id: EvalId) -> Result<crate::ChangeOutcome> {
        if thread::current().id() == self.ui_thread {
            return Err(busy());
        }
        let (sender, receiver) = mpsc::channel();
        self.submit_message(AutomationMessage::Cancel(id, sender))?;
        receiver.recv()?
    }
}

impl Canopy {
    /// Drive the shared runtime until one synchronous headless evaluation
    /// completes.
    pub(super) fn eval_headless(
        &mut self,
        source: &str,
        timeout: Option<Duration>,
    ) -> Result<ArgValue> {
        if self.driver.in_turn || self.driver.active.is_some() || script::in_live_scope(self) {
            return Err(busy());
        }
        let request = EvalRequest {
            source: source.to_owned(),
            timeout,
            anchor: self.root_id(),
        };
        let runtime = if Handle::try_current().is_ok() {
            None
        } else {
            Some(
                RuntimeBuilder::new_current_thread()
                    .enable_time()
                    .build()
                    .map_err(|error| Error::RunLoop(format!("headless runtime: {error}")))?,
            )
        };
        let mut events = self.event_rx.take().ok_or_else(busy)?;
        let future = async {
            use futures::{FutureExt, StreamExt};
            let mut outcome = self.turn(Work::StartEval(request))?;
            let id = outcome.started.expect("start turn accepts evaluation");
            loop {
                if let Some(index) = outcome.completed.iter().position(|done| done.id == id) {
                    let completion = outcome.completed.swap_remove(index);
                    return Arc::try_unwrap(completion.result).map_err(|_| {
                        Error::Internal("headless completion unexpectedly shared".into())
                    })?;
                }
                let work = {
                    let deadline = self.next_deadline();
                    let timer = async {
                        match deadline {
                            Some(deadline) => {
                                sleep(deadline.saturating_duration_since(self.now())).await
                            }
                            None => pending().await,
                        }
                    }
                    .fuse();
                    let notified = poll_fn(|cx| self.poll_runtime_wake(cx)).fuse();
                    let event = events.next().fuse();
                    futures::pin_mut!(timer, notified, event);
                    futures::select! {
                        result = notified => { result?; Work::Wake },
                        () = timer => Work::Wake,
                        event = event => Work::Input(event.ok_or_else(||Error::RunLoop("headless event channel closed".into()))?),
                    }
                };
                // All waiting futures release application references before dispatch.
                outcome = self.turn(work)?;
            }
        };
        let result = match runtime {
            Some(runtime) => runtime.block_on(future),
            None => executor::block_on(future),
        };
        self.event_rx = Some(events);
        if result.is_err()
            && let Some(mut active) = self.driver.active.take()
        {
            if let Err(error) = self.script_host.abort_invocation(&mut active.invocation) {
                tracing::error!(%error, "aborting failed headless evaluation");
            }
            let (logs, assertions) = active.invocation.take_diagnostics();
            self.script_host.set_diagnostics(logs, assertions);
            self.driver.publication.clear_waiters();
            self.driver.wake.ready.store(false, Ordering::Release);
            self.record_script_journal(
                "eval",
                &active.request.source,
                super::ScriptJournalBaseline {
                    started: active.started,
                    logs: 0,
                    assertions: 0,
                },
                &result,
            );
        }
        result
    }
}

impl Drop for Canopy {
    fn drop(&mut self) {
        if let Err(error) = self.core.wake_registry.close() {
            tracing::error!(%error, "closing wake registry");
        }
        self.driver.publication.clear_waiters();
        self.driver.active.take();
        for (id, sender) in self.driver.tickets.drain() {
            let _closed = sender.send(EvalOutcome {
                id,
                result: Arc::new(Err(Error::RunLoop(
                    "application stopped before evaluation completed".into(),
                ))),
                logs: Vec::new(),
                assertions: Vec::new(),
            });
        }
    }
}
