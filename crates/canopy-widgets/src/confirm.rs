//! A modal question with a yes or no answer.

use canopy::{
    Canopy, Context, ContextExt, EventOutcome, FocusDirection, FocusScope, Loader, NodeId,
    NodeName, Render, ViewContext, Widget,
    commands::{CommandCall, CommandStatus, CommandTarget},
    derive_commands,
    error::{Error, Result},
    event::Event,
    geom::Size,
    layout::{
        Align, CanvasContext, Direction, Edges, Layout, LayoutOverride, MeasureConstraints,
        Measurement, Sizing,
    },
    text,
};

use crate::{Button, Container, boxed::ROUND, frame::Frame};

/// Columns of blank between the body's text and each of its sides.
const SIDE_PADDING: u32 = 1;
/// Columns a row spends on its blank lead column and a trailing gutter.
const ROW_PADDING: u32 = SIDE_PADDING * 2;
/// Rows and columns left around the frame, so the view shows through.
const FRAME_MARGIN: u32 = 1;
/// Columns between the buttons.
const BUTTON_GAP: u32 = 2;
/// Columns a button spends on its border and the space inside it.
const BUTTON_PADDING: u32 = 4;
/// Rows a button occupies, border included.
const BUTTON_ROWS: u32 = 3;
/// Rows the body keeps above the buttons: the message and a blank line.
const MESSAGE_ROWS: u32 = 2;

/// Default answer bindings exposed through `confirm.default_bindings()`.
///
/// `y` and `n` answer whichever button holds focus, because an accelerator
/// names an answer rather than a focus. The arrows and tabs move between the
/// answers inside the dialog alone, so neither reaches the application behind
/// it. A dialog inside an exclusive modal admits none of these, and its owner
/// installs the same records in its own group.
const DEFAULT_BINDINGS: &str = r#"
canopy.keymap({
    path = "**/confirm/**/",
    { key = "y", description = "Yes", action = command.confirm.yes() },
    { key = "n", description = "No", action = command.confirm.no() },
    { key = "Left", description = "Previous answer", action = command.confirm.focus("Left") },
    { key = "Right", description = "Next answer", action = command.confirm.focus("Right") },
    { key = "Tab", description = "Next answer", action = command.confirm.focus("Next") },
    { key = "BackTab", description = "Previous answer", action = command.confirm.focus("Prev") },
})
"#;

/// One of the two answers a question takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Answer {
    /// Agree to the question.
    Yes,
    /// Decline it, which is where a dialog opens unless a host says otherwise.
    #[default]
    No,
}

/// Return the columns a button with `label` occupies.
fn button_width(label: &str) -> u32 {
    u32::try_from(text::display_width(label))
        .unwrap_or(u32::MAX)
        .saturating_add(BUTTON_PADDING)
}

/// Return the columns every button and the gap between them occupy.
fn buttons_width() -> u32 {
    button_width("yes")
        .saturating_add(button_width("no"))
        .saturating_add(BUTTON_GAP)
}

/// Return whether `width` leaves the answers a column off centre.
///
/// The answers are centred as a group, so a remainder that will not halve
/// evenly puts one more column on one side than on the other.
fn off_centre(width: u32) -> bool {
    width.saturating_sub(buttons_width()) % 2 == 1
}

/// Return `width` widened so the answers sit the same distance from each side.
///
/// Spending a column is better than taking one, because the width that holds
/// the answers holds the message too, and narrowing the body to centre them
/// would clip a message that fits.
fn centred_width(width: u32) -> u32 {
    if off_centre(width) {
        width.saturating_add(1)
    } else {
        width
    }
}

