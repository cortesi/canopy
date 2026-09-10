use std::{f32::consts::TAU, time::Duration};

use canopy::{
    CanopyBuilder, ChangeOutcome, Context, ContextExt, EventOutcome, Loader, NodeId, ViewContext,
    Widget,
    cursor::{Cursor, CursorShape},
    error::Result,
    event::{Event, key},
    geom::{Line, Point, Size},
    layout::{Align, Edges, Layout, MeasureConstraints, Measurement},
    render::Render,
    rgb,
    state::NodeName,
    style::{Attr, Color, StyleMap},
    text,
};
use canopy_widgets::{
    Frame, List, Pad, SINGLE_THICK, Selectable, Text, VStack,
    font::{Font, FontBanner, FontEffects, FontRenderer, LayoutOptions},
    wrap,
};

/// Initial text rendered by the banners.
const DEFAULT_TEXT: &str = "Canopy";
/// Fixed height for the input frame.
const INPUT_HEIGHT: u32 = 3;
/// Fixed height for each font banner.
const BANNER_HEIGHT: u32 = 16;
/// Minimum height for banner blocks.
const MIN_BANNER_HEIGHT: u32 = 4;
/// Fixed height for the status row.
const STATUS_HEIGHT: u32 = 13;
/// Minimum width for the controls panel.
const CONTROLS_PANEL_WIDTH: u32 = 44;
/// Minimum width for the status panel.
const STATUS_PANEL_WIDTH: u32 = 48;
/// Vertical padding inside control panels.
const PANEL_PADDING_V: u32 = 1;
/// Horizontal padding inside control panels.
const PANEL_PADDING_H: u32 = 1;
/// Wrap width for status text.
const STATUS_WRAP_WIDTH: u32 = 44;
/// Label height beneath each banner.
const LABEL_HEIGHT: u32 = 1;
/// Gap between banner and label.
const LABEL_GAP: u32 = 1;
/// Milliseconds between gradient animation steps.
const GRADIENT_POLL_MS: u64 = 50;
/// Phase step for gradient color animation.
const GRADIENT_PHASE_STEP: f32 = 0.01;
/// Base angle for gradient direction.
const GRADIENT_BASE_ANGLE: f32 = 25.0;
/// Hue sweep amplitude for animated palettes.
const HUE_SWEEP_DEG: f32 = 60.0;
/// One banner: its style path, gradient angle and palette, and font bytes.
struct BannerSpec {
    /// Style path the banner renders through.
    style: &'static str,
    /// Gradient angle offset from `GRADIENT_BASE_ANGLE`.
    angle_offset: f32,
    /// Four-stop gradient palette before the animated hue shift.
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
        font: include_bytes!("../../canopy-widgets/assets/fonts/Bungee-Regular.ttf"),
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
        font: include_bytes!("../../canopy-widgets/assets/fonts/FiraMono-Regular.ttf"),
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
        font: include_bytes!("../../canopy-widgets/assets/fonts/FiraMono-Regular.ttf"),
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
        font: include_bytes!("../../canopy-widgets/assets/fonts/Tangerine-Regular.ttf"),
    },
];

/// Demo node that renders ASCII font banners.
pub struct FontGym {
    /// Animated gradient phase.
    gradient_phase: f32,
}

impl Default for FontGym {
    fn default() -> Self {
        Self::new()
    }
}

impl FontGym {
    /// Construct a new font gym demo.
    pub fn new() -> Self {
        Self {
            gradient_phase: 0.0,
        }
    }
}

impl Widget for FontGym {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let style_state = FontEffects::default();
        ctx.set_style(font_styles(self.gradient_phase));

        let list_id = ctx.create_detached(List::new())?;
        ctx.set_layout_of(list_id, Layout::fill())?;

