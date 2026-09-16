//! A modal question with a yes or no answer.

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, Render, ViewContext, Widget,
    derive_commands,
    error::{Error, Result},
    event::Event,
    geom::{self, Line, Rect, Size},
    layout::{
        CanvasContext, Direction, Edges, Layout, LayoutOverride, MeasureConstraints, Measurement,
        Sizing,
    },
    text,
};

use crate::{boxed::ROUND, frame::Frame};

/// Columns a row spends on its blank lead column and a trailing gutter.
const ROW_PADDING: u32 = 2;
/// Rows and columns left around the frame, so the view shows through.
const FRAME_MARGIN: u32 = 1;
/// Columns between the buttons.
const BUTTON_GAP: u32 = 2;
/// Columns a button spends on its border and the space inside it.
const BUTTON_PADDING: u32 = 4;
/// Rows a button occupies, border included.
const BUTTON_ROWS: u32 = 3;
/// Rows the dialog draws: the message, a blank, and the buttons.
const DIALOG_ROWS: u32 = 2 + BUTTON_ROWS;

/// One answer the dialog offers.
struct Answer {
    /// Text on the button.
    label: &'static str,
    /// The key that gives this answer, which is the label's first letter.
    key: char,
}

/// The answers every question takes, in the order they are drawn.
///
/// Each key is the first letter of its own label, so the button shows the key
/// by highlighting it rather than by repeating it.
const ANSWERS: [Answer; 2] = [
    Answer {
        label: "Yes",
        key: 'y',
    },
    Answer {
        label: "No",
        key: 'n',
    },
];

/// Return the columns a button with `label` occupies.
fn button_width(label: &str) -> u32 {
    u32::try_from(text::display_width(label))
        .unwrap_or(u32::MAX)
        .saturating_add(BUTTON_PADDING)
}

/// Return the columns every button and the gaps between them occupy.
fn buttons_width() -> u32 {
    let buttons: u32 = ANSWERS
        .iter()
        .map(|answer| button_width(answer.label))
        .sum();
    let gaps = u32::try_from(ANSWERS.len().saturating_sub(1)).unwrap_or(0) * BUTTON_GAP;
    buttons.saturating_add(gaps)
}

/// A centred modal asking a yes or no question.
///
/// The dialog centres a titled frame over whatever it covers, states its
/// question, and offers it as two buttons. It decides nothing: it holds no
/// answer and runs no command, so a host keeps both the question and what
/// agreeing to it does, and binds the keys that answer it.
///
/// Open it inside a modal scope, with [`Confirm::body`] as the initial focus,
/// and the scope dims what the dialog covers and gives it the keyboard.
pub struct Confirm {
    /// The question, once mounted.
    body: Option<NodeId>,
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
        Self { body: None }
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

    /// Return the question, which takes the keyboard while the dialog is open,
    /// or an error before it mounts.
    pub fn body(&self) -> Result<NodeId> {
        self.body
            .ok_or_else(|| Error::NotFound("confirm body".to_string()))
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
        self.body = Some(body.into());
        Ok(())
    }

    fn on_event(&mut self, event: &Event, _context: &mut dyn Context) -> Result<EventOutcome> {
        // A click on the margin belongs to the dialog, not to what it covers.
        match event {
            Event::Mouse(_) => Ok(EventOutcome::Handle),
            _ => Ok(EventOutcome::Ignore),
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("confirm")
    }
}

/// The question a dialog asks and the buttons that answer it.
struct ConfirmBody {
    /// What the question is about.
    message: String,
    /// Width that shows the message and the buttons unclipped.
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
        self.fitted_width = widest
            .saturating_add(ROW_PADDING)
            .max(buttons_width().saturating_add(ROW_PADDING));
        self.message = message;
        context.invalidate_layout();
    }

