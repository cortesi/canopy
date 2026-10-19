//! The font gym: TrueType fonts drawn as banners, with controls for the
//! text, its size, its effects, and the mosaic tier that draws it.

use std::{iter, time::Duration};

use canopy::{
    CanopyBuilder, Context, ContextExt, NodeName, Register, Setup, TypedId, ViewContext, Widget,
    derive_commands,
    error::{Error, Result},
    geom::{Line, Size},
    layout::{Align, Edges, Layout, MeasureConstraints, Measurement, ScrollOp},
    render::Render,
    rgb,
    style::{Attr, Color, Drift, GradientSpec, GradientStop, Paint, StyleRules, themes::Palette},
    text,
};
use canopy_widgets::{
    BoxGlyphs, Button, Container, FontBanner, Frame, Input, List, Selectable, Text,
    font::{Font, FontEffects, FontRenderer, LayoutOptions},
    mosaic::{self, Tier},
};

/// Initial text rendered by the banners.
const DEFAULT_TEXT: &str = "Canopy";
/// Initial height of each font banner, in rows.
const BANNER_HEIGHT: u32 = 16;
/// Least height of a font banner, in rows.
const MIN_BANNER_HEIGHT: u32 = 4;
/// Columns of the label at the start of each row of controls.
const CONTROL_LABEL_WIDTH: u32 = 8;
/// Buttons to a row in a group of controls, so the panel fits a terminal 80
/// columns wide.
const CONTROL_COLUMNS: usize = 3;
/// Label height beneath each banner.
const LABEL_HEIGHT: u32 = 1;
/// Gap between banner and label.
const LABEL_GAP: u32 = 1;
/// Time in which a banner gradient slides once across its banner.
const GRADIENT_DRIFT: Duration = Duration::from_secs(6);
/// Base angle for gradient direction.
const GRADIENT_BASE_ANGLE: f32 = 25.0;
/// One banner: its style path, gradient angle and palette, and font bytes.
struct BannerSpec {
    /// Style path the banner renders through.
    style: &'static str,
    /// Gradient angle offset from `GRADIENT_BASE_ANGLE`.
    angle_offset: f32,
    /// Four-stop gradient palette.
    palette: [Color; 4],
    /// Embedded font bytes.
    font: &'static [u8],
}

/// The banners the demo renders, in display order.
const BANNERS: [BannerSpec; 4] = [
    BannerSpec {
        style: "font/banner/solar",
        angle_offset: 0.0,
        palette: [
            rgb!("#FFF200"),
            rgb!("#FF9F00"),
            rgb!("#FF003C"),
            rgb!("#7A00FF"),
        ],
        font: include_bytes!("../../../crates/canopy-widgets/assets/fonts/Bungee-Regular.ttf"),
    },
    BannerSpec {
        style: "font/banner/ocean",
        angle_offset: 120.0,
        palette: [
            rgb!("#00F5FF"),
            rgb!("#0084FF"),
            rgb!("#003BFF"),
            rgb!("#00FF9D"),
        ],
        font: include_bytes!("../../../crates/canopy-widgets/assets/fonts/FiraMono-Regular.ttf"),
    },
    BannerSpec {
        style: "font/banner/ember",
        angle_offset: 240.0,
        palette: [
            rgb!("#FFD000"),
            rgb!("#FF7A00"),
            rgb!("#FF1F00"),
            rgb!("#B00000"),
        ],
        font: include_bytes!("../../../crates/canopy-widgets/assets/fonts/FiraMono-Regular.ttf"),
    },
    BannerSpec {
        style: "font/banner/violet",
        angle_offset: 300.0,
        palette: [
            rgb!("#FFD6FF"),
            rgb!("#B5179E"),
            rgb!("#7209B7"),
            rgb!("#4361EE"),
        ],
        font: include_bytes!("../../../crates/canopy-widgets/assets/fonts/Tangerine-Regular.ttf"),
    },
];

/// A text effect that the controls toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    /// Thicker strokes.
    Bold,
    /// Slanted glyphs.
    Italic,
    /// A line beneath the text.
    Underline,
    /// Lower contrast.
    Dim,
    /// A line above the text.
    Overline,
    /// A line through the text.
    Strike,
}

