//! Adapter-independent runtime turns and frame publication.

use std::{
    error::Error as StdError,
    fmt,
    future::Future,
    mem,
    ops::{Deref, DerefMut},
    result::Result as StdResult,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

use futures::{
    StreamExt,
    channel::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    future::{pending, poll_fn},
    pin_mut,
};
use tokio::{task::yield_now, time::sleep};

use super::{AdapterEvent, Canopy, scripting::Startup};
use crate::{
    commands::ArgValue,
    core::wake::PollOwner,
    error::{Error, Result},
    input::Event,
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
///
/// The public receiver is intentionally the futures oneshot type: evaluation
/// completion is a single-consumer event, and wrapping it would duplicate the
/// same polling and cancellation contract. Dropping the completion receiver
/// cancels queued or active evaluation work on its next driver turn.
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
}

/// One top-level script request. An evaluation's origin is always the root.
#[derive(Clone, Debug)]
pub struct EvalRequest {
    /// Owned Luau source.
    pub source: String,
    /// Absolute execution budget, including parked time.
    pub timeout: Option<Duration>,
}

impl EvalRequest {
    /// Request an evaluation of `source` with no timeout.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            timeout: None,
        }
    }

    /// Bound the evaluation, including parked time, by `timeout`.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}
/// One runtime input.
pub enum TurnInput {
    /// Deliver input events that arrived together, in order.
    ///
    /// The turn dispatches each event and prepares one frame for the batch, so
    /// a burst of input costs one render. Layout settles before a mouse event
    /// that follows another event, so hit testing sees the geometry the earlier
    /// events left. Dispatch stops at the first error or exit request.
    Events(Vec<Event>),
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
pub struct EvalOutcome {
    /// Evaluation that completed.
    pub id: EvalId,
    /// Completion value or failure.
    pub result: Result<ArgValue>,
    /// Output isolated to this evaluation.
    pub logs: Vec<String>,
    /// Assertions isolated to this evaluation.
    pub assertions: Vec<script::ScriptAssertion>,
    /// Typecheck diagnostics for the source. A source that failed to
    /// typecheck never ran, and its result is the parse error.
    pub diagnostics: Vec<script::ScriptCheckDiagnostic>,
}

impl EvalOutcome {
    /// Return the evaluation result.
    pub fn into_result(self) -> Result<ArgValue> {
        self.result
    }
}
/// Observable effects of one turn.
#[derive(Default)]
pub struct TurnOutcome {
    /// Frame published by this turn, if any.
    pub frame: Option<FrameId>,
    /// Whether moving cells changed without a new frame, so the adapter
    /// emits again.
    pub motion: bool,
    /// Evaluation accepted by this turn.
    pub started: Option<EvalId>,
    /// Evaluations completed after publication without an automation ticket.
    pub completed: Vec<EvalOutcome>,
    /// Requested application exit status.
    pub exit_code: Option<i32>,
}

/// A turn that failed, with the exit that the turn requested before it
/// failed.
///
/// A turn takes the exit request on every path, so no request survives into
/// the next turn. `?` converts it to [`Error`], keeping the inner error and
/// dropping the code; a host that exits on request reads `exit_code` first.
#[derive(Debug)]
pub struct TurnError {
    /// The failure.
    pub error: Error,
    /// Exit status requested in the turn, if any.
    pub exit_code: Option<i32>,
}

impl fmt::Display for TurnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl StdError for TurnError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.error)
    }
}

impl From<TurnError> for Error {
    fn from(failure: TurnError) -> Self {
        failure.error
    }
}