        let blocks = ctx.with_widget_mut(list_id, |list: &mut List<FontBlock>, ctx| {
            let mut ids = Vec::new();
            let centered = LayoutOptions {
                h_align: Align::Center,
                v_align: Align::Center,
            };
            for spec in &BANNERS {
                let font = Font::from_bytes(spec.font).expect("embedded font loads");
                let label = font_label(&font);
                let banner = FontBanner::new(DEFAULT_TEXT, FontRenderer::new(font))
                    .with_effects(style_state)
                    .with_style(spec.style)
                    .with_layout_options(centered);
                let id = list.append(ctx, FontBlock::new(banner, label, BANNER_HEIGHT))?;
                ctx.set_layout_of(id, block_layout(BANNER_HEIGHT))?;
                ids.push(id);
            }

            Ok(ids)
        })?;

        let font_frame_id = ctx.create_detached(FocusFrame::new(
            Frame::new().with_title("Fonts").with_glyphs(SINGLE_THICK),
            list_id,
        ))?;
        ctx.set_children_of(font_frame_id.into(), vec![list_id.into()])?;
        ctx.set_layout_of(font_frame_id, Layout::fill().padding(Edges::all(1)))?;

        let controls_id = ctx.create_detached(ControlsLegend)?;
        let controls_frame = panel(ctx, controls_id, "Controls", CONTROLS_PANEL_WIDTH)?;

        let status_id = ctx.create_detached(
            Text::new(status_text(BANNER_HEIGHT, style_state))
                .with_style("fontgym/legend")
                .with_wrap_width(STATUS_WRAP_WIDTH),
        )?;
        let status_frame = panel(ctx, status_id, "Status", STATUS_PANEL_WIDTH)?;

        let status_row_id = ctx.create_detached(StatusRow)?;
        ctx.set_children_of(status_row_id.into(), vec![controls_frame, status_frame])?;

        let input_id = ctx.create_detached(FontGymInput::new(
            DEFAULT_TEXT,
            blocks,
            BANNER_HEIGHT,
            style_state,
            status_id,
        ))?;
        ctx.set_layout_of(input_id, Layout::fill())?;

        let input_frame = wrap(ctx, input_id, Frame::new().with_title("Text input"))?;
        let stack = VStack::new()
            .push_fixed(input_frame, INPUT_HEIGHT)
            .push_fixed(status_row_id, STATUS_HEIGHT)
            .push_flex(font_frame_id, 1);
        let stack_id = ctx.add_child(stack)?;
        ctx.set_layout_of(stack_id, Layout::fill())?;
        ctx.set_focus(input_id.into())?;
        Ok(())
    }

    fn poll(&mut self, ctx: &mut dyn Context) -> Option<Duration> {
        self.gradient_phase = (self.gradient_phase + GRADIENT_PHASE_STEP).fract();
        ctx.set_style(font_styles(self.gradient_phase));
        Some(Duration::from_millis(GRADIENT_POLL_MS))
    }
}

impl Loader for FontGym {}

/// Focusable frame wrapper that delegates rendering and handles keyboard
/// scroll.
struct FocusFrame {
    /// Inner frame widget.
    frame: Frame,
    /// List widget to scroll.
    list_id: canopy::TypedId<List<FontBlock>>,
}

impl FocusFrame {
    /// Build a new focusable frame for the font list.
    fn new(frame: Frame, list_id: canopy::TypedId<List<FontBlock>>) -> Self {
        Self { frame, list_id }
    }

    /// Scroll the list view using the provided action.
    fn scroll_list(
        &self,
        ctx: &mut dyn Context,
        action: impl FnOnce(&mut dyn Context) -> ChangeOutcome,
    ) -> Result<bool> {
        ctx.with_widget_mut(self.list_id, |_: &mut List<FontBlock>, list_ctx| {
            Ok(action(list_ctx).changed())
        })
    }
}

impl Widget for FocusFrame {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        self.frame.render(rndr, ctx)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Key(raw) = event {
            let normalized = raw.normalize();
            let handled = match normalized.key {
                key::KeyCode::Up => self.scroll_list(ctx, |ctx| ctx.scroll_up())?,
                key::KeyCode::Down => self.scroll_list(ctx, |ctx| ctx.scroll_down())?,
                key::KeyCode::PageUp => self.scroll_list(ctx, |ctx| ctx.page_up())?,
                key::KeyCode::PageDown => self.scroll_list(ctx, |ctx| ctx.page_down())?,
                _ => false,
            };

            if handled {
                return Ok(EventOutcome::Handle);
            }
        }