impl Effect {
    /// Every effect, in the order the controls show them.
    const ALL: [Self; 6] = [
        Self::Bold,
        Self::Italic,
        Self::Underline,
        Self::Dim,
        Self::Overline,
        Self::Strike,
    ];

    /// The name of the effect, as its control shows it.
    fn name(self) -> &'static str {
        match self {
            Self::Bold => "Bold",
            Self::Italic => "Italic",
            Self::Underline => "Underline",
            Self::Dim => "Dim",
            Self::Overline => "Overline",
            Self::Strike => "Strike",
        }
    }

    /// The effect a command names, without regard to case.
    fn named(name: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|effect| effect.name().eq_ignore_ascii_case(name))
            .ok_or_else(|| Error::Invalid(format!("no text effect named {name}")))
    }

    /// The letter of the name that its key, Ctrl and the letter, uses.
    fn key(self) -> char {
        self.name().chars().next().unwrap_or(' ')
    }

    /// The flag of the effect in `effects`.
    fn flag(self, effects: &mut FontEffects) -> &mut bool {
        match self {
            Self::Bold => &mut effects.bold,
            Self::Italic => &mut effects.italic,
            Self::Underline => &mut effects.underline,
            Self::Dim => &mut effects.dim,
            Self::Overline => &mut effects.overline,
            Self::Strike => &mut effects.strike,
        }
    }

    /// Whether the effect is on in `effects`.
    fn on(self, mut effects: FontEffects) -> bool {
        *self.flag(&mut effects)
    }

    /// The label of its control, a box checked when the effect is on.
    fn label(self, on: bool) -> String {
        format!("{} {}", if on { "[x]" } else { "[ ]" }, self.name())
    }
}

/// The tiers the glyph controls offer: the terminal's own glyphs, then each
/// tier from half blocks to the full tier.
fn tiers() -> impl Iterator<Item = Option<Tier>> {
    iter::once(None).chain(Tier::ALL.map(Some))
}

/// The label of a glyph control, marked when its tier is the one in use.
fn tier_label(tier: Option<Tier>, chosen: bool) -> String {
    format!(
        "{} {}",
        if chosen { "(•)" } else { "( )" },
        mosaic::label(tier)
    )
}

/// The value the size control shows.
fn size_label(height: u32) -> String {
    format!("{height} rows")
}

/// The nodes that the gym updates as its state changes.
struct Nodes {
    /// The list of font blocks.
    list: TypedId<List<FontBlock>>,
    /// The font blocks.
    blocks: Vec<TypedId<FontBlock>>,
    /// The value of the size control.
    size: TypedId<Text>,
    /// The glyph controls, one to each tier in [`tiers`] order.
    tiers: Vec<TypedId<Button>>,
    /// The effect controls, one to each effect in [`Effect::ALL`] order.
    effects: Vec<TypedId<Button>>,
}

/// Demo node that renders TrueType fonts as banners, under one panel of
/// controls that change the banners and show what they are set to.
pub struct FontGym {
    /// The text the banners draw.
    text: String,
    /// The height of each banner, in rows.
    height: u32,
    /// The effects the banners draw with.
    effects: FontEffects,
    /// The banners' own mosaic tier, or none to follow the terminal.
    tier: Option<Tier>,
    /// The nodes to update, once mounted.
    nodes: Option<Nodes>,
}

impl Default for FontGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl FontGym {
    /// Construct a new font gym demo.
    pub fn new() -> Self {
        Self {
            text: DEFAULT_TEXT.to_owned(),
            height: BANNER_HEIGHT,
            effects: FontEffects::default(),
            tier: None,
            nodes: None,
        }
    }

    /// Draw `text` in every banner.
    #[command]
    pub fn set_text(&mut self, c: &mut dyn Context, text: String) -> Result<()> {
        self.text = text;
        self.each_block(c, |block, ctx, gym| block.set_text(ctx, gym.text.clone()))
    }

    /// Change the height of the banners by `delta` rows.
    #[command]
    pub fn resize(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        let height = self
            .height
            .saturating_add_signed(delta)
            .max(MIN_BANNER_HEIGHT);
        if height == self.height {
            return Ok(());
        }
        self.height = height;
        self.each_block(c, |block, ctx, gym| {
            block.set_banner_height(ctx, gym.height)
        })?;
        if let Some(nodes) = &self.nodes {
            for block in &nodes.blocks {
                c.set_layout_override((*block).into(), block_layout(height).into())?;
            }
        }
        self.sync_controls(c)
    }

