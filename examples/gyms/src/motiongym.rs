//! Motion gym: colors that change over time.
//!
//! Each row fills a swatch with a moving paint. Frames publish at rest, and
//! the terminal adapter moves the swatches without layout or render.

use std::time::Duration;

use canopy::{
    CanopyBuilder, NodeName, ViewContext, Widget,
    error::Result,
    geom::{Line, Rect},
    layout::Layout,
    render::Render,
    style::{
        Animation, Color, Drift, GradientSpec, GradientStop, Mix, Paint, Pause, StyleRules,
        themes::Palette,
    },
};

/// Width of the label column.
const LABEL_WIDTH: u32 = 12;
/// Width of each swatch.
const SWATCH_WIDTH: u32 = 32;

/// Rows: a label and the style path of its swatch.
const ROWS: [(&str, &str); 7] = [
    ("blink", "motiongym/blink"),
    ("pulse", "motiongym/pulse"),
    ("fade", "motiongym/fade"),
    ("hue loop", "motiongym/hue"),
    ("drift", "motiongym/drift"),
    ("sweep", "motiongym/sweep"),
    ("text", "motiongym/text"),
];

/// Demo node that paints the motion swatches.
#[derive(Default)]
pub struct MotionGym;

impl MotionGym {
    /// Construct the motion gym.
    pub fn new() -> Self {
        Self
    }
}

impl Widget for MotionGym {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();
        r.fill("", rect, ' ')?;
        let origin = view.content_origin();
        for (index, (label, style)) in ROWS.iter().enumerate() {
            let y = origin.y.saturating_add(2 * index as u32 + 1);
            let x = origin.x.saturating_add(1);
            r.text("motiongym/label", Line::new(x, y, LABEL_WIDTH), label)?;
            let swatch = x.saturating_add(LABEL_WIDTH);
            if *style == "motiongym/text" {
                let text = "the quick brown fox";
                r.text(style, Line::new(swatch, y, text.len() as u32), text)?;
            } else {
                r.fill(style, Rect::new(swatch, y, SWATCH_WIDTH, 1), ' ')?;
            }
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("motion_gym")
    }
}

/// Seconds.
fn secs(n: f32) -> Duration {
    Duration::from_secs_f32(n)
}

/// Build the swatch paints from a palette.
fn swatches(p: &Palette, rules: StyleRules<'_>) {
    let hue = Animation::new(
        vec![
            GradientStop::new(0.0, p.red),
            GradientStop::new(0.33, p.green),
            GradientStop::new(0.67, p.blue),
            GradientStop::new(1.0, p.red),
        ],
        secs(6.0),
    )
    .with_mix(Mix::Oklch);
    let drift = GradientSpec::with_stops(
        0.0,
        vec![
            GradientStop::new(0.0, p.violet),
            GradientStop::new(0.5, p.cyan),
            GradientStop::new(1.0, p.violet),
        ],
    )
    .with_drift(Drift::Slide(secs(3.0)));
    // A busy crest: it swings to and fro, and moves on while input is idle.
    let sweep = GradientSpec::with_stops(
        0.0,
        vec![
            GradientStop::new(0.0, p.element_bg),
            GradientStop::new(0.35, p.element_bg),
            GradientStop::new(0.5, p.blue),
            GradientStop::new(0.65, p.element_bg),
            GradientStop::new(1.0, p.element_bg),
        ],
    )
    .with_drift(Drift::Sweep(secs(3.0)))
    .with_pause(Pause::Never);
    rules
        .fg("motiongym/label", p.muted_fg)
        .bg(
            "motiongym/blink",
            Animation::blink(p.accent, p.element_bg, secs(0.5), secs(0.5)),
        )
        .bg(
            "motiongym/pulse",
            Animation::pulse(p.blue, p.magenta, secs(2.0)),
        )
        .bg("motiongym/fade", Animation::fade(p.bg, p.green, secs(2.0)))
        .bg("motiongym/hue", hue)
        .bg("motiongym/drift", Paint::gradient(drift))
        .bg("motiongym/sweep", Paint::gradient(sweep))
        .fg(
            "motiongym/text",
            Animation::pulse(
                p.fg,
                Color::Rgb { r: 0, g: 0, b: 0 }.mix(p.fg, 0.3, Mix::Oklab),
                secs(1.5),
            ),
        )
        .apply();
}

/// The root's default bindings.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()
"#;

/// Queue this demo's styles and bindings.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder
        .configure(|setup| {
            setup.widget_styles(swatches);
            Ok(())
        })
        .script("motiongym", DEFAULT_BINDINGS)
}
