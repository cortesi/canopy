//! A modal question with labelled answers.

use std::time::Duration;

use canopy::{
    Context, ContextExt, Mount, NodeId, NodeName, Register, Setup, TypedId, ViewContext,
    ViewContextExt, Widget,
    commands::{CommandCall, CommandStatus, CommandTarget},
    derive_commands,
    error::{Error, Result},
    geom::Size,
    input::{
        BindingAction, BindingOptions, BindingPhase, BindingTier, FrameworkBindingGroup, InputSpec,
        ModalBindings, ModalOptions, ModalToken, key::Key,
    },
    layout::{CanvasContext, Direction, Edges, Layout, MeasureConstraints, Measurement},
    render::Render,
    text::{self, WrapOptions},
    tree::{FocusDirection, FocusScope},
};

use crate::{Answer, Answers, ButtonLook, Dialog, Text, answers, frame::Frame};

/// Columns of blank between the body's text and each of its sides.
const SIDE_PADDING: u32 = 1;
/// Rows between the message and the answers.
const MESSAGE_GAP_ROWS: u32 = 1;
/// Widest message, in columns, unless a request says otherwise.
const MESSAGE_WIDTH: u32 = 64;
/// Tab stop of the message.
const TAB_STOP: usize = 4;
/// Style of the message.
const MESSAGE_STYLE: &str = "message";
/// Layer of a question that reports a failure.
const ERROR_LAYER: &str = "error";

/// How a question shows: plain, or as a failure, with the frame and the title
/// in the error color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    /// The plain dialog.
    #[default]
    Plain,
    /// A failure: the dialog pushes the `error` layer, and the theme draws its
    /// frame and title in red.
    Error,
}

/// Who closes the dialog when an answer is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Closing {
    /// The call of the answer closes the dialog. A call that fails leaves the
    /// question open.
    #[default]
    ByCall,
    /// The dialog closes, then posts the call of the answer.
    BeforeCall,
}

/// What a dialog asks, and its answers.
#[derive(Clone, Debug)]
pub struct ConfirmRequest {
    /// Title on the dialog's frame.
    pub title: String,
    /// The question. Each line starts a paragraph, and a paragraph wraps to
    /// `message_width`.
    pub message: String,
    /// The answers, left to right.
    pub answers: Vec<Answer>,
    /// The answer that Esc gives and that has focus at first. It declines.
    pub decline: usize,
    /// How the dialog shows.
    pub tone: Tone,
    /// Who closes the dialog when an answer is given.
    pub closing: Closing,
    /// Ignore the answers for this long after the dialog opens.
    pub arm_after: Option<Duration>,
    /// Widest message, in columns. The message wraps to it.
    pub message_width: u32,
    /// Node dimmed while the dialog is open.
    pub dim_target: Option<NodeId>,
}

impl ConfirmRequest {
    /// A question with no answers yet, in the plain tone, closed by its calls,
    /// with a message width of 64. Until [`ConfirmRequest::with_declining`]
    /// names another, the first answer declines.
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            answers: Vec::new(),
            decline: 0,
            tone: Tone::Plain,
            closing: Closing::ByCall,
            arm_after: None,
            message_width: MESSAGE_WIDTH,
            dim_target: None,
        }
    }

    /// Add an answer at the right.
    #[must_use]
    pub fn with_answer(mut self, answer: Answer) -> Self {
        self.answers.push(answer);
        self
    }

    /// Add the answer that declines, at the right, and make it the `decline`
    /// answer.
    #[must_use]
    pub fn with_declining(mut self, answer: Answer) -> Self {
        self.decline = self.answers.len();
        self.answers.push(answer);
        self
    }

    /// A `yes` and a `no` answer, where `no` declines.
    pub fn yes_no(
        title: impl Into<String>,
        message: impl Into<String>,
        yes: CommandCall,
        no: CommandCall,
    ) -> Self {
        Self::new(title, message)
            .with_answer(Answer::new("yes", yes))
            .with_declining(Answer::new("no", no))
    }

    /// Show the dialog in `tone`.
    #[must_use]
    pub fn with_tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Let `closing` say who closes the dialog.
    #[must_use]
    pub fn with_closing(mut self, closing: Closing) -> Self {
        self.closing = closing;
        self
    }

    /// Ignore the answers for `delay` after the dialog opens.
    #[must_use]
    pub fn with_arm_after(mut self, delay: Duration) -> Self {
        self.arm_after = Some(delay);
        self
    }

    /// Wrap the message to at most `columns`.
    #[must_use]
    pub fn with_message_width(mut self, columns: u32) -> Self {
        self.message_width = columns;
        self
    }

    /// Dim `target` while the dialog is open.
    #[must_use]
    pub fn with_dim_target(mut self, target: NodeId) -> Self {
        self.dim_target = Some(target);
        self
    }
}

/// A centred modal that asks a question and offers labelled answers.
///
/// The dialog centres a titled frame over whatever it covers, states its
/// question, and offers its answers as a row of [`Answers`]. The key of each
/// answer gives it, Esc gives the declining answer, and the declining answer
/// has focus when the dialog opens. A host opens it with [`Confirm::open`].
///
/// Each answer posts its call. Under [`Closing::ByCall`] that call closes the
/// dialog; under [`Closing::BeforeCall`] the dialog closes first.
pub struct Confirm {
    /// How each answer draws, and the parts of the dialog once mounted.
    parts: Mount<ButtonLook, Parts>,
    /// The latest question that opened. It is open while its modal is.
    question: Option<Question>,
}

/// A question that opened: what it asks, and the modal it opened in.
struct Question {
    /// What the question asks, and its answers.
    request: ConfirmRequest,
    /// The modal of the question.
    token: ModalToken,
}

/// The children a confirm dialog builds when it mounts.
struct Parts {
    /// The frame, which shows the title.
    frame: TypedId<Frame>,
    /// The body.
    body: TypedId<ConfirmBody>,
    /// The message.
    message: TypedId<Text>,
    /// The row of answers.
    row: TypedId<Answers>,
}

impl Default for Confirm {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl Confirm {
    /// The framework group of the dialog: Esc gives the declining answer, and
    /// Tab and Shift-Tab move focus inside the dialog. [`Confirm::open`]
    /// admits it with [`Answers::BINDINGS`].
    pub const BINDINGS: FrameworkBindingGroup = FrameworkBindingGroup::new("confirm");

    /// Build an empty dialog.
    pub fn new() -> Self {
        Self {
            parts: Mount::new(ButtonLook::Solid),
            question: None,
        }
    }

    /// Draw each answer in `look`, in place of [`ButtonLook::Solid`]. The
    /// dialog takes the rows that the look needs. Set it before the dialog
    /// mounts.
    #[must_use]
    pub fn with_look(mut self, look: ButtonLook) -> Self {
        *self.parts.pending_mut() = look;
        self
    }

