//! Owned detached script invocations, polled only at runtime turn boundaries.

use std::{
    fmt, mem,
    sync::{Arc, Mutex},
    task::{Context as TaskContext, Poll},
    time::Duration,
};

use ruau::{
    session::{InvocationError, InvocationHandle},
    vm::{CallOptions, Limits, ValueSnapshot},
};

use super::{
    ActiveEvalGuard, ArgValue, Canopy, LuauHost, NodeId, Result, ScriptAssertion, ScriptId, error,
    invocation_limits, invocation_options, marshaled_to_arg_value,
    retained_runtime_error_to_canopy,
};

/// Total gas available to one top-level evaluation across all VM segments.
pub const SCRIPT_GAS_LIMIT: u64 = 500_000_000;

/// Maximum VM work before the adapter can service cancellation and other input.
const SCRIPT_QUANTUM: u64 = 10_000;

/// A pending evaluation with no borrowed runtime, widget, or application state.
pub struct ScriptInvocation {
    /// Host responsible for releasing a pending invocation on drop.
    host: LuauHost,
    /// Present only while Ruau still owns a pending invocation.
    handle: Option<InvocationHandle>,
    /// Original dispatch origin restored for each VM segment.
    anchor: NodeId,
    /// Incarnation captured at admission or the first direct test poll.
    anchor_incarnation: Option<u64>,
    /// Error context retained across polls.
    label: String,
    /// Options retain their print-quota state across VM segments.
    options: CallOptions,
    /// Original timeout budget used in VM timeout diagnostics.
    reporting_timeout: Option<Duration>,
    /// Captured print output awaiting transfer to evaluation diagnostics.
    print_lines: Arc<Mutex<Vec<String>>>,
    /// Owned logs, separate from unrelated input callbacks while parked.
    logs: Vec<String>,
    /// Owned assertion outcomes, separate from unrelated input callbacks.
    assertions: Vec<ScriptAssertion>,
    /// Top-level admission remains held until the owner completes the task.
    active: Option<ActiveEvalGuard>,
}

impl fmt::Debug for ScriptInvocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScriptInvocation")
            .field("handle", &self.handle)
            .field("anchor", &self.anchor)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl ScriptInvocation {
    /// Bind this task to the anchor widget that existed at admission.
    pub(crate) fn set_anchor_incarnation(&mut self, incarnation: u64) {
        self.anchor_incarnation = Some(incarnation);
    }

    /// Preserve the requested budget while individual polls use the remaining
    /// time.
    pub(crate) fn set_reporting_timeout(&mut self, timeout: Option<Duration>) {
        self.reporting_timeout = timeout;
    }

    /// Take the complete diagnostics for final publication and journaling.
    pub(crate) fn take_diagnostics(&mut self) -> (Vec<String>, Vec<ScriptAssertion>) {
        (mem::take(&mut self.logs), mem::take(&mut self.assertions))
    }
}

impl Drop for ScriptInvocation {
    fn drop(&mut self) {
        if self.handle.is_some() {
            let host = self.host.clone();
            if let Err(error) = host.abort_invocation(self) {
                tracing::error!(%error, "failed to release pending script invocation");
            }
        }
    }
}

/// One VM segment's outcome and charged gas, without live VM values.
pub struct ScriptPoll {
    /// Pending work or the final owned evaluation result.
    pub poll: Poll<Result<ArgValue>>,
    /// Gas consumed by this segment, to subtract from the driver's total
    /// budget.
    pub gas_spent: u64,
}

/// Restore unrelated host diagnostics after one evaluation segment.
struct SegmentDiagnostics<'a> {
    /// Host whose ambient diagnostics are temporarily exchanged.
    host: &'a LuauHost,
    /// Task logs held outside the host between segments.
    logs: &'a mut Vec<String>,
    /// Task assertions held outside the host between segments.
    assertions: &'a mut Vec<ScriptAssertion>,
}

impl<'a> SegmentDiagnostics<'a> {
    /// Install task diagnostics and retain ambient diagnostics for restoration.
    fn enter(
        host: &'a LuauHost,
        logs: &'a mut Vec<String>,
        assertions: &'a mut Vec<ScriptAssertion>,
    ) -> Self {
        let mut state = host.state.borrow_mut();
        mem::swap(&mut state.logs, logs);
        mem::swap(&mut state.assertions, assertions);
        Self {
            host,
            logs,
            assertions,
        }
    }
}

