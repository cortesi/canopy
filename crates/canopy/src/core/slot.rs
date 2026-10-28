//! A request box that holds at most one request, for a worker where only the
//! newest request matters.

use std::{
    error, fmt,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

/// The box, under one lock.
struct Slot<T> {
    /// The request that waits for the worker, and its generation.
    waiting: Option<(T, u64)>,
    /// The cancel flag of the job the worker took last.
    running: Option<Arc<AtomicBool>>,
    /// Advances on each send and cancel.
    generation: u64,
    /// Live senders. The worker stops waiting when none is left.
    senders: usize,
    /// Whether the receiver is still alive.
    receiver: bool,
}

impl<T> Slot<T> {
    /// Cancel the running job and drop the waiting request.
    fn clear(&mut self) {
        self.waiting = None;
        if let Some(running) = self.running.take() {
            running.store(true, Ordering::Relaxed);
        }
    }
}

/// State shared by the senders and the receiver.
struct Shared<T> {
    /// The box.
    slot: Mutex<Slot<T>>,
    /// Signals a request, or the last sender's drop.
    ready: Condvar,
}

impl<T> Shared<T> {
    /// Lock the box. A panic in another holder leaves the box consistent:
    /// each change under the lock is a single assignment.
    fn lock(&self) -> MutexGuard<'_, Slot<T>> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Create a request box that holds at most one request: a new request
/// replaces a waiting one and cancels a running one.
///
/// A widget keeps the sender and a worker thread loops on
/// [`SlotReceiver::recv`]. Each send and cancel advances the generation, so a
/// result is current only while its job's generation equals
/// [`SlotSender::generation`].
pub fn request_slot<T>() -> (SlotSender<T>, SlotReceiver<T>) {
    let shared = Arc::new(Shared {
        slot: Mutex::new(Slot {
            waiting: None,
            running: None,
            generation: 0,
            senders: 1,
            receiver: true,
        }),
        ready: Condvar::new(),
    });
    (
        SlotSender {
            shared: Arc::clone(&shared),
        },
        SlotReceiver { shared },
    )
}

/// The receiver has gone, so the slot takes no request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotClosed;

impl fmt::Display for SlotClosed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the request slot has no receiver")
    }
}

impl error::Error for SlotClosed {}

/// The sending half of a [`request_slot`].
///
/// Dropping the last sender cancels the running job and ends the worker's
/// [`SlotReceiver::recv`].
pub struct SlotSender<T> {
    /// The box.
    shared: Arc<Shared<T>>,
}

impl<T> Clone for SlotSender<T> {
    fn clone(&self) -> Self {
        self.shared.lock().senders += 1;
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> Drop for SlotSender<T> {
    fn drop(&mut self) {
        let mut slot = self.shared.lock();
        slot.senders -= 1;
        if slot.senders == 0 {
            slot.clear();
            drop(slot);
            self.shared.ready.notify_all();
        }
    }
}

impl<T> fmt::Debug for SlotSender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let slot = self.shared.lock();
        f.debug_struct("SlotSender")
            .field("generation", &slot.generation)
            .field("waiting", &slot.waiting.is_some())
            .finish()
    }
}

impl<T> SlotSender<T> {
    /// Put `request` in the box, cancel the running job, and return the
    /// generation of the new request.
    pub fn send(&self, request: T) -> Result<u64, SlotClosed> {
        let mut slot = self.shared.lock();
        if !slot.receiver {
            return Err(SlotClosed);
        }
        slot.clear();
        slot.generation = slot.generation.wrapping_add(1);
        let generation = slot.generation;
        slot.waiting = Some((request, generation));
        drop(slot);
        self.shared.ready.notify_one();
        Ok(generation)
    }

    /// Drop the waiting request, cancel the running job, and advance the
    /// generation, so a late result of the running job is no longer current.
    /// Return the new generation.
    pub fn cancel(&self) -> u64 {
        let mut slot = self.shared.lock();
        slot.clear();
        slot.generation = slot.generation.wrapping_add(1);
        slot.generation
    }

    /// Return the current generation.
    pub fn generation(&self) -> u64 {
        self.shared.lock().generation
    }
}

