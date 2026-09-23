//! Terminal input: event translation, batching, and coalescing.

use std::{
    future::poll_fn,
    io,
    pin::Pin,
    task::{Context, Poll},
};

use crossterm::event as cevent;
use futures::{channel::mpsc::UnboundedReceiver, stream::Stream};

use crate::{
    TurnInput,
    core::canopy::AdapterEvent,
    error::{self, Result},
    event::{Event, key, mouse},
    geom::{PointI32, Size},
};

/// Most ready input events one turn takes before it renders.
const INPUT_BATCH_LIMIT: usize = 256;

/// Simple event source wrapper for receiving events.
///
/// The run loop takes one event, then drains the input already waiting into
/// the same batch, so a burst of input costs one render.
pub(super) struct EventSource<S> {
    /// Cancellable terminal event stream owned by the run loop.
    terminal: S,
    /// Framework event receiver channel.
    internal: UnboundedReceiver<AdapterEvent>,
    /// Framework wake that ended the last batch, returned by the next call.
    pending: Option<AdapterEvent>,
    /// Stream error met while draining, returned by the next call after its
    /// batch.
    deferred_error: Option<error::Error>,
    /// Alternate terminal and framework input when both remain ready.
    prefer_internal: bool,
}

impl<S> EventSource<S>
where
    S: Stream<Item = io::Result<cevent::Event>> + Unpin,
{
    /// Construct a new event source.
    pub(super) fn new(terminal: S, internal: UnboundedReceiver<AdapterEvent>) -> Self {
        Self {
            terminal,
            internal,
            pending: None,
            deferred_error: None,
            prefer_internal: false,
        }
    }

    /// Poll one input source, preserving stream errors and termination.
    fn poll_source(
        &mut self,
        cx: &mut Context<'_>,
        internal: bool,
    ) -> Poll<Result<Option<AdapterEvent>>> {
        if internal {
            Pin::new(&mut self.internal).poll_next(cx).map(|event| {
                event
                    .map(Some)
                    .ok_or_else(|| error::Error::Driver("framework event channel closed".into()))
            })
        } else {
            Pin::new(&mut self.terminal)
                .poll_next(cx)
                .map(terminal_event)
                .map(|result| result.map(|event| event.map(AdapterEvent::Input)))
        }
    }

    /// Poll fairly while bounding ignored terminal events in one executor turn.
    fn poll_uncoalesced(&mut self, cx: &mut Context<'_>) -> Poll<Result<AdapterEvent>> {
        for _ in 0..64 {
            let first = self.prefer_internal;
            let (result, internal) = match self.poll_source(cx, first) {
                Poll::Pending => (self.poll_source(cx, !first), !first),
                ready => (ready, first),
            };
            match result {
                Poll::Ready(Ok(event)) => {
                    self.prefer_internal = !internal;
                    if let Some(event) = event {
                        return Poll::Ready(Ok(event));
                    }
                }
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            }
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }

    /// Await one event from either the terminal or framework channel.
    async fn next_uncoalesced(&mut self) -> Result<AdapterEvent> {
        poll_fn(|cx| self.poll_uncoalesced(cx)).await
    }

    /// Await the next event, returning a held stream error or framework wake
    /// first.
    async fn next(&mut self) -> Result<AdapterEvent> {
        if let Some(error) = self.deferred_error.take() {
            return Err(error);
        }
        if let Some(event) = self.pending.take() {
            return Ok(event);
        }
        self.next_uncoalesced().await
    }

    /// Await the next unit of work: a framework wake, or a batch of input.
    ///
    /// A batch starts with the next input event and takes the input already
    /// waiting, at most [`INPUT_BATCH_LIMIT`] events, so a burst of input costs
    /// one render. Consecutive pointer moves, and consecutive drags with the
    /// same button and modifiers, collapse to the latest position.
    ///
    /// Draining polls with the caller's waker. A terminal stream such as
    /// crossterm's keeps the waker from its first pending poll to report the
    /// next event, so draining with a throwaway waker would leave the run
    /// loop asleep.
    pub(super) async fn next_input(&mut self) -> Result<TurnInput> {
        let AdapterEvent::Input(first) = self.next().await? else {
            return Ok(TurnInput::Wake);
        };
        let mut batch = vec![first];
        poll_fn(|cx| {
            self.drain_ready(cx, &mut batch);
            Poll::Ready(())
        })
        .await;
        Ok(TurnInput::Events(batch))
    }

    /// Move waiting input into `batch` until none is ready.
    ///
    /// A framework wake ends the batch and waits for the next call, and so does
    /// a stream error, so the input read before it is still delivered.
    fn drain_ready(&mut self, cx: &mut Context<'_>, batch: &mut Vec<Event>) {
        for _ in 0..INPUT_BATCH_LIMIT {
            let next = match self.poll_uncoalesced(cx) {
                Poll::Ready(Ok(event)) => event,
                Poll::Ready(Err(error)) => {
                    self.deferred_error = Some(error);
                    return;
                }
                Poll::Pending => return,
            };
            match next {
                AdapterEvent::Input(event) => push_coalesced(batch, event),
                AdapterEvent::Wake => {
                    self.pending = Some(AdapterEvent::Wake);
                    return;
                }
            }
        }
    }
}

/// Append `event` to `batch`, replacing the last event when both are the same
/// pointer motion.
fn push_coalesced(batch: &mut Vec<Event>, event: Event) {
    if let (Some(Event::Mouse(last)), Event::Mouse(next)) = (batch.last_mut(), &event)
        && same_motion(last, next)
    {
        *last = *next;
        return;
    }
    batch.push(event);
}

/// Return whether `next` continues the motion of `last`: a move after a move,
/// or a drag after a drag with the same button and modifiers.
fn same_motion(last: &mouse::MouseEvent, next: &mouse::MouseEvent) -> bool {
    matches!(
        (last.action, next.action),
        (mouse::Action::Moved, mouse::Action::Moved) | (mouse::Action::Drag, mouse::Action::Drag)
    ) && last.button == next.button
        && last.modifiers == next.modifiers
}

/// Translate one terminal stream item or report reader termination.
fn terminal_event(event: Option<io::Result<cevent::Event>>) -> Result<Option<Event>> {
    match event {
        Some(Ok(cevent::Event::Key(event)))
            if event.kind == cevent::KeyEventKind::Release
                || matches!(
                    event.code,
                    cevent::KeyCode::Media(_) | cevent::KeyCode::Modifier(_)
                ) =>
        {
            Ok(None)
        }
        Some(Ok(event)) => Ok(Some(translate_event(event))),
        Some(Err(error)) => Err(error::Error::TerminalIo(error)),
        None => Err(error::Error::Driver("terminal event stream closed".into())),
    }
}

/// Translate crossterm key modifiers into canopy modifiers.
fn translate_key_modifiers(mods: cevent::KeyModifiers) -> key::Mods {
    key::Mods {
        shift: mods.contains(cevent::KeyModifiers::SHIFT),
        ctrl: mods.contains(cevent::KeyModifiers::CONTROL),
        alt: mods.contains(cevent::KeyModifiers::ALT),
    }
}

/// Translate a crossterm mouse button into a canopy button.
fn translate_button(b: cevent::MouseButton) -> mouse::Button {
    match b {
        cevent::MouseButton::Left => mouse::Button::Left,
        cevent::MouseButton::Right => mouse::Button::Right,
        cevent::MouseButton::Middle => mouse::Button::Middle,
    }
}

/// Translate a crossterm event into a canopy event.
fn translate_event(e: cevent::Event) -> Event {
    match e {
        cevent::Event::Key(k) => Event::Key(key::Key {
            mods: translate_key_modifiers(k.modifiers),
            key: match k.code {
                cevent::KeyCode::Backspace => key::KeyCode::Backspace,
                cevent::KeyCode::Enter => key::KeyCode::Enter,
                cevent::KeyCode::Left => key::KeyCode::Left,
                cevent::KeyCode::Right => key::KeyCode::Right,
                cevent::KeyCode::Up => key::KeyCode::Up,
                cevent::KeyCode::Down => key::KeyCode::Down,
                cevent::KeyCode::Home => key::KeyCode::Home,
                cevent::KeyCode::End => key::KeyCode::End,
                cevent::KeyCode::PageUp => key::KeyCode::PageUp,
                cevent::KeyCode::PageDown => key::KeyCode::PageDown,
                cevent::KeyCode::Tab => key::KeyCode::Tab,
                cevent::KeyCode::BackTab => key::KeyCode::BackTab,
                cevent::KeyCode::Delete => key::KeyCode::Delete,
                cevent::KeyCode::Insert => key::KeyCode::Insert,
                cevent::KeyCode::F(x) => key::KeyCode::F(x),
                cevent::KeyCode::Char(c) => key::KeyCode::Char(c),
                cevent::KeyCode::Null => key::KeyCode::Null,
                cevent::KeyCode::Esc => key::KeyCode::Esc,
                cevent::KeyCode::CapsLock => key::KeyCode::CapsLock,
                cevent::KeyCode::ScrollLock => key::KeyCode::ScrollLock,
                cevent::KeyCode::NumLock => key::KeyCode::NumLock,
                cevent::KeyCode::PrintScreen => key::KeyCode::PrintScreen,
                cevent::KeyCode::Pause => key::KeyCode::Pause,
                cevent::KeyCode::Menu => key::KeyCode::Menu,
                cevent::KeyCode::KeypadBegin => key::KeyCode::KeypadBegin,
                cevent::KeyCode::Media(_) | cevent::KeyCode::Modifier(_) => {
                    unreachable!("media and modifier keys are filtered before translation")
                }
            },
        }),
        cevent::Event::Mouse(m) => {
            let mut button = mouse::Button::None;
            let action = match m.kind {
                cevent::MouseEventKind::Down(b) => {
                    button = translate_button(b);
                    mouse::Action::Down
                }
                cevent::MouseEventKind::Up(b) => {
                    button = translate_button(b);
                    mouse::Action::Up
                }
                cevent::MouseEventKind::Drag(b) => {
                    button = translate_button(b);
                    mouse::Action::Drag
                }
                cevent::MouseEventKind::Moved => mouse::Action::Moved,
                cevent::MouseEventKind::ScrollDown => mouse::Action::ScrollDown,
                cevent::MouseEventKind::ScrollUp => mouse::Action::ScrollUp,
                cevent::MouseEventKind::ScrollLeft => mouse::Action::ScrollLeft,
                cevent::MouseEventKind::ScrollRight => mouse::Action::ScrollRight,
            };
            Event::Mouse(mouse::MouseEvent {
                button,
                action,
                location: PointI32 {
                    x: m.column.into(),
                    y: m.row.into(),
                },
                modifiers: translate_key_modifiers(m.modifiers),
            })
        }
        cevent::Event::Resize(x, y) => Event::Resize(Size::new(x.into(), y.into())),
        cevent::Event::FocusGained => Event::FocusGained,
        cevent::Event::FocusLost => Event::FocusLost,
        cevent::Event::Paste(s) => Event::Paste(s),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        task::Waker,
        thread,
        time::Duration,
    };

    use futures::{StreamExt, channel::mpsc::unbounded, executor::block_on, stream};

    use super::*;

    /// Pending stream that records when cancellation drops it.
    struct DropReader {
        dropped: Arc<AtomicBool>,
    }

    impl Stream for DropReader {
        type Item = io::Result<cevent::Event>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Pending
        }
    }

    impl Drop for DropReader {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Relaxed);
        }
    }

    #[test]
    fn event_source_surfaces_terminal_reader_failure() {
        let (_internal_tx, internal_rx) = unbounded();
        let terminal = stream::iter(vec![Err(io::Error::other("reader failed"))]);
        let mut events = EventSource::new(terminal, internal_rx);

        let error = block_on(events.next()).expect_err("reader failure should reach run loop");
        assert!(matches!(
            error,
            error::Error::TerminalIo(source) if source.to_string() == "reader failed"
        ));
    }

    #[test]
    fn dropping_event_source_cancels_terminal_reader() {
        let dropped = Arc::new(AtomicBool::new(false));
        let terminal = DropReader {
            dropped: Arc::clone(&dropped),
        };
        let (_internal_tx, internal_rx) = unbounded();

        drop(EventSource::new(terminal, internal_rx));

        assert!(dropped.load(Ordering::Relaxed));
    }

    #[test]
    fn internal_event_wakes_pending_terminal_reader() {
        let dropped = Arc::new(AtomicBool::new(false));
        let terminal = DropReader {
            dropped: Arc::clone(&dropped),
        };
        let (internal_tx, internal_rx) = unbounded();
        internal_tx
            .unbounded_send(AdapterEvent::Wake)
            .expect("internal event receiver should be open");
        let mut events = EventSource::new(terminal, internal_rx);

        assert!(matches!(block_on(events.next()), Ok(AdapterEvent::Wake)));
        drop(events);
        assert!(dropped.load(Ordering::Relaxed));
    }

    fn terminal_key(kind: cevent::KeyEventKind) -> cevent::Event {
        cevent::Event::Key(cevent::KeyEvent::new_with_kind(
            cevent::KeyCode::Char('a'),
            cevent::KeyModifiers::empty(),
            kind,
        ))
    }

    fn terminal_drag(column: u16) -> cevent::Event {
        cevent::Event::Mouse(cevent::MouseEvent {
            kind: cevent::MouseEventKind::Drag(cevent::MouseButton::Left),
            column,
            row: 0,
            modifiers: cevent::KeyModifiers::empty(),
        })
    }

    fn terminal_move(column: u16) -> cevent::Event {
        cevent::Event::Mouse(cevent::MouseEvent {
            kind: cevent::MouseEventKind::Moved,
            column,
            row: 0,
            modifiers: cevent::KeyModifiers::empty(),
        })
    }

    #[test]
    fn event_source_ignores_releases_and_preserves_repeats() -> Result<()> {
        let (_tx, rx) = unbounded();
        let terminal = stream::iter([
            Ok(terminal_key(cevent::KeyEventKind::Press)),
            Ok(terminal_key(cevent::KeyEventKind::Release)),
            Ok(terminal_key(cevent::KeyEventKind::Repeat)),
            Ok(terminal_key(cevent::KeyEventKind::Release)),
        ]);
        let mut events = EventSource::new(terminal, rx);
        for _ in 0..2 {
            assert!(matches!(
                block_on(events.next())?,
                AdapterEvent::Input(Event::Key(key::Key {
                    key: key::KeyCode::Char('a'),
                    ..
                }))
            ));
        }
        assert!(matches!(
            block_on(events.next()),
            Err(error::Error::Driver(_))
        ));
        Ok(())
    }

    #[test]
    fn event_source_errors_surface_directly_and_after_a_batch() -> Result<()> {
        // Awaiting the next event returns the error at once.
        let (_tx, rx) = unbounded();
        let terminal = stream::iter([
            Ok(terminal_key(cevent::KeyEventKind::Release)),
            Err(io::Error::other("after release")),
        ]);
        let mut events = EventSource::new(terminal, rx);
        assert!(
            matches!(block_on(events.next()), Err(error::Error::TerminalIo(error)) if error.to_string() == "after release")
        );

        // An error met while draining keeps the batch and returns next.
        let (_tx, rx) = unbounded();
        let terminal = stream::iter([
            Ok(terminal_key(cevent::KeyEventKind::Press)),
            Ok(terminal_key(cevent::KeyEventKind::Release)),
            Err(io::Error::other("after release")),
        ]);
        let mut events = EventSource::new(terminal, rx);
        assert!(
            matches!(block_on(events.next_input())?, TurnInput::Events(batch) if batch.len() == 1)
        );
        assert!(
            matches!(block_on(events.next_input()), Err(error::Error::TerminalIo(error)) if error.to_string() == "after release")
        );
        Ok(())
    }

    #[test]
    fn event_source_returns_eof_after_the_batch_before_it() -> Result<()> {
        let (_tx, rx) = unbounded();
        let terminal = stream::iter([
            Ok(terminal_key(cevent::KeyEventKind::Press)),
            Ok(terminal_key(cevent::KeyEventKind::Release)),
        ]);
        let mut events = EventSource::new(terminal, rx);
        assert!(
            matches!(block_on(events.next_input())?, TurnInput::Events(batch) if batch.len() == 1)
        );
        assert!(matches!(
            block_on(events.next_input()),
            Err(error::Error::Driver(_))
        ));
        Ok(())
    }

    /// Return each batched event as a short label.
    fn batch_summary(batch: &[Event]) -> Vec<String> {
        batch
            .iter()
            .map(|event| match event {
                Event::Mouse(mouse) => format!("{:?}@{}", mouse.action, mouse.location.x),
                Event::Key(_) => "key".to_string(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn ready_input_batches_and_coalesces_moves_and_drags() -> Result<()> {
        let (_tx, rx) = unbounded();
        let terminal = stream::iter([
            Ok(terminal_move(1)),
            Ok(terminal_key(cevent::KeyEventKind::Release)),
            Ok(terminal_move(2)),
            Ok(terminal_drag(3)),
            Ok(terminal_drag(4)),
            Ok(terminal_key(cevent::KeyEventKind::Press)),
            Ok(terminal_drag(5)),
        ])
        .chain(stream::pending());
        let mut events = EventSource::new(terminal, rx);
        let TurnInput::Events(batch) = block_on(events.next_input())? else {
            panic!("the terminal produced input");
        };
        assert_eq!(
            batch_summary(&batch),
            ["Moved@2", "Drag@4", "key", "Drag@5"]
        );
        Ok(())
    }

    #[test]
    fn a_ready_input_batch_takes_a_bounded_number_of_events() -> Result<()> {
        let (_tx, rx) = unbounded();
        let drags = (0..INPUT_BATCH_LIMIT as u16 + 10)
            .map(|column| Ok(terminal_drag(column)))
            .collect::<Vec<_>>();
        let terminal = stream::iter(drags).chain(stream::pending());
        let mut events = EventSource::new(terminal, rx);
        let TurnInput::Events(first) = block_on(events.next_input())? else {
            panic!("the terminal produced input");
        };
        assert_eq!(
            batch_summary(&first),
            [format!("Drag@{INPUT_BATCH_LIMIT}")],
            "the drags collapse to the last one taken"
        );
        let TurnInput::Events(rest) = block_on(events.next_input())? else {
            panic!("the terminal produced input");
        };
        assert_eq!(
            batch_summary(&rest),
            [format!("Drag@{}", INPUT_BATCH_LIMIT + 9)],
            "input past the limit waits for the next batch"
        );
        Ok(())
    }

    /// A terminal stream that, like crossterm's, keeps only the waker from its
    /// first pending poll until an event arrives.
    struct OneWakerStream(Arc<Mutex<(VecDeque<cevent::Event>, Option<Waker>)>>);

    impl Stream for OneWakerStream {
        type Item = io::Result<cevent::Event>;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let mut state = self.0.lock().expect("stream state");
            if let Some(event) = state.0.pop_front() {
                return Poll::Ready(Some(Ok(event)));
            }
            if state.1.is_none() {
                state.1 = Some(cx.waker().clone());
            }
            Poll::Pending
        }
    }

    #[test]
    fn input_after_a_drain_wakes_the_loop() {
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let state = Arc::new(Mutex::new((
                VecDeque::from([terminal_key(cevent::KeyEventKind::Press)]),
                None,
            )));
            let (_tx, rx) = unbounded();
            let mut events = EventSource::new(OneWakerStream(state.clone()), rx);
            // Draining after the first key finds nothing and leaves its waker
            // with the stream, as crossterm does.
            let first = block_on(events.next_input());
            let pusher = thread::spawn(move || {
                thread::sleep(Duration::from_millis(50));
                let mut guard = state.lock().expect("stream state");
                guard.0.push_back(terminal_key(cevent::KeyEventKind::Press));
                if let Some(waker) = guard.1.take() {
                    waker.wake();
                }
            });
            let second = block_on(events.next_input());
            pusher.join().expect("pusher thread");
            let _sent = done_tx.send(
                matches!(first, Ok(TurnInput::Events(_)))
                    && matches!(second, Ok(TurnInput::Events(_))),
            );
        });
        // A regression parks the worker forever, so wait with a timeout and
        // join only a worker that finished.
        let woke = done_rx.recv_timeout(Duration::from_secs(5));
        if woke.is_ok() {
            worker.join().expect("worker thread");
        }
        assert_eq!(
            woke,
            Ok(true),
            "input that arrives after a drain must wake the loop"
        );
    }

    #[test]
    fn event_source_release_does_not_consume_framework_wake() -> Result<()> {
        let (tx, rx) = unbounded();
        tx.unbounded_send(AdapterEvent::Wake).unwrap();
        let terminal = stream::iter([Ok(terminal_key(cevent::KeyEventKind::Release))])
            .chain(stream::pending());
        let mut events = EventSource::new(terminal, rx);
        assert!(matches!(block_on(events.next())?, AdapterEvent::Wake));
        Ok(())
    }

    #[test]
    fn event_source_alternates_ready_terminal_and_internal_events() -> Result<()> {
        let (tx, rx) = unbounded();
        tx.unbounded_send(AdapterEvent::Wake).unwrap();
        tx.unbounded_send(AdapterEvent::Wake).unwrap();
        let terminal = stream::iter([
            Ok(terminal_key(cevent::KeyEventKind::Press)),
            Ok(terminal_key(cevent::KeyEventKind::Press)),
        ])
        .chain(stream::pending());
        let mut events = EventSource::new(terminal, rx);
        assert!(matches!(
            block_on(events.next())?,
            AdapterEvent::Input(Event::Key(_))
        ));
        assert!(matches!(block_on(events.next())?, AdapterEvent::Wake));
        assert!(matches!(
            block_on(events.next())?,
            AdapterEvent::Input(Event::Key(_))
        ));
        assert!(matches!(block_on(events.next())?, AdapterEvent::Wake));
        Ok(())
    }
}