    /// Turn the effect named `effect` on or off.
    #[command]
    #[allow(
        clippy::needless_pass_by_value,
        reason = "command arguments are owned values"
    )]
    pub fn toggle(&mut self, c: &mut dyn Context, effect: String) -> Result<()> {
        let flag = Effect::named(&effect)?.flag(&mut self.effects);
        *flag = !*flag;
        self.each_block(c, |block, ctx, gym| block.set_effects(ctx, gym.effects))?;
        self.sync_controls(c)
    }

    /// Draw the banners in the tier at `index` of the glyph controls: 0
    /// follows the terminal, and 1 to 4 run from half blocks to the full
    /// tier.
    #[command]
    pub fn set_tier(&mut self, c: &mut dyn Context, index: usize) -> Result<()> {
        let tier = tiers()
            .nth(index)
            .ok_or_else(|| Error::Invalid(format!("no mosaic tier at {index}")))?;
        self.use_tier(c, tier)
    }

    /// Draw the banners in the next tier of the glyph controls, wrapping to
    /// the terminal's own glyphs.
    #[command]
    pub fn next_tier(&mut self, c: &mut dyn Context) -> Result<()> {
        self.use_tier(c, mosaic::cycle(self.tier))
    }

    /// Scroll the fonts by `lines` rows: negative moves up.
    #[command]
    pub fn scroll(&self, c: &mut dyn Context, lines: i32) -> Result<()> {
        self.scroll_fonts(c, ScrollOp::By(0, lines))
    }

    /// Scroll the fonts by `pages` pages: negative moves up.
    #[command]
    pub fn page(&self, c: &mut dyn Context, pages: i32) -> Result<()> {
        self.scroll_fonts(c, ScrollOp::pages(pages))
    }

    /// Draw the banners in `tier`.
    fn use_tier(&mut self, c: &mut dyn Context, tier: Option<Tier>) -> Result<()> {
        self.tier = tier;
        self.each_block(c, |block, ctx, gym| block.set_tier(ctx, gym.tier))?;
        self.sync_controls(c)
    }

    /// Scroll the list of fonts.
    fn scroll_fonts(&self, c: &mut dyn Context, op: ScrollOp) -> Result<()> {
        let Some(nodes) = &self.nodes else {
            return Ok(());
        };
        c.with_widget_mut(nodes.list, |_: &mut List<FontBlock>, ctx| {
            ctx.scroll(op);
            Ok(())
        })
    }

    /// Apply `f` to every font block.
    fn each_block(
        &self,
        c: &mut dyn Context,
        f: impl Fn(&mut FontBlock, &mut dyn Context, &Self) -> Result<()>,
    ) -> Result<()> {
        let Some(nodes) = &self.nodes else {
            return Ok(());
        };
        for block in &nodes.blocks {
            c.with_widget_mut(*block, |block: &mut FontBlock, ctx| f(block, ctx, self))?;
        }
        Ok(())
    }

    /// Show the current state in the controls.
    fn sync_controls(&self, c: &mut dyn Context) -> Result<()> {
        let Some(nodes) = &self.nodes else {
            return Ok(());
        };
        let height = self.height;
        c.with_widget_mut(nodes.size, |text: &mut Text, _| {
            text.set_text(size_label(height));
            Ok(())
        })?;
        for (tier, button) in tiers().zip(&nodes.tiers) {
            let label = tier_label(tier, tier == self.tier);
            c.with_widget_mut(*button, |button: &mut Button, _| {
                button.set_label(label);
                Ok(())
            })?;
        }
        for (effect, button) in Effect::ALL.into_iter().zip(&nodes.effects) {
            let label = effect.label(effect.on(self.effects));
            c.with_widget_mut(*button, |button: &mut Button, _| {
                button.set_label(label);
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Add a row of controls under `parent`, headed by `label`.
    fn control_row(
        c: &mut dyn Context,
        parent: TypedId<Container>,
        label: &str,
    ) -> Result<TypedId<Container>> {
        let row = c.add_child(parent, Container::row())?;
        c.set_layout_override(
            row.into(),
            Layout::row()
                .gap(1)
                .flex_horizontal(1)
                .fixed_height(1)
                .into(),
        )?;
        let label = c.add_child(row, Text::new(label).with_style("font/control"))?;
        c.set_layout_override(
            label.into(),
            Layout::fill().fixed_width(CONTROL_LABEL_WIDTH).into(),
        )?;
        Ok(row)
    }

    /// Add a group of `buttons` under `parent`, headed by `label`, in rows of
    /// [`CONTROL_COLUMNS`], each column as wide as its widest button.
    fn button_grid(
        c: &mut dyn Context,
        parent: TypedId<Container>,
        label: &str,
        buttons: Vec<Button>,
    ) -> Result<Vec<TypedId<Button>>> {
        // A row between rows of buttons keeps their faces apart.
        let group = c.add_child(parent, Container::column())?;
        let rows = u32::try_from(buttons.len().div_ceil(CONTROL_COLUMNS)).unwrap_or(u32::MAX);
        c.set_layout_override(
            group.into(),
            Layout::column()
                .gap(1)
                .flex_horizontal(1)
                .fixed_height(rows * 2 - 1)
                .into(),
        )?;
        let widths: Vec<u32> = (0..CONTROL_COLUMNS)
            .map(|column| {
                buttons
                    .iter()
                    .skip(column)
                    .step_by(CONTROL_COLUMNS)
                    .map(|button| button.size().w)
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let mut ids = Vec::with_capacity(buttons.len());
        let mut row = None;
        for (index, button) in buttons.into_iter().enumerate() {
            let column = index % CONTROL_COLUMNS;
            if column == 0 {
                let heading = if index == 0 { label } else { "" };
                row = Some(Self::control_row(c, group, heading)?);
            }
            let Some(row) = row else {
                continue;
            };
            let id = c.add_child(row, button)?;
            c.set_layout_override(id.into(), Layout::fill().fixed_width(widths[column]).into())?;
            ids.push(id);
        }
        Ok(ids)
    }

    /// Rows of the panel of controls, inside its frame: text, size, and the
    /// glyph and style groups, with a row between each group and between the
    /// rows of a group.
    fn control_rows() -> u32 {
        let grid =
            |count: usize| u32::try_from(count.div_ceil(CONTROL_COLUMNS)).unwrap_or(0) * 2 - 1;
        let groups = 4;
        1 + 1 + grid(tiers().count()) + grid(Effect::ALL.len()) + (groups - 1)
    }

    /// Build the panel of controls over the fonts in `list`, and return it
    /// with its text field and the nodes that the gym updates.
    fn mount_controls(
        &self,
        c: &mut dyn Context,
        list: TypedId<List<FontBlock>>,
        blocks: Vec<TypedId<FontBlock>>,
    ) -> Result<(TypedId<Frame>, TypedId<Input>, Nodes)> {
        let frame = c.create_detached(
            Frame::new()
                .with_title("Controls")
                .with_glyphs(BoxGlyphs::SINGLE_THICK),
        )?;
        let rows = c.add_child(frame, Container::column())?;
        c.set_layout_override(
            rows.into(),
            Layout::column()
                .gap(1)
                .flex_horizontal(1)
                .padding(Edges::symmetric(0, 1))
                .into(),
        )?;

        let text = Self::control_row(c, rows, "Text")?;
        let input = c.add_child(
            text,
            Input::new(&self.text).with_on_change(Self::spec_set_text().call()),
        )?;
        c.set_layout_override(input.into(), Layout::fill().flex_horizontal(1).into())?;

        let size_row = Self::control_row(c, rows, "Size")?;
        c.add_child(
            size_row,
            Button::new("−").with_command(Self::call_resize(-1)),
        )?;
        let size = c.add_child(
            size_row,
            Text::new(size_label(self.height)).with_style("font/value"),
        )?;
        c.set_layout_override(size.into(), Layout::fill().fixed_width(8).into())?;
        c.add_child(
            size_row,
            Button::new("+").with_command(Self::call_resize(1)),
        )?;

        let tier_buttons = tiers()
            .enumerate()
            .map(|(index, tier)| {
                Button::new(tier_label(tier, tier == self.tier))
                    .with_command(Self::call_set_tier(index))
            })
            .collect();
        let tier_buttons = Self::button_grid(c, rows, "Glyphs", tier_buttons)?;

        let effect_buttons = Effect::ALL
            .into_iter()
            .map(|effect| {
                Button::new(effect.label(effect.on(self.effects)))
                    .with_accelerator(effect.key())
                    .with_command(Self::call_toggle(effect.name().to_lowercase()))
            })
            .collect();
        let effect_buttons = Self::button_grid(c, rows, "Style", effect_buttons)?;

        let nodes = Nodes {
            list,
            blocks,
            size,
            tiers: tier_buttons,
            effects: effect_buttons,
        };
        Ok((frame, input, nodes))
    }
}

impl Widget for FontGym {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let list = ctx.create_detached(List::new())?;
        ctx.set_layout_override(list.into(), Layout::fill().into())?;
        let centered = LayoutOptions {
            h_align: Align::Center,
            v_align: Align::Center,
            ..LayoutOptions::default()
        };
        let (effects, height, text) = (self.effects, self.height, self.text.clone());
        let blocks = ctx.with_widget_mut(list, |list: &mut List<FontBlock>, ctx| {
            BANNERS
                .iter()
                .map(|spec| {
                    let font = Font::from_bytes(spec.font)?;
                    let label = font_label(&font);
                    let banner = FontBanner::new(text.clone(), FontRenderer::new(font))
                        .with_effects(effects)
                        .with_style(spec.style)
                        .with_layout_options(centered);
                    let id = list.append(ctx, FontBlock::new(banner, label, height))?;
                    ctx.set_layout_override(id.into(), block_layout(height).into())?;
                    Ok(id)
                })
                .collect::<Result<Vec<_>>>()
        })?;
        let fonts = ctx.create_detached(
            Frame::new()
                .with_title("Fonts")
                .with_glyphs(BoxGlyphs::SINGLE_THICK),
        )?;
        ctx.set_children(fonts.into(), vec![list.into()])?;
        ctx.set_layout_override(fonts.into(), Layout::fill().padding(Edges::all(1)).into())?;

        let (controls, input, nodes) = self.mount_controls(ctx, list, blocks)?;
        // The rows of controls, and the frame.
        ctx.set_layout_override(controls.into(), crate::fixed_row(Self::control_rows() + 2))?;
        let stack = ctx.add_child(ctx.node_id(), Container::column())?;
        ctx.set_children(stack.into(), vec![controls.into(), fonts.into()])?;
        ctx.set_focus(input.into())?;
        self.nodes = Some(nodes);
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("font_gym")
    }
}

impl Register for FontGym {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        Input::register(setup)
    }
}

/// Composite widget that renders a banner with a label beneath it.
struct FontBlock {
    /// Banner widget before mounting.
    banner: Option<FontBanner>,
    /// Mounted banner node ID.
    banner_id: Option<canopy::TypedId<FontBanner>>,
    /// Label text shown beneath the banner.
    label: String,
    /// Current banner height in rows.
    banner_height: u32,
}

impl FontBlock {
    /// Construct a new font block with a banner and label.
    fn new(banner: FontBanner, label: impl Into<String>, banner_height: u32) -> Self {
        Self {
            banner: Some(banner),
            banner_id: None,
            label: label.into(),
            banner_height,
        }
    }

    /// Apply a mutation to the mounted banner, or to the unmounted banner
    /// widget before it is mounted.
    fn with_banner(
        &mut self,
        ctx: &mut dyn Context,
        f: impl FnOnce(&mut FontBanner),
    ) -> Result<()> {
        if let Some(banner_id) = self.banner_id {
            ctx.with_widget_mut(banner_id, |banner: &mut FontBanner, _| {
                f(banner);
                Ok(())
            })?;
        } else if let Some(banner) = self.banner.as_mut() {
            f(banner);
        }
        Ok(())
    }

    /// Update the banner text.
    fn set_text(&mut self, ctx: &mut dyn Context, text: String) -> Result<()> {
        self.with_banner(ctx, |banner| banner.set_text(text))
    }

    /// Update the banner effects.
    fn set_effects(&mut self, ctx: &mut dyn Context, effects: FontEffects) -> Result<()> {
        self.with_banner(ctx, |banner| banner.set_effects(effects))
    }

    /// Update the banner's own mosaic tier.
    fn set_tier(&mut self, ctx: &mut dyn Context, tier: Option<Tier>) -> Result<()> {
        self.with_banner(ctx, |banner| banner.set_tier(tier))
    }

    /// Update the banner height.
    fn set_banner_height(&mut self, ctx: &mut dyn Context, height: u32) -> Result<()> {
        self.banner_height = height;
        if let Some(banner_id) = self.banner_id {
            ctx.set_layout_override(banner_id.into(), Layout::fill().fixed_height(height).into())?;
        }
        Ok(())
    }
}

impl Selectable for FontBlock {
    fn set_selected(&mut self, _selected: bool) {}
}

impl Widget for FontBlock {
    fn layout(&self) -> Layout {
        Layout::column().gap(LABEL_GAP)
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let banner = self.banner.take().expect("banner available on mount");
        let banner_id = ctx.create_detached(banner)?;
        ctx.set_layout_override(
            banner_id.into(),
            Layout::fill().fixed_height(self.banner_height).into(),
        )?;

        let label_id = ctx.create_detached(FontLabel::new(self.label.clone(), "font/label"))?;
        ctx.set_layout_override(
            label_id.into(),
            Layout::fill().fixed_height(LABEL_HEIGHT).into(),
        )?;

        ctx.set_children(ctx.node_id(), vec![banner_id.into(), label_id.into()])?;
        self.banner_id = Some(banner_id);
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("fontgym-block")
    }
}

/// Center-aligned single-line label.
struct FontLabel {
    /// Label text.
    text: String,
    /// Style path for label rendering.
    style: String,
}

impl FontLabel {
    /// Create a label with the provided style.
    fn new(text: impl Into<String>, style: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: style.into(),
        }
    }
}

impl Widget for FontLabel {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let origin = view.content_origin();
        if view_rect.w == 0 || view_rect.h == 0 {
            return Ok(());
        }
        let full_width = text::width(&self.text) as u32;
        let available = view_rect.w.max(1);
        let offset = if full_width >= available {
            0
        } else {
            (available - full_width) / 2
        };
        let (out, out_width) = text::slice_by_columns(&self.text, 0, available as usize);
        if out_width == 0 {
            return Ok(());
        }
        let line = Line::new(origin.x.saturating_add(offset), origin.y, out_width as u32);
        rndr.text(&self.style, line, out)?;
        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let width = text::width(&self.text).max(1) as u32;
        c.clamp(Size::new(width, 1))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("fontgym-label")
    }
}

/// Build a label string for a font.
fn font_label(font: &Font) -> String {
    let name = font.name().unwrap_or("Unknown font");
    format!("Font: {name}")
}

/// Compute total block height including the label.
fn block_height(banner_height: u32) -> u32 {
    banner_height
        .saturating_add(LABEL_GAP)
        .saturating_add(LABEL_HEIGHT)
}

/// Layout for a font block with the provided banner height.
fn block_layout(banner_height: u32) -> Layout {
    Layout::column()
        .gap(LABEL_GAP)
        .flex_horizontal(1)
        .fixed_height(block_height(banner_height))
}

/// Construct a banner gradient that drifts across its banner.
///
/// The stops return to the first color, so the gradient wraps without a
/// seam.
fn drifting_gradient(angle_deg: f32, colors: [Color; 4]) -> Paint {
    Paint::gradient(
        GradientSpec::with_stops(
            angle_deg,
            vec![
                GradientStop::new(0.0, colors[0]),
                GradientStop::new(0.25, colors[1]),
                GradientStop::new(0.5, colors[2]),
                GradientStop::new(0.75, colors[3]),
                GradientStop::new(1.0, colors[0]),
            ],
        )
        .with_drift(Drift::Slide(GRADIENT_DRIFT)),
    )
}

/// Install the gym's styles, over whatever theme is in use.
fn setup_style(_palette: &Palette, rules: StyleRules<'_>) {
    let mut rules = rules
        .attr("font/control", Attr::Bold)
        .fg("font/control", rgb!("#E9ECEF"))
        .attr("font/value", Attr::Bold)
        .fg("font/value", rgb!("#FFD166"))
        .fg("font/label", rgb!("#A3B1C2"));
    for spec in &BANNERS {
        rules = rules.fg(
            spec.style,
            drifting_gradient(GRADIENT_BASE_ANGLE + spec.angle_offset, spec.palette),
        );
    }
    rules.apply();
}

/// The gym's bindings. Every key it answers is a binding here, so the help
/// lists them all; the buttons show what each control is set to.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()

canopy.keymap({
    path = "font_gym/",
    phase = "before_widget",
    { key = "Tab", description = "Next control", action = command.root.focus("next") },
    { key = "BackTab", description = "Previous control", action = command.root.focus("prev") },
})

canopy.keymap({
    path = "font_gym/",
    { key = "Ctrl+Up", description = "Larger text", action = command.font_gym.resize(1) },
    { key = "Ctrl+Down", description = "Smaller text", action = command.font_gym.resize(-1) },
    { key = "Ctrl+t", description = "Next mosaic tier", action = command.font_gym.next_tier() },
    { key = "Ctrl+b", description = "Toggle bold", action = command.font_gym.toggle("bold") },
    { key = "Ctrl+i", description = "Toggle italic", action = command.font_gym.toggle("italic") },
    {
        key = "Ctrl+u",
        description = "Toggle underline",
        action = command.font_gym.toggle("underline"),
    },
    { key = "Ctrl+d", description = "Toggle dim", action = command.font_gym.toggle("dim") },
    {
        key = "Ctrl+o",
        description = "Toggle overline",
        action = command.font_gym.toggle("overline"),
    },
    { key = "Ctrl+s", description = "Toggle strike", action = command.font_gym.toggle("strike") },
    { key = "Up", description = "Scroll fonts up", action = command.font_gym.scroll(-1) },
    { key = "Down", description = "Scroll fonts down", action = command.font_gym.scroll(1) },
    { key = "PageUp", description = "Page fonts up", action = command.font_gym.page(-1) },
    { key = "PageDown", description = "Page fonts down", action = command.font_gym.page(1) },
})
"#;

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder
        .configure(|setup| {
            setup.widget_styles(setup_style);
            Ok(())
        })
        .script("font", DEFAULT_BINDINGS)
}