/// Shared state for publication subscriptions.
#[derive(Default)]
struct PublicationState {
    /// Last published generation.
    generation: u64,
    /// Count of observable changes: publications, and turns that dispatched
    /// input, which changes the input count even when no frame changes.
    changes: u64,
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
    /// Read the count of observable changes.
    pub(crate) fn changes(&self) -> u64 {
        self.0.lock().expect("publication lock poisoned").changes
    }
    /// Register and recheck atomically before parking, until the count of
    /// observable changes moves from `since` or the deadline passes.
    pub(crate) fn poll_changed(
        &self,
        since: u64,
        deadline: Option<Instant>,
        cx: &Context<'_>,
    ) -> Poll<u64> {
        let mut state = self.0.lock().expect("publication lock poisoned");
        if state.changes != since || deadline.is_some_and(|d| state.now.is_some_and(|now| now >= d))
        {
            return Poll::Ready(state.changes);
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
        self.change(true);
    }
    /// Wake subscribers after a turn took input, which changes what they
    /// observe even when no frame follows.
    fn took_input(&self) {
        self.change(false);
    }
    /// Count one observable change, and a new frame when `frame` is set, then
    /// wake every subscriber.
    fn change(&self, frame: bool) {
        let waiters = {
            let mut state = self.0.lock().expect("publication lock poisoned");
            if frame {
                state.generation += 1;
            }
            state.changes += 1;
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
    /// Count the pending subscribers.
    #[cfg(test)]
    pub(super) fn waiter_count(&self) -> usize {
        self.0
            .lock()
            .expect("publication lock poisoned")
            .waiters
            .len()
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
    tx: UnboundedSender<AdapterEvent>,
}
impl Wake for VmWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        if !self.ready.swap(true, Ordering::AcqRel) {
            let _closed = self.tx.unbounded_send(AdapterEvent::Wake);
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
    /// Completion receiver of an automation ticket. An evaluation that a turn
    /// started reports in that turn's outcome instead.
    reply: Option<oneshot::Sender<EvalOutcome>>,
}

/// The driver's one evaluation, from admission until its outcome is settled.
#[derive(Default)]
enum EvalSlot {
    /// No evaluation is admitted.
    #[default]
    Idle,
    /// An admitted evaluation waits for its next VM step.
    Parked(ActiveEval),
    /// An evaluation has ended, and settles at the end of this turn.
    Finishing(ActiveEval, Result<ArgValue>),
}

impl EvalSlot {
    /// Return whether no evaluation holds the slot.
    fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    /// Return the parked evaluation.
    fn parked(&self) -> Option<&ActiveEval> {
        match self {
            Self::Parked(active) => Some(active),
            Self::Idle | Self::Finishing(..) => None,
        }
    }

    /// Take the parked evaluation, leaving the slot idle.
    fn take_parked(&mut self) -> Option<ActiveEval> {
        match mem::take(self) {
            Self::Parked(active) => Some(active),
            other => {
                *self = other;
                None
            }
        }
    }

    /// Take the evaluation that has ended, with its result.
    fn take_finishing(&mut self) -> Option<(ActiveEval, Result<ArgValue>)> {
        match mem::take(self) {
            Self::Finishing(active, result) => Some((active, result)),
            other => {
                *self = other;
                None
            }
        }
    }

    /// Take the evaluation in any state, leaving the slot idle.
    fn take(&mut self) -> Option<ActiveEval> {
        match mem::take(self) {
            Self::Idle => None,
            Self::Parked(active) | Self::Finishing(active, _) => Some(active),
        }
    }
}

/// Runtime scheduling state, held independently of adapters.
pub(super) struct Driver {
    /// The admitted evaluation, until its outcome is settled.
    eval: EvalSlot,
    /// VM notification shared with host futures.
    wake: Arc<VmWake>,
    /// Snapshot publication subscribers.
    pub(super) publication: PublicationWatch,
    /// Reject nested synchronous driver entry.
    pub(super) in_turn: bool,
    /// Whether automatic startup has run.
    pub(super) startup: Startup,
}
impl Driver {
    /// Construct an idle driver.
    pub(super) fn new(tx: UnboundedSender<AdapterEvent>) -> Self {
        Self {
            eval: EvalSlot::Idle,
            wake: Arc::new(VmWake {
                ready: AtomicBool::new(false),
                tx,
            }),
            publication: PublicationWatch::default(),
            in_turn: false,
            startup: Startup::Pending,
        }
    }
}
impl Canopy {
    /// Admit a top-level evaluation, or report the runtime busy.
    ///
    /// The runtime is busy while the driver holds an evaluation, while the
    /// script host holds one that it started itself, and while a script runs
    /// in a live VM scope.
    pub(crate) fn eval_admission(&self) -> Result<()> {
        if !self.driver.eval.is_idle()
            || self.script.host.is_eval_active()
            || script::in_live_scope(self)
        {
            return Err(busy());
        }
        Ok(())
    }

    /// Admit an evaluation before changing retained runtime state.
    ///
    /// A busy runtime is an error. Any other failure to start completes the
    /// evaluation at once, and the returned outcome reports it with the
    /// source's diagnostics.
    fn start_eval(&mut self, id: EvalId, request: EvalRequest) -> Result<Option<EvalOutcome>> {
        self.eval_admission()?;
        let started = self.now();
        let origin = self.core.root_id();
        let mut diagnostics = Vec::new();
        let prepared = (|| {
            self.prepare_frame(false)?;
            let deadline = request
                .timeout
                .map(|d| {
                    self.now()
                        .checked_add(d)
                        .ok_or_else(|| Error::Invalid("evaluation deadline overflow".into()))
                })
                .transpose()?;
            let script =
                self.script
                    .host
                    .compile_eval(&request.source)
                    .map_err(|(error, found)| {
                        diagnostics = found;
                        error
                    })?;
            let incarnation = self.core.nodes[origin].incarnation;
            let mut invocation = self
                .script
                .host
                .start_invocation(origin, incarnation, script)?;
            invocation.set_reporting_timeout(request.timeout);
            Ok((deadline, invocation))
        })();
        let (deadline, invocation) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let result: Result<()> = Err(error);
                self.record_eval_journal(&request.source, started, &result, &[], &[]);
                return Ok(Some(EvalOutcome {
                    id,
                    result: result.map(|()| ArgValue::Null),
                    logs: Vec::new(),
                    assertions: Vec::new(),
                    diagnostics,
                }));
            }
        };
        self.driver.eval = EvalSlot::Parked(ActiveEval {
            id,
            request,
            deadline,
            gas: script::SCRIPT_GAS_LIMIT,
            invocation,
            started,
            reply: None,
        });
        self.driver.wake.ready.store(true, Ordering::Release);
        Ok(None)
    }
    /// Service one typed automation request without starting a nested turn.
    pub(super) fn service_message(&mut self, message: AutomationMessage) {
        match message {
            AutomationMessage::Callback(callback) => callback(self),
            AutomationMessage::Eval(_, _, sender) if sender.is_canceled() => {}
            AutomationMessage::Eval(id, request, sender) => match self.start_eval(id, request) {
                Ok(None) => {
                    if let EvalSlot::Parked(active) = &mut self.driver.eval {
                        active.reply = Some(sender);
                    }
                }
                Ok(Some(outcome)) => {
                    let _closed = sender.send(outcome);
                }
                Err(error) => {
                    let _closed = sender.send(EvalOutcome {
                        id,
                        result: Err(error),
                        logs: Vec::new(),
                        assertions: Vec::new(),
                        diagnostics: Vec::new(),
                    });
                }
            },
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
        if self.driver.eval.parked().is_some() && self.driver.wake.ready.load(Ordering::Acquire) {
            return Poll::Ready(Ok(()));
        }
        self.core.wake_registry.poll_notified(cx)
    }
    /// Return the next time at which the adapter must deliver a wake.
    pub(crate) fn next_deadline(&mut self) -> Option<Instant> {
        [
            self.poller.next_deadline(),
            self.driver.eval.parked().and_then(|a| a.deadline),
            self.driver.publication.next_deadline(),
            self.motion_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }
    /// Dispatch a batch of input events in order.
    ///
    /// Layout settles before each mouse event that follows another event, so
    /// hit testing sees current geometry. Dispatch stops after an exit request.
    fn dispatch_batch(&mut self, events: &[Event]) -> Result<()> {
        let now = self.now();
        for event in events {
            match event {
                Event::Key(_) | Event::Mouse(_) | Event::Paste(_) => self.motion.input(now),
                Event::FocusGained => self.motion.focus(true, now),
                Event::FocusLost => self.motion.focus(false, now),
                Event::Resize(_) => {}
            }
        }
        for (index, event) in events.iter().enumerate() {
            if index > 0 {
                if self.core.exit_requested.is_some() {
                    break;
                }
                // Work an earlier input woke, such as closing an inspector
                // that escape asked to close, lands before the next input,
                // as it would had the inputs arrived apart.
                self.poll_woken()?;
                if matches!(event, Event::Mouse(_)) {
                    self.settle_layout()?;
                }
            }
            self.count_input();
            self.event(event)?;
        }
        Ok(())
    }

    /// Poll each live node whose wake is pending, once.
    fn poll_woken(&mut self) -> Result<()> {
        let woken = self.core.wake_registry.drain()?;
        self.poll_owners(woken)
    }

    /// Poll each live owner in `owners` once.
    fn poll_owners(&mut self, mut owners: Vec<PollOwner>) -> Result<()> {
        owners.sort_unstable();
        owners.dedup_by_key(|stamp| (stamp.node, stamp.incarnation));
        for stamp in owners {
            if self.core.work_stamp_valid(stamp) {
                self.poll_node(stamp.node)?;
            }
        }
        Ok(())
    }

    /// Advance one bounded runtime turn.
    ///
    /// The turn takes the application's exit request whether it succeeds or
    /// fails: a success reports it in [`TurnOutcome::exit_code`], and a
    /// failure in [`TurnError::exit_code`]. A turn refused because another
    /// runs takes nothing.
    pub fn turn(&mut self, work: TurnInput) -> StdResult<TurnOutcome, TurnError> {
        if self.driver.in_turn {
            return Err(TurnError {
                error: busy(),
                exit_code: None,
            });
        }
        let mut scope = TurnScope::enter(self);
        let result = scope.turn_inner(work);
        let exit_code = scope.core.exit_requested.take();
        match result {
            Ok(mut outcome) => {
                outcome.exit_code = exit_code;
                Ok(outcome)
            }
            Err(error) => Err(TurnError { error, exit_code }),
        }
    }
    /// Dispatch, poll and publish one turn after admission.
    ///
    /// An evaluation that ends in this turn settles at its end on every path:
    /// a runtime failure replaces its result, and the turn returns that
    /// failure after the evaluation is journaled and its ticket answered.
    fn turn_inner(&mut self, work: TurnInput) -> Result<TurnOutcome> {
        let mut outcome = TurnOutcome::default();
        let before = self.driver.publication.generation();
        let mut dispatch_error = None;
        self.driver.publication.advance(self.now());
        let mut failure = None;
        match work {
            TurnInput::Events(events) => {
                dispatch_error = self.dispatch_batch(&events).err();
                self.driver.publication.took_input();
            }
            TurnInput::StartEval(request) => {
                let id = EvalId::next();
                if let Some(failed) = self.start_eval(id, request)? {
                    outcome.completed.push(failed);
                }
                outcome.started = Some(id);
            }
            TurnInput::CancelEval(id) => {
                if self.driver.eval.parked().is_some_and(|a| a.id == id)
                    && let Some(active) = self.driver.eval.take_parked()
                {
                    failure = self.end_eval(active, Err(Error::ScriptCancelled), true);
                }
            }
            TurnInput::Wake | TurnInput::Prepare => {}
        }
        if failure.is_none() {
            failure = self.service_turn().err();
        }
        let prepared = match failure {
            Some(_) => Ok(()),
            None => self.prepare_frame(false).map(|_| ()),
        };
        self.settle_eval(failure, prepared, &mut outcome)?;
        if self.driver.publication.generation() != before {
            outcome.frame = Some(FrameId(self.driver.publication.generation()));
        } else {
            outcome.motion = self.motion_due();
        }
        if let Some(error) = dispatch_error {
            return Err(error);
        }
        Ok(outcome)
    }

    /// Service automation, poll due and woken owners, and step the parked
    /// evaluation.
    fn service_turn(&mut self) -> Result<()> {
        self.service_automation();
        self.poller
            .retain(|stamp| self.core.work_stamp_valid(stamp));
        let mut due = self.poller.collect_due();
        due.extend(self.core.wake_registry.drain()?);
        self.poll_owners(due)?;
        self.step_eval().map_or(Ok(()), Err)
    }

    /// Run one VM step of the parked evaluation, or end it on cancellation or
    /// timeout. Return a runtime failure, which the evaluation then reports.
    fn step_eval(&mut self) -> Option<Error> {
        let mut active = self.driver.eval.take_parked()?;
        let now = self.now();
        // Clear before registering cancellation: a concurrent receiver
        // drop must leave a fresh wake for the next turn.
        let vm_ready = self.driver.wake.ready.swap(false, Ordering::AcqRel);
        let waker = Waker::from(Arc::clone(&self.driver.wake));
        let mut cx = Context::from_waker(&waker);
        let cancelled = active
            .reply
            .as_mut()
            .is_some_and(|sender| sender.poll_canceled(&mut cx).is_ready());
        if cancelled {
            return self.end_eval(active, Err(Error::ScriptCancelled), true);
        }
        if active.deadline.is_some_and(|d| now >= d) {
            let error = script::script_timeout(active.request.timeout);
            return self.end_eval(active, Err(error), true);
        }
        if !vm_ready {
            self.driver.eval = EvalSlot::Parked(active);
            return None;
        }
        let host = self.script.host.clone();
        let step = host.poll_invocation(
            self,
            &mut active.invocation,
            &mut cx,
            active.gas,
            active.deadline.map(|d| d.saturating_duration_since(now)),
        );
        active.gas = active.gas.saturating_sub(step.gas_spent);
        match step.poll {
            Poll::Ready(result) => self.end_eval(active, result, false),
            Poll::Pending => {
                self.driver.eval = EvalSlot::Parked(active);
                None
            }
        }
    }

    /// Move an evaluation to `Finishing` with `result`, aborting its VM work
    /// first when `abort` is set. Return the abort's failure.
    fn end_eval(
        &mut self,
        mut active: ActiveEval,
        result: Result<ArgValue>,
        abort: bool,
    ) -> Option<Error> {
        let failure = if abort {
            self.script
                .host
                .abort_invocation(&mut active.invocation)
                .err()
        } else {
            None
        };
        self.driver.eval = EvalSlot::Finishing(active, result);
        failure
    }

    /// Settle an evaluation that ended in this turn, and return the turn's
    /// result.
    ///
    /// A runtime `failure` replaces the evaluation's own result, and the turn
    /// still returns it. Otherwise a frame failure in `prepared` joins the
    /// evaluation's result, and the turn succeeds. With no evaluation to
    /// settle, the turn returns the failure, or the frame result.
    fn settle_eval(
        &mut self,
        failure: Option<Error>,
        prepared: Result<()>,
        outcome: &mut TurnOutcome,
    ) -> Result<()> {
        let Some((mut active, result)) = self.driver.eval.take_finishing() else {
            return failure.map_or(prepared, Err);
        };
        // The error is not `Clone`: the evaluation reports its text, and the
        // turn returns the error itself.
        let (result, turn) = match failure {
            Some(error) => (Err(Error::Driver(error.to_string())), Err(error)),
            None => (prepared.and(result), Ok(())),
        };
        let (logs, assertions) = self.finish_eval(&mut active, &result);
        let completion = EvalOutcome {
            id: active.id,
            result,
            logs,
            assertions,
            diagnostics: Vec::new(),
        };
        match active.reply.take() {
            Some(sender) => {
                let _closed = sender.send(completion);
            }
            None => outcome.completed.push(completion),
        }
        turn
    }
}
/// A turn in progress, which admits no nested turn until it ends, by return
/// or by unwind.
struct TurnScope<'a>(&'a mut Canopy);

impl<'a> TurnScope<'a> {
    /// Mark a turn as running on `canopy`.
    fn enter(canopy: &'a mut Canopy) -> Self {
        canopy.driver.in_turn = true;
        Self(canopy)
    }
}

impl Deref for TurnScope<'_> {
    type Target = Canopy;

    fn deref(&self) -> &Canopy {
        self.0
    }
}

impl DerefMut for TurnScope<'_> {
    fn deref_mut(&mut self) -> &mut Canopy {
        self.0
    }
}

impl Drop for TurnScope<'_> {
    fn drop(&mut self) {
        self.0.driver.in_turn = false;
    }
}

