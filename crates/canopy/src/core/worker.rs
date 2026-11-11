//! A background worker: a thread that serves the newest request of a
//! [`request_slot`], wakes its owner with each reply, and wakes it again when
//! the thread ends.

use std::{
    io,
    sync::mpsc::{Receiver, TryRecvError},
    thread,
};

use super::{
    slot::{Job, SlotClosed, SlotReceiver, SlotSender, request_slot},
    wake::{NodeWakeHandle, WakeSender, wake_channel},
};

/// What a poll of a [`Worker`] found.
#[derive(Debug)]
pub struct Replies<R> {
    /// The replies that arrived since the last poll, oldest first.
    pub replies: Vec<R>,
    /// Whether the thread has gone: it returned or panicked, and sends no
    /// more replies.
    pub gone: bool,
}

/// One background thread with a request slot and a channel of replies.
///
/// A widget keeps the worker and polls it from `Widget::poll`. A request
/// replaces a waiting one and cancels a running one. Each reply wakes the
/// owner of the worker's wake handle, and so does the end of the thread, by
/// return or by panic, so the owner's next poll sees a worker that has gone
/// and can start another. Dropping the worker closes its slot, which ends
/// the thread once its job returns.
#[derive(Debug)]
pub struct Worker<Q, R> {
    /// The requests.
    requests: SlotSender<Q>,
    /// The replies.
    replies: Receiver<R>,
}

impl<Q: Send + 'static, R: Send + 'static> Worker<Q, R> {
    /// Start a thread named `name` that runs `run` with the receiving half
    /// of the request slot and a sender of replies. Each reply wakes the
    /// owner of `wake`, and at most `capacity` replies wait at once. The
    /// thread ends when `run` returns, which it should do once
    /// [`SlotReceiver::recv`] returns `None` or a send fails.
    ///
    /// # Errors
    ///
    /// Fails when the thread cannot start.
    pub fn spawn(
        name: impl Into<String>,
        wake: NodeWakeHandle,
        capacity: usize,
        run: impl FnOnce(SlotReceiver<Q>, WakeSender<R>) + Send + 'static,
    ) -> io::Result<Self> {
        let (requests, jobs) = request_slot();
        let exit = WakeOnExit(wake.clone());
        let (sender, replies) = wake_channel(wake, capacity);
        thread::Builder::new().name(name.into()).spawn(move || {
            let _exit = exit;
            run(jobs, sender);
        })?;
        Ok(Self { requests, replies })
    }

    /// Start a thread that serves each job with `serve` and sends the reply
    /// it returns, if any, as [`Self::spawn`] does. The thread ends when the
    /// worker drops, or when nobody reads its replies.
    ///
    /// # Errors
    ///
    /// Fails when the thread cannot start.
    pub fn serve(
        name: impl Into<String>,
        wake: NodeWakeHandle,
        capacity: usize,
        mut serve: impl FnMut(Job<Q>) -> Option<R> + Send + 'static,
    ) -> io::Result<Self> {
        Self::spawn(name, wake, capacity, move |jobs, replies| {
            while let Some(job) = jobs.recv() {
                if let Some(reply) = serve(job)
                    && replies.send(reply).is_err()
                {
                    break;
                }
            }
        })
    }
}

impl<Q, R> Worker<Q, R> {
    /// Make a worker of the two ends that a thread would hold the others
    /// of, as a test does to stand in for the thread.
    pub fn from_parts(requests: SlotSender<Q>, replies: Receiver<R>) -> Self {
        Self { requests, replies }
    }

    /// Queue `request`, replacing a waiting one and cancelling a running
    /// one, and return its generation.
    ///
    /// # Errors
    ///
    /// Fails when the thread has gone.
    pub fn send(&self, request: Q) -> Result<u64, SlotClosed> {
        self.requests.send(request)
    }

    /// Cancel the running job and drop the waiting request, and return the
    /// new generation.
    pub fn cancel(&self) -> u64 {
        self.requests.cancel()
    }

    /// Return the generation of the last send or cancel. A reply is current
    /// while its job's generation equals it.
    pub fn generation(&self) -> u64 {
        self.requests.generation()
    }

