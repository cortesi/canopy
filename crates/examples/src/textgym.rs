use canopy::{command, derive_commands, layout::Edges, prelude::*};
use canopy_widgets::{CanvasWidth, Frame, Pad, Selectable, Text, VStack};

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

#[derive_commands]
impl TextGym {
    /// Construct a new text gym demo.
    pub fn new() -> Self {
        Self
    }

    #[command]
    /// Trigger a redraw.
    pub fn redraw(&mut self, _ctx: &mut dyn Context) {}
}

impl Widget for TextGym {
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let default_id = section(
            c,
            "Default (tab stop 4)",
            Text::new(DEFAULT_TEXT).with_canvas_width(CanvasWidth::View),
            Layout::fill().fixed_width(34).fixed_height(6),
        )?;

        let wrap_id = section(
            c,
            "Wrap width 24 + italic",
            Text::new(WRAP_TEXT)
                .with_wrap_width(24)
                .with_style("text/italic"),
            Layout::fill().fixed_width(34).fixed_height(7),
        )?;

        let intrinsic_id = section(
            c,
            "Intrinsic canvas + tab stop 8",
            Text::new(INTRINSIC_TEXT)
                .with_canvas_width(CanvasWidth::Intrinsic)
                .with_tab_stop(8)
                .with_wrap_width(32),
            Layout::fill().fixed_width(26).fixed_height(6),
        )?;

        let mut fixed_text = Text::new(FIXED_TEXT)
            .with_canvas_width(CanvasWidth::Fixed(40))
            .with_selected_style("text/underline");
        fixed_text.set_selected(true);
        let fixed_id = section(
            c,
            "Fixed canvas 40 + selected",
            fixed_text,
            Layout::fill().fixed_width(26).fixed_height(6),
        )?;

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

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }
}

/// Wrap a text widget in a titled frame.
fn section(c: &mut dyn Context, title: &str, text: Text, layout: Layout) -> Result<NodeId> {
    let text_id = c.create_detached(text);
    c.set_layout_of(text_id, Layout::fill())?;
    let frame_id = Frame::wrap_with(c, text_id, Frame::new().with_title(title))?;
    let pad_id = Pad::wrap_with(c, frame_id, Pad::uniform(OUTER_PADDING))?;
    let padded_layout = outer_layout(layout, OUTER_PADDING);
    c.set_layout_of(pad_id, padded_layout)?;
    Ok(pad_id)
}

/// Convert a frame layout into a padded container layout.
fn outer_layout(layout: Layout, padding: u32) -> Layout {
    let mut layout = layout.padding(Edges::all(padding));
    let bump = padding.saturating_mul(2);
    if let Some(min_width) = layout.min_width {
        layout.min_width = Some(min_width.saturating_add(bump));
    }
    if let Some(max_width) = layout.max_width {
        layout.max_width = Some(max_width.saturating_add(bump));
    }
    if let Some(min_height) = layout.min_height {
        layout.min_height = Some(min_height.saturating_add(bump));
    }
    if let Some(max_height) = layout.max_height {
        layout.max_height = Some(max_height.saturating_add(bump));
    }
    layout
}

impl Loader for TextGym {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        Ok(())
    }
}