/// Structured admission error shared by driver entry points.
fn busy() -> Error {
    Error::ScriptBusy("another runtime turn or evaluation is active".into())
}

impl super::AutomationHandle {
    /// Submit evaluation work without blocking the UI thread while it runs.
    pub fn submit_eval(&self, request: EvalRequest) -> Result<EvalTicket> {
        let id = EvalId::next();
        let (sender, completion) = oneshot::channel();
        self.submit_message(AutomationMessage::Eval(id, request, sender))?;
        Ok(EvalTicket { id, completion })
    }
}

impl Canopy {
    /// Drive the shared runtime until one synchronous headless evaluation
    /// completes.
    ///
    /// Current-thread Tokio tasks and `LocalSet` callers must move the
    /// complete operation, including application construction, to a blocking
    /// worker. The application stays on its owning thread.
    pub fn eval(&mut self, request: EvalRequest) -> Result<EvalOutcome> {
        script::block_on(self.eval_async(request))
    }

    /// Drive a synchronous headless evaluation until completion or
    /// cancellation.
    ///
    /// When `cancelled` resolves, abort the evaluation and return
    /// [`Error::ScriptCancelled`]. The application remains reusable. Native
    /// callbacks must return before cancellation can take effect. The future
    /// is polled on the application's owning thread in a time-enabled Tokio
    /// runtime. The same caller restrictions as [`Self::eval`] apply.
    pub fn eval_with_cancellation(
        &mut self,
        request: EvalRequest,
        cancelled: impl Future<Output = ()>,
    ) -> Result<EvalOutcome> {
        script::block_on(async {
            tokio::select! {
                biased;
                () = cancelled => Err(Error::ScriptCancelled),
                result = self.eval_async(request) => result,
            }
        })
    }