    /// Open `request` in `confirm`, as a modal of `owner`, and return its
    /// token. A host that asks over another modal names that modal's node as
    /// the owner.
    ///
    /// A request with no answers, a `decline` out of range, or two answers with
    /// one key is refused, as is a request while the dialog is open. An open
    /// that fails, such as one while the earlier question's close waits,
    /// leaves the dialog as it was, with the earlier question.
    pub fn open(
        ctx: &mut dyn Context,
        confirm: TypedId<Self>,
        owner: NodeId,
        request: ConfirmRequest,
    ) -> Result<ModalToken> {
        ctx.with_widget(confirm, |dialog: &Self| dialog.check(ctx, &request))?;
        let mut request = Some(request);
        let mut replaced = None;
        let mut opened = None;
        // The answers and the modal change in one tree edit, so a failure
        // gives the row back the buttons of the earlier question.
        let edit = ctx.edit_structure(&mut |ctx| {
            let request = request
                .take()
                .ok_or_else(|| Error::Invalid("a question opens once".into()))?;
            let focus = ctx.with_widget_mut(confirm, |dialog: &mut Self, ctx| {
                replaced = Some(dialog.answers_shown(ctx)?);
                dialog.show(ctx, &request)
            })?;
            let token = ctx.open_modal(ModalOptions {
                owner,
                modal: confirm.into(),
                initial_focus: focus,
                dim_target: request.dim_target,
                bindings: ModalBindings::Framework {
                    groups: &[Self::BINDINGS, Answers::BINDINGS],
                    intents: &[],
                },
            })?;
            ctx.update(confirm, |dialog: &mut Self| {
                dialog.question = Some(Question { request, token })
            })?;
            opened = Some(token);
            Ok(())
        });
        if let Err(error) = edit {
            // A question that opened stays open with its parts, whatever
            // failed after it. One that did not open rolled back the buttons,
            // and the dialog shows the earlier question again.
            if opened.is_none()
                && let Some(shown) = replaced
            {
                ctx.with_widget_mut(confirm, |dialog: &mut Self, ctx| dialog.restore(ctx, shown))?;
            }
            return Err(error);
        }
        opened.ok_or_else(|| Error::Invalid("the question did not open".into()))
    }

    /// Return whether the dialog is open. A dialog that a call closed reads as
    /// closed.
    pub fn is_open(&self, ctx: &dyn ViewContext) -> bool {
        self.open_question(ctx).is_some()
    }

    /// Give the answer at `index`, as the request's `closing` says. A dialog
    /// that is not open gives nothing.
    /// @param index Position of the answer, from the left.
    #[command(enabled = "answer_status")]
    pub fn answer(&mut self, ctx: &mut dyn Context, index: usize) -> Result<()> {
        let Some(question) = self.open_question(ctx) else {
            return Ok(());
        };
        let Some(answer) = question.request.answers.get(index) else {
            return Ok(());
        };
        let call = answer.call().clone();
        if question.request.closing == Closing::BeforeCall {
            ctx.close_modal(question.token)?;
        }
        ctx.post(&call)
    }

    /// Give the declining answer, through the same check as every other
    /// answer.
    #[command(enabled = "decline_status")]
    pub fn decline(&mut self, ctx: &mut dyn Context) -> Result<()> {
        // The row checks and posts the answer once this command returns, so
        // the check can read this dialog.
        let Some(question) = self.open_question(ctx) else {
            return Ok(());
        };
        let decline = question.request.decline;
        let row = CommandTarget::Exact(self.row()?.into());
        ctx.post(&Answers::call_give(decline).with_target(row))
    }

    /// Move focus inside the dialog.
    ///
    /// The scope is the dialog, so focus never leaves it for the application
    /// the dialog covers.
    /// @param direction Which way to move.
    #[command]
    pub fn focus(&self, ctx: &mut dyn Context, direction: FocusDirection) -> Result<()> {
        ctx.focus_move(FocusScope::Node(ctx.node_id()), direction)?;
        Ok(())
    }

    /// Report the status of the call of the answer at `index`, so a disabled
    /// answer stays inert by key and by click. A dialog that is not open
    /// takes no answer.
    fn answer_status(&self, ctx: &dyn ViewContext, index: usize) -> Result<CommandStatus> {
        let Some(question) = self.open_question(ctx) else {
            return Ok(CommandStatus::Disabled("no question is open".into()));
        };
        match question.request.answers.get(index) {
            Some(answer) => ctx.command_status(answer.call()),
            None => Ok(CommandStatus::Disabled(format!("no answer {index}"))),
        }
    }

    /// Report the status of the declining answer in the row: disabled before
    /// the row arms, and while the declining call is disabled.
    fn decline_status(&self, ctx: &dyn ViewContext) -> Result<CommandStatus> {
        let Ok(&Parts { row, .. }) = self.parts.mounted() else {
            return Ok(CommandStatus::Disabled("the dialog has not mounted".into()));
        };
        // The check of the row reads this dialog, which a dispatch holds, so
        // the two halves of the check run here.
        let now = ctx.now();
        let armed = ctx.with_widget(row, |row: &Answers| Ok(row.armed(now)))?;
        if !armed {
            return Ok(CommandStatus::Disabled(
                "the answers are not armed yet".into(),
            ));
        }
        let Some(question) = self.open_question(ctx) else {
            return Ok(CommandStatus::Disabled("no question is open".into()));
        };
        self.answer_status(ctx, question.request.decline)
    }

    /// Return the question, while its modal is open.
    fn open_question(&self, ctx: &dyn ViewContext) -> Option<&Question> {
        self.question
            .as_ref()
            .filter(|question| ctx.modal_is_open(question.token))
    }

    /// Return the row of answers, or an error before it mounts.
    fn row(&self) -> Result<TypedId<Answers>> {
        Ok(self.parts.mounted()?.row)
    }

    /// Refuse `request` while the dialog is open, and a request with no
    /// answers or with a `decline` out of range.
    fn check(&self, ctx: &dyn ViewContext, request: &ConfirmRequest) -> Result<()> {
        if self.is_open(ctx) {
            return Err(Error::Invalid("the dialog is already open".into()));
        }
        if request.answers.is_empty() {
            return Err(Error::Invalid("a question needs an answer".into()));
        }
        if request.decline >= request.answers.len() {
            return Err(Error::Invalid(format!(
                "the declining answer {} is not one of {} answers",
                request.decline,
                request.answers.len()
            )));
        }
        Ok(())
    }

    /// Return what the row of answers shows.
    fn answers_shown(&self, ctx: &dyn ViewContext) -> Result<answers::Shown> {
        ctx.with_widget(self.row()?, |row: &Answers| Ok(row.shown()))
    }

    /// Show `request`, and return the button that takes the keyboard.
    fn show(&self, ctx: &mut dyn Context, request: &ConfirmRequest) -> Result<NodeId> {
        let row = self.row()?;
        // The row's buttons give the dialog's own answers, so the dialog
        // decides when the modal closes.
        let owner = CommandTarget::Exact(ctx.node_id());
        let shown = request
            .answers
            .iter()
            .enumerate()
            .map(|(index, answer)| {
                Answer::new(answer.label(), Self::call_answer(index).with_target(owner))
                    .with_key(answer.key())
            })
            .collect();
        let focus = ctx.with_widget_mut(row, |row: &mut Answers, ctx| {
            row.set(ctx, shown)?;
            if let Some(delay) = request.arm_after {
                row.arm_after(ctx, delay)?;
            }
            row.button(request.decline)
                .ok_or_else(|| Error::NotFound("declining answer".into()))
        })?;
        self.show_text(ctx, &request.title, &request.message, request.message_width)?;
        Ok(focus)
    }

    /// Put back the row of answers that `shown` holds, after a failed open
    /// rolled back its buttons, with the text of the earlier question.
    fn restore(&self, ctx: &mut dyn Context, shown: answers::Shown) -> Result<()> {
        ctx.update(self.row()?, |row: &mut Answers| row.restore(shown))?;
        match &self.question {
            Some(Question { request, .. }) => {
                self.show_text(ctx, &request.title, &request.message, request.message_width)
            }
            None => self.show_text(ctx, "", "", MESSAGE_WIDTH),
        }
    }

