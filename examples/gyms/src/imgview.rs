//! Image viewer gym: an image drawn in block mosaics, with a toggle that
//! steps through the tiers of symbols to compare them, and a chart of the
//! glyphs that each tier draws.
//!
//! The label row names the tier. t steps the tier, from the terminal's
//! glyphs through half blocks, quadrants, block elements, and the full
//! tier, and c switches between the image and the glyph chart. The chart
//! shows each family between border characters, with an ASCII sentinel after
//! each glyph, so a glyph that the terminal draws at the wrong width breaks
//! its row. Runs of one glyph should join into seamless bars.

use canopy::{
    CanopyBuilder, Context, ContextExt, NodeName, Register, Setup, TypedId, ViewContext, Widget,
    derive_commands,
    error::{Error, Result},
    geom::Line,
    layout::{Direction, Layout, LayoutOverride},
    render::Render,
};
use canopy_widgets::{
    ImageView, Text,
    mosaic::{self, Family, Tier},
};

/// Default bindings for the image viewer demo.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()
canopy.keymap({
    path = "image_view/",
    phase = "before_widget",
    { key = "i", description = "Zoom in", action = command.image_view.zoom("in") },
    { key = "o", description = "Zoom out", action = command.image_view.zoom("out") },
    { key = { "h", "Left" }, description = "Pan left", action = command.image_view.pan("left") },
    { key = { "j", "Down" }, description = "Pan down", action = command.image_view.pan("down") },
    { key = { "k", "Up" }, description = "Pan up", action = command.image_view.pan("up") },
    {
        key = { "l", "Right" },
        description = "Pan right",
        action = command.image_view.pan("right"),
    },
})
canopy.keymap({
    path = "img_gym",
    { key = "t", description = "Next mosaic tier", action = command.img_gym.cycle_tier() },
    { key = "c", description = "Show the glyph chart or the image", action = command.img_gym.toggle_chart() },
})
"#;

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder
        .configure(ImageView::register)
        .configure(ImgGym::register)
        .script("imgview", DEFAULT_BINDINGS)
}

/// The rows of the glyph chart: each family of the full tier between
/// borders, each glyph followed by a sentinel, then runs that should join.
pub fn chart_rows() -> Vec<String> {
    /// Glyphs to a row of the chart.
    const PER_ROW: usize = 24;
    let mut rows = vec![
        "Each glyph sits between bars. A bar out of line marks a glyph of the wrong width."
            .to_owned(),
        String::new(),
    ];
    for family in Family::ALL {
        let chars: Vec<char> = family.chars().into_iter().filter(|ch| *ch != ' ').collect();
        if chars.is_empty() {
            continue;
        }
        rows.push(format!("{family:?}"));
        for chunk in chars.chunks(PER_ROW) {
            let row: String = chunk.iter().map(|ch| format!("{ch}|")).collect();
            rows.push(format!("|{row}"));
        }
    }
    rows.push(String::new());
    rows.push("Runs that should join into seamless bars and slopes:".to_owned());
    for run in [
        "█▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀█",
        "🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂🬂",
        "\u{1FB82}\u{1FB82}\u{1FB82}\u{1FB82}\u{1FB82}\u{1FB82}\u{1FB82}\u{1FB82}\u{1FB82}\u{1FB82}\u{1FB82}\u{1FB82}",
        "\u{1CD03}\u{1CD03}\u{1CD03}\u{1CD03}\u{1CD03}\u{1CD03}\u{1CD03}\u{1CD03}\u{1CD03}\u{1CD03}\u{1CD03}\u{1CD03}",
        "\u{1FB3D}\u{1FB3F}\u{1FB51}\u{2588}\u{1FB3D}\u{1FB3F}\u{1FB51}\u{2588}\u{1FB3D}\u{1FB3F}\u{1FB51}\u{2588}",
    ] {
        rows.push(format!("|{run}|"));
    }
    rows
}

/// The glyph chart.
struct GlyphChart;

impl Widget for GlyphChart {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    /// The chart takes focus while it shows, so the gym's keys still apply.
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        let area = ctx.view().view_rect_local();
        render.fill("", area, ' ')?;
        for (row, text) in chart_rows().iter().enumerate() {
            let Ok(row) = u32::try_from(row) else {
                break;
            };
            if row >= area.h {
                break;
            }
            render.text(
                "",
                Line::new(area.tl.x, area.tl.y.saturating_add(row), area.w),
                text,
            )?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("glyph_chart")
    }
}

