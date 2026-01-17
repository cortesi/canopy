use std::{f32::consts::TAU, time::Duration};

use canopy::{
    Binder, Canopy, Context, EventOutcome, Loader, ReadContext, Widget, command,
    cursor::{Cursor, CursorShape},
    derive_commands,
    error::Result,
    event::{Event, key},
    geom::{Line, Point},
    layout::{Align, Edges, Layout, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    style::{AttrSet, Color, GradientSpec, GradientStop, Paint, StyleMap},
    text,
};
use canopy_widgets::{
    Font, FontBanner, FontEffects, FontRenderer, Frame, GlyphRamp, LayoutOptions, List, Pad,
    ROUND_THICK, SINGLE_THICK, Selectable, Text, VStack,
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
/// Angle offset for the solar gradient.
const SOLAR_ANGLE_OFFSET: f32 = 0.0;
/// Angle offset for the ocean gradient.
const OCEAN_ANGLE_OFFSET: f32 = 120.0;
/// Angle offset for the ember gradient.
const EMBER_ANGLE_OFFSET: f32 = 240.0;
/// Angle offset for the violet gradient.
const VIOLET_ANGLE_OFFSET: f32 = 300.0;

/// Toggle state for banner font styles.
#[derive(Debug, Clone, Copy, Default)]
struct FontStyleState {
    /// Bold attribute.
    bold: bool,
    /// Italic attribute.
    italic: bool,
    /// Underline attribute.
    underline: bool,
    /// Dim attribute.
    dim: bool,
    /// Overline attribute.
    overline: bool,
    /// Crossed out attribute.
    crossed_out: bool,
}

impl FontStyleState {
    /// Build font effects from the toggles.
    fn effects(&self) -> FontEffects {
        FontEffects {
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            dim: self.dim,
            overline: self.overline,
            strike: self.crossed_out,
        }
    }
}

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

#[derive_commands]
impl FontGym {
    /// Construct a new font gym demo.
    pub fn new() -> Self {
        Self {
            gradient_phase: 0.0,
        }
    }

    #[command]
    /// Trigger a redraw.
    pub fn redraw(&mut self, _ctx: &mut dyn Context) {}
}

impl Widget for FontGym {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let style_state = FontStyleState::default();
        ctx.set_style(font_styles(self.gradient_phase));

        let list_id = ctx.create_detached(List::new());
        ctx.set_layout_of(list_id, Layout::fill())?;

        let blocks = ctx.with_typed(list_id, |list: &mut List<FontBlock>, ctx| {
            let mut ids = Vec::new();
            let centered = LayoutOptions {
                h_align: Align::Center,
                v_align: Align::Center,
                ..LayoutOptions::default()
            };
            let font_a = load_font_bungee();
            let font_a_name = font_label(&font_a);
            let banner_a = FontBanner::new(
                DEFAULT_TEXT,
                FontRenderer::new(font_a)
                    .with_ramp(GlyphRamp::blocks())
                    .with_fallback('?'),
            )
            .with_effects(style_state.effects())
            .with_style("font/banner/solar")
            .with_layout_options(centered);
            let font_b = load_font_fira();
            let font_b_name = font_label(&font_b);
            let banner_b = FontBanner::new(
                DEFAULT_TEXT,
                FontRenderer::new(font_b)
                    .with_ramp(GlyphRamp::blocks())
                    .with_fallback('?'),
            )
            .with_effects(style_state.effects())
            .with_style("font/banner/ocean")
            .with_layout_options(centered);
            let font_c = load_font_fira();
            let font_c_name = font_label(&font_c);
            let banner_c = FontBanner::new(
                DEFAULT_TEXT,
                FontRenderer::new(font_c)
                    .with_ramp(GlyphRamp::blocks())
                    .with_fallback('?'),
            )
            .with_effects(style_state.effects())
            .with_style("font/banner/ember")
            .with_layout_options(centered);
            let font_d = load_font_tangerine();
            let font_d_name = font_label(&font_d);
            let banner_d = FontBanner::new(
                DEFAULT_TEXT,
                FontRenderer::new(font_d)
                    .with_ramp(GlyphRamp::blocks())
                    .with_fallback('?'),
            )
            .with_effects(style_state.effects())
            .with_style("font/banner/violet")
            .with_layout_options(centered);

            for (banner, label) in [
                (banner_a, font_a_name),
                (banner_b, font_b_name),
                (banner_c, font_c_name),
                (banner_d, font_d_name),
            ] {
                let id = list.append(ctx, FontBlock::new(banner, label, BANNER_HEIGHT))?;
                ctx.set_layout_of(id, block_layout(BANNER_HEIGHT))?;
                ids.push(id);
            }

            Ok(ids)
        })?;

        let font_frame_id = ctx.create_detached(FocusFrame::new(
            Frame::new().with_title("Fonts").with_glyphs(ROUND_THICK),
            list_id,
        ));
        ctx.set_children_of(font_frame_id.into(), vec![list_id.into()])?;
        ctx.set_layout_of(font_frame_id, Layout::fill().padding(Edges::all(1)))?;

        let controls_id = ctx.create_detached(ControlsLegend);
        let controls_pad = Pad::wrap_with(
            ctx,
            controls_id,
            Pad::new(Edges::symmetric(PANEL_PADDING_V, PANEL_PADDING_H)),
        )?;
        let controls_frame = Frame::wrap_with(
            ctx,
            controls_pad,
            Frame::new().with_title("Controls").with_glyphs(ROUND_THICK),
        )?;
        ctx.set_layout_of(
            controls_frame,
            Layout::column()
                .flex_horizontal(1)
                .min_width(CONTROLS_PANEL_WIDTH)
                .padding(Edges::all(1)),
        )?;

        let status_id = ctx.create_detached(
            Text::new(status_text(BANNER_HEIGHT, style_state))
                .with_style("fontgym/legend")
                .with_wrap_width(STATUS_WRAP_WIDTH),
        );
        let status_pad = Pad::wrap_with(
            ctx,
            status_id,
            Pad::new(Edges::symmetric(PANEL_PADDING_V, PANEL_PADDING_H)),
        )?;
        let status_frame = Frame::wrap_with(
            ctx,
            status_pad,
            Frame::new().with_title("Status").with_glyphs(SINGLE_THICK),
        )?;
        ctx.set_layout_of(
            status_frame,
            Layout::column()
                .flex_horizontal(1)
                .min_width(STATUS_PANEL_WIDTH)
                .padding(Edges::all(1)),
        )?;

        let status_row_id = ctx.create_detached(StatusRow);
        ctx.set_children_of(status_row_id.into(), vec![controls_frame, status_frame])?;

        let input_id = ctx.create_detached(FontGymInput::new(
            DEFAULT_TEXT,
            blocks,
            BANNER_HEIGHT,
            style_state,
            status_id,
        ));
        ctx.set_layout_of(input_id, Layout::fill())?;

        let input_frame = Frame::wrap_with(ctx, input_id, Frame::new().with_title("Text input"))?;
        let stack = VStack::new()
            .push_fixed(input_frame, INPUT_HEIGHT)
            .push_fixed(status_row_id, STATUS_HEIGHT)
            .push_flex(font_frame_id, 1);
        let stack_id = ctx.add_child(stack)?;
        ctx.set_layout_of(stack_id, Layout::fill())?;
        ctx.set_focus(input_id.into());
        Ok(())
    }

    fn poll(&mut self, ctx: &mut dyn Context) -> Option<Duration> {
        self.gradient_phase = (self.gradient_phase + GRADIENT_PHASE_STEP).fract();
        ctx.set_style(font_styles(self.gradient_phase));
        Some(Duration::from_millis(GRADIENT_POLL_MS))
    }
}

impl Loader for FontGym {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        Ok(())
    }
}