/// A centred modal asking a yes or no question.
///
/// The dialog centres a titled frame over whatever it covers, states its
/// question, and offers it as two buttons. It holds no answer: a host supplies
/// the command each button runs, and keeps whatever agreeing to it does,
/// including closing the dialog.
///
/// Open it inside a modal scope with [`Confirm::initial_focus`] as the initial
/// focus, and the scope dims what the dialog covers and gives it the keyboard.
pub struct Confirm {
    /// The question, once mounted.
    body: Option<NodeId>,
    /// The affirmative answer, once mounted.
    yes: Option<NodeId>,
    /// The negative answer, once mounted.
    no: Option<NodeId>,
    /// Answer that takes focus each time the dialog opens.
    default_answer: Answer,
}

impl Default for Confirm {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl Confirm {
    /// Build an empty dialog.
    pub fn new() -> Self {
        Self {
            body: None,
            yes: None,
            no: None,
            default_answer: Answer::default(),
        }
    }

    /// Ask `message` under `title`.
    pub fn ask(&mut self, context: &mut dyn Context, title: &str, message: &str) -> Result<()> {
        let body = self.body()?;
        let title = title.to_owned();
        let message = message.to_owned();
        context.with_widget_mut(body, |body: &mut ConfirmBody, context| {
            body.show(context, message);
            Ok(())
        })?;
        let frame = context
            .parent_of(body)
            .ok_or_else(|| Error::NotFound("confirm frame".to_string()))?;
        context.with_widget_mut(frame, |frame: &mut Frame, _| {
            frame.set_title(title);
            Ok(())
        })
    }

