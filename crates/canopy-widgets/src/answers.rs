//! A row of answers: a centred row of buttons, each of which gives one answer.

use std::time::{Duration, Instant};

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, Register, Setup, ViewContext, Widget,
    commands::{CommandCall, CommandStatus, CommandTarget},
    derive_commands,
    error::{Error, Result},
    input::{
        BindingAction, BindingOptions, BindingPhase, BindingTier, Event, FrameworkBindingGroup,
        InputSpec,
        key::{Empty, Key, KeyCode},
        mouse::Mouse,
    },
    layout::{Align, Direction, Layout},
    tree::{FocusDirection, FocusScope},
};

use crate::{Button, ButtonLook};

/// Columns between two answers.
const GAP: u32 = 2;

/// One answer of a row: a label, the key that gives it, and the call it posts.
#[derive(Clone, Debug)]
pub struct Answer {
    /// Label of the button.
    label: String,
    /// Key that gives the answer, in ASCII lower case.
    key: Option<char>,
    /// Call that the answer posts.
    call: CommandCall,
}

impl Answer {
    /// An answer whose key is the first character of `label`, in ASCII lower
    /// case. A label that is empty, or that starts with a character that is
    /// not ASCII alphanumeric, gives no key.
    pub fn new(label: impl Into<String>, call: CommandCall) -> Self {
        let label = label.into();
        let key = label
            .chars()
            .next()
            .filter(char::is_ascii_alphanumeric)
            .map(|key| key.to_ascii_lowercase());
        Self { label, key, call }
    }

    /// Replace the key, or remove it with `None`. The key is stored in ASCII
    /// lower case.
    #[must_use]
    pub fn with_key(mut self, key: Option<char>) -> Self {
        self.key = key.map(|key| key.to_ascii_lowercase());
        self
    }

    /// Return the label of the button.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Return the key that gives the answer, in ASCII lower case.
    pub fn key(&self) -> Option<char> {
        self.key
    }

    /// Return the call that the answer posts.
    pub fn call(&self) -> &CommandCall {
        &self.call
    }

    /// Return whether a typed `character` names this answer. Case does not
    /// count for an ASCII letter.
    fn named_by(&self, character: char) -> bool {
        self.key
            .is_some_and(|key| key.eq_ignore_ascii_case(&character))
    }
}

/// A centred row of buttons, one for each answer.
///
/// Each button posts [`Answers::give`] with its index, aimed at the row, and
/// the row posts the call of the answer. The key of each answer gives it too:
/// the row takes a plain character that names an answer. These keys depend on
/// the labels, so they are not bindings, and help does not list them. Each
/// button marks its key in its label.
///
/// [`Answers::arm_after`] makes the row ignore every answer for a while, so a
/// key pressed for the view below does not answer a question that has just
/// opened. Until then the buttons show as disabled, and the row takes a key
/// that names an answer and does nothing with it.
///
/// The row measures to the width of its buttons and the height of one button,
/// and fills the width it is given, with the buttons centred as a group.
pub struct Answers {
    /// How each button draws.
    look: ButtonLook,
    /// The answers, left to right.
    answers: Vec<Answer>,
    /// The button of each answer, left to right.
    buttons: Vec<NodeId>,
    /// Time from which the answers act, when the row waits to arm.
    armed_at: Option<Instant>,
}

impl Default for Answers {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl Answers {
    /// The framework group of a row of answers: Enter, Space, and a click give
    /// the focused answer, and the arrows move between answers. A dialog
    /// admits it beside its own group.
    pub const BINDINGS: FrameworkBindingGroup = FrameworkBindingGroup::new("answers");

    /// Build an empty row of solid buttons.
    pub fn new() -> Self {
        Self {
            look: ButtonLook::Solid,
            answers: Vec::new(),
            buttons: Vec::new(),
            armed_at: None,
        }
    }

    /// Draw each button in `look`.
    #[must_use]
    pub fn with_look(mut self, look: ButtonLook) -> Self {
        self.look = look;
        self
    }