/// Focusable frame wrapper that delegates rendering and handles keyboard scroll.
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
        action: impl FnOnce(&mut dyn Context) -> bool,
    ) -> Result<bool> {
        ctx.with_typed(self.list_id, |_, list_ctx| Ok(action(list_ctx)))
    }
}

impl Widget for FocusFrame {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
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
    /// Mounted label node ID.
    label_id: Option<canopy::TypedId<FontLabel>>,
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
            label_id: None,
            banner_height,
        }
    }

    /// Update the banner text.
    fn set_text(&mut self, ctx: &mut dyn Context, text: String) -> Result<()> {
        if let Some(banner_id) = self.banner_id {
            ctx.with_typed(banner_id, |banner, _| {
                banner.set_text(text);
                Ok(())
            })?;
        } else if let Some(banner) = self.banner.as_mut() {
            banner.set_text(text);
        }
        Ok(())
    }

    /// Update the banner effects.
    fn set_effects(&mut self, ctx: &mut dyn Context, effects: FontEffects) -> Result<()> {
        if let Some(banner_id) = self.banner_id {
            ctx.with_typed(banner_id, |banner, _| {
                banner.set_effects(effects);
                Ok(())
            })?;
        } else if let Some(banner) = self.banner.as_mut() {
            banner.set_effects(effects);
        }
        Ok(())
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
        let banner_id = ctx.create_detached(banner);
        ctx.set_layout_of(banner_id, Layout::fill().fixed_height(self.banner_height))?;

        let label_id = ctx.create_detached(FontLabel::new(self.label.clone(), "fontgym/label"));
        ctx.set_layout_of(label_id, Layout::fill().fixed_height(LABEL_HEIGHT))?;

        ctx.set_children(vec![banner_id.into(), label_id.into()])?;
        self.banner_id = Some(banner_id);
        self.label_id = Some(label_id);
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

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let origin = view.content_origin();
        if view_rect.w == 0 || view_rect.h == 0 {
            return Ok(());
        }
        let full_width = text::slice_by_columns(&self.text, 0, usize::MAX).1 as u32;
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
        let width = text::slice_by_columns(&self.text, 0, usize::MAX).1.max(1) as u32;
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
    style_state: FontStyleState,
    /// Status text widget to update.
    status_text: canopy::TypedId<Text>,
}