        self.frame.on_event(event, ctx)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("fontgym-focus-frame")
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

    /// Update the banner height.
    fn set_banner_height(&mut self, ctx: &mut dyn Context, height: u32) -> Result<()> {
        self.banner_height = height;
        if let Some(banner_id) = self.banner_id {
            ctx.set_layout_of(banner_id, Layout::fill().fixed_height(height))?;
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
        ctx.set_layout_of(banner_id, Layout::fill().fixed_height(self.banner_height))?;

        let label_id = ctx.create_detached(FontLabel::new(self.label.clone(), "fontgym/label"))?;
        ctx.set_layout_of(label_id, Layout::fill().fixed_height(LABEL_HEIGHT))?;

        ctx.set_children(vec![banner_id.into(), label_id.into()])?;
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
        let full_width = text::display_width(&self.text) as u32;
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
        let width = text::display_width(&self.text).max(1) as u32;
        c.clamp(Size::new(width, 1))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("fontgym-label")
    }
}

/// Single-line text input that updates font banners.
struct FontGymInput {
    /// Current input text.
    text: String,
    /// Cursor position in characters.
    cursor: usize,
    /// Font blocks to update.
    targets: Vec<canopy::TypedId<FontBlock>>,
    /// Current banner height.
    banner_height: u32,
    /// Active style toggles.
    style_state: FontEffects,
    /// Status text widget to update.
    status_text: canopy::TypedId<Text>,
}

impl FontGymInput {
    /// Create a new input widget targeting the provided banners.
    fn new(
        text: impl Into<String>,
        targets: Vec<canopy::TypedId<FontBlock>>,
        banner_height: u32,
        style_state: FontEffects,
        status_text: canopy::TypedId<Text>,
    ) -> Self {
        let text = text.into();
        let cursor = text.chars().count();
        Self {
            text,
            cursor,
            targets,
            banner_height,
            style_state,
            status_text,
        }
    }

    /// Insert a character at the cursor.
    fn insert_char(&mut self, ch: char) {
        let idx = byte_index_for_char(&self.text, self.cursor);
        self.text.insert(idx, ch);
        self.cursor = self.cursor.saturating_add(1);
    }

    /// Delete the character before the cursor.
    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let idx = byte_index_for_char(&self.text, self.cursor.saturating_sub(1));
        self.text.remove(idx);
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Move the cursor left.
    fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Move the cursor right.
    fn move_right(&mut self) {
        let max = self.text.chars().count();
        if self.cursor < max {
            self.cursor += 1;
        }
    }

    /// Move the cursor to the start of the line.
    fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move the cursor to the end of the line.
    fn move_end(&mut self) {
        self.cursor = self.text.chars().count();
    }

    /// Push the current text into all target banners.
    fn sync_targets(&self, ctx: &mut dyn Context) -> Result<()> {
        for target in &self.targets {
            ctx.with_widget_mut(*target, |block: &mut FontBlock, ctx| {
                block.set_text(ctx, self.text.clone())
            })?;
        }
        Ok(())
    }

    /// Update block layouts to the current height.
    fn sync_heights(&self, ctx: &mut dyn Context) -> Result<()> {
        for target in &self.targets {
            ctx.with_widget_mut(*target, |block: &mut FontBlock, ctx| {
                block.set_banner_height(ctx, self.banner_height)
            })?;
            ctx.set_layout_of(*target, block_layout(self.banner_height))?;
        }
        Ok(())
    }

    /// Apply the current style toggles to the banners.
    fn sync_effects(&self, ctx: &mut dyn Context) -> Result<()> {
        let effects = self.style_state;
        for target in &self.targets {
            ctx.with_widget_mut(*target, |block: &mut FontBlock, ctx| {
                block.set_effects(ctx, effects)
            })?;
        }
        Ok(())
    }

    /// Adjust the banner height by a delta.
    fn adjust_height(&mut self, ctx: &mut dyn Context, delta: i32) -> Result<()> {
        let current = self.banner_height as i32;
        let next = (current + delta).max(MIN_BANNER_HEIGHT as i32) as u32;
        if next == self.banner_height {
            return Ok(());
        }
        self.banner_height = next;
        self.sync_heights(ctx)?;
        self.sync_status(ctx)?;
        Ok(())
    }