    /// Show `answers`, left to right, in place of the current ones. `ctx` is
    /// the context of the row. The row is armed until
    /// [`Answers::arm_after`] says otherwise.
    ///
    /// Two answers with the same key are refused, and the row keeps its
    /// current answers.
    pub fn set(&mut self, ctx: &mut dyn Context, answers: Vec<Answer>) -> Result<()> {
        for (index, answer) in answers.iter().enumerate() {
            if let Some(key) = answer.key
                && answers[..index].iter().any(|other| other.key == Some(key))
            {
                return Err(Error::Invalid(format!("two answers take the key {key:?}")));
            }
        }
        for button in self.buttons.drain(..) {
            ctx.remove_subtree(button)?;
        }
        let row = ctx.node_id();
        for (index, answer) in answers.iter().enumerate() {
            let call = Self::call_give(index).with_target(CommandTarget::Exact(row));
            let mut button = Button::new(answer.label.as_str())
                .with_look(self.look)
                .with_command(call);
            if let Some(key) = answer.key {
                button = button.with_accelerator(key);
            }
            self.buttons.push(ctx.add_child(row, button)?.into());
        }
        self.answers = answers;
        self.armed_at = None;
        Ok(())
    }

    /// Return the button of the answer at `index`.
    pub fn button(&self, index: usize) -> Option<NodeId> {
        self.buttons.get(index).copied()
    }

    /// Ignore every answer for `delay` from now. `ctx` is the context of the
    /// row. The buttons show as disabled until then.
    pub fn arm_after(&mut self, ctx: &mut dyn Context, delay: Duration) -> Result<()> {
        self.armed_at = Some(ctx.now() + delay);
        // The poll that ends the wait repaints the buttons as armed.
        ctx.request_poll()
    }

    /// Return the columns that the buttons and the gaps between them take.
    pub(crate) fn width(&self) -> u32 {
        let gaps = GAP.saturating_mul(
            u32::try_from(self.answers.len().saturating_sub(1)).unwrap_or(u32::MAX),
        );
        self.answers
            .iter()
            .map(|answer| {
                Button::new(answer.label.as_str())
                    .with_look(self.look)
                    .size()
                    .w
            })
            .fold(gaps, u32::saturating_add)
    }

    /// Give the answer at `index`: post its call, when the row is armed and
    /// the call is enabled.
    /// @param index Position of the answer, from the left.
    #[command(enabled = "give_status")]
    pub fn give(&mut self, ctx: &mut dyn Context, index: usize) -> Result<()> {
        if !matches!(self.give_status(ctx, index)?, CommandStatus::Enabled) {
            return Ok(());
        }
        let call = self.answers[index].call.clone();
        ctx.post(&call)
    }

    /// Move focus among the answers.
    ///
    /// The scope is the row, so focus never leaves it.
    /// @param direction Which way to move.
    #[command]
    pub fn focus(&self, ctx: &mut dyn Context, direction: FocusDirection) -> Result<()> {
        ctx.focus_move(FocusScope::Node(ctx.node_id()), direction)?;
        Ok(())
    }

    /// Report whether the answer at `index` can be given: the row is armed,
    /// and the call of the answer is enabled.
    fn give_status(&self, ctx: &dyn ViewContext, index: usize) -> Result<CommandStatus> {
        if !self.armed(ctx.now()) {
            return Ok(CommandStatus::Disabled(
                "the answers are not armed yet".into(),
            ));
        }
        match self.answers.get(index) {
            Some(answer) => ctx.command_status(&answer.call),
            None => Ok(CommandStatus::Disabled(format!("no answer {index}"))),
        }
    }

    /// Return whether the answers act at `now`.
    pub(crate) fn armed(&self, now: Instant) -> bool {
        self.armed_at.is_none_or(|at| now >= at)
    }