#[cfg(test)]
mod tests {
    use canopy::{ViewContextExt, geom::Point, input::key::Key, testing::harness::Harness};

    use super::*;
    use crate::tests::{Mount, root_harness};

    /// Returns a harness of the gym at `size`.
    fn gym(size: Size) -> Result<Harness> {
        root_harness(
            FontGym::new(),
            |builder| binding_setup(builder.configure(FontGym::register)),
            size,
            Mount::Wrap,
        )
    }

    /// Sends key specs in order, then renders.
    fn keys(harness: &mut Harness, specs: &[&str]) -> Result<()> {
        for spec in specs {
            harness.key(Key::parse_spec(spec)?)?;
        }
        harness.render()
    }

    /// Returns the screen as text, one line a row.
    fn screen(harness: &Harness) -> String {
        harness.tbuf().lines().join("\n")
    }

    /// Reads the gym's state through `f`.
    fn state<R>(harness: &mut Harness, f: impl FnOnce(&FontGym) -> R) -> Result<R> {
        let gym = harness
            .canopy
            .with_root_view(|ctx| ctx.unique_descendant::<FontGym>(ctx.root_id()))?
            .ok_or_else(|| Error::NotFound("font gym".into()))?;
        harness
            .canopy
            .with_root_context(|ctx| ctx.with_widget_mut(gym, |gym: &mut FontGym, _| Ok(f(gym))))
    }