impl FontGymInput {
    /// Create a new input widget targeting the provided banners.
    fn new(
        text: impl Into<String>,
        targets: Vec<canopy::TypedId<FontBlock>>,
        banner_height: u32,
        style_state: FontStyleState,
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
            ctx.with_typed(*target, |block, ctx| block.set_text(ctx, self.text.clone()))?;
        }
        Ok(())
    }

    /// Update block layouts to the current height.
    fn sync_heights(&self, ctx: &mut dyn Context) -> Result<()> {
        for target in &self.targets {
            ctx.with_typed(*target, |block, ctx| {
                block.set_banner_height(ctx, self.banner_height)
            })?;
            ctx.set_layout_of(*target, block_layout(self.banner_height))?;
        }
        Ok(())
    }

    /// Apply the current style toggles to the banners.
    fn sync_effects(&self, ctx: &mut dyn Context) -> Result<()> {
        let effects = self.style_state.effects();
        for target in &self.targets {
            ctx.with_typed(*target, |block, ctx| block.set_effects(ctx, effects))?;
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
        let handled = match key.to_ascii_lowercase() {
            'b' => {
                self.style_state.bold = !self.style_state.bold;
                true
            }
            'i' => {
                self.style_state.italic = !self.style_state.italic;
                true
            }
            'u' => {
                self.style_state.underline = !self.style_state.underline;
                true
            }
            'd' => {
                self.style_state.dim = !self.style_state.dim;
                true
            }
            'o' => {
                self.style_state.overline = !self.style_state.overline;
                true
            }
            'x' => {
                self.style_state.crossed_out = !self.style_state.crossed_out;
                true
            }
            _ => false,
        };

        if handled {
            self.sync_effects(ctx)?;
            self.sync_status(ctx)?;
        }
        Ok(handled)
    }

    /// Update the status panel contents.
    fn sync_status(&self, ctx: &mut dyn Context) -> Result<()> {
        let status = status_text(self.banner_height, self.style_state);
        ctx.with_typed(self.status_text, |text, _| {
            text.set_raw(status);
            Ok(())
        })?;
        Ok(())
    }
}