    /// Return the answer that `key` names, when it is a plain character.
    fn keyed(&self, key: Key) -> Option<usize> {
        let KeyCode::Char(character) = key.key else {
            return None;
        };
        if key.mods != Empty {
            return None;
        }
        self.answers
            .iter()
            .position(|answer| answer.named_by(character))
    }
}

impl Widget for Answers {
    fn layout(&self) -> Layout {
        // The row fills the width it is given, so the buttons centre in it,
        // and a measurement of its parent takes the width of the buttons.
        Layout::fill()
            .direction(Direction::Row)
            .align_horizontal(Align::Center)
            .gap(GAP)
            .fixed_height(self.look.rows())
    }

    fn poll(&mut self, ctx: &mut dyn Context) -> Result<Option<Duration>> {
        Ok(self
            .armed_at
            .map(|at| at.saturating_duration_since(ctx.now()))
            .filter(|wait| !wait.is_zero()))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Key(key) = event else {
            return Ok(EventOutcome::Ignore);
        };
        match self.keyed(*key) {
            // A key that names an answer is the row's even before the row
            // arms, so it never reaches the view below.
            Some(index) => {
                self.give(ctx, index)?;
                Ok(EventOutcome::Handle)
            }
            None => Ok(EventOutcome::Ignore),
        }
    }

    fn key_outcome(&self, key: Key, _ctx: &dyn ViewContext) -> EventOutcome {
        if self.keyed(key).is_some() {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("answers")
    }
}

impl Register for Answers {
    fn register(setup: &mut Setup) -> Result<()> {
        Button::register(setup)?;
        setup.add_commands::<Self>()?;
        let bindings: [(&str, &str, CommandCall); 7] = [
            ("Enter", "Give the focused answer", Button::call_press()),
            ("Space", "Give the focused answer", Button::call_press()),
            ("LeftDown", "Give the answer clicked", Button::call_press()),
            (
                "Left",
                "Previous answer",
                Self::call_focus(FocusDirection::Left),
            ),
            (
                "Right",
                "Next answer",
                Self::call_focus(FocusDirection::Right),
            ),
            (
                "Up",
                "Previous answer",
                Self::call_focus(FocusDirection::Prev),
            ),
            (
                "Down",
                "Next answer",
                Self::call_focus(FocusDirection::Next),
            ),
        ];
        for (input, description, command) in bindings {
            // Activation reaches a button, so it binds on the buttons alone,
            // and a click beside them gives nothing.
            let path = if command == Button::call_press() {
                "**/answers/**/button/**/"
            } else {
                "**/answers/**/"
            };
            let input = if input == "LeftDown" {
                InputSpec::Mouse(Mouse::parse_spec(input)?)
            } else {
                InputSpec::Key(Key::parse_spec(input)?)
            };
            setup.bind(
                input,
                BindingOptions {
                    path: Some(path.parse()?),
                    tier: BindingTier::Framework(Self::BINDINGS),
                    description: description.to_owned(),
                    show_in_help: true,
                    source: None,
                    phase: Some(BindingPhase::AfterWidget),
                },
                BindingAction::Command(command),
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Return the key of an answer labelled `label`.
    fn key_of(label: &str) -> Option<char> {
        Answer::new(label, Answers::call_focus(FocusDirection::Next)).key()
    }

    #[test]
    fn a_label_gives_its_first_character_as_the_key() {
        assert_eq!(key_of("remove"), Some('r'));
        assert_eq!(key_of("Retry"), Some('r'), "the key is in lower case");
        assert_eq!(key_of("2 more"), Some('2'), "a digit is a key");
        assert_eq!(key_of(""), None, "an empty label gives no key");
        assert_eq!(
            key_of("über"),
            None,
            "a label that starts past ASCII gives no key"
        );
        assert_eq!(key_of("(skip)"), None, "punctuation gives no key");
    }

    #[test]
    fn a_host_replaces_or_removes_the_key() {
        let answer = Answer::new(
            "delete the secret too",
            Answers::call_focus(FocusDirection::Next),
        );
        assert_eq!(answer.clone().with_key(Some('S')).key(), Some('s'));
        assert_eq!(answer.clone().with_key(None).key(), None);
        assert_eq!(answer.label(), "delete the secret too");
        assert!(
            answer.with_key(Some('S')).named_by('S'),
            "case does not count"
        );
    }
}