    /// Set what each answer does, before the dialog opens.
    ///
    /// Each command is stored on its own button, so a click, a key, and an
    /// accelerator all run the same one. The dialog decides nothing: closing it
    /// is part of what a host's answer command does.
    pub fn set_actions(
        &mut self,
        context: &mut dyn Context,
        yes: CommandCall,
        no: CommandCall,
    ) -> Result<()> {
        for (answer, command) in [(Answer::Yes, yes), (Answer::No, no)] {
            let button = self.answer(answer)?;
            context.with_widget_mut(button, |button: &mut Button, _| {
                button.set_command(command);
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Choose the answer that takes focus each time the dialog opens.
    ///
    /// Declining is the default, because a question worth asking is one whose
    /// affirmative answer should not be reached by accident.
    pub fn set_default_answer(&mut self, answer: Answer) {
        self.default_answer = answer;
    }

    /// Return the question, or an error before it mounts.
    ///
    /// This is the dialog's body, not its initial focus. Use
    /// [`Confirm::initial_focus`] when opening a modal scope.
    pub fn body(&self) -> Result<NodeId> {
        self.body
            .ok_or_else(|| Error::NotFound("confirm body".to_string()))
    }

    /// Return the answer that takes the keyboard when the dialog opens, or an
    /// error before it mounts.
    pub fn initial_focus(&self) -> Result<NodeId> {
        self.answer(self.default_answer)
    }

    /// Return one answer's button, or an error before it mounts.
    fn answer(&self, answer: Answer) -> Result<NodeId> {
        match answer {
            Answer::Yes => self.yes,
            Answer::No => self.no,
        }
        .ok_or_else(|| Error::NotFound("confirm answer".to_string()))
    }

    /// Give the affirmative answer, whichever answer holds focus.
    #[command(enabled = "yes_status")]
    pub fn yes(&mut self, context: &mut dyn Context) -> Result<()> {
        self.press(context, Answer::Yes)
    }

    /// Give the negative answer, whichever answer holds focus.
    #[command(enabled = "no_status")]
    pub fn no(&mut self, context: &mut dyn Context) -> Result<()> {
        self.press(context, Answer::No)
    }

    /// Move focus between the answers.
    ///
    /// The scope is the dialog, so focus never leaves it for the application
    /// the dialog covers.
    /// @param direction Which way to move.
    #[command]
    pub fn focus(&mut self, context: &mut dyn Context, direction: FocusDirection) -> Result<()> {
        context.focus_move(FocusScope::Node(context.node_id()), direction)?;
        Ok(())
    }

    /// Activate one answer's button.
    fn press(&self, context: &mut dyn Context, answer: Answer) -> Result<()> {
        let button = self.answer(answer)?;
        context.dispatch(
            CommandTarget::Exact(button),
            &Button::call_press().invocation(),
        )?;
        Ok(())
    }

    /// Report the affirmative button's eligibility as this command's own.
    fn yes_status(&self, context: &dyn ViewContext) -> Result<CommandStatus> {
        self.answer_status(context, Answer::Yes)
    }

    /// Report the negative button's eligibility as this command's own.
    fn no_status(&self, context: &dyn ViewContext) -> Result<CommandStatus> {
        self.answer_status(context, Answer::No)
    }

    /// Report one answer button's own eligibility.
    ///
    /// A key and a click on the same answer therefore agree about whether that
    /// answer can be given, and discovery describes both the same way.
    fn answer_status(&self, context: &dyn ViewContext, answer: Answer) -> Result<CommandStatus> {
        let Ok(button) = self.answer(answer) else {
            return Ok(CommandStatus::Disabled(
                "the dialog has not mounted".to_string(),
            ));
        };
        context.command_status(
            CommandTarget::Exact(button),
            &Button::call_press().invocation(),
        )
    }
}

impl Loader for Confirm {
    fn load(canopy: &mut Canopy) -> Result<()> {
        Button::load(canopy)?;
        canopy.add_commands::<Self>()?;
        canopy.register_default_bindings("confirm", DEFAULT_BINDINGS)
    }
}

impl Widget for Confirm {
    fn layout(&self) -> Layout {
        // A stack centres the frame over what the dialog covers, and the margin
        // keeps that visible around it.
        Layout::fill()
            .direction(Direction::Stack)
            .align_center()
            .padding(Edges::all(FRAME_MARGIN))
    }

    fn render(&mut self, render: &mut Render, _context: &dyn ViewContext) -> Result<()> {
        render.push_layer("confirm");
        Ok(())
    }

    fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
        let root = context.node_id();
        let frame = context.add_child_to(root, Frame::new())?;
        // The frame fits the question rather than filling the view, so the
        // dialog is only as large as what it asks.
        context.set_layout_override_of(
            frame.into(),
            LayoutOverride {
                width: Some(Sizing::Measure),
                height: Some(Sizing::Measure),
                ..LayoutOverride::new()
            },
        )?;
        let body = context.add_child_to(frame, ConfirmBody::new())?;
        // One row holds both answers, centred as a group, so the gap between
        // them belongs to the dialog rather than to either button.
        let answers = context.add_child_to(
            body,
            Container::new(
                Layout::fill()
                    .direction(Direction::Row)
                    .align_horizontal(Align::Center)
                    .align_vertical(Align::Start)
                    .gap(BUTTON_GAP)
                    .fixed_height(BUTTON_ROWS),
            )
            .with_name("answers"),
        )?;
        // The labels are lower case, because each one spells the key that
        // gives it and an upper-case letter would ask for a shift that the
        // binding does not want.
        for (answer, label, key) in [(Answer::Yes, "yes", 'y'), (Answer::No, "no", 'n')] {
            let button: NodeId = context
                .add_child_to(
                    answers,
                    Button::new(label).with_glyphs(ROUND).with_accelerator(key),
                )?
                .into();
            context.set_layout_of(
                button,
                Layout::fill()
                    .fixed_width(button_width(label))
                    .fixed_height(BUTTON_ROWS),
            )?;
            match answer {
                Answer::Yes => self.yes = Some(button),
                Answer::No => self.no = Some(button),
            }
        }
        self.body = Some(body.into());
        Ok(())
    }

    fn on_event(&mut self, event: &Event, _context: &mut dyn Context) -> Result<EventOutcome> {
        // A click on the margin, the frame, or the gap between the answers
        // belongs to the dialog, not to what it covers and not to an answer.
        match event {
            Event::Mouse(_) => Ok(EventOutcome::Handle),
            _ => Ok(EventOutcome::Ignore),
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("confirm")
    }
}

/// The question a dialog asks, above the row that answers it.
struct ConfirmBody {
    /// What the question is about.
    message: String,
    /// Content width that shows the message and the answers unclipped.
    fitted_width: u32,
}

impl ConfirmBody {
    /// Build an empty question.
    fn new() -> Self {
        Self {
            message: String::new(),
            fitted_width: 0,
        }
    }

    /// Show `message` as the question.
    fn show(&mut self, context: &mut dyn Context, message: String) {
        let widest = u32::try_from(text::display_width(&message)).unwrap_or(u32::MAX);
        // This is the body's content box. The blank columns beside it are the
        // body's own padding, which layout adds around whatever is measured
        // here, so counting them again would spend them twice.
        self.fitted_width = widest.max(buttons_width());
        self.message = message;
        context.invalidate_layout();
    }
}

impl Widget for ConfirmBody {
    fn layout(&self) -> Layout {
        // The message and the blank line under it are the body's own paint, so
        // the padding keeps the answers below them.
        Layout::fill()
            .direction(Direction::Column)
            .padding(Edges::new(MESSAGE_ROWS, SIDE_PADDING, 0, SIDE_PADDING))
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        // The message and the blank line under it are this body's top padding,
        // which layout adds to what is measured here, so the content is the
        // row of answers alone.
        let content = c.clamp_size(Size::new(centred_width(self.fitted_width), BUTTON_ROWS));
        // Widening keeps the message whole, but a view too narrow to grow into
        // has the last word, so an odd remainder there gives a column up.
        let width = if off_centre(content.w) {
            content.w.saturating_sub(1)
        } else {
            content.w
        };
        Measurement::Fixed(Size::new(width, content.h))
    }

    fn canvas(&self, view: Size, _context: &CanvasContext) -> Size {
        // The dialog never scrolls: it shrinks its message to fit instead.
        view
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        // The padding that holds the answers down is layout, so the message
        // measures from the outer rect and sits above it.
        let area = context.view().outer_rect_local();
        render.fill("background", area, ' ')?;
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        // A message is identified by its tail, which is what a path needs, so
        // one too wide loses its head.
        let budget = (area.w as usize).saturating_sub(ROW_PADDING as usize);
        let message = text::truncate_start(&self.message, budget);
        render.text("confirm/message", area.line(0)?, &format!(" {message}"))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("confirm_body")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        ModalBindings, ModalOptions,
        commands::CommandStatus,
        event::{key, mouse},
        geom::PointI32,
        testing::harness::Harness,
    };

    use super::*;

    /// A host that opens one dialog and records the answer it was given.
    #[derive(Default)]
    struct Host {
        /// The dialog, once mounted.
        dialog: Option<NodeId>,
        /// Answers the host was given, in order.
        answered: Vec<Answer>,
        /// Whether the affirmative answer reports itself eligible.
        yes_enabled: bool,
    }

    #[derive_commands]
    impl Host {
        fn yes_status(&self, _context: &dyn ViewContext) -> Result<CommandStatus> {
            Ok(if self.yes_enabled {
                CommandStatus::Enabled
            } else {
                CommandStatus::Disabled("not yet".into())
            })
        }

        /// Accept, and close the dialog.
        #[command(enabled = "yes_status")]
        fn accept(&mut self, context: &mut dyn Context) -> Result<()> {
            self.answered.push(Answer::Yes);
            self.close(context)
        }

        /// Decline, and close the dialog.
        #[command]
        fn decline(&mut self, context: &mut dyn Context) -> Result<()> {
            self.answered.push(Answer::No);
            self.close(context)
        }

        /// Hide the dialog the way closing a modal scope would.
        fn close(&self, context: &mut dyn Context) -> Result<()> {
            let dialog = self.dialog()?;
            context.set_hidden_of(dialog, true)?;
            Ok(())
        }

        /// Return the dialog, or an error before it mounts.
        fn dialog(&self) -> Result<NodeId> {
            self.dialog
                .ok_or_else(|| Error::NotFound("dialog".to_string()))
        }
    }

    impl Widget for Host {
        fn layout(&self) -> Layout {
            Layout::fill().direction(Direction::Stack)
        }

        fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
            let owner = context.node_id();
            let dialog = context.add_child(Confirm::new())?;
            self.dialog = Some(dialog.into());
            context.with_widget_mut(dialog, |confirm: &mut Confirm, context| {
                confirm.set_actions(
                    context,
                    Self::call_accept().with_target(CommandTarget::Exact(owner)),
                    Self::call_decline().with_target(CommandTarget::Exact(owner)),
                )
            })
        }

        fn name(&self) -> NodeName {
            NodeName::convert("host")
        }
    }

    impl Loader for Host {
        fn load(canopy: &mut Canopy) -> Result<()> {
            Confirm::load(canopy)?;
            canopy.add_commands::<Self>()
        }
    }

    /// Build a host asking `message`, with both answers installed and drawn.
    fn dialog(message: &str, width: u32, height: u32) -> Result<Harness> {
        let mut harness = Harness::builder(Host {
            yes_enabled: true,
            ..Host::default()
        })
        .bindings(
            "dialog-defaults",
            "button.default_bindings()\nconfirm.default_bindings()",
        )
        .size(width, height)
        .build()?;
        harness.render()?;
        ask(&mut harness, message)?;
        Ok(harness)
    }

    /// Ask `message` and focus the dialog's default answer.
    fn ask(harness: &mut Harness, message: &str) -> Result<()> {
        let focus = harness.with_root_context(|host: &mut Host, context| {
            let dialog = host.dialog()?;
            context.set_hidden_of(dialog, false)?;
            context.with_widget_mut(dialog, |confirm: &mut Confirm, context| {
                confirm.ask(context, "Bookmark", message)?;
                confirm.initial_focus()
            })
        })?;
        harness.canopy.with_root_context(|context| {
            context.set_focus(focus)?;
            Ok(())
        })?;
        harness.render()
    }

    /// Return the answers the host has been given.
    fn answered(harness: &mut Harness) -> Vec<Answer> {
        harness.with_root_widget(|host: &mut Host| host.answered.clone())
    }

    /// Return one answer's button node.
    fn answer_node(harness: &mut Harness, answer: Answer) -> Result<NodeId> {
        let dialog = harness.with_root_context(|host: &mut Host, _| host.dialog())?;
        harness.canopy.with_context(dialog, |context| {
            context.with_widget_mut(dialog, |confirm: &mut Confirm, _| confirm.answer(answer))
        })
    }

    /// Click the top-left cell of `node`.
    fn click(harness: &mut Harness, node: NodeId) -> Result<()> {
        let location = harness
            .canopy
            .with_root_view(|context| context.view_of(node).expect("live node").outer.tl);
        harness.mouse(mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location,
        })
    }

    #[test]
    fn the_dialog_states_its_question_and_frames_its_answers() -> Result<()> {
        let harness = dialog("/tmp/alpha/notes.txt", 50, 14)?;
        let screen = harness.tbuf().lines().join("\n");
        assert!(screen.contains("Bookmark"), "the frame carries the title");
        assert!(
            screen.contains("/tmp/alpha/notes.txt"),
            "the question shows its message"
        );
        // Each button is framed, so the button row carries its own corners
        // besides the dialog frame's.
        let corners = screen.matches('\u{256d}').count();
        assert_eq!(
            corners, 3,
            "the dialog and both buttons are framed, got {screen}"
        );
        assert!(
            screen.contains("\u{2502} yes \u{2502}"),
            "the affirmative button keeps both borders, got {screen}"
        );
        assert!(
            screen.contains("\u{2502} no \u{2502}"),
            "the negative button keeps both borders, got {screen}"
        );
        Ok(())
    }

    #[test]
    fn the_answers_are_centred_and_the_dialog_keeps_no_spare_rows() -> Result<()> {
        for (message, width) in [
            ("/tmp/alpha/notes.txt", 50),
            // An odd remainder would otherwise sit the group off to one side.
            ("/tmp/alpha/note.txt", 50),
            ("/tmp/a", 40),
            // A message wider than the view is clamped, and centres anyway.
            ("/tmp/a-rather-long-path/that/keeps/going/notes.txt", 30),
            ("/tmp/a-rather-long-path/that/keeps/going/notes.txt", 31),
        ] {
            let mut harness = dialog(message, width, 14)?;
            let frame = harness
                .find_nodes("**/confirm/**/frame")?
                .first()
                .copied()
                .expect("the dialog is framed");
            let yes = answer_node(&mut harness, Answer::Yes)?;
            let no = answer_node(&mut harness, Answer::No)?;
            let rect = |node| {
                harness
                    .canopy
                    .with_root_view(|context| context.view_of(node).expect("live node").outer)
            };
            let bounds = |node| {
                let outer = rect(node);
                let w = i32::try_from(outer.w).unwrap_or(0);
                let h = i32::try_from(outer.h).unwrap_or(0);
                (outer.tl.x, outer.tl.x + w, outer.tl.y + h)
            };
            let (frame_left, frame_right, frame_bottom) = bounds(frame);
            let (yes_left, _, yes_bottom) = bounds(yes);
            let (_, no_right, _) = bounds(no);

            // The border is one row, so answers that end where the frame's own
            // bottom row starts leave nothing blank under them.
            assert_eq!(
                yes_bottom,
                frame_bottom - 1,
                "the answers reach the bottom border, asking {message:?} in {width}"
            );
            assert_eq!(
                yes_left - frame_left - 1,
                frame_right - 1 - no_right,
                "the answers are centred, asking {message:?} in {width}"
            );
        }
        Ok(())
    }

    #[test]
    fn the_answer_keys_are_highlighted_rather_than_repeated() -> Result<()> {
        let harness = dialog("/tmp/a", 50, 14)?;
        let screen = harness.tbuf().lines().join("\n");
        assert!(
            !screen.contains("y  yes") && !screen.contains("(y)"),
            "the key is not repeated beside its label"
        );

        // The key takes its own style, and the rest of the label does not.
        let style_of = |needle: char| {
            harness
                .canopy
                .snapshot()
                .expect("published dialog")
                .cells
                .iter()
                .find(|cell| cell.ch == needle)
                .map(|cell| (cell.style.fg, cell.style.bg))
                .expect("the label renders")
        };
        assert_ne!(
            style_of('y'),
            style_of('e'),
            "the key stands out from its label"
        );
        assert_ne!(
            style_of('n'),
            style_of('o'),
            "both keys stand out from their labels"
        );
        Ok(())
    }

    #[test]
    fn the_dialog_shares_one_background_with_its_frame() -> Result<()> {
        let harness = dialog("/tmp/a", 50, 14)?;
        let snapshot = harness.canopy.snapshot().expect("published dialog");
        let background = |needle: char| {
            snapshot
                .cells
                .iter()
                .find(|cell| cell.ch == needle)
                .map(|cell| cell.style.bg)
                .expect("the cell renders")
        };
        // The frame's border and title sit on the same ground as the message,
        // and so do the buttons.
        assert_eq!(
            background('\u{256d}'),
            background('B'),
            "the frame shares the dialog's background"
        );
        assert_eq!(
            background('\u{2570}'),
            background('B'),
            "every frame edge shares it"
        );
        assert_eq!(background('y'), background('B'), "a button shares it too");
        Ok(())
    }

    #[test]
    fn a_long_message_loses_its_head_and_a_tiny_view_renders() -> Result<()> {
        let deep = format!("/tmp/{}/notes.txt", "segment/".repeat(12));
        let harness = dialog(&deep, 30, 12)?;
        let screen = harness.tbuf().lines().join("\n");
        assert!(
            screen.contains("notes.txt"),
            "a long message keeps its tail"
        );
        assert!(screen.contains('…'), "a trimmed message is marked");

        for (width, height) in [(4, 3), (1, 1), (12, 4), (20, 6)] {
            let mut harness = dialog("/tmp/a-rather-long-path/notes.txt", width, height)?;
            // A view too small to show an answer must not answer the question
            // by accident, and must not leak the keys to what it covers.
            harness.key('y')?;
            harness.key(key::KeyCode::Enter)?;
            assert_eq!(answered(&mut harness), [Answer::Yes]);
        }
        Ok(())
    }

    #[test]
    fn every_way_of_giving_an_answer_runs_its_own_action() -> Result<()> {
        for (answer, give) in [
            (
                Answer::Yes,
                Box::new(|harness: &mut Harness| harness.key('y'))
                    as Box<dyn Fn(&mut Harness) -> Result<()>>,
            ),
            (
                Answer::No,
                Box::new(|harness: &mut Harness| harness.key('n')),
            ),
            (
                Answer::No,
                Box::new(|harness: &mut Harness| harness.key(key::KeyCode::Enter)),
            ),
            (
                Answer::No,
                Box::new(|harness: &mut Harness| harness.key(' ')),
            ),
        ] {
            let mut harness = dialog("/tmp/a", 40, 12)?;
            give(&mut harness)?;
            assert_eq!(
                answered(&mut harness),
                [answer],
                "exactly one action ran, for the expected answer"
            );
        }

        // A click reaches the answer it landed on, whichever holds focus.
        for answer in [Answer::Yes, Answer::No] {
            let mut harness = dialog("/tmp/a", 40, 12)?;
            let button = answer_node(&mut harness, answer)?;
            click(&mut harness, button)?;
            assert_eq!(answered(&mut harness), [answer]);
        }
        Ok(())
    }

    #[test]
    fn focus_moves_between_the_answers_and_stays_in_the_dialog() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        let yes = answer_node(&mut harness, Answer::Yes)?;
        let no = answer_node(&mut harness, Answer::No)?;
        let focus = |harness: &Harness| harness.canopy.with_root_view(|c| c.focused_node());
        assert_eq!(focus(&harness), Some(no), "the dialog opens on declining");

        for (input, expected) in [
            (key::Key::parse_spec("Left")?, yes),
            // Spatial movement stops at the edge rather than wrapping.
            (key::Key::parse_spec("Left")?, yes),
            (key::Key::parse_spec("Right")?, no),
            (key::Key::parse_spec("Right")?, no),
            // Tab wraps, so it always reaches the other answer.
            (key::Key::parse_spec("Tab")?, yes),
            (key::Key::parse_spec("Tab")?, no),
            (key::Key::parse_spec("BackTab")?, yes),
        ] {
            harness.key(input)?;
            assert_eq!(focus(&harness), Some(expected), "after {input}");
        }

        // The focused answer is the one Enter gives.
        harness.key(key::KeyCode::Enter)?;
        assert_eq!(answered(&mut harness), [Answer::Yes]);
        Ok(())
    }

    #[test]
    fn a_disabled_answer_is_inert_by_key_and_by_click() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        harness.with_root_widget(|host: &mut Host| host.yes_enabled = false);
        let yes = answer_node(&mut harness, Answer::Yes)?;

        harness.key('y')?;
        click(&mut harness, yes)?;
        assert!(
            answered(&mut harness).is_empty(),
            "a disabled answer runs nothing"
        );

        // The reason stays reachable, and declining still works.
        let dialog_node = harness.with_root_context(|host: &mut Host, _| host.dialog())?;
        let status = harness.canopy.with_context(dialog_node, |context| {
            context.with_widget_mut(dialog_node, |confirm: &mut Confirm, context| {
                confirm.yes_status(context)
            })
        })?;
        assert!(matches!(status, CommandStatus::Disabled(_)));
        harness.key('n')?;
        assert_eq!(answered(&mut harness), [Answer::No]);
        Ok(())
    }

