use canopy::{
    CanopyBuilder, Context, ContextExt, Loader, NodeId, ViewContext, Widget,
    error::Result,
    layout::{Edges, Layout},
};
use canopy_widgets::{CanvasWidth, Frame, Pad, Selectable, Text, VStack, wrap};

/// Text sample using the default tab stop.
const DEFAULT_TEXT: &str = concat!(
    "col1\tcol2\tcol3\n",
    "wide\t界\twide\n",
    "align\tcols\tmore\n",
    "final\trow\tend",
);
/// Text sample exercising wrap width and custom style.
const WRAP_TEXT: &str = concat!(
    "alpha\tbeta\tgamma delta epsilon zeta eta theta iota kappa lambda mu\n",
    "nu xi omicron pi rho sigma tau upsilon phi chi psi omega",
);
/// Text sample exercising intrinsic canvas width and a custom tab stop.
const INTRINSIC_TEXT: &str = concat!(
    "col1\tcol2\tcol3\n",
    "tab8\twide\tcolumns\n",
    "longer\trow\tfor scroll",
);
/// Text sample exercising fixed canvas width and selected styling.
const FIXED_TEXT: &str = concat!(
    "selected style enabled\n",
    "0123456789\tabcdef\n",
    "wrapless\tline\tcontent",
);

/// Demo node that displays multiple text variants.
pub struct TextGym;

/// Outer padding around each framed section.
const OUTER_PADDING: u32 = 1;

impl Default for TextGym {
    fn default() -> Self {
        Self::new()
    }
}

impl TextGym {
    /// Construct a new text gym demo.
    pub fn new() -> Self {
        Self
    }
}

impl Widget for TextGym {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let default_id = section(
            c,
            "Default (tab stop 4)",
            Text::new(DEFAULT_TEXT).with_canvas_width(CanvasWidth::View),
            34,
        )?;

        let wrap_id = section(
            c,
            "Wrap width 24 + italic",
            Text::new(WRAP_TEXT)
                .with_wrap_width(24)
                .with_style("text/italic"),
            34,
        )?;

        let intrinsic_id = section(
            c,
            "Intrinsic canvas + tab stop 8",
            Text::new(INTRINSIC_TEXT)
                .with_canvas_width(CanvasWidth::Intrinsic)
                .with_tab_stop(8)
                .with_wrap_width(32),
            26,
        )?;

        let mut fixed_text = Text::new(FIXED_TEXT)
            .with_canvas_width(CanvasWidth::Fixed(40))
            .with_selected_style("text/underline");
        fixed_text.set_selected(true);
        let fixed_id = section(c, "Fixed canvas 40 + selected", fixed_text, 26)?;

        let stack = VStack::new()
            .push_fixed(default_id, 6)
            .push_fixed(wrap_id, 7)
            .push_fixed(intrinsic_id, 6)
            .push_fixed(fixed_id, 6);
        let stack_id = c.add_child(stack)?;

        c.set_layout(Layout::fill())?;
        c.set_layout_of(stack_id, Layout::fill())?;
        Ok(())
    }
}

/// Wrap a text widget in a titled frame.
///
/// `VStack::push_fixed` sets the row height, so `section` sets only the width
/// it controls.
fn section(c: &mut dyn Context, title: &str, text: Text, width: u32) -> Result<NodeId> {
    let text_id = c.create_detached(text)?;
    c.set_layout_of(text_id, Layout::fill())?;
    let frame_id = wrap(c, text_id, Frame::new().with_title(title))?;
    let pad_id = wrap(c, frame_id, Pad::uniform(OUTER_PADDING))?;
    c.set_layout_of(
        pad_id,
        Layout::fill()
            .fixed_width(width.saturating_add(2 * OUTER_PADDING))
            .padding(Edges::all(OUTER_PADDING)),
    )?;
    Ok(pad_id.into())
}

impl Loader for TextGym {}

/// Default bindings for the text gym demo.
const DEFAULT_BINDINGS: &str = r#"
canopy.bind_command("q", { path = "root", description = "Quit" }, "root::quit")
"#;

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.bindings("textgym", DEFAULT_BINDINGS)
}