    /// Drive bounded turns inside the owned blocking runtime.
    async fn eval_async(&mut self, request: EvalRequest) -> Result<EvalOutcome> {
        if self.driver.in_turn {
            return Err(busy());
        }
        self.eval_admission()?;
        let events = self.event_rx.take().ok_or_else(busy)?;
        let mut guard = HeadlessEval {
            canopy: self,
            events: Some(events),
        };
        let result = guard.run(request).await;
        if result.is_err() {
            guard.canopy.abort_headless_eval(&result);
        }
        result
    }

    /// Release pending work and preserve its diagnostics on driver failure.
    fn abort_headless_eval<T>(&mut self, result: &Result<T>) {
        if let Some(mut active) = self.driver.eval.take() {
            if let Err(error) = self.script.host.abort_invocation(&mut active.invocation) {
                tracing::error!(%error, "aborting failed headless evaluation");
            }
            self.finish_eval(&mut active, result);
        }
    }

    /// Close an evaluation that has left the driver.
    ///
    /// Release its publication subscriptions and VM wake, make its logs and
    /// assertions the host's most recent diagnostics, and journal it. Return
    /// the logs and assertions for its outcome.
    fn finish_eval<T>(
        &mut self,
        active: &mut ActiveEval,
        result: &Result<T>,
    ) -> (Vec<String>, Vec<script::ScriptAssertion>) {
        self.driver.publication.clear_waiters();
        self.driver.wake.ready.store(false, Ordering::Release);
        let (logs, assertions) = active.invocation.take_diagnostics();
        self.record_eval_journal(
            &active.request.source,
            active.started,
            result,
            &logs,
            &assertions,
        );
        (logs, assertions)
    }
}