    /// Show `title` on the frame, and `message`, wrapped to `width`, above
    /// the answers the row holds.
    fn show_text(
        &self,
        ctx: &mut dyn Context,
        title: &str,
        message: &str,
        width: u32,
    ) -> Result<()> {
        let &Parts {
            frame,
            body,
            message: text,
            row,
        } = self.parts.mounted()?;
        let row_width = ctx.with_widget(row, |row: &Answers| Ok(row.width()))?;
        ctx.update(frame, |frame: &mut Frame| frame.set_title(title))?;
        ctx.update(text, |text: &mut Text| {
            text.set_text(message);
            text.set_wrap_width(width);
        })?;
        ctx.update(body, |body: &mut ConfirmBody| {
            body.show(message, width, row_width)
        })
    }
}

impl Register for Confirm {
    fn register(setup: &mut Setup) -> Result<()> {
        Answers::register(setup)?;
        setup.add_commands::<Self>()?;
        let bindings: [(&str, &str, CommandCall); 3] = [
            ("Esc", "Decline", Self::call_decline()),
            ("Tab", "Next answer", Self::call_focus(FocusDirection::Next)),
            (
                "BackTab",
                "Previous answer",
                Self::call_focus(FocusDirection::Prev),
            ),
        ];
        for (input, description, command) in bindings {
            setup.bind(
                InputSpec::Key(Key::parse_spec(input)?),
                BindingOptions::new(BindingTier::Framework(Self::BINDINGS), description)
                    .with_path("**/confirm/**/".parse()?)
                    .with_phase(BindingPhase::AfterWidget),
                BindingAction::Command(command),
            )?;
        }
        Ok(())
    }
}

impl Widget for Confirm {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, render: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        render.push_layer("confirm");
        let tone = self
            .question
            .as_ref()
            .map_or(Tone::Plain, |question| question.request.tone);
        if tone == Tone::Error {
            render.push_layer(ERROR_LAYER);
        }
        Ok(())
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let root = ctx.node_id();
        self.parts.mount(|&mut look| {
            // The dialog fits the question, so it is only as large as what it
            // asks.
            let (dialog, body) = Dialog::new().add(ctx, root, ConfirmBody::new(look.rows()))?;
            let frame = Dialog::frame(ctx, dialog.into())?;
            let message = ctx.add_child(
                body,
                Text::new("")
                    .with_style(MESSAGE_STYLE)
                    .with_tab_stop(TAB_STOP)
                    .with_wrap_width(MESSAGE_WIDTH)
                    .with_wrap_options(WrapOptions::PROSE)
                    .with_fit(true),
            )?;
            let row = ctx.add_child(body, Answers::new().with_look(look))?;
            Ok(Parts {
                frame,
                body,
                message,
                row,
            })
        })
    }

    fn on_unmount(&mut self, _ctx: &mut dyn Context) {
        self.parts.unmount();
    }

    fn name(&self) -> NodeName {
        NodeName::convert("confirm")
    }
}

/// The ground of a dialog: the message above the row that answers it.
///
/// The body measures what it holds, so the dialog keeps no spare rows, and
/// widens by a column when that centres the answers.
struct ConfirmBody {
    /// The message.
    message: String,
    /// Widest message, in columns.
    message_width: u32,
    /// Columns the answers take.
    row_width: u32,
    /// Rows the answers take.
    row_rows: u32,
}

impl ConfirmBody {
    /// Build an empty body, above answers that take `row_rows` rows.
    fn new(row_rows: u32) -> Self {
        Self {
            message: String::new(),
            message_width: MESSAGE_WIDTH,
            row_width: 0,
            row_rows,
        }
    }

    /// Measure `message`, wrapped to `message_width`, above answers that take
    /// `row_width` columns.
    fn show(&mut self, message: &str, message_width: u32, row_width: u32) {
        self.message = message.to_string();
        self.message_width = message_width;
        self.row_width = row_width;
    }

    /// Return whether `width` leaves the answers a column off centre.
    ///
    /// The answers are centred as a group, so a remainder that will not halve
    /// evenly puts one more column on one side than on the other.
    fn off_centre(&self, width: u32) -> bool {
        width.saturating_sub(self.row_width) % 2 == 1
    }
}

impl Widget for ConfirmBody {
    fn layout(&self) -> Layout {
        Layout::fill()
            .direction(Direction::Column)
            .gap(MESSAGE_GAP_ROWS)
            .padding(Edges::new(0, SIDE_PADDING, 0, SIDE_PADDING))
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        // The message wraps as its text does: at the narrower of the view and
        // its width. The text then fits the body, and wraps the same way.
        let offered = c.width.limit().unwrap_or(u32::MAX);
        let message = text::expand_tabs(&self.message, TAB_STOP);
        let wrap = |width: u32| text::wrap_with(&message, width.max(1), WrapOptions::PROSE);
        let mut lines = wrap(offered.min(self.message_width));
        let widest = lines
            .iter()
            .map(|line| text::width(line))
            .max()
            .unwrap_or(0);
        // Spending a column is better than taking one, because the width that
        // holds the answers holds the message too, and narrowing the body to
        // centre them would clip a message that fits.
        let mut width = widest.max(self.row_width);
        if self.off_centre(width) {
            width = width.saturating_add(1);
        }
        let width = c.clamp_size(Size::new(width, 0)).w;
        // A view too narrow to grow into has the last word, so an odd
        // remainder there gives a column up, and the message wraps to what is
        // left.
        let width = if self.off_centre(width) {
            width.saturating_sub(1)
        } else {
            width
        };
        if width < widest {
            lines = wrap(width);
        }
        let rows = u32::try_from(lines.len())
            .unwrap_or(u32::MAX)
            .saturating_add(MESSAGE_GAP_ROWS)
            .saturating_add(self.row_rows);
        Measurement::Fixed(Size::new(width, c.clamp_size(Size::new(width, rows)).h))
    }