    /// Toggle a style attribute and refresh styles.
    fn toggle_style(&mut self, ctx: &mut dyn Context, key: char) -> Result<bool> {
        let flag = match key.to_ascii_lowercase() {
            'b' => &mut self.style_state.bold,
            'i' => &mut self.style_state.italic,
            'u' => &mut self.style_state.underline,
            'd' => &mut self.style_state.dim,
            'o' => &mut self.style_state.overline,
            'x' => &mut self.style_state.strike,
            _ => return Ok(false),
        };
        *flag = !*flag;
        self.sync_effects(ctx)?;
        self.sync_status(ctx)?;
        Ok(true)
    }

    /// Update the status panel contents.
    fn sync_status(&self, ctx: &mut dyn Context) -> Result<()> {
        let status = status_text(self.banner_height, self.style_state);
        ctx.with_widget_mut(self.status_text, |text: &mut Text, _| {
            text.set_text(status);
            Ok(())
        })?;
        Ok(())
    }
}

impl Widget for FontGymInput {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn cursor(&self) -> Option<Cursor> {
        Some(Cursor {
            location: Point {
                x: text::slice_by_columns(
                    &self.text[..byte_index_for_char(&self.text, self.cursor)],
                    0,
                    usize::MAX,
                )
                .1 as u32,
                y: 0,
            },
            shape: CursorShape::Block,
        })
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let origin = view.content_origin();
        let line = Line::new(origin.x, origin.y, view_rect.w);
        rndr.text("text", line, &self.text)?;
        Ok(())
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Key(raw) = event {
            let normalized = raw.normalize();
            if normalized.mods.ctrl {
                match normalized.key {
                    key::KeyCode::Up => {
                        self.adjust_height(ctx, 1)?;
                        return Ok(EventOutcome::Handle);
                    }
                    key::KeyCode::Down => {
                        self.adjust_height(ctx, -1)?;
                        return Ok(EventOutcome::Handle);
                    }
                    key::KeyCode::Char(ch) if self.toggle_style(ctx, ch)? => {
                        return Ok(EventOutcome::Handle);
                    }
                    _ => {}
                }
                return Ok(EventOutcome::Ignore);
            }
        }

        let mut changed = false;
        let outcome = match event {
            Event::Key(key::Key {
                key: key::KeyCode::Char(c),
                ..
            }) => {
                self.insert_char(*c);
                changed = true;
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Backspace,
                ..
            }) => {
                self.backspace();
                changed = true;
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Left,
                ..
            }) => {
                self.move_left();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Right,
                ..
            }) => {
                self.move_right();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::Home,
                ..
            }) => {
                self.move_home();
                EventOutcome::Handle
            }
            Event::Key(key::Key {
                key: key::KeyCode::End,
                ..
            }) => {
                self.move_end();
                EventOutcome::Handle
            }
            _ => EventOutcome::Ignore,
        };

        if changed {
            self.sync_targets(ctx)?;
        }

        Ok(outcome)
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let width = text::display_width(&self.text).max(1) as u32;
        c.clamp(Size::new(width, 1))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("fontgym-input")
    }
}

/// Horizontal row container for status panels.
struct StatusRow;

impl Widget for StatusRow {
    fn layout(&self) -> Layout {
        Layout::row()
            .fixed_height(STATUS_HEIGHT)
            .gap(3)
            .align_vertical(Align::Center)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("fontgym-status-row")
    }
}

/// Legend segment with a style and text.
struct LegendSegment {
    /// Style path for the segment.
    style: &'static str,
    /// Text to render for the segment.
    text: &'static str,
}