/// The receiving half of a [`request_slot`], which a worker thread holds.
///
/// Dropping the receiver cancels the running job, and each later
/// [`SlotSender::send`] fails.
pub struct SlotReceiver<T> {
    /// The box.
    shared: Arc<Shared<T>>,
}

impl<T> Drop for SlotReceiver<T> {
    fn drop(&mut self) {
        let mut slot = self.shared.lock();
        slot.receiver = false;
        slot.clear();
    }
}

impl<T> fmt::Debug for SlotReceiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let slot = self.shared.lock();
        f.debug_struct("SlotReceiver")
            .field("generation", &slot.generation)
            .field("senders", &slot.senders)
            .finish()
    }
}

impl<T> SlotReceiver<T> {
    /// Wait for the next request. Return `None` when every sender is gone.
    pub fn recv(&self) -> Option<Job<T>> {
        let mut slot = self.shared.lock();
        loop {
            if let Some((request, generation)) = slot.waiting.take() {
                let flag = Arc::new(AtomicBool::new(false));
                slot.running = Some(Arc::clone(&flag));
                return Some(Job {
                    request,
                    generation,
                    cancel: CancelToken { flag },
                });
            }
            if slot.senders == 0 {
                return None;
            }
            slot = self
                .shared
                .ready
                .wait(slot)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// A request that a worker took from a [`request_slot`].
#[derive(Debug)]
pub struct Job<T> {
    /// The request.
    pub request: T,
    /// The generation that [`SlotSender::send`] returned for the request.
    pub generation: u64,
    /// Set when a newer request or a cancel replaces the job.
    pub cancel: CancelToken,
}

/// Tells a worker that a newer request or a cancel replaced its job.
#[derive(Clone, Debug)]
pub struct CancelToken {
    /// The flag that the slot sets.
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    /// Return whether a newer request or a cancel replaced the job.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    /// Return the flag under the token, for an API that takes an
    /// `Arc<AtomicBool>`.
    pub fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.flag)
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    #[test]
    fn a_send_replaces_a_waiting_request_and_cancels_a_running_job() {
        let (sender, receiver) = request_slot();
        sender.send(1).unwrap();
        let running = receiver.recv().unwrap();
        assert_eq!(running.request, 1);
        assert!(!running.cancel.is_cancelled());
        sender.send(2).unwrap();
        assert!(running.cancel.is_cancelled());
        let newest = sender.send(3).unwrap();
        let job = receiver.recv().unwrap();
        assert_eq!((job.request, job.generation), (3, newest));
        assert_eq!(job.generation, sender.generation());
    }

    #[test]
    fn cancel_leaves_the_late_result_of_a_job_that_ignores_its_token_stale() {
        let (sender, receiver) = request_slot();
        sender.send("first").unwrap();
        let job = receiver.recv().unwrap();
        sender.send("waiting").unwrap();
        let generation = sender.cancel();
        assert!(job.cancel.is_cancelled());
        assert!(job.cancel.flag().load(Ordering::Relaxed));
        assert_eq!(generation, sender.generation());
        assert_ne!(
            job.generation,
            sender.generation(),
            "the late result is stale"
        );
        drop(sender);
        assert!(
            receiver.recv().is_none(),
            "cancel dropped the waiting request"
        );
    }

    #[test]
    fn the_last_dropped_sender_ends_recv_and_cancels_the_running_job() {
        let (sender, receiver) = request_slot::<u32>();
        let other = sender.clone();
        sender.send(1).unwrap();
        let job = receiver.recv().unwrap();
        let worker = thread::spawn(move || receiver.recv().map(|job| job.request));
        drop(sender);
        assert!(!job.cancel.is_cancelled(), "one sender is still alive");
        drop(other);
        assert_eq!(worker.join().unwrap(), None);
        assert!(job.cancel.is_cancelled());
    }

    #[test]
    fn a_dropped_receiver_cancels_the_running_job_and_fails_each_send() {
        let (sender, receiver) = request_slot();
        sender.send(1).unwrap();
        let job = receiver.recv().unwrap();
        drop(receiver);
        assert!(job.cancel.is_cancelled());
        assert_eq!(sender.send(2), Err(SlotClosed));
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn the_halves_and_the_token_are_send_and_sync() {
        assert_send_sync::<SlotSender<String>>();
        assert_send_sync::<SlotReceiver<String>>();
        assert_send_sync::<CancelToken>();
    }
}