    /// Take every reply that waits, and say whether the thread has gone.
    pub fn poll(&self) -> Replies<R> {
        let mut replies = Vec::new();
        loop {
            match self.replies.try_recv() {
                Ok(reply) => replies.push(reply),
                Err(TryRecvError::Empty) => {
                    return Replies {
                        replies,
                        gone: false,
                    };
                }
                Err(TryRecvError::Disconnected) => {
                    return Replies {
                        replies,
                        gone: true,
                    };
                }
            }
        }
    }
}

/// Wakes the owner when a worker thread ends, by return or by panic.
#[derive(Debug)]
pub struct WakeOnExit(pub NodeWakeHandle);

impl Drop for WakeOnExit {
    fn drop(&mut self) {
        // Only a poisoned registry fails a wake, and the application that
        // owns it has already panicked.
        self.0.wake().ok();
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::core::{
        id::testing_node_id,
        wake::{PollOwner, WakeRegistry},
    };

    /// Wait a few seconds at most for `registry` to record a wake.
    fn woke(registry: &WakeRegistry) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if !registry.drain().unwrap().is_empty() {
                return true;
            }
            thread::yield_now();
        }
        false
    }

    /// Return a registry and a wake handle that it records.
    fn wake() -> (WakeRegistry, NodeWakeHandle) {
        let registry = WakeRegistry::default();
        let handle = registry
            .handle(PollOwner {
                node: testing_node_id(),
                incarnation: 1,
                attachment: None,
            })
            .unwrap();
        (registry, handle)
    }

    /// Poll `worker` until `done` holds of what it found, within a few
    /// seconds, and return everything it found.
    fn poll_until<Q, R>(worker: &Worker<Q, R>, done: impl Fn(&Replies<R>) -> bool) -> Replies<R> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut found = Replies {
            replies: Vec::new(),
            gone: false,
        };
        loop {
            let polled = worker.poll();
            found.replies.extend(polled.replies);
            found.gone |= polled.gone;
            if done(&found) || Instant::now() > deadline {
                return found;
            }
            thread::yield_now();
        }
    }

    #[test]
    fn a_worker_serves_the_newest_request_and_wakes_its_owner() {
        let (registry, wake) = wake();
        let worker = Worker::serve("test-worker", wake, 4, |job: Job<u32>| {
            Some((job.generation, job.request * 2))
        })
        .unwrap();
        let generation = worker.send(21).unwrap();
        let found = poll_until(&worker, |found| !found.replies.is_empty());
        assert_eq!(found.replies, [(generation, 42)]);
        assert!(!found.gone);
        assert_eq!(worker.generation(), generation);
        assert!(woke(&registry), "the reply woke the owner");
    }

    #[test]
    fn a_thread_that_returns_or_panics_is_gone_and_wakes_its_owner() {
        let (registry, wake) = wake();
        let returns =
            Worker::<u32, u32>::spawn("test-returns", wake.clone(), 1, |_, _| {}).unwrap();
        assert!(poll_until(&returns, |found| found.gone).gone);
        // The channel closes as `run` returns, just before the wake.
        assert!(woke(&registry), "the end woke the owner");
        assert!(returns.send(1).is_err(), "a gone worker takes no request");

        let panics = Worker::serve("test-panics", wake, 1, |_: Job<u32>| -> Option<u32> {
            panic!("the job fails")
        })
        .unwrap();
        panics.send(1).unwrap();
        assert!(poll_until(&panics, |found| found.gone).gone);
        assert!(woke(&registry), "the panic woke the owner");
    }

    #[test]
    fn a_worker_of_parts_reports_its_ends() {
        let (requests, jobs) = request_slot::<u32>();
        let (replies, receiver) = std::sync::mpsc::channel::<u32>();
        let worker = Worker::from_parts(requests, receiver);
        replies.send(7).unwrap();
        drop((jobs, replies));
        let found = worker.poll();
        assert_eq!(found.replies, [7]);
        assert!(found.gone);
    }
}