impl Drop for SegmentDiagnostics<'_> {
    fn drop(&mut self) {
        let mut state = self.host.state.borrow_mut();
        mem::swap(&mut state.logs, self.logs);
        mem::swap(&mut state.assertions, self.assertions);
    }
}

/// Restore the original script-context stack after a borrowed VM segment.
struct SegmentAnchor<'a> {
    /// Application whose dispatch origin is temporarily installed.
    canopy: &'a mut Canopy,
    /// Context stack length to restore when this segment ends.
    depth: usize,
}

impl<'a> SegmentAnchor<'a> {
    /// Push the admitted task origin until this segment releases its borrow.
    fn enter(canopy: &'a mut Canopy, anchor: NodeId) -> Self {
        let depth = canopy.script_context_stack.len();
        canopy.script_context_stack.push(anchor);
        Self { canopy, depth }
    }
}

impl Drop for SegmentAnchor<'_> {
    fn drop(&mut self) {
        self.canopy.script_context_stack.truncate(self.depth);
    }
}

impl LuauHost {
    /// Check admission before compilation, reload, or diagnostics mutation.
    pub(crate) fn ensure_eval_idle(&self) -> Result<()> {
        if self.is_eval_active() {
            return Err(error::Error::script_structured(
                error::ScriptErrorKind::ScriptBusy,
                "a script evaluation is already active",
            ));
        }
        Ok(())
    }

    /// Whether a top-level evaluation still owns admission.
    pub(crate) fn is_eval_active(&self) -> bool {
        self.state.borrow().active_eval
    }

    /// Begin a compiled root without borrowing Canopy or retaining a VM borrow.
    pub(crate) fn start_invocation(
        &self,
        anchor: NodeId,
        script: ScriptId,
    ) -> Result<ScriptInvocation> {
        let active = self.begin_active_eval()?;
        let root = self.loaded_root(script)?;
        let label = format!("script {script} on node {anchor:?}");
        let handle = self
            .runtime_mut("script VM re-entered without a live scope")?
            .create_root_invocation(&root)
            .map_err(|error| retained_runtime_error_to_canopy(&error, &label, None))?;
        let print_lines = Arc::new(Mutex::new(Vec::new()));
        Ok(ScriptInvocation {
            host: self.clone(),
            handle: Some(handle),
            anchor,
            anchor_incarnation: None,
            label,
            options: invocation_options(None, &print_lines),
            reporting_timeout: None,
            print_lines,
            logs: Vec::new(),
            assertions: Vec::new(),
            active: Some(active),
        })
    }

    /// Poll one segment, releasing all borrowed state before returning Pending.
    pub(crate) fn poll_invocation(
        &self,
        canopy: &mut Canopy,
        invocation: &mut ScriptInvocation,
        cx: &mut TaskContext<'_>,
        remaining_gas: u64,
        remaining_timeout: Option<Duration>,
    ) -> ScriptPoll {
        match self.poll_segment(canopy, invocation, cx, remaining_gas, remaining_timeout) {
            Ok(poll) => poll,
            Err(error) => {
                if let Err(cleanup) = self.abort_invocation(invocation) {
                    tracing::error!(%cleanup, "script invocation cleanup failed after poll error");
                }
                ScriptPoll {
                    poll: Poll::Ready(Err(error)),
                    gas_spent: 0,
                }
            }
        }
    }