/// Restore driver resources even when an async caller drops its evaluation.
struct HeadlessEval<'a> {
    /// Application borrowed for the complete headless operation.
    canopy: &'a mut Canopy,
    /// Event receiver restored to the application on every exit path.
    events: Option<UnboundedReceiver<AdapterEvent>>,
}

impl HeadlessEval<'_> {
    /// Poll bounded turns, returning control to Tokio between ready turns.
    async fn run(&mut self, request: EvalRequest) -> Result<EvalOutcome> {
        let canopy = &mut *self.canopy;
        let events = self.events.as_mut().expect("headless driver owns events");
        let mut outcome = canopy.turn(TurnInput::StartEval(request))?;
        let id = outcome.started.expect("start turn accepts evaluation");
        let mut selector = TurnSelector::default();
        loop {
            if let Some(index) = outcome.completed.iter().position(|done| done.id == id) {
                let completion = outcome.completed.swap_remove(index);
                return Ok(completion);
            }
            // VM and adapter wakes can stay ready indefinitely. Yield
            // explicitly so Tokio replenishes its cooperative budget.
            yield_now().await;
            let work = selector.next_from(canopy, events).await?;
            outcome = canopy.turn(work)?;
        }
    }
}

/// Chooses the next turn input from adapter input, a runtime notification,
/// or the next driver deadline.
///
/// Sources that are ready together are taken in rotating order, so a source
/// that stays ready cannot starve the others. The terminal adapter and headless
/// evaluation share this selector.
#[derive(Default)]
pub struct TurnSelector {
    /// Source polled first on the next wait.
    next_source: usize,
}