/// The mounted nodes of the gym.
struct Nodes {
    /// The label row.
    label: TypedId<Text>,
    /// The image view.
    view: TypedId<ImageView>,
    /// The glyph chart.
    chart: TypedId<GlyphChart>,
}

/// The image viewer gym.
pub struct ImgGym {
    /// The image view, before the gym mounts.
    view: Option<ImageView>,
    /// The mounted nodes.
    nodes: Option<Nodes>,
    /// The view's own tier, or none to follow the terminal's glyphs.
    tier: Option<Tier>,
    /// Whether the stage shows the glyph chart.
    chart: bool,
}

#[derive_commands]
impl ImgGym {
    /// Construct the gym around an image view.
    pub fn new(view: ImageView) -> Self {
        Self {
            view: Some(view),
            nodes: None,
            tier: None,
            chart: false,
        }
    }

    /// The text of the label row.
    fn label(&self) -> String {
        let stage = if self.chart { "glyph chart" } else { "image" };
        format!(
            "glyphs: {} · showing the {stage} · t: next tier · c: chart",
            mosaic::label(self.tier)
        )
    }

    /// Show the label, and the image or the chart.
    fn sync(&self, ctx: &mut dyn Context) -> Result<()> {
        let Some(nodes) = &self.nodes else {
            return Ok(());
        };
        let label = self.label();
        ctx.with_widget_mut(nodes.label, |text: &mut Text, _| {
            text.set_text(label);
            Ok(())
        })?;
        ctx.set_hidden(nodes.view.into(), self.chart)?;
        ctx.set_hidden(nodes.chart.into(), !self.chart)?;
        // The stage that shows holds the focus, so the gym's keys apply.
        let stage = if self.chart {
            nodes.chart.into()
        } else {
            nodes.view.into()
        };
        ctx.set_focus(stage)?;
        Ok(())
    }

    /// Step the image to the next mosaic tier, to compare them.
    #[command]
    pub fn cycle_tier(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.tier = mosaic::cycle(self.tier);
        if let Some(nodes) = &self.nodes {
            let tier = self.tier;
            ctx.with_widget_mut(nodes.view, |view: &mut ImageView, _| {
                view.set_tier(tier);
                Ok(())
            })?;
        }
        self.sync(ctx)
    }

    /// Show the glyph chart, or the image.
    #[command]
    pub fn toggle_chart(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.chart = !self.chart;
        self.sync(ctx)
    }
}

impl Widget for ImgGym {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let view = self
            .view
            .take()
            .ok_or_else(|| Error::Internal("image view missing".into()))?;
        let label = ctx.add_child(ctx.node_id(), Text::new(self.label()))?;
        ctx.set_layout_override(
            label.into(),
            LayoutOverride::new().flex_horizontal(1).fixed_height(1),
        )?;
        let view = ctx.add_child(ctx.node_id(), view)?;
        let chart = ctx.add_child(ctx.node_id(), GlyphChart)?;
        for node in [view.into(), chart.into()] {
            ctx.set_layout_override(
                node,
                LayoutOverride::new().flex_horizontal(1).flex_vertical(1),
            )?;
        }
        self.nodes = Some(Nodes { label, view, chart });
        self.sync(ctx)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("img_gym")
    }
}

impl Register for ImgGym {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use canopy::text;

    use super::*;

    #[test]
    fn every_chart_glyph_has_width_one_and_its_sentinel() {
        for row in chart_rows().iter().filter(|row| row.starts_with('|')) {
            for ch in row.chars() {
                assert_eq!(text::grapheme_width(&ch.to_string()), 1, "{ch:?}");
            }
        }
        let glyphs: usize = chart_rows()
            .iter()
            .filter(|row| row.starts_with('|') && row.matches('|').count() > 2)
            .map(|row| row.chars().filter(|ch| *ch != '|').count())
            .sum();
        assert!(
            glyphs >= 400,
            "every glyph of the full tier appears: {glyphs}"
        );
    }
}