    #[test]
    fn a_click_outside_the_answers_does_not_answer() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        let dialog_node = harness.with_root_context(|host: &mut Host, _| host.dialog())?;
        let body = harness.canopy.with_context(dialog_node, |context| {
            context.with_widget_mut(dialog_node, |confirm: &mut Confirm, _| confirm.body())
        })?;
        // The dialog's own margin, its frame corner, and the gap between the
        // answers all belong to the dialog.
        for node in [dialog_node, body] {
            click(&mut harness, node)?;
        }
        let gap = harness.canopy.with_root_view(|context| {
            let view = context.view_of(body).expect("body view");
            PointI32 {
                x: view.outer.tl.x + i32::try_from(view.outer.w).unwrap_or(0) / 2,
                y: view.outer.tl.y + 3,
            }
        });
        harness.mouse(mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: gap,
        })?;
        assert!(answered(&mut harness).is_empty());
        Ok(())
    }

    #[test]
    fn reopening_returns_to_the_configured_default_answer() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        let yes = answer_node(&mut harness, Answer::Yes)?;
        let no = answer_node(&mut harness, Answer::No)?;
        let focus = |harness: &Harness| harness.canopy.with_root_view(|c| c.focused_node());

        harness.key(key::Key::parse_spec("Left")?)?;
        assert_eq!(focus(&harness), Some(yes));
        harness.key('n')?;

        // A second question starts at the default rather than at whichever
        // answer the last one left focused.
        ask(&mut harness, "/tmp/b")?;
        assert_eq!(focus(&harness), Some(no));

        // A host can open on the affirmative answer deliberately.
        let dialog_node = harness.with_root_context(|host: &mut Host, _| host.dialog())?;
        harness.canopy.with_context(dialog_node, |context| {
            context.with_widget_mut(dialog_node, |confirm: &mut Confirm, _| {
                confirm.set_default_answer(Answer::Yes);
                Ok(())
            })
        })?;
        ask(&mut harness, "/tmp/c")?;
        assert_eq!(focus(&harness), Some(yes));
        Ok(())
    }

    #[test]
    fn an_exclusive_modal_keeps_the_question_standing() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        let (owner, dialog_node, focus) =
            harness.with_root_context(|host: &mut Host, context| {
                let dialog = host.dialog()?;
                let focus = context
                    .with_widget_mut(dialog, |confirm: &mut Confirm, _| confirm.initial_focus())?;
                Ok((context.node_id(), dialog, focus))
            })?;
        let group = canopy::FrameworkBindingGroup::new("confirm.test_dialog");
        harness.canopy.with_root_context(|context| {
            context.open_modal(ModalOptions {
                owner,
                modal: dialog_node,
                initial_focus: focus,
                dim_target: None,
                bindings: ModalBindings::Framework(group),
            })?;
            Ok(())
        })?;
        harness.render()?;

        // The group admits nothing, so no application default reaches the
        // dialog and the question stands until its owner binds an answer.
        harness.key('y')?;
        harness.key(key::KeyCode::Enter)?;
        let button = answer_node(&mut harness, Answer::Yes)?;
        click(&mut harness, button)?;
        assert!(
            answered(&mut harness).is_empty(),
            "an exclusive group admits no application binding"
        );
        assert!(
            harness.canopy.available_bindings(None)?.bindings.is_empty(),
            "discovery agrees that nothing is admitted"
        );
        Ok(())
    }
}