    fn canvas(&self, view: Size, _ctx: &CanvasContext) -> Size {
        // The dialog never scrolls: it fits its message instead.
        view
    }

    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        render.fill("background", ctx.view().outer_rect_local(), ' ')
    }

    fn name(&self) -> NodeName {
        NodeName::convert("confirm_body")
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use canopy::{
        commands::CommandStatus,
        geom::PointI32,
        input::{FrameworkBindingGroup, ModalBindings, ModalOptions, key, mouse},
        style::{AttrSet, Color, themes},
        testing::{ManualClock, harness::Harness},
    };

    use super::*;
    use crate::{Button, remount_test_support::attach_and_roll_back};

    /// A host that opens one dialog and records the answers it was given.
    #[derive(Default)]
    struct Host {
        /// The dialog, once mounted.
        dialog: Option<TypedId<Confirm>>,
        /// The open question's modal, if one is open.
        token: Option<ModalToken>,
        /// Answers the host was given, in order.
        answered: Vec<String>,
        /// Answers whose calls are disabled.
        disabled: Vec<String>,
        /// How the dialog draws its answers.
        look: ButtonLook,
    }

    #[derive_commands]
    impl Host {
        #[expect(
            clippy::needless_pass_by_value,
            reason = "an enabled hook takes the parameters of its command"
        )]
        fn can_record(&self, _ctx: &dyn ViewContext, answer: String) -> Result<CommandStatus> {
            Ok(if self.disabled.contains(&answer) {
                CommandStatus::Disabled("not yet".into())
            } else {
                CommandStatus::Enabled
            })
        }

        /// Record an answer, and close the dialog.
        /// @param answer The answer.
        #[command(enabled = "can_record")]
        fn record(&mut self, ctx: &mut dyn Context, answer: String) -> Result<()> {
            self.answered.push(answer);
            self.close(ctx)
        }

        /// Record an answer, and leave the dialog open.
        /// @param answer The answer.
        #[command]
        fn note(&mut self, answer: String) {
            self.answered.push(answer);
        }

        /// Close the open question.
        fn close(&mut self, ctx: &mut dyn Context) -> Result<()> {
            if let Some(token) = self.token.take() {
                ctx.close_modal(token)?;
            }
            Ok(())
        }

        /// Return the typed dialog, or an error before it mounts.
        fn dialog(&self) -> Result<TypedId<Confirm>> {
            self.dialog
                .ok_or_else(|| Error::NotFound("dialog".to_string()))
        }
    }

    impl Widget for Host {
        fn layout(&self) -> Layout {
            Layout::fill().direction(Direction::Stack)
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let dialog = ctx.add_child(ctx.node_id(), Confirm::new().with_look(self.look))?;
            ctx.set_hidden(dialog.into(), true)?;
            self.dialog = Some(dialog);
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("host")
        }
    }

    impl Register for Host {
        fn register(setup: &mut Setup) -> Result<()> {
            Confirm::register(setup)?;
            setup.add_commands::<Self>()
        }
    }

    /// Build a host with its dialog mounted and closed, on `clock`.
    fn host_on(width: u32, height: u32, clock: Arc<ManualClock>) -> Result<Harness> {
        host_with(Host::default(), width, height, clock)
    }

    /// Build `host` with its dialog mounted and closed, on `clock`.
    fn host_with(host: Host, width: u32, height: u32, clock: Arc<ManualClock>) -> Result<Harness> {
        let mut harness = Harness::builder(host)
            .register::<Host>()
            .size(width, height)
            .clock(clock)
            .build()?;
        harness.render()?;
        Ok(harness)
    }

    /// Build a host with its dialog mounted and closed.
    fn host(width: u32, height: u32) -> Result<Harness> {
        host_on(width, height, Arc::new(ManualClock::new()))
    }

    /// Return the call that records `answer` on the host at `owner`.
    fn record(owner: CommandTarget, answer: &str) -> CommandCall {
        Host::call_record(answer.to_string()).with_target(owner)
    }

    /// Return the answer `label`, which records its label.
    fn labelled(owner: CommandTarget, label: &str) -> Answer {
        Answer::new(label, record(owner, label))
    }

    /// Open the request that `build` makes for the host's node, closing any
    /// question already open.
    fn open_with(
        harness: &mut Harness,
        build: impl FnOnce(CommandTarget) -> ConfirmRequest,
    ) -> Result<()> {
        // A modal closes after the callback that closes it returns, so the old
        // question closes in its own turn before the new one opens.
        harness.with_root_widget_context(|host: &mut Host, ctx| host.close(ctx))?;
        harness.render()?;
        harness.with_root_widget_context(|host: &mut Host, ctx| {
            let owner = ctx.node_id();
            let request = build(CommandTarget::Exact(owner));
            let token = Confirm::open(ctx, host.dialog()?, owner, request)?;
            host.token = Some(token);
            Ok(())
        })?;
        harness.render()
    }

    /// Ask `message` with the answers `yes` and `no`.
    fn ask(harness: &mut Harness, message: &str) -> Result<()> {
        open_with(harness, |owner| {
            ConfirmRequest::yes_no(
                "Bookmark",
                message,
                record(owner, "yes"),
                record(owner, "no"),
            )
        })
    }

    /// Build a host asking `message`.
    fn dialog(message: &str, width: u32, height: u32) -> Result<Harness> {
        let mut harness = host(width, height)?;
        ask(&mut harness, message)?;
        Ok(harness)
    }

    /// Return the answers the host has been given.
    fn answered(harness: &mut Harness) -> Vec<String> {
        harness.with_root_widget(|host: &mut Host| host.answered.clone())
    }

    /// Return the dialog's node.
    fn dialog_node(harness: &mut Harness) -> Result<NodeId> {
        harness
            .with_root_widget_context(|host: &mut Host, _| host.dialog())
            .map(Into::into)
    }

    /// Read the dialog.
    fn on_dialog<R>(
        harness: &mut Harness,
        f: impl FnOnce(&Confirm, &dyn ViewContext) -> R,
    ) -> Result<R> {
        let node = dialog_node(harness)?;
        harness
            .canopy
            .with_root_view(|ctx| ctx.with_widget(node, |confirm: &Confirm| Ok(f(confirm, ctx))))
    }

    /// Return the button of the answer at `index`.
    fn answer_node(harness: &mut Harness, index: usize) -> Result<NodeId> {
        let row = on_dialog(harness, |confirm, _| confirm.row())??;
        harness
            .canopy
            .with_root_view(|ctx| ctx.with_widget(row, |row: &Answers| Ok(row.button(index))))?
            .ok_or_else(|| Error::NotFound("answer".into()))
    }

    /// Return the node holding the keyboard.
    fn focus(harness: &Harness) -> Option<NodeId> {
        harness.canopy.with_root_view(|ctx| ctx.focused_node())
    }

    /// Return whether the dialog is open.
    fn open(harness: &mut Harness) -> Result<bool> {
        on_dialog(harness, |confirm, ctx| confirm.is_open(ctx))
    }

    /// Click the top-left cell of `node`.
    fn click(harness: &mut Harness, node: NodeId) -> Result<()> {
        let location = harness
            .canopy
            .with_root_view(|ctx| ctx.view_of(node).expect("live node").outer.tl);
        click_at(harness, location)
    }

    /// Click the cell at `location`.
    fn click_at(harness: &mut Harness, location: PointI32) -> Result<()> {
        harness.mouse(mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location,
        })
    }

    /// Return the rows of the screen.
    fn screen(harness: &Harness) -> Vec<String> {
        harness.tbuf().lines()
    }

    /// Return the foreground, background, and attributes of the first cell
    /// that shows `needle`.
    fn style_of(harness: &Harness, needle: char) -> (Color, Color, AttrSet) {
        harness
            .canopy
            .snapshot()
            .expect("published dialog")
            .buffer
            .cells()
            .iter()
            .find(|cell| cell.ch == needle)
            .map(|cell| (cell.style.fg, cell.style.bg, cell.style.attrs))
            .expect("the cell renders")
    }

    #[test]
    fn the_dialog_states_its_question_and_offers_its_answers() -> Result<()> {
        let harness = dialog("/tmp/alpha/notes.txt", 50, 14)?;
        let screen = screen(&harness).join("\n");
        assert!(screen.contains("Bookmark"), "the frame carries the title");
        assert!(
            screen.contains("/tmp/alpha/notes.txt"),
            "the question shows its message"
        );
        // The answers are filled rows, so only the dialog is framed.
        let corners = screen.matches('\u{256d}').count();
        assert_eq!(corners, 1, "only the dialog is framed, got {screen}");
        assert!(
            screen.contains("  yes  ") && screen.contains("  no  "),
            "each answer pads its label on its face, got {screen}"
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
            // A message wider than the view wraps, and centres anyway.
            ("/tmp/a-rather-long-path/that/keeps/going/notes.txt", 30),
            ("/tmp/a-rather-long-path/that/keeps/going/notes.txt", 31),
        ] {
            let mut harness = dialog(message, width, 14)?;
            let frame = harness
                .find_nodes("**/confirm/**/frame")?
                .first()
                .copied()
                .expect("the dialog is framed");
            let yes = answer_node(&mut harness, 0)?;
            let no = answer_node(&mut harness, 1)?;
            let bounds = |node| {
                let outer = harness
                    .canopy
                    .with_root_view(|ctx| ctx.view_of(node).expect("live node").outer);
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
    fn a_dialog_draws_its_answers_in_its_look_and_fits_their_rows() -> Result<()> {
        let host = Host {
            look: ButtonLook::Inset,
            ..Host::default()
        };
        let mut harness = host_with(host, 50, 16, Arc::new(ManualClock::new()))?;
        ask(&mut harness, "/tmp/alpha/notes.txt")?;
        let frame = harness
            .find_nodes("**/confirm/**/frame")?
            .first()
            .copied()
            .expect("the dialog is framed");
        let yes = answer_node(&mut harness, 0)?;
        let (frame_bottom, yes_outer) = harness.canopy.with_root_view(|ctx| {
            let frame = ctx.view_of(frame).expect("live node").outer;
            let yes = ctx.view_of(yes).expect("live node").outer;
            (frame.tl.y + i32::try_from(frame.h).unwrap_or(0), yes)
        });
        assert_eq!(yes_outer.h, ButtonLook::Inset.rows(), "an inset answer");
        assert_eq!(
            yes_outer.tl.y + i32::try_from(yes_outer.h).unwrap_or(0),
            frame_bottom - 1,
            "the dialog takes the rows of its answers, and no more"
        );
        // An inset button draws its edges in half blocks around its label.
        let screen = screen(&harness).join("\n");
        assert!(screen.contains("yes"), "{screen}");
        assert!(
            screen.contains('\u{2584}'),
            "the inset look draws, got {screen}"
        );
        Ok(())
    }

    #[test]
    fn a_message_of_several_lines_shows_each_line_above_the_answers() -> Result<()> {
        let mut harness = dialog("/tmp/alpha\n- first fact\n- second fact", 50, 14)?;
        let screen = screen(&harness);
        let row = |needle: &str| {
            screen
                .iter()
                .position(|line| line.contains(needle))
                .unwrap_or_else(|| panic!("{needle:?} is on screen: {screen:#?}"))
        };
        let (first, second, third) = (row("/tmp/alpha"), row("- first fact"), row("- second fact"));
        assert_eq!((second, third), (first + 1, first + 2), "{screen:#?}");
        let yes = answer_node(&mut harness, 0)?;
        let top = harness
            .canopy
            .with_root_view(|ctx| ctx.view_of(yes).expect("live node").outer.tl.y);
        assert_eq!(
            usize::try_from(top).expect("answer row"),
            third + 2,
            "one blank line separates the message from the answers: {screen:#?}"
        );
        Ok(())
    }

    #[test]
    fn the_answer_keys_are_highlighted_rather_than_repeated() -> Result<()> {
        let harness = dialog("/tmp/a", 50, 14)?;
        let screen = screen(&harness).join("\n");
        assert!(
            !screen.contains("y  yes") && !screen.contains("(y)"),
            "the key is not repeated beside its label"
        );

        // The key takes its own style, and the rest of the label does not.
        let style = |needle| {
            let (fg, bg, _) = style_of(&harness, needle);
            (fg, bg)
        };
        assert_ne!(style('y'), style('e'), "the key stands out from its label");
        assert_ne!(
            style('n'),
            style('o'),
            "both keys stand out from their labels"
        );
        // The dialog opens on `no`, whose face differs from the face of the
        // unfocused answer.
        assert_ne!(
            style('n').1,
            style('y').1,
            "focus changes the face of the answer"
        );
        assert_ne!(
            style('o').1,
            style('e').1,
            "the focused answer takes a face of its own"
        );
        Ok(())
    }

    #[test]
    fn the_dialog_shares_one_background_with_its_frame() -> Result<()> {
        let harness = dialog("/tmp/a", 50, 14)?;
        let background = |needle| style_of(&harness, needle).1;
        // The frame's border and title sit on the same ground as the message.
        // The buttons stand on it with faces of their own.
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
        assert_ne!(background('y'), background('B'), "a button has its face");
        // The padding of a button is part of its face.
        let snapshot = harness.canopy.snapshot().expect("published dialog");
        let cells = snapshot.buffer.cells();
        let label = cells
            .iter()
            .position(|cell| cell.ch == 'n')
            .expect("the label of `no` renders");
        assert_eq!(
            cells[label - 1].style.bg,
            cells[label].style.bg,
            "the padding of a button takes its face"
        );
        Ok(())
    }

    #[test]
    fn a_long_message_wraps_whole_and_a_tiny_view_renders() -> Result<()> {
        let deep = format!("/tmp/{}/notes.txt", "segment/".repeat(12));
        let harness = dialog(&deep, 30, 16)?;
        let screen = screen(&harness).join("\n");
        assert!(
            screen.contains("/tmp/segment"),
            "a long message keeps its head: {screen}"
        );
        assert!(
            screen.contains("notes.txt"),
            "a long message keeps its tail: {screen}"
        );
        assert!(
            !screen.contains('…'),
            "a wrapped message loses nothing: {screen}"
        );

        for (width, height) in [(4, 3), (1, 1), (12, 4), (20, 6)] {
            let mut harness = dialog("/tmp/a-rather-long-path/notes.txt", width, height)?;
            // A view too small to show an answer must not answer the question
            // by accident, and must not leak the keys to what it covers.
            harness.key('y')?;
            harness.key(key::KeyCode::Enter)?;
            assert_eq!(answered(&mut harness), ["yes"]);
        }
        Ok(())
    }

    #[test]
    fn a_message_wraps_to_its_width() -> Result<()> {
        let message = "one two three four five six seven eight nine ten";
        let mut harness = host(60, 16)?;
        open_with(&mut harness, |owner| {
            ConfirmRequest::yes_no("Words", message, record(owner, "yes"), record(owner, "no"))
                .with_message_width(14)
        })?;
        let lines = screen(&harness);
        let text = lines.join("\n");
        for line in ["one two three", "four five six", "seven eight", "nine ten"] {
            assert!(
                lines.iter().any(|row| row.contains(&format!(" {line} "))),
                "{line:?} is a line of its own: {text}"
            );
        }
        assert!(!text.contains("one two three four"), "{text}");
        Ok(())
    }

    #[test]
    fn every_way_of_giving_an_answer_runs_its_own_action() -> Result<()> {
        type Give = Box<dyn Fn(&mut Harness) -> Result<()>>;
        let ways: [(&str, Give); 6] = [
            ("yes", Box::new(|harness| harness.key('y'))),
            ("no", Box::new(|harness| harness.key('n'))),
            // An ASCII letter gives its answer in either case.
            ("yes", Box::new(|harness| harness.key('Y'))),
            ("no", Box::new(|harness| harness.key(key::KeyCode::Enter))),
            ("no", Box::new(|harness| harness.key(' '))),
            ("no", Box::new(|harness| harness.key(key::KeyCode::Esc))),
        ];
        for (answer, give) in ways {
            let mut harness = dialog("/tmp/a", 40, 12)?;
            give(&mut harness)?;
            assert_eq!(
                answered(&mut harness),
                [answer],
                "exactly one action ran, for the expected answer"
            );
        }

        // A click reaches the answer it landed on, whichever holds focus.
        for (index, answer) in ["yes", "no"].into_iter().enumerate() {
            let mut harness = dialog("/tmp/a", 40, 12)?;
            let button = answer_node(&mut harness, index)?;
            click(&mut harness, button)?;
            assert_eq!(answered(&mut harness), [answer]);
        }
        Ok(())
    }

    #[test]
    fn three_answers_take_their_keys_from_their_labels() -> Result<()> {
        let mut harness = host(70, 14)?;
        open_with(&mut harness, |owner| {
            ConfirmRequest::new("Remove", "Remove the provider?")
                .with_answer(labelled(owner, "remove"))
                .with_answer(labelled(owner, "delete the secret too"))
                .with_declining(labelled(owner, "cancel"))
        })?;
        let screen = screen(&harness).join("\n");
        for label in ["remove", "delete the secret too", "cancel"] {
            assert!(screen.contains(label), "{label:?} shows: {screen}");
        }
        let cancel = answer_node(&mut harness, 2)?;
        assert_eq!(
            focus(&harness),
            Some(cancel),
            "the declining answer has focus"
        );
        harness.key('d')?;
        assert_eq!(answered(&mut harness), ["delete the secret too"]);

        for (key, answer) in [('r', "remove"), ('c', "cancel")] {
            open_with(&mut harness, |owner| {
                ConfirmRequest::new("Remove", "Remove the provider?")
                    .with_answer(labelled(owner, "remove"))
                    .with_answer(labelled(owner, "delete the secret too"))
                    .with_declining(labelled(owner, "cancel"))
            })?;
            harness.with_root_widget(|host: &mut Host| host.answered.clear());
            harness.key(key)?;
            assert_eq!(answered(&mut harness), [answer]);
        }
        Ok(())
    }

    #[test]
    fn an_answer_without_a_key_takes_no_key_but_a_click() -> Result<()> {
        let mut harness = host(50, 12)?;
        open_with(&mut harness, |owner| {
            ConfirmRequest::new("Key", "Check the key?")
                .with_answer(labelled(owner, "check").with_key(None))
                .with_declining(labelled(owner, "cancel"))
        })?;
        harness.key('c')?;
        assert_eq!(answered(&mut harness), ["cancel"], "c names only cancel");

        open_with(&mut harness, |owner| {
            ConfirmRequest::new("Key", "Check the key?")
                .with_answer(labelled(owner, "check").with_key(None))
                .with_declining(labelled(owner, "quit").with_key(None))
        })?;
        harness.key('c')?;
        harness.key('q')?;
        assert_eq!(answered(&mut harness), ["cancel"], "no key gives an answer");
        let check = answer_node(&mut harness, 0)?;
        click(&mut harness, check)?;
        assert_eq!(answered(&mut harness), ["cancel", "check"]);
        Ok(())
    }

    #[test]
    fn closing_before_the_call_closes_the_dialog_first() -> Result<()> {
        let mut harness = host(50, 12)?;
        open_with(&mut harness, |owner| {
            ConfirmRequest::new("Note", "Take a note?")
                .with_answer(Answer::new(
                    "take",
                    Host::call_note("take".into()).with_target(owner),
                ))
                .with_declining(Answer::new(
                    "skip",
                    Host::call_note("skip".into()).with_target(owner),
                ))
                .with_closing(Closing::BeforeCall)
        })?;
        assert!(open(&mut harness)?);
        harness.key('t')?;
        assert_eq!(answered(&mut harness), ["take"]);
        assert!(!open(&mut harness)?, "the dialog closed itself");

        // Under ByCall, a call that does not close leaves the question up.
        open_with(&mut harness, |owner| {
            ConfirmRequest::new("Note", "Take a note?")
                .with_answer(Answer::new(
                    "take",
                    Host::call_note("take".into()).with_target(owner),
                ))
                .with_declining(Answer::new(
                    "skip",
                    Host::call_note("skip".into()).with_target(owner),
                ))
        })?;
        harness.key('t')?;
        assert_eq!(answered(&mut harness), ["take", "take"]);
        assert!(open(&mut harness)?, "the call left the question open");
        Ok(())
    }

    #[test]
    fn is_open_reads_closed_after_a_call_closes_the_dialog() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        assert!(open(&mut harness)?);
        harness.key('y')?;
        harness.render()?;
        assert!(!open(&mut harness)?, "the host's call closed the dialog");
        // A closed dialog opens again.
        ask(&mut harness, "/tmp/b")?;
        assert!(open(&mut harness)?);
        Ok(())
    }

    #[test]
    fn regression_an_answer_after_a_call_closes_the_dialog_posts_nothing() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        // The first answer's call closes the dialog, and the second finds the
        // dialog before the close commits.
        let scripted = harness.script("confirm.answer(0)\nconfirm.answer(0)");
        assert!(scripted.is_err(), "the second answer finds no open dialog");
        harness.render()?;
        assert_eq!(
            answered(&mut harness),
            ["yes"],
            "a closing question takes no answer"
        );
        assert!(!open(&mut harness)?, "the host's call closed the dialog");

        // A call aimed at the closed dialog posts nothing either.
        let node = dialog_node(&mut harness)?;
        let posted = harness.with_root_widget_context(|_: &mut Host, ctx| {
            ctx.post(&Confirm::call_answer(0).with_target(CommandTarget::Exact(node)))
        });
        let Err(Error::Command(error)) = posted else {
            panic!("a closed dialog refuses the call: {posted:?}");
        };
        assert!(error.to_string().contains("no question is open"), "{error}");
        harness.canopy.with_context(node, |ctx| {
            ctx.with_widget_mut(node, |confirm: &mut Confirm, ctx| confirm.answer(ctx, 0))
        })?;
        harness.render()?;
        assert_eq!(
            answered(&mut harness),
            ["yes"],
            "a closed question takes no answer"
        );
        Ok(())
    }

    /// Return the question that a test asks while another closes.
    fn other_request(target: CommandTarget) -> ConfirmRequest {
        ConfirmRequest::new("Other", "/tmp/other")
            .with_answer(labelled(target, "keep"))
            .with_declining(labelled(target, "drop"))
    }

    /// Ask another question in the callback that closes the open one, before
    /// the close commits, so the open fails. Return the error of the open.
    fn ask_while_closing(harness: &mut Harness) -> Result<Error> {
        harness.with_root_widget_context(|host: &mut Host, ctx| {
            host.close(ctx)?;
            let owner = ctx.node_id();
            let request = other_request(CommandTarget::Exact(owner));
            match Confirm::open(ctx, host.dialog()?, owner, request) {
                Ok(_) => Err(Error::Invalid("the open succeeded".into())),
                Err(error) => Ok(error),
            }
        })
    }

    #[test]
    fn regression_a_failed_open_leaves_the_dialog_closed_with_the_earlier_parts() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        let error = ask_while_closing(&mut harness)?;
        assert!(matches!(error, Error::ModalClosing), "{error:?}");
        harness.render()?;
        assert!(!open(&mut harness)?, "the earlier question closed");
        let node = dialog_node(&mut harness)?;
        harness.with_root_widget_context(|_: &mut Host, ctx| ctx.set_hidden(node, false))?;
        harness.render()?;
        let shown = screen(&harness).join("\n");
        assert!(shown.contains("/tmp/a"), "{shown}");
        assert!(
            !shown.contains("Other") && !shown.contains("keep"),
            "{shown}"
        );

        // The dialog asks again.
        ask(&mut harness, "/tmp/b")?;
        harness.key('y')?;
        assert_eq!(answered(&mut harness), ["yes"]);
        Ok(())
    }

    #[test]
    fn regression_a_failed_open_whose_dispatch_fails_keeps_the_earlier_question() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        let declining = focus(&harness);
        // A dispatch that fails discards the close it queued, so the earlier
        // question stands, with its own parts.
        let propagated = harness.with_root_widget_context(|host: &mut Host, ctx| {
            host.close(ctx)?;
            let owner = ctx.node_id();
            let request = other_request(CommandTarget::Exact(owner));
            Confirm::open(ctx, host.dialog()?, owner, request).map(|_| ())
        });
        assert!(
            matches!(propagated, Err(Error::ModalClosing)),
            "{propagated:?}"
        );
        harness.render()?;
        assert!(
            open(&mut harness)?,
            "the discarded close leaves the question open"
        );
        let shown = screen(&harness).join("\n");
        assert!(shown.contains("/tmp/a"), "{shown}");
        assert!(
            !shown.contains("Other") && !shown.contains("keep"),
            "{shown}"
        );
        assert_eq!(
            focus(&harness),
            declining,
            "the declining answer keeps the keyboard"
        );
        harness.key('n')?;
        assert_eq!(answered(&mut harness), ["no"]);
        Ok(())
    }

    #[test]
    fn open_refuses_a_bad_request_and_a_second_question() -> Result<()> {
        let mut harness = host(40, 12)?;
        let refused = |harness: &mut Harness, build: &dyn Fn(CommandTarget) -> ConfirmRequest| {
            harness
                .with_root_widget_context(|host: &mut Host, ctx| {
                    let owner = ctx.node_id();
                    Confirm::open(
                        ctx,
                        host.dialog()?,
                        owner,
                        build(CommandTarget::Exact(owner)),
                    )
                })
                .is_err()
        };
        assert!(refused(&mut harness, &|_| ConfirmRequest::new(
            "Empty",
            "No answers"
        )));
        let mut out_of_range = ConfirmRequest::new("Range", "Out of range");
        out_of_range.decline = 1;
        let out_of_range =
            out_of_range.with_answer(Answer::new("ok", Host::call_note("ok".into())));
        assert!(refused(&mut harness, &|_| out_of_range.clone()));
        assert!(
            refused(&mut harness, &|owner| {
                ConfirmRequest::new("Keys", "Two keys")
                    .with_answer(labelled(owner, "keep"))
                    .with_declining(labelled(owner, "Kill"))
            }),
            "two answers with the key k are refused"
        );
        assert!(!open(&mut harness)?, "a refused request opens nothing");

        ask(&mut harness, "/tmp/a")?;
        assert!(
            refused(&mut harness, &|owner| {
                ConfirmRequest::yes_no("Again", "Again", record(owner, "yes"), record(owner, "no"))
            }),
            "an open dialog refuses a second question"
        );
        Ok(())
    }

    #[test]
    fn focus_moves_between_the_answers_and_stays_in_the_dialog() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        let yes = answer_node(&mut harness, 0)?;
        let no = answer_node(&mut harness, 1)?;
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
        assert_eq!(answered(&mut harness), ["yes"]);
        Ok(())
    }

    #[test]
    fn a_disabled_answer_is_inert_by_key_and_by_click() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        harness.with_root_widget(|host: &mut Host| host.disabled = vec!["yes".into()]);
        let yes = answer_node(&mut harness, 0)?;

        harness.key('y')?;
        click(&mut harness, yes)?;
        assert!(
            answered(&mut harness).is_empty(),
            "a disabled answer runs nothing"
        );

        // The reason stays reachable, and declining still works.
        let status = harness.canopy.with_root_view(|ctx| {
            ctx.command_status(&Button::call_press().with_target(CommandTarget::Exact(yes)))
        })?;
        assert!(matches!(status, CommandStatus::Disabled(_)), "{status:?}");
        let status = on_dialog(&mut harness, |confirm, ctx| confirm.answer_status(ctx, 0))??;
        assert!(matches!(status, CommandStatus::Disabled(_)), "{status:?}");
        harness.key('n')?;
        assert_eq!(answered(&mut harness), ["no"]);
        Ok(())
    }

    #[test]
    fn a_click_outside_the_answers_does_not_answer() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        let node = dialog_node(&mut harness)?;
        let body: NodeId = on_dialog(&mut harness, |confirm, _| {
            confirm.parts.mounted().map(|parts| parts.body)
        })??
        .into();
        // The dialog's own margin, its frame corner, and the gap between the
        // answers all belong to the dialog.
        for node in [node, body] {
            click(&mut harness, node)?;
        }
        let no = answer_node(&mut harness, 1)?;
        let gap = harness.canopy.with_root_view(|ctx| {
            let view = ctx.view_of(no).expect("answer view");
            PointI32 {
                x: view.outer.tl.x - 1,
                y: view.outer.tl.y,
            }
        });
        click_at(&mut harness, gap)?;
        assert!(answered(&mut harness).is_empty());
        Ok(())
    }

    #[test]
    fn a_rolled_back_dialog_reads_unmounted_and_asks_again() -> Result<()> {
        let mut harness = host(40, 12)?;
        harness.with_root_widget_context(|host: &mut Host, ctx| {
            let dialog = ctx.create_detached(Confirm::new())?;
            attach_and_roll_back(ctx, dialog.into());
            let unmounted =
                ctx.with_widget(dialog, |dialog: &Confirm| Ok(dialog.row().is_err()))?;
            assert!(unmounted, "a rolled-back dialog has no answers");
            ctx.attach(ctx.node_id(), dialog.into())?;
            ctx.set_hidden(dialog.into(), true)?;
            host.dialog = Some(dialog);
            Ok(())
        })?;
        ask(&mut harness, "/tmp/again")?;
        assert!(open(&mut harness)?);
        assert!(
            screen(&harness)
                .iter()
                .any(|row| row.contains("/tmp/again"))
        );
        harness.key('y')?;
        assert_eq!(answered(&mut harness), ["yes"]);
        Ok(())
    }

    #[test]
    fn reopening_returns_to_the_declining_answer() -> Result<()> {
        let mut harness = dialog("/tmp/a", 40, 12)?;
        let yes = answer_node(&mut harness, 0)?;

        harness.key(key::Key::parse_spec("Left")?)?;
        assert_eq!(focus(&harness), Some(yes));
        harness.key('n')?;

        // A second question starts at the declining answer rather than at
        // whichever answer the last one left focused.
        ask(&mut harness, "/tmp/b")?;
        let no = answer_node(&mut harness, 1)?;
        assert_eq!(focus(&harness), Some(no));

        // A host can make the first answer the one that declines.
        open_with(&mut harness, |owner| {
            ConfirmRequest::new("Bookmark", "/tmp/c")
                .with_declining(labelled(owner, "keep"))
                .with_answer(labelled(owner, "drop"))
        })?;
        let keep = answer_node(&mut harness, 0)?;
        assert_eq!(focus(&harness), Some(keep));
        harness.key(key::KeyCode::Esc)?;
        assert_eq!(answered(&mut harness), ["no", "keep"]);
        Ok(())
    }

    #[test]
    fn arming_ignores_keys_and_clicks_until_the_delay_ends() -> Result<()> {
        let clock = Arc::new(ManualClock::new());
        let mut harness = host_on(40, 12, Arc::clone(&clock))?;
        open_with(&mut harness, |owner| {
            ConfirmRequest::yes_no(
                "Allow",
                "Allow the call?",
                record(owner, "yes"),
                record(owner, "no"),
            )
            .with_arm_after(Duration::from_millis(600))
        })?;
        let yes = answer_node(&mut harness, 0)?;
        let status = |harness: &Harness| {
            harness.canopy.with_root_view(|ctx| {
                ctx.command_status(&Button::call_press().with_target(CommandTarget::Exact(yes)))
            })
        };
        assert!(
            matches!(status(&harness)?, CommandStatus::Disabled(_)),
            "a button shows as disabled"
        );

        harness.key('y')?;
        click(&mut harness, yes)?;
        harness.key(key::KeyCode::Enter)?;
        harness.key(key::KeyCode::Esc)?;
        assert!(
            answered(&mut harness).is_empty(),
            "nothing answers before the row arms"
        );
        assert!(open(&mut harness)?, "the question stands");

        clock.advance(Duration::from_millis(600))?;
        harness.render()?;
        assert_eq!(status(&harness)?, CommandStatus::Enabled);
        click(&mut harness, yes)?;
        assert_eq!(
            answered(&mut harness),
            ["yes"],
            "a click answers once armed"
        );

        open_with(&mut harness, |owner| {
            ConfirmRequest::yes_no(
                "Allow",
                "Allow the call?",
                record(owner, "yes"),
                record(owner, "no"),
            )
            .with_arm_after(Duration::from_millis(600))
        })?;
        harness.key('y')?;
        assert_eq!(answered(&mut harness), ["yes"]);
        clock.advance(Duration::from_millis(600))?;
        harness.key('y')?;
        assert_eq!(
            answered(&mut harness),
            ["yes", "yes"],
            "a key answers once armed"
        );
        Ok(())
    }

    #[test]
    fn esc_declines_only_when_the_declining_answer_can_be_given() -> Result<()> {
        let clock = Arc::new(ManualClock::new());
        let mut harness = host_on(40, 12, Arc::clone(&clock))?;
        open_with(&mut harness, |owner| {
            ConfirmRequest::yes_no(
                "Allow",
                "Allow the call?",
                record(owner, "yes"),
                record(owner, "no"),
            )
            .with_arm_after(Duration::from_millis(600))
        })?;
        // Before the row arms, Esc gives nothing.
        harness.key(key::KeyCode::Esc)?;
        assert!(answered(&mut harness).is_empty());
        let status = on_dialog(&mut harness, |confirm, ctx| confirm.decline_status(ctx))??;
        assert!(matches!(status, CommandStatus::Disabled(_)), "{status:?}");

        // While the declining call is disabled, Esc gives nothing.
        clock.advance(Duration::from_millis(600))?;
        harness.with_root_widget(|host: &mut Host| host.disabled = vec!["no".into()]);
        harness.key(key::KeyCode::Esc)?;
        assert!(answered(&mut harness).is_empty());
        assert!(open(&mut harness)?);

        // Once both allow it, Esc declines.
        harness.with_root_widget(|host: &mut Host| host.disabled.clear());
        harness.key(key::KeyCode::Esc)?;
        assert_eq!(answered(&mut harness), ["no"]);
        Ok(())
    }

    #[test]
    fn the_error_tone_draws_the_frame_and_title_in_red() -> Result<()> {
        let mut harness = host(50, 12)?;
        open_with(&mut harness, |owner| {
            ConfirmRequest::new("Failed", "The store is gone.")
                .with_declining(labelled(owner, "quit"))
                .with_tone(Tone::Error)
        })?;
        let palette = themes::default_dark();
        let (border, _, _) = style_of(&harness, '\u{256d}');
        let (title, _, attrs) = style_of(&harness, 'F');
        assert_eq!(border, palette.red, "the frame takes the error color");
        assert_eq!(title, palette.red, "the title takes the error color");
        assert!(attrs.bold, "the title is bold");

        open_with(&mut harness, |owner| {
            ConfirmRequest::new("Failed", "The store is gone.")
                .with_declining(labelled(owner, "quit"))
        })?;
        let (border, _, _) = style_of(&harness, '\u{256d}');
        assert_ne!(border, palette.red, "a plain dialog keeps its frame");
        Ok(())
    }

    #[test]
    fn a_dialog_owned_by_another_modal_opens_over_it() -> Result<()> {
        let mut harness = host(50, 14)?;
        // The host opens an outer modal over itself, then asks over it, with
        // the outer modal as the owner.
        let (host_node, outer, confirm) =
            harness.with_root_widget_context(|_: &mut Host, ctx| {
                let outer: NodeId = ctx
                    .add_modal(Text::new("outer").with_focusable(true))?
                    .into();
                let confirm = ctx.add_modal(Confirm::new())?;
                ctx.open_modal(ModalOptions {
                    owner: ctx.node_id(),
                    modal: outer,
                    initial_focus: outer,
                    dim_target: None,
                    bindings: ModalBindings::Application,
                })?;
                Ok((ctx.node_id(), outer, confirm))
            })?;
        harness.render()?;
        let target = CommandTarget::Exact(host_node);
        let token = harness.canopy.with_root_context(|ctx| {
            Confirm::open(
                ctx,
                confirm,
                outer,
                ConfirmRequest::yes_no(
                    "Nested",
                    "Over the outer modal?",
                    record(target, "yes"),
                    record(target, "no"),
                )
                .with_closing(Closing::BeforeCall),
            )
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Nested"), "the question shows");
        assert!(
            harness
                .canopy
                .with_root_view(|ctx| ctx.focused_within(confirm.into()))
                .is_some(),
            "the question takes the keyboard"
        );
        harness.key('n')?;
        harness.render()?;
        assert_eq!(answered(&mut harness), ["no"]);
        assert!(
            !harness
                .canopy
                .with_root_view(|ctx| ctx.modal_is_open(token)),
            "the question closed"
        );
        assert!(
            !harness.tbuf().contains_text("Nested"),
            "the question is gone from the screen"
        );
        Ok(())
    }

    #[test]
    fn a_modal_without_the_dialog_group_keeps_the_question_standing() -> Result<()> {
        const GROUP: FrameworkBindingGroup = FrameworkBindingGroup::new("confirm.test_dialog");
        let mut harness = host(40, 12)?;
        let (owner, node, focus, request) =
            harness.with_root_widget_context(|host: &mut Host, ctx| {
                let dialog = host.dialog()?;
                let owner = CommandTarget::Exact(ctx.node_id());
                ctx.set_hidden(dialog.into(), false)?;
                let request = ConfirmRequest::yes_no(
                    "Bookmark",
                    "/tmp/a",
                    record(owner, "yes"),
                    record(owner, "no"),
                );
                let focus = ctx.with_widget_mut(dialog, |confirm: &mut Confirm, ctx| {
                    confirm.show(ctx, &request)
                })?;
                Ok((ctx.node_id(), NodeId::from(dialog), focus, request))
            })?;
        harness.canopy.with_root_context(|ctx| {
            let token = ctx.open_modal(ModalOptions {
                owner,
                modal: node,
                initial_focus: focus,
                dim_target: None,
                bindings: ModalBindings::Framework {
                    groups: &[GROUP],
                    intents: &[],
                },
            })?;
            // The question stands, so only the admission keeps it from an
            // answer.
            ctx.update(node, |confirm: &mut Confirm| {
                confirm.question = Some(Question { request, token })
            })
        })?;
        harness.render()?;

        // The modal admits only a group of its own, not the dialog's groups,
        // so nothing but the row's own keys answers.
        harness.key(key::KeyCode::Enter)?;
        harness.key(key::KeyCode::Esc)?;
        let button = answer_node(&mut harness, 0)?;
        click(&mut harness, button)?;
        assert!(
            answered(&mut harness).is_empty(),
            "a framework group admits no application binding"
        );
        assert!(
            harness.canopy.available_bindings(None)?.bindings.is_empty(),
            "discovery agrees that nothing is admitted"
        );
        Ok(())
    }
}