impl Widget for FontGymInput {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn cursor(&self) -> Option<Cursor> {
        Some(Cursor {
            location: Point {
                x: self.cursor as u32,
                y: 0,
            },
            shape: CursorShape::Block,
            blink: true,
        })
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
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
                    key::KeyCode::Char(ch) => {
                        if self.toggle_style(ctx, ch)? {
                            return Ok(EventOutcome::Handle);
                        }
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
        let width = self.text.chars().count().max(1) as u32;
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

impl LegendSegment {
    /// Create a segment styled as a key.
    fn key(text: &'static str) -> Self {
        Self {
            style: "fontgym/key",
            text,
        }
    }

    /// Create a segment styled as a title.
    fn title(text: &'static str) -> Self {
        Self {
            style: "fontgym/legend/title",
            text,
        }
    }

    /// Create a segment styled as body text.
    fn text(text: &'static str) -> Self {
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

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect_local();
        let lines = controls_legend_lines();

        for (row_idx, segments) in lines.iter().enumerate() {
            let y = view_rect.tl.y.saturating_add(row_idx as u32);
            if y >= view_rect.tl.y.saturating_add(view_rect.h) {
                break;
            }
            let mut x = view_rect.tl.x;
            for segment in segments {
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
        let lines = controls_legend_lines();
        let mut max_width = 1u32;
        for segments in &lines {
            let width = segments
                .iter()
                .map(|segment| segment.text.len() as u32)
                .sum();
            max_width = max_width.max(width);
        }
        let height = lines.len().max(1) as u32;
        c.clamp(Size::new(max_width, height))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("fontgym-controls-legend")
    }
}

/// Build legend lines with styled segments.
fn controls_legend_lines() -> Vec<Vec<LegendSegment>> {
    let indent = "         ";
    vec![
        vec![
            LegendSegment::title("Focus"),
            LegendSegment::text(" : "),
            LegendSegment::key("Tab"),
            LegendSegment::text(" / "),
            LegendSegment::key("Shift+Tab"),
        ],
        vec![
            LegendSegment::title("Scroll"),
            LegendSegment::text(" : "),
            LegendSegment::key("PgUp"),
            LegendSegment::text(" / "),
            LegendSegment::key("PgDn"),
        ],
        vec![
            LegendSegment::title("Height"),
            LegendSegment::text(" : "),
            LegendSegment::key("Ctrl+Up"),
            LegendSegment::text(" / "),
            LegendSegment::key("Ctrl+Down"),
        ],
        vec![
            LegendSegment::title("Styles"),
            LegendSegment::text(" : "),
            LegendSegment::key("Ctrl+B"),
            LegendSegment::text(" Bold  "),
            LegendSegment::key("Ctrl+I"),
            LegendSegment::text(" Italic"),
        ],
        vec![
            LegendSegment::text(indent),
            LegendSegment::key("Ctrl+U"),
            LegendSegment::text(" Underline  "),
            LegendSegment::key("Ctrl+D"),
            LegendSegment::text(" Dim"),
        ],
        vec![
            LegendSegment::text(indent),
            LegendSegment::key("Ctrl+O"),
            LegendSegment::text(" Overline   "),
            LegendSegment::key("Ctrl+X"),
            LegendSegment::text(" Strike"),
        ],
        vec![
            LegendSegment::title("Input"),
            LegendSegment::text(" : "),
            LegendSegment::text("Type to edit text"),
        ],
    ]
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

/// Load the Bungee display font.
fn load_font_bungee() -> Font {
    Font::from_bytes(include_bytes!(
        "../../canopy-widgets/assets/fonts/Bungee-Regular.ttf"
    ))
    .expect("bungee font loads")
}

/// Load the Fira Mono font.
fn load_font_fira() -> Font {
    Font::from_bytes(include_bytes!(
        "../../canopy-widgets/assets/fonts/FiraMono-Regular.ttf"
    ))
    .expect("fira mono font loads")
}

/// Load the Tangerine calligraphic font.
fn load_font_tangerine() -> Font {
    Font::from_bytes(include_bytes!(
        "../../canopy-widgets/assets/fonts/Tangerine-Regular.ttf"
    ))
    .expect("tangerine font loads")
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
    let solar = shift_palette(
        [
            Color::rgb("#FFF200"),
            Color::rgb("#FF9F00"),
            Color::rgb("#FF003C"),
            Color::rgb("#7A00FF"),
        ],
        hue,
    );
    let ocean = shift_palette(
        [
            Color::rgb("#00F5FF"),
            Color::rgb("#0084FF"),
            Color::rgb("#003BFF"),
            Color::rgb("#00FF9D"),
        ],
        hue,
    );
    let ember = shift_palette(
        [
            Color::rgb("#FFD000"),
            Color::rgb("#FF7A00"),
            Color::rgb("#FF1F00"),
            Color::rgb("#B00000"),
        ],
        hue,
    );
    let violet = shift_palette(
        [
            Color::rgb("#FFD6FF"),
            Color::rgb("#B5179E"),
            Color::rgb("#7209B7"),
            Color::rgb("#4361EE"),
        ],
        hue,
    );
    style
        .rules()
        .attrs(
            "fontgym/legend",
            AttrSet {
                dim: true,
                ..AttrSet::default()
            },
        )
        .attrs(
            "fontgym/legend/title",
            AttrSet {
                bold: true,
                ..AttrSet::default()
            },
        )
        .attrs(
            "fontgym/key",
            AttrSet {
                bold: true,
                ..AttrSet::default()
            },
        )
        .fg("fontgym/legend/title", Color::rgb("#E9ECEF"))
        .fg("fontgym/key", Color::rgb("#FFD166"))
        .fg("fontgym/label", Color::rgb("#A3B1C2"))
        .fg(
            "font/banner/solar",
            Paint::gradient(GradientSpec::with_stops(
                GRADIENT_BASE_ANGLE + SOLAR_ANGLE_OFFSET,
                gradient_stops(solar),
            )),
        )
        .fg(
            "font/banner/ocean",
            Paint::gradient(GradientSpec::with_stops(
                GRADIENT_BASE_ANGLE + OCEAN_ANGLE_OFFSET,
                gradient_stops(ocean),
            )),
        )
        .fg(
            "font/banner/ember",
            Paint::gradient(GradientSpec::with_stops(
                GRADIENT_BASE_ANGLE + EMBER_ANGLE_OFFSET,
                gradient_stops(ember),
            )),
        )
        .fg(
            "font/banner/violet",
            Paint::gradient(GradientSpec::with_stops(
                GRADIENT_BASE_ANGLE + VIOLET_ANGLE_OFFSET,
                gradient_stops(violet),
            )),
        )
        .apply();
    style
}

/// Shift a palette by the provided hue delta in degrees.
fn shift_palette(colors: [Color; 4], hue_deg: f32) -> [Color; 4] {
    [
        colors[0].shift_hue(hue_deg),
        colors[1].shift_hue(hue_deg),
        colors[2].shift_hue(hue_deg),
        colors[3].shift_hue(hue_deg),
    ]
}

/// Build gradient stops from a four-color palette.
fn gradient_stops(colors: [Color; 4]) -> Vec<GradientStop> {
    vec![
        GradientStop::new(0.0, colors[0]),
        GradientStop::new(0.35, colors[1]),
        GradientStop::new(0.7, colors[2]),
        GradientStop::new(1.0, colors[3]),
    ]
}

/// Build the controls help text.
/// Build the status text for the current state.
fn status_text(height: u32, state: FontStyleState) -> String {
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
            flag(state.crossed_out)
        ),
    ]
    .join("\n")
}

/// Install key bindings for focus navigation.
pub fn setup_bindings(c: &mut Canopy) {
    Binder::new(c)
        .key(key::KeyCode::Tab, "root::focus_next()")
        .key(key::KeyCode::BackTab, "root::focus_prev()");
}