    /// Draw one button, with the key that gives its answer highlighted.
    ///
    /// The border and the fill are drawn first, so the label and the key sit on
    /// the button rather than on the panel behind it.
    fn button(render: &mut Render, answer: &Answer, rect: Rect) -> Result<()> {
        let border = geom::FrameRects::new(rect, 1);
        ROUND.draw(render, "confirm/button", border)?;
        if border.inner.w == 0 || border.inner.h == 0 {
            return Ok(());
        }
        render.fill("confirm/button/label", border.inner, ' ')?;
        // The label sits one column inside the border. Indenting gives up that
        // column rather than carrying the full width past the inner rect, which
        // would pad over the border on the far side.
        let label = border.inner.line(0)?.indent(1);
        render.text("confirm/button/label", label, answer.label)?;
        // The key is the label's first letter, repainted in its own style.
        let key = Line::new(label.tl.x, label.tl.y, 1);
        render.text("confirm/key", key, &answer.key.to_uppercase().to_string())
    }
}

impl Widget for ConfirmBody {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.clamp(Size::new(self.fitted_width, DIALOG_ROWS))
    }

    fn canvas(&self, view: Size, _context: &CanvasContext) -> Size {
        // The dialog never scrolls: it shrinks its message to fit instead.
        view
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        let area = context.view().view_rect_local();
        render.fill("background", area, ' ')?;
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        // A message is identified by its tail, which is what a path needs, so
        // one too wide loses its head.
        let budget = (area.w as usize).saturating_sub(ROW_PADDING as usize);
        let message = text::truncate_start(&self.message, budget);
        render.text("confirm/message", area.line(0)?, &format!(" {message}"))?;

        // The buttons sit under the message, centred as a group.
        let top = area.tl.y.saturating_add(2);
        if area.h < DIALOG_ROWS || area.w < buttons_width() {
            return Ok(());
        }
        let mut x = area
            .tl
            .x
            .saturating_add((area.w.saturating_sub(buttons_width())) / 2);
        for answer in &ANSWERS {
            let width = button_width(answer.label);
            Self::button(render, answer, Rect::new(x, top, width, BUTTON_ROWS))?;
            x = x.saturating_add(width).saturating_add(BUTTON_GAP);
        }
        Ok(())
    }

    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("confirm_body")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{Loader, testing::harness::Harness};

    use super::*;

    impl Loader for Confirm {}

    /// Build a dialog asking `message`, rendered once.
    fn dialog(message: &str, width: u32, height: u32) -> Result<Harness> {
        let mut harness = Harness::builder(Confirm::new())
            .size(width, height)
            .build()?;
        harness.render()?;
        harness.with_root_context(|confirm: &mut Confirm, context| {
            confirm.ask(context, "Bookmark", message)
        })?;
        harness.render()?;
        Ok(harness)
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
        assert!(screen.contains("Yes"), "the affirmative button shows");
        assert!(screen.contains("No"), "the negative button shows");
        // Each button is framed, so the button row carries its own corners
        // besides the dialog frame's.
        let corners = screen.matches('\u{256d}').count();
        assert_eq!(
            corners, 3,
            "the dialog and both buttons are framed, got {screen}"
        );

        // A button keeps the border on both sides of its label. Painting a
        // label pads to the width of its line, so a line that starts inside the
        // border while keeping the full inner width writes over the border on
        // the far side, and the corners above still look intact.
        assert!(
            screen.contains("\u{2502} Yes \u{2502}"),
            "the affirmative button keeps both borders, got {screen}"
        );
        assert!(
            screen.contains("\u{2502} No \u{2502}"),
            "the negative button keeps both borders, got {screen}"
        );
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
            style_of('Y'),
            style_of('e'),
            "the key stands out from its label"
        );
        assert_ne!(
            style_of('N'),
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
        // The frame's border and title sit on the same ground as the message.
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
            dialog("/tmp/a-rather-long-path/notes.txt", width, height)?;
        }
        Ok(())
    }
}
