//! A framed panel centred over the view.

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, TypedId, ViewContext, Widget,
    error::Result,
    input::Event,
    layout::{Direction, Edges, Layout, LayoutOverride, Sizing},
    render::Render,
};

use crate::frame::Frame;

canopy::slot!(FrameSlot: Frame);

/// Rows and columns left around the frame by default, so the view shows
/// through.
const MARGIN: u32 = 1;

/// A titled frame that fits its body, centred over whatever the dialog
/// covers.
///
/// The dialog keeps a margin clear around its frame and swallows clicks on
/// it, so a click beside the frame never reaches the view behind. It pushes
/// the `dialog` layer, so one set of `dialog/<part>` rules styles every
/// dialog's frame, ground, and buttons. A host builds one with
/// `Dialog::new().with_title(..)` and mounts it with its body through
/// [`Dialog::add`].
pub struct Dialog {
    /// Title drawn on the frame.
    title: Option<String>,
    /// Widest the frame gets, when capped.
    max_width: Option<u32>,
    /// Space kept clear around the frame.
    margin: Edges,
}

impl Default for Dialog {
    fn default() -> Self {
        Self::new()
    }
}

impl Dialog {
    /// Construct an untitled dialog with a one-cell margin.
    pub fn new() -> Self {
        Self {
            title: None,
            max_width: None,
            margin: Edges::all(MARGIN),
        }
    }

    /// Title the frame.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Cap the frame's width, borders included.
    #[must_use]
    pub fn with_max_width(mut self, width: u32) -> Self {
        self.max_width = Some(width);
        self
    }

    /// Replace the space kept clear around the frame.
    #[must_use]
    pub fn with_margin(mut self, margin: Edges) -> Self {
        self.margin = margin;
        self
    }

    /// Add this dialog under `parent`, framing `body`, and return both.
    ///
    /// The frame measures its body, so the dialog is only as large as what it
    /// shows; the margin caps a large body, which then scrolls.
    pub fn add<W: Widget + 'static>(
        self,
        ctx: &mut dyn Context,
        parent: NodeId,
        body: W,
    ) -> Result<(TypedId<Self>, TypedId<W>)> {
        let frame = match &self.title {
            Some(title) => Frame::new().with_title(title.clone()),
            None => Frame::new(),
        };
        let max_width = self.max_width;
        let dialog = ctx.add_child(parent, self)?;
        let frame = ctx.add_slot::<FrameSlot>(dialog, frame)?;
        ctx.set_layout_override(
            frame.into(),
            LayoutOverride {
                width: Some(Sizing::Measure),
                height: Some(Sizing::Measure),
                max_width: max_width.map(Some),
                ..LayoutOverride::new()
            },
        )?;
        let body = ctx.add_child(frame, body)?;
        Ok((dialog, body))
    }

    /// Return the frame of the dialog at `dialog`, to retitle it.
    pub fn frame(ctx: &dyn Context, dialog: NodeId) -> Result<TypedId<Frame>> {
        ctx.get_slot::<FrameSlot>(dialog)
    }
}

impl Widget for Dialog {
    fn layout(&self) -> Layout {
        // A stack centres the frame over what the dialog covers, and the
        // margin keeps that visible around it.
        Layout::fill()
            .direction(Direction::Stack)
            .align_center()
            .padding(self.margin)
    }

    fn render(&mut self, render: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        render.push_layer("dialog");
        Ok(())
    }

    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        // A click on the margin or the frame belongs to the dialog, not to
        // what it covers.
        match event {
            Event::Mouse(_) => Ok(EventOutcome::Handle),
            _ => Ok(EventOutcome::Ignore),
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("dialog")
    }
}