    /// Run one borrowed segment and synchronize its callback registry.
    fn poll_segment(
        &self,
        canopy: &mut Canopy,
        invocation: &mut ScriptInvocation,
        cx: &mut TaskContext<'_>,
        remaining_gas: u64,
        remaining_timeout: Option<Duration>,
    ) -> Result<ScriptPoll> {
        let anchor = invocation.anchor;
        let entry = canopy.core.nodes.get(anchor).ok_or_else(|| {
            error::Error::script_structured(
                error::ScriptErrorKind::InvalidNode,
                "script anchor no longer exists",
            )
            .with_owner(format!("{anchor:?}"))
        })?;
        if invocation
            .anchor_incarnation
            .is_some_and(|incarnation| incarnation != entry.incarnation)
        {
            return Err(error::Error::script_structured(
                error::ScriptErrorKind::InvalidNode,
                "script anchor widget was replaced",
            )
            .with_owner(format!("{anchor:?}")));
        }
        if !canopy.core.is_attached_to_root(anchor) {
            return Err(error::Error::script_structured(
                error::ScriptErrorKind::NodeDetached,
                "script anchor is detached",
            )
            .with_owner(format!("{anchor:?}")));
        }
        invocation.anchor_incarnation = Some(entry.incarnation);
        let handle = invocation.handle.ok_or_else(|| {
            error::Error::InvalidOperation("script invocation is already complete".to_string())
        })?;
        let mut runtime = self.runtime_mut("script VM re-entered without a live scope")?;
        invocation.options = mem::take(&mut invocation.options).limits(Limits {
            gas: Some(remaining_gas),
            quantum: Some(SCRIPT_QUANTUM),
            ..invocation_limits(remaining_timeout)
        });
        let diagnostics =
            SegmentDiagnostics::enter(self, &mut invocation.logs, &mut invocation.assertions);
        let anchor = SegmentAnchor::enter(canopy, invocation.anchor);
        let step = runtime.poll_invocation_with_context_and_result(
            handle,
            &mut *anchor.canopy,
            &invocation.options,
            cx,
            |scope, values| scope.marshal_values(values),
        );
        drop(anchor);
        if step.poll.is_ready() {
            invocation.handle = None;
        }
        let synchronized = self.synchronize_closures(
            &mut runtime,
            &invocation.label,
            invocation.reporting_timeout,
        );
        self.push_print_lines(&invocation.print_lines);
        drop(diagnostics);
        if let Err(error) = synchronized {
            if let Some(handle) = invocation.handle.take() {
                runtime.abort_invocation(handle).map_err(|error| {
                    retained_runtime_error_to_canopy(
                        &error,
                        &invocation.label,
                        invocation.reporting_timeout,
                    )
                })?;
            }
            return Ok(ScriptPoll {
                poll: Poll::Ready(Err(error)),
                gas_spent: step.usage.gas_spent,
            });
        }
        let poll = step.poll.map(|result| match result {
            Ok(values) => marshaled_to_arg_value(values.first().unwrap_or(&ValueSnapshot::Nil))
                .map_err(|message| {
                    error::Error::script(format!("{}: {message}", invocation.label))
                }),
            Err(InvocationError::Lifecycle(error)) => Err(retained_runtime_error_to_canopy(
                &error,
                &invocation.label,
                invocation.reporting_timeout,
            )),
            Err(InvocationError::Completion(error)) => Err(error::Error::script(format!(
                "{}: {}",
                invocation.label,
                error.message()
            ))),
        });
        Ok(ScriptPoll {
            poll,
            gas_spent: step.usage.gas_spent,
        })
    }

    /// Abort pending work once. A completed or previously aborted task is
    /// unchanged.
    pub(crate) fn abort_invocation(&self, invocation: &mut ScriptInvocation) -> Result<()> {
        if let Some(handle) = invocation.handle {
            let mut runtime = self.runtime_mut("cannot abort a script inside a live VM scope")?;
            invocation.handle = None;
            runtime.abort_invocation(handle).map_err(|error| {
                retained_runtime_error_to_canopy(&error, &invocation.label, None)
            })?;
            self.synchronize_closures(&mut runtime, &invocation.label, None)?;
        }
        self.push_pending_invocation_print(invocation);
        invocation.active = None;
        Ok(())
    }

    /// Preserve already captured output when cancellation ends parked work.
    fn push_pending_invocation_print(&self, invocation: &mut ScriptInvocation) {
        if let Ok(mut lines) = invocation.print_lines.lock() {
            invocation.logs.extend(lines.drain(..));
        }
    }

    /// Publish completed diagnostics for the existing journal and eval result
    /// APIs.
    pub(crate) fn set_diagnostics(&self, logs: Vec<String>, assertions: Vec<ScriptAssertion>) {
        let mut state = self.state.borrow_mut();
        state.logs = logs;
        state.assertions = assertions;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        task::{Wake, Waker},
    };