impl TurnSelector {
    /// Wait for the next turn input.
    ///
    /// The wait holds only a shared application borrow, which ends before the
    /// caller runs the turn, so no widget or VM borrow crosses suspension.
    pub async fn next<E>(&mut self, canopy: &mut Canopy, event: E) -> Result<TurnInput>
    where
        E: Future<Output = Result<TurnInput>>,
    {
        let deadline = canopy.next_deadline();
        let canopy = &*canopy;
        let timer = async move {
            match deadline {
                Some(deadline) => sleep(deadline.saturating_duration_since(canopy.now())).await,
                None => pending::<()>().await,
            }
        };
        self.select(event, poll_fn(|cx| canopy.poll_runtime_wake(cx)), timer)
            .await
    }

    /// Wait for the next turn input, taking adapter events from `events`.
    pub(crate) async fn next_from(
        &mut self,
        canopy: &mut Canopy,
        events: &mut UnboundedReceiver<AdapterEvent>,
    ) -> Result<TurnInput> {
        let event = async {
            match events.next().await {
                Some(AdapterEvent::Input(event)) => Ok(TurnInput::Events(vec![event])),
                Some(AdapterEvent::Wake) => Ok(TurnInput::Wake),
                None => Err(Error::Driver("adapter event channel closed".into())),
            }
        };
        self.next(canopy, event).await
    }