    /// Returns the scroll offset of the list of fonts.
    fn font_list_scroll(harness: &Harness) -> Result<Point> {
        harness.canopy.with_root_view(|ctx| {
            let list = ctx
                .unique_descendant::<List<FontBlock>>(ctx.root_id())?
                .ok_or_else(|| Error::NotFound("font list".into()))?;
            Ok(ctx.view_of(list.into()).expect("font list view").scroll)
        })
    }

    #[test]
    fn the_text_field_has_the_keyboard_and_typing_draws_in_every_banner() -> Result<()> {
        let mut harness = gym(Size::new(110, 36))?;
        keys(&mut harness, &["End", "!"])?;
        assert_eq!(state(&mut harness, |gym| gym.text.clone())?, "Canopy!");
        Ok(())
    }

    #[test]
    fn keys_change_the_banners_and_the_controls_show_what_they_are_set_to() -> Result<()> {
        let mut harness = gym(Size::new(110, 36))?;
        let shown = screen(&harness);
        for expected in [
            "16 rows",
            "(•) terminal",
            "( ) halves",
            "[ ] Bold",
            "[ ] Strike",
        ] {
            assert!(shown.contains(expected), "{expected}:\n{shown}");
        }

        keys(
            &mut harness,
            &["Ctrl+b", "Ctrl+s", "Ctrl+t", "Ctrl+Up", "Ctrl+Up"],
        )?;
        let shown = screen(&harness);
        for expected in [
            "18 rows",
            "( ) terminal",
            "(•) halves",
            "[x] Bold",
            "[x] Strike",
        ] {
            assert!(shown.contains(expected), "{expected}:\n{shown}");
        }
        let (effects, height, tier) =
            state(&mut harness, |gym| (gym.effects, gym.height, gym.tier))?;
        assert!(effects.bold && effects.strike && !effects.italic);
        assert_eq!((height, tier), (18, Some(Tier::Halves)));
        let tiers: Vec<Option<Tier>> = harness.canopy.with_root_view(|ctx| {
            ctx.descendants::<FontBanner>(ctx.root_id())
                .map(|banner| ctx.with_widget(banner, |banner: &FontBanner| Ok(banner.tier())))
                .collect::<Result<_>>()
        })?;
        assert_eq!(tiers, vec![Some(Tier::Halves); BANNERS.len()]);

        // The smallest banner stops shrinking.
        let shrink = vec!["Ctrl+Down"; 20];
        keys(&mut harness, &shrink)?;
        assert_eq!(state(&mut harness, |gym| gym.height)?, MIN_BANNER_HEIGHT);
        Ok(())
    }