/// The Controls legend, one entry per rendered row.
const CONTROLS_LEGEND: &[&[LegendSegment]] = &[
    &[
        LegendSegment::title("Focus"),
        LegendSegment::text(" : "),
        LegendSegment::key("Tab"),
        LegendSegment::text(" / "),
        LegendSegment::key("Shift+Tab"),
    ],
    &[
        LegendSegment::title("Scroll"),
        LegendSegment::text(" : "),
        LegendSegment::key("PgUp"),
        LegendSegment::text(" / "),
        LegendSegment::key("PgDn"),
    ],
    &[
        LegendSegment::title("Height"),
        LegendSegment::text(" : "),
        LegendSegment::key("Ctrl+Up"),
        LegendSegment::text(" / "),
        LegendSegment::key("Ctrl+Down"),
    ],
    &[
        LegendSegment::title("Styles"),
        LegendSegment::text(" : "),
        LegendSegment::key("Ctrl+B"),
        LegendSegment::text(" Bold  "),
        LegendSegment::key("Ctrl+I"),
        LegendSegment::text(" Italic"),
    ],
    &[
        LegendSegment::text("         "),
        LegendSegment::key("Ctrl+U"),
        LegendSegment::text(" Underline  "),
        LegendSegment::key("Ctrl+D"),
        LegendSegment::text(" Dim"),
    ],
    &[
        LegendSegment::text("         "),
        LegendSegment::key("Ctrl+O"),
        LegendSegment::text(" Overline   "),
        LegendSegment::key("Ctrl+X"),
        LegendSegment::text(" Strike"),
    ],
    &[
        LegendSegment::title("Input"),
        LegendSegment::text(" : "),
        LegendSegment::text("Type to edit text"),
    ],
];

impl LegendSegment {
    /// Create a segment styled as a key.
    const fn key(text: &'static str) -> Self {
        Self {
            style: "fontgym/key",
            text,
        }
    }

    /// Create a segment styled as a title.
    const fn title(text: &'static str) -> Self {
        Self {
            style: "fontgym/legend/title",
            text,
        }
    }

    /// Create a segment styled as body text.
    const fn text(text: &'static str) -> Self {
        Self {
            style: "fontgym/legend",
            text,
        }
    }
}

/// Controls legend widget with styled key hints.
struct ControlsLegend;

impl Widget for ControlsLegend {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect_local();
        for (row_idx, segments) in CONTROLS_LEGEND.iter().enumerate() {
            let y = view_rect.tl.y.saturating_add(row_idx as u32);
            if y >= view_rect.tl.y.saturating_add(view_rect.h) {
                break;
            }
            let mut x = view_rect.tl.x;
            for segment in *segments {
                if segment.text.is_empty() {
                    continue;
                }
                if x >= view_rect.tl.x.saturating_add(view_rect.w) {
                    break;
                }
                let width = segment.text.len() as u32;
                let line = Line::new(x, y, width);
                rndr.text(segment.style, line, segment.text)?;
                x = x.saturating_add(width);
            }
        }

        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let max_width = CONTROLS_LEGEND
            .iter()
            .map(|segments| {
                segments
                    .iter()
                    .map(|segment| segment.text.len() as u32)
                    .sum()
            })
            .max()
            .unwrap_or(1)
            .max(1);
        let height = CONTROLS_LEGEND.len().max(1) as u32;
        c.clamp(Size::new(max_width, height))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("fontgym-controls-legend")
    }
}