    /// Return the first ready source, starting from the rotating priority.
    async fn select<E, W, D>(&mut self, event: E, wake: W, deadline: D) -> Result<TurnInput>
    where
        E: Future<Output = Result<TurnInput>>,
        W: Future<Output = Result<()>>,
        D: Future<Output = ()>,
    {
        pin_mut!(event, wake, deadline);
        poll_fn(|cx| {
            for offset in 0..3 {
                let source = (self.next_source + offset) % 3;
                let ready = match source {
                    0 => event.as_mut().poll(cx),
                    1 => wake
                        .as_mut()
                        .poll(cx)
                        .map(|wake| wake.map(|()| TurnInput::Wake)),
                    _ => deadline.as_mut().poll(cx).map(|()| Ok(TurnInput::Wake)),
                };
                if ready.is_ready() {
                    self.next_source = (source + 1) % 3;
                    return ready;
                }
            }
            Poll::Pending
        })
        .await
    }
}

impl Drop for HeadlessEval<'_> {
    fn drop(&mut self) {
        self.canopy
            .abort_headless_eval(&Err::<(), _>(Error::ScriptCancelled));
        self.canopy.event_rx = self.events.take();
    }
}

impl Drop for Canopy {
    fn drop(&mut self) {
        if let Err(error) = self.core.wake_registry.close() {
            tracing::error!(%error, "closing wake registry");
        }
        self.driver.publication.clear_waiters();
        if let Some(mut active) = self.driver.eval.take()
            && let Some(sender) = active.reply.take()
        {
            let _closed = sender.send(EvalOutcome {
                id: active.id,
                result: Err(Error::Driver(
                    "application stopped before evaluation completed".into(),
                )),
                logs: Vec::new(),
                assertions: Vec::new(),
                diagnostics: Vec::new(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::ready;

    use futures::executor::block_on;

    use super::*;

    #[test]
    fn selector_rotates_between_ready_input_wake_and_deadline() -> Result<()> {
        let mut selector = TurnSelector::default();
        for expected_source in 0..3 {
            let work = block_on(selector.select(
                ready(Ok(TurnInput::Events(vec![Event::FocusGained]))),
                ready(Ok(())),
                ready(()),
            ))?;
            assert_eq!(selector.next_source, (expected_source + 1) % 3);
            if expected_source == 0 {
                assert!(
                    matches!(&work, TurnInput::Events(events) if matches!(events.as_slice(), [Event::FocusGained]))
                );
            } else {
                assert!(matches!(work, TurnInput::Wake));
            }
        }
        Ok(())
    }

    #[test]
    fn selector_services_deadlines_without_input() -> Result<()> {
        let work = block_on(TurnSelector::default().select(
            pending::<Result<TurnInput>>(),
            pending::<Result<()>>(),
            ready(()),
        ))?;
        assert!(matches!(work, TurnInput::Wake));
        Ok(())
    }
}
