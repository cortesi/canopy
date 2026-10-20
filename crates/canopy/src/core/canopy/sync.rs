//! Points at which automation that drives the terminal can synchronize with
//! the runtime: the input events it has dispatched, and APC strings written
//! to the terminal after a frame.
//!
//! A host that sends keys through a terminal waits until the input count
//! rises by the keys it sent. To read the screen, it queues an APC string
//! that its terminal recognizes. Terminal output is ordered, so once the
//! terminal has read the string, it has read every frame before it.

use std::mem;

use super::Canopy;
use crate::error::{Error, Result};

/// Longest APC payload, in bytes.
const APC_LIMIT: usize = 256;
/// Most APC strings that wait for the next frame.
const APC_QUEUE: usize = 64;

/// Progress that automation observes through a terminal.
#[derive(Debug, Default)]
pub(super) struct TerminalSync {
    /// Input events taken since the instance started.
    inputs: u64,
    /// Whether a terminal adapter draws this instance and writes its APC
    /// strings.
    attached: bool,
    /// APC payloads that wait to follow the next frame.
    apcs: Vec<String>,
}

impl Canopy {
    /// Return how many input events the runtime has taken from its input,
    /// such as a terminal's, a count that only rises. A host that sends input
    /// through a terminal waits for the count to rise by the events it sent.
    /// An event counts whether a widget, a binding, or the interrupt hook
    /// takes it. Consecutive pointer moves that arrive together count once,
    /// and keys a script sends do not count.
    pub fn input_count(&self) -> u64 {
        self.terminal_sync.inputs
    }

    /// Count one input event taken from the runtime's input.
    pub(crate) fn count_input(&mut self) {
        self.terminal_sync.inputs += 1;
    }

    /// Queue an APC string, `ESC _ payload ESC \`, to follow the frame of
    /// this turn on the terminal, and return whether a terminal adapter will
    /// write it. Without one, as in headless automation, nothing is queued.
    ///
    /// The payload is 1 to 256 bytes of printable ASCII, so it cannot end the
    /// string early or reach the terminal as a control.
    pub fn queue_terminal_apc(&mut self, payload: &str) -> Result<bool> {
        if payload.is_empty()
            || payload.len() > APC_LIMIT
            || !payload
                .bytes()
                .all(|byte| byte.is_ascii_graphic() || byte == b' ')
        {
            return Err(Error::Invalid(format!(
                "an APC payload is 1 to {APC_LIMIT} bytes of printable ASCII"
            )));
        }
        if !self.terminal_sync.attached {
            return Ok(false);
        }
        if self.terminal_sync.apcs.len() >= APC_QUEUE {
            return Err(Error::Invalid(format!(
                "{APC_QUEUE} APC strings already wait for the next frame"
            )));
        }
        self.terminal_sync.apcs.push(payload.to_owned());
        Ok(true)
    }

    /// Record that a terminal adapter draws this instance and writes its APC
    /// strings.
    pub(crate) fn attach_terminal(&mut self) {
        self.terminal_sync.attached = true;
    }

    /// Take the APC payloads that wait for the terminal, oldest first.
    pub(crate) fn take_terminal_apcs(&mut self) -> Vec<String> {
        mem::take(&mut self.terminal_sync.apcs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CanopyBuilder,
        geom::Size,
        input::{Event, key::Key},
        runtime::TurnInput,
    };

    #[test]
    fn each_dispatched_input_event_counts_once() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        canopy.set_screen_size(Size::new(10, 2))?;
        canopy.turn(TurnInput::Prepare)?;
        assert_eq!(canopy.input_count(), 0);
        let keys = ['a', 'b', 'c'].map(|key| Event::Key(Key::from(key)));
        canopy.turn(TurnInput::Events(keys.to_vec()))?;
        assert_eq!(canopy.input_count(), 3);
        canopy.turn(TurnInput::Wake)?;
        assert_eq!(canopy.input_count(), 3, "a wake dispatches no input");
        Ok(())
    }

    #[test]
    fn a_wait_on_the_input_count_resumes_after_input() -> Result<()> {
        use crate::{commands::ArgValue, script::EvalRequest};
        let mut canopy = CanopyBuilder::new().build()?;
        canopy.set_screen_size(Size::new(10, 2))?;
        canopy.turn(TurnInput::Prepare)?;
        let mut outcome = canopy.turn(TurnInput::StartEval(EvalRequest {
            source: "canopy.wait_for(function() return canopy.input_count() >= 1 end) \
                     return canopy.input_count()"
                .into(),
            timeout: None,
        }))?;
        for _ in 0..4 {
            assert!(outcome.completed.is_empty(), "the wait parks until input");
            outcome = canopy.turn(TurnInput::Wake)?;
        }
        outcome = canopy.turn(TurnInput::Events(vec![Event::Key(Key::from('x'))]))?;
        for _ in 0..4 {
            if !outcome.completed.is_empty() {
                break;
            }
            outcome = canopy.turn(TurnInput::Wake)?;
        }
        assert!(
            matches!(
                outcome.completed.first().map(|done| &done.result),
                Some(Ok(ArgValue::Int(1)))
            ),
            "{:?}",
            outcome.completed.first().map(|done| &done.result)
        );
        Ok(())
    }

    #[test]
    fn apc_strings_queue_only_for_a_terminal_and_only_printable() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        assert!(
            !canopy.queue_terminal_apc("itty:mark:1")?,
            "headless queues nothing"
        );
        canopy.attach_terminal();
        assert!(canopy.queue_terminal_apc("itty:mark:1")?);
        assert!(canopy.queue_terminal_apc("itty:mark:2")?);
        for bad in ["", "esc\u{1b}\\", "bell\u{7}", "é"] {
            assert!(canopy.queue_terminal_apc(bad).is_err(), "{bad:?}");
        }
        assert!(
            canopy
                .queue_terminal_apc(&"x".repeat(APC_LIMIT + 1))
                .is_err()
        );
        assert_eq!(canopy.take_terminal_apcs(), ["itty:mark:1", "itty:mark:2"]);
        assert!(canopy.take_terminal_apcs().is_empty());
        Ok(())
    }
}