    #[test]
    fn controls_take_focus_in_turn_and_run_their_action() -> Result<()> {
        let mut harness = gym(Size::new(110, 36))?;
        // The text field, the size buttons, then the first two tiers.
        keys(&mut harness, &["Tab", "Tab", "Tab", "Tab", "Enter"])?;
        assert_eq!(state(&mut harness, |gym| gym.tier)?, Some(Tier::Halves));
        keys(&mut harness, &["BackTab", "BackTab", "Space"])?;
        assert_eq!(state(&mut harness, |gym| gym.height)?, BANNER_HEIGHT + 1);
        Ok(())
    }

    #[test]
    fn page_down_scrolls_the_fonts_from_the_text_field() -> Result<()> {
        let mut harness = gym(Size::new(80, 24))?;
        let before = font_list_scroll(&harness)?;
        keys(&mut harness, &["PageDown"])?;
        assert!(font_list_scroll(&harness)?.y > before.y);
        keys(&mut harness, &["PageUp"])?;
        assert_eq!(font_list_scroll(&harness)?, before);
        Ok(())
    }

    #[test]
    fn help_lists_every_key_the_gym_answers() -> Result<()> {
        let mut harness = gym(Size::new(110, 50))?;
        keys(&mut harness, &["Ctrl+g"])?;
        let shown = screen(&harness);
        for description in [
            "Next control",
            "Previous control",
            "Larger text",
            "Smaller text",
            "Next mosaic tier",
            "Toggle bold",
            "Toggle italic",
            "Toggle underline",
            "Toggle dim",
            "Toggle overline",
            "Toggle strike",
            "Scroll fonts up",
            "Scroll fonts down",
            "Page fonts up",
            "Page fonts down",
        ] {
            assert!(shown.contains(description), "{description}:\n{shown}");
        }
        Ok(())
    }

    #[test]
    fn effects_and_tiers_name_their_controls() -> Result<()> {
        for effect in Effect::ALL {
            assert_eq!(Effect::named(&effect.name().to_lowercase())?, effect);
            assert!(effect.label(false).contains(effect.key()));
        }
        assert!(Effect::named("blink").is_err());
        let labels: Vec<String> = tiers().map(|tier| tier_label(tier, false)).collect();
        assert_eq!(
            labels,
            [
                "( ) terminal",
                "( ) halves",
                "( ) quadrants",
                "( ) blocks",
                "( ) full"
            ]
        );
        Ok(())
    }
}