/// Convert a char index into a byte offset.
fn byte_index_for_char(text: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    text.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

/// Wrap a detached child in a padded, titled panel frame of a minimum width.
fn panel(
    ctx: &mut dyn Context,
    child: impl Into<NodeId>,
    title: &str,
    min_width: u32,
) -> Result<NodeId> {
    let pad = wrap(
        ctx,
        child,
        Pad::new(Edges::symmetric(PANEL_PADDING_V, PANEL_PADDING_H)),
    )?;
    let frame = wrap(
        ctx,
        pad,
        Frame::new().with_title(title).with_glyphs(SINGLE_THICK),
    )?;
    ctx.set_layout_of(
        frame,
        Layout::column()
            .flex_horizontal(1)
            .min_width(min_width)
            .padding(Edges::all(1)),
    )?;
    Ok(frame.into())
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

/// Construct the style map used by the demo banners.
fn font_styles(phase: f32) -> StyleMap {
    let mut style = StyleMap::new();
    let hue = (phase * TAU).sin() * HUE_SWEEP_DEG;
    let mut rules = style
        .rules()
        .attr("fontgym/legend", Attr::Dim)
        .attr("fontgym/legend/title", Attr::Bold)
        .attr("fontgym/key", Attr::Bold)
        .fg("fontgym/legend/title", rgb!("#E9ECEF"))
        .fg("fontgym/key", rgb!("#FFD166"))
        .fg("fontgym/label", rgb!("#A3B1C2"));
    for spec in &BANNERS {
        rules = rules.fg(
            spec.style,
            crate::banner_gradient(
                GRADIENT_BASE_ANGLE + spec.angle_offset,
                spec.palette.map(|color| color.shift_hue(hue)),
            ),
        );
    }
    rules.apply();
    style
}

/// Build the status text for the current state.
fn status_text(height: u32, state: FontEffects) -> String {
    let flag = |enabled: bool| if enabled { "on " } else { "off" };
    [
        format!("Height : {}", height),
        format!(
            "Bold: {}  Italic: {}  Underline: {}",
            flag(state.bold),
            flag(state.italic),
            flag(state.underline)
        ),
        format!(
            "Dim : {}  Overline: {}  Strike: {}",
            flag(state.dim),
            flag(state.overline),
            flag(state.strike)
        ),
    ]
    .join("\n")
}

/// Focus controls shared by eager and builder setup.
const DEFAULT_BINDINGS: &str = r#"
canopy.bind_command("Tab", { phase = "after_widget", description = "Next focus" }, "root::focus", "Next")
canopy.bind_command("BackTab", { phase = "after_widget", description = "Previous focus" }, "root::focus", "Prev")
"#;

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.bindings("fontgym", DEFAULT_BINDINGS)
}

#[cfg(test)]
mod tests {
    use canopy::{ViewContextExt, layout::Constraint, testing::harness::Harness};

    use super::*;
    use crate::tests::{Mount, root_harness};

    fn font_list_scroll(harness: &Harness) -> Point {
        harness.canopy.with_root_view(|ctx| {
            let list = ctx
                .unique_descendant::<List<FontBlock>>()
                .expect("list lookup")
                .expect("font list");
            ctx.view_of(list.into()).expect("font list view").scroll
        })
    }

    #[test]
    fn page_down_scrolls_the_font_list() -> Result<()> {
        let mut harness = root_harness(
            FontGym::new(),
            binding_setup,
            Size::new(80, 24),
            Mount::Wrap,
        )?;
        let before = font_list_scroll(&harness);

        let frame = harness
            .canopy
            .with_root_view(|ctx| ctx.unique_descendant::<FocusFrame>())
            .expect("frame lookup")
            .expect("focus frame");
        harness.canopy.with_root_context(|ctx| {
            ctx.set_focus(frame.into())?;
            Ok(())
        })?;

        harness.key(key::Key::parse_spec("PageDown").expect("valid key"))?;

        let after = font_list_scroll(&harness);
        assert!(after.y > before.y, "PageDown must scroll the font list");
        Ok(())
    }

    #[test]
    fn input_cursor_and_measurement_use_display_columns() -> Result<()> {
        let mut canopy = canopy::Canopy::new();
        let status = canopy.create_detached(Text::new(""))?;
        for (text, columns, width) in [
            ("界a", vec![0, 2, 3], 3),
            ("e\u{301}a", vec![0, 1, 1, 2], 2),
            ("abc", vec![0, 1, 2, 3], 3),
            ("", vec![0], 1),
        ] {
            let mut input = FontGymInput::new(text, Vec::new(), 1, FontEffects::default(), status);
            for (position, column) in columns.into_iter().enumerate() {
                input.cursor = position;
                let cursor = input.cursor().expect("input cursor");
                assert_eq!(cursor.location, Point { x: column, y: 0 });
            }
            assert_eq!(
                input.measure(MeasureConstraints {
                    width: Constraint::Unbounded,
                    height: Constraint::Unbounded,
                }),
                Measurement::Fixed(Size::new(width, 1))
            );
            assert_eq!(
                input.measure(MeasureConstraints {
                    width: Constraint::AtMost(1),
                    height: Constraint::Exact(1),
                }),
                Measurement::Fixed(Size::new(1, 1))
            );
        }
        Ok(())
    }
}