    use super::*;
    use crate::{
        core::inputmap::BindingTarget,
        testing::ttree::{R, run_ttree},
    };

    /// Observe whether the VM requested another immediate poll.
    struct TestWake(AtomicBool);

    impl Wake for TestWake {
        fn wake(self: Arc<Self>) {
            self.0.store(true, Ordering::Release);
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.store(true, Ordering::Release);
        }
    }

    /// Drive signaled segments until completion or an external event is
    /// required.
    fn drive_ready(
        host: &LuauHost,
        canopy: &mut Canopy,
        invocation: &mut ScriptInvocation,
        gas: &mut u64,
    ) -> Poll<Result<ArgValue>> {
        let wake = Arc::new(TestWake(AtomicBool::new(true)));
        let waker = Waker::from(wake.clone());
        let mut context = TaskContext::from_waker(&waker);
        for _ in 0..100_000 {
            wake.0.store(false, Ordering::Release);
            let step = host.poll_invocation(canopy, invocation, &mut context, *gas, None);
            *gas = gas.saturating_sub(step.gas_spent);
            if step.poll.is_ready() || !wake.0.load(Ordering::Acquire) {
                return step.poll;
            }
        }
        panic!("script failed to complete or park within its bounded segment count");
    }

    #[test]
    fn parked_script_releases_borrows_and_isolates_callback_diagnostics() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            canopy.finalize_api()?;
            let host = canopy.script_host.clone();
            let script = host.compile(r#"
                canopy.bind("x", {description = "Independent callback", phase = "before_widget"}, function()
                    canopy.log("binding output")
                end)
                print("root output")
                canopy.assert(true, "root assertion")
                canopy.wait_for(function() return false end)
            "#)?;
            host.push_log("ambient output".to_string());
            let mut invocation = host.start_invocation(tree.root, script)?;
            let mut gas = SCRIPT_GAS_LIMIT;
            assert!(drive_ready(&host, canopy, &mut invocation, &mut gas).is_pending());
            assert!(gas < SCRIPT_GAS_LIMIT);
            assert!(canopy.script_context_stack.is_empty());
            assert!(host.runtime.try_borrow_mut().is_ok());
            assert_eq!(host.logs(), vec!["ambient output"]);
            assert_eq!(invocation.logs, vec!["root output"]);
            assert_eq!(invocation.assertions.len(), 1);
            let function = host
                .state
                .borrow()
                .closures
                .functions
                .keys()
                .next()
                .copied()
                .expect("callback was registered");
            let target = canopy
                .core
                .input_map
                .bindings()
                .iter()
                .find_map(|binding| {
                    if let BindingTarget::Script(id) = &binding.target {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .expect("script binding exists");
            assert_eq!(function, target);
            host.call_function(canopy, tree.root, target)?;
            assert_eq!(host.logs(), vec!["ambient output", "binding output"]);
            assert_eq!(invocation.logs, vec!["root output"]);
            assert!(matches!(
                host.compile("return 1"),
                Err(error::Error::ScriptStructured {
                    kind: error::ScriptErrorKind::ScriptBusy,
                    ..
                })
            ));
            assert!(matches!(
                host.compile_startup_named("invalid Luau", "busy_startup"),
                Err(error::Error::ScriptStructured {
                    kind: error::ScriptErrorKind::ScriptBusy,
                    ..
                })
            ));
            assert!(matches!(
                canopy.invalidate_script_modules(None),
                Err(error::Error::ScriptStructured {
                    kind: error::ScriptErrorKind::ScriptBusy,
                    ..
                })
            ));
            assert!(matches!(
                host.start_invocation(tree.root, script),
                Err(error::Error::ScriptStructured {
                    kind: error::ScriptErrorKind::ScriptBusy,
                    ..
                })
            ));
            assert_eq!(host.logs(), vec!["ambient output", "binding output"]);
            assert!(host.state.borrow().closures.functions.contains_key(&target));
            assert_eq!(invocation.logs, vec!["root output"]);
            host.abort_invocation(&mut invocation)?;
            host.abort_invocation(&mut invocation)?;
            assert!(invocation.handle.is_none());
            assert!(!host.is_eval_active());
            assert_eq!(invocation.take_diagnostics().0, vec!["root output"]);
            Ok(())
        })
    }

    #[test]
    fn detached_print_quota_and_result_survive_multiple_segments() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            canopy.finalize_api()?;
            let host = canopy.script_host.clone();
            let script = host.compile(
                r#"
                for i = 1, 4100 do print("before") end
                canopy.wait_for(function() return true end)
                for i = 1, 100 do print("after") end
                return 42
            "#,
            )?;
            let mut invocation = host.start_invocation(tree.root, script)?;
            let mut gas = SCRIPT_GAS_LIMIT;
            let Poll::Ready(result) = drive_ready(&host, canopy, &mut invocation, &mut gas) else {
                panic!("an immediately satisfied wait must complete");
            };
            assert_eq!(result?, ArgValue::Int(42));
            assert!(gas < SCRIPT_GAS_LIMIT);
            assert!(invocation.handle.is_none());
            let (logs, _) = invocation.take_diagnostics();
            assert_eq!(logs.len(), 4097);
            assert_eq!(
                logs.iter().filter(|line| line.contains("truncat")).count(),
                1
            );
            assert!(!logs.iter().any(|line| line == "after"));
            drop(invocation);
            assert!(!host.is_eval_active());
            Ok(())
        })
    }

    #[test]
    fn resumed_script_rejects_replaced_anchor_before_returning() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            canopy.finalize_api()?;
            let host = canopy.script_host.clone();
            let script = host.compile("canopy.wait_for(function() return false end); return 42")?;
            let mut invocation = host.start_invocation(tree.root, script)?;
            let waker = Waker::from(Arc::new(TestWake(AtomicBool::new(true))));
            let mut context = TaskContext::from_waker(&waker);
            let step = host.poll_invocation(
                canopy,
                &mut invocation,
                &mut context,
                SCRIPT_GAS_LIMIT,
                None,
            );
            assert!(step.poll.is_pending(), "false predicate must park the root");
            canopy.core.replace_subtree(tree.root, R::new())?;
            let step = host.poll_invocation(
                canopy,
                &mut invocation,
                &mut context,
                SCRIPT_GAS_LIMIT.saturating_sub(step.gas_spent),
                None,
            );
            assert!(matches!(
                step.poll,
                Poll::Ready(Err(error::Error::ScriptStructured {
                    kind: error::ScriptErrorKind::InvalidNode,
                    ..
                }))
            ));
            assert!(invocation.handle.is_none());
            assert!(!host.is_eval_active());
            assert!(canopy.script_context_stack.is_empty());
            Ok(())
        })
    }

    #[test]
    fn detached_vm_timeout_reports_the_original_budget() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            canopy.finalize_api()?;
            let host = canopy.script_host.clone();
            let script = host.compile("while true do end")?;
            let mut invocation = host.start_invocation(tree.root, script)?;
            invocation.set_reporting_timeout(Some(Duration::from_millis(125)));
            let waker = Waker::from(Arc::new(TestWake(AtomicBool::new(true))));
            let mut context = TaskContext::from_waker(&waker);
            let step = host.poll_invocation(
                canopy,
                &mut invocation,
                &mut context,
                SCRIPT_GAS_LIMIT,
                Some(Duration::ZERO),
            );
            assert!(matches!(
                step.poll,
                Poll::Ready(Err(error::Error::ScriptTimeout { timeout_ms: 125 }))
            ));
            assert!(invocation.handle.is_none());
            Ok(())
        })
    }

    #[test]
    fn detached_segments_share_a_total_gas_budget() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            canopy.finalize_api()?;
            let host = canopy.script_host.clone();
            let script = host.compile(
                r#"
                for i = 1, 10000 do
                    canopy.wait_for(function() return true end)
                end
            "#,
            )?;
            let mut invocation = host.start_invocation(tree.root, script)?;
            let mut gas = 10_000;
            let Poll::Ready(result) = drive_ready(&host, canopy, &mut invocation, &mut gas) else {
                panic!("gas exhaustion must terminate immediately satisfied waits");
            };
            let error = result.expect_err("the total budget must not reset after each wait");
            assert!(
                error.to_string().contains("instruction budget exhausted"),
                "{error}"
            );
            assert!(invocation.handle.is_none());
            assert!(canopy.script_context_stack.is_empty());
            Ok(())
        })
    }
}
