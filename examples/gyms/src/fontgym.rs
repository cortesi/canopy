use std::time::Duration;

use canopy::{
    CanopyBuilder, ChangeOutcome, Context, ContextExt, EventOutcome, NodeId, NodeName, ViewContext,
    Widget,
    error::Result,
    geom::{Line, Point, Size},
    input::{Event, key},
    layout::{
        Align, Edges, Layout, MeasureConstraints, Measurement, ScrollDirection, ScrollOp, View,
    },
    render::{
        Render,
        cursor::{self, CursorRequest},
    },
    rgb,
    style::{Attr, Color, Drift, GradientSpec, GradientStop, Paint, StyleMap},
    text,
};
use canopy_widgets::{
    BoxGlyphs, Container, FontBanner, Frame, List, Selectable, Text,
    font::{Font, FontEffects, FontRenderer, LayoutOptions},
};

use crate::fixed_row;

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

/// Demo node that renders ASCII font banners.
#[derive(Default)]
pub struct FontGym;

impl FontGym {
    /// Construct a new font gym demo.
    pub fn new() -> Self {
        Self
    }
}

impl Widget for FontGym {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let style_state = FontEffects::default();
        ctx.set_style(font_styles());

        let list_id = ctx.create_detached(List::new())?;
        ctx.set_layout_override(list_id.into(), Layout::fill().into())?;

        let blocks = ctx.with_widget_mut(list_id, |list: &mut List<FontBlock>, ctx| {
            let mut ids = Vec::new();
            let centered = LayoutOptions {
                h_align: Align::Center,
                v_align: Align::Center,
                ..LayoutOptions::default()
            };
            for spec in &BANNERS {
                let font = Font::from_bytes(spec.font).expect("embedded font loads");
                let label = font_label(&font);
                let banner = FontBanner::new(DEFAULT_TEXT, FontRenderer::new(font))
                    .with_effects(style_state)
                    .with_style(spec.style)
                    .with_layout_options(centered);
                let id = list.append(ctx, FontBlock::new(banner, label, BANNER_HEIGHT))?;
                ctx.set_layout_override(id.into(), block_layout(BANNER_HEIGHT).into())?;
                ids.push(id);
            }

            Ok(ids)
        })?;

        let font_frame_id = ctx.create_detached(FocusFrame::new(
            Frame::new()
                .with_title("Fonts")
                .with_glyphs(BoxGlyphs::SINGLE_THICK),
            list_id,
        ))?;
        ctx.set_children(font_frame_id.into(), vec![list_id.into()])?;
        ctx.set_layout_override(
            font_frame_id.into(),
            Layout::fill().padding(Edges::all(1)).into(),
        )?;

        let controls_id = ctx.create_detached(ControlsLegend)?;
        let controls_frame = panel(ctx, controls_id, "Controls", CONTROLS_PANEL_WIDTH)?;

        let status_id = ctx.create_detached(
            Text::new(status_text(BANNER_HEIGHT, style_state))
                .with_style("fontgym/legend")
                .with_wrap_width(STATUS_WRAP_WIDTH),
        )?;
        let status_frame = panel(ctx, status_id, "Status", STATUS_PANEL_WIDTH)?;

        let status_row_id = ctx.create_detached(StatusRow)?;
        ctx.set_children(status_row_id.into(), vec![controls_frame, status_frame])?;

        let input_id = ctx.create_detached(FontGymInput::new(
            DEFAULT_TEXT,
            blocks,
            BANNER_HEIGHT,
            style_state,
            status_id,
        ))?;
        ctx.set_layout_override(input_id.into(), Layout::fill().into())?;

        let input_frame = ctx.wrap_node(input_id, Frame::new().with_title("Text input"))?;
        let stack_id = ctx.add_child(ctx.node_id(), Container::column())?;
        ctx.set_children(
            stack_id.into(),
            vec![
                input_frame.into(),
                status_row_id.into(),
                font_frame_id.into(),
            ],
        )?;
        ctx.set_layout_override(input_frame.into(), fixed_row(INPUT_HEIGHT))?;
        ctx.set_layout_override(status_row_id.into(), fixed_row(STATUS_HEIGHT))?;
        ctx.set_focus(input_id.into())?;
        Ok(())
    }
}

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

    /// Classify one key after the same normalization the handler applies.
    fn classify_key(key: key::Key) -> Option<FocusScroll> {
        match key.normalize().key {
            key::KeyCode::Up => Some(FocusScroll::Up),
            key::KeyCode::Down => Some(FocusScroll::Down),
            key::KeyCode::PageUp => Some(FocusScroll::PageUp),
            key::KeyCode::PageDown => Some(FocusScroll::PageDown),
            _ => None,
        }
    }

    /// Return the vertical scroll delta `command` requests.
    fn scroll_delta(view: View, command: FocusScroll) -> i32 {
        let page = i32::try_from(view.content.h).unwrap_or(i32::MAX);
        match command {
            FocusScroll::Up => -1,
            FocusScroll::Down => 1,
            FocusScroll::PageUp => -page,
            FocusScroll::PageDown => page,
        }
    }
}

/// One focus-frame scroll action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusScroll {
    /// Scroll up one line.
    Up,
    /// Scroll down one line.
    Down,
    /// Scroll up one page.
    PageUp,
    /// Scroll down one page.
    PageDown,
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
        if let Event::Key(raw) = event
            && let Some(command) = Self::classify_key(*raw)
        {
            let changed = match command {
                FocusScroll::Up => self.scroll_list(ctx, |ctx| {
                    ctx.scroll(ScrollOp::Lines(ScrollDirection::Up, 1))
                })?,
                FocusScroll::Down => self.scroll_list(ctx, |ctx| {
                    ctx.scroll(ScrollOp::Lines(ScrollDirection::Down, 1))
                })?,
                FocusScroll::PageUp => self.scroll_list(ctx, |ctx| {
                    ctx.scroll(ScrollOp::Pages(ScrollDirection::Up, 1))
                })?,
                FocusScroll::PageDown => self.scroll_list(ctx, |ctx| {
                    ctx.scroll(ScrollOp::Pages(ScrollDirection::Down, 1))
                })?,
            };
            if changed {
                return Ok(EventOutcome::Handle);
            }
        }

        self.frame.on_event(event, ctx)
    }

    fn key_outcome(&self, key: key::Key, context: &dyn ViewContext) -> EventOutcome {
        let Some(command) = Self::classify_key(key) else {
            return EventOutcome::Ignore;
        };
        let node = NodeId::from(self.list_id);
        let Some(view) = context.view_of(node) else {
            return EventOutcome::Ignore;
        };
        let delta = Self::scroll_delta(view, command);
        if context.scroll_outcome(node, ScrollOp::By(0, delta)) == Some(ChangeOutcome::Changed) {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        }
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

        let label_id = ctx.create_detached(FontLabel::new(self.label.clone(), "fontgym/label"))?;
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
    /// Return the display column of the cursor.
    fn cursor_column(&self) -> u32 {
        text::slice_by_columns(
            &self.text[..byte_index_for_char(&self.text, self.cursor)],
            0,
            usize::MAX,
        )
        .1 as u32
    }

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
            ctx.set_layout_override((*target).into(), block_layout(self.banner_height).into())?;
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

    /// Classify one key without running its effect.
    ///
    /// The control branch reads the normalized key, and the editing branch
    /// reads the raw key, exactly as [`FontGymInput::on_event`] does.
    fn classify_key(key: key::Key) -> Option<FontInputCommand> {
        let normalized = key.normalize();
        if normalized.mods.ctrl {
            return match normalized.key {
                key::KeyCode::Up => Some(FontInputCommand::Height(1)),
                key::KeyCode::Down => Some(FontInputCommand::Height(-1)),
                key::KeyCode::Char(character) if is_style_key(character) => {
                    Some(FontInputCommand::ToggleStyle(character))
                }
                _ => None,
            };
        }
        Some(match key.key {
            key::KeyCode::Char(character) => FontInputCommand::Insert(character),
            key::KeyCode::Backspace => FontInputCommand::Backspace,
            key::KeyCode::Left => FontInputCommand::Move(FontInputMove::Left),
            key::KeyCode::Right => FontInputCommand::Move(FontInputMove::Right),
            key::KeyCode::Home => FontInputCommand::Move(FontInputMove::Home),
            key::KeyCode::End => FontInputCommand::Move(FontInputMove::End),
            _ => return None,
        })
    }
}

/// One fontgym input action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FontInputCommand {
    /// Adjust the banner height.
    Height(i32),
    /// Toggle a style attribute.
    ToggleStyle(char),
    /// Insert one character.
    Insert(char),
    /// Delete backward.
    Backspace,
    /// Move the caret.
    Move(FontInputMove),
}

/// A fontgym input caret movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FontInputMove {
    /// Move left.
    Left,
    /// Move right.
    Right,
    /// Move to the start.
    Home,
    /// Move to the end.
    End,
}

/// Return whether `key` names a style toggle.
fn is_style_key(key: char) -> bool {
    matches!(key.to_ascii_lowercase(), 'b' | 'i' | 'u' | 'd' | 'o' | 'x')
}

impl Widget for FontGymInput {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let view_rect = view.view_rect();
        let origin = view.content_origin();
        let line = Line::new(origin.x, origin.y, view_rect.w);
        rndr.text("text", line, &self.text)?;
        if ctx.is_focused() {
            let location = Point {
                x: origin.x.saturating_add(self.cursor_column()),
                y: origin.y,
            };
            rndr.cursor(location, CursorRequest::new(cursor::TEXT));
        }
        Ok(())
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Key(key) = event else {
            return Ok(EventOutcome::Ignore);
        };
        let Some(command) = Self::classify_key(*key) else {
            return Ok(EventOutcome::Ignore);
        };
        let mut changed = false;
        match command {
            FontInputCommand::Height(delta) => self.adjust_height(ctx, delta)?,
            FontInputCommand::ToggleStyle(character) => {
                let _ = self.toggle_style(ctx, character)?;
            }
            FontInputCommand::Insert(character) => {
                self.insert_char(character);
                changed = true;
            }
            FontInputCommand::Backspace => {
                self.backspace();
                changed = true;
            }
            FontInputCommand::Move(FontInputMove::Left) => self.move_left(),
            FontInputCommand::Move(FontInputMove::Right) => self.move_right(),
            FontInputCommand::Move(FontInputMove::Home) => self.move_home(),
            FontInputCommand::Move(FontInputMove::End) => self.move_end(),
        }
        if changed {
            self.sync_targets(ctx)?;
        }
        Ok(EventOutcome::Handle)
    }

    fn key_outcome(&self, key: key::Key, _context: &dyn ViewContext) -> EventOutcome {
        if Self::classify_key(key).is_some() {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        }
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let width = text::width(&self.text).max(1) as u32;
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
    let pad = ctx.wrap_node(
        child,
        Container::padded(Edges::symmetric(PANEL_PADDING_V, PANEL_PADDING_H)),
    )?;
    let frame = ctx.wrap_node(
        pad,
        Frame::new()
            .with_title(title)
            .with_glyphs(BoxGlyphs::SINGLE_THICK),
    )?;
    ctx.set_layout_override(
        frame.into(),
        Layout::column()
            .flex_horizontal(1)
            .min_width(min_width)
            .padding(Edges::all(1))
            .into(),
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

/// Construct the style map used by the demo banners.
fn font_styles() -> StyleMap {
    let mut style = StyleMap::new();
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
            drifting_gradient(GRADIENT_BASE_ANGLE + spec.angle_offset, spec.palette),
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
root.default_bindings()
canopy.keymap({
    { key = "Tab", description = "Next focus", action = command.root.focus("next") },
    { key = "BackTab", description = "Previous focus", action = command.root.focus("prev") },
})
"#;

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.script("fontgym", DEFAULT_BINDINGS)
}

#[cfg(test)]
mod tests {
    use canopy::{
        ViewContextExt,
        geom::Rect,
        layout::{Constraint, RevealAlign},
        testing::harness::Harness,
    };

    use super::*;
    use crate::tests::{Mount, root_harness};

    fn font_list_scroll(harness: &Harness) -> Point {
        harness.canopy.with_root_view(|ctx| {
            let list = ctx
                .unique_descendant::<List<FontBlock>>(ctx.root_id())
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
            .with_root_view(|ctx| ctx.unique_descendant::<FocusFrame>(ctx.root_id()))
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
    fn focus_frame_predicts_scroll_boundaries_and_pending_reveals() -> Result<()> {
        let mut harness = root_harness(
            FontGym::new(),
            binding_setup,
            Size::new(80, 24),
            Mount::Wrap,
        )?;
        let (frame, list) = harness.canopy.with_root_view(|ctx| {
            (
                ctx.unique_descendant::<FocusFrame>(ctx.root_id())
                    .expect("frame lookup")
                    .expect("focus frame"),
                ctx.unique_descendant::<List<FontBlock>>(ctx.root_id())
                    .expect("list lookup")
                    .expect("font list"),
            )
        });
        harness.canopy.with_root_context(|ctx| {
            ctx.set_focus(frame.into())?;
            Ok(())
        })?;

        let check = |harness: &mut Harness, spec: &str| -> Result<()> {
            harness.canopy.with_root_context(|ctx| {
                ctx.with_widget_mut(frame, |frame: &mut FocusFrame, ctx| {
                    let key = key::Key::parse_spec(spec)?;
                    let predicted = frame.key_outcome(key, ctx);
                    let actual = frame.on_event(&Event::Key(key), ctx)?;
                    assert_eq!(
                        predicted, actual,
                        "prediction must match handling for {spec}"
                    );
                    Ok(())
                })
            })
        };

        for offset in [3, 0, u32::MAX] {
            harness.canopy.with_root_context(|ctx| {
                ctx.scroll_node(list.into(), ScrollOp::To(Point { x: 0, y: offset }))
            })?;
            check(&mut harness, "up")?;
            harness.canopy.with_root_context(|ctx| {
                ctx.scroll_node(list.into(), ScrollOp::To(Point { x: 0, y: offset }))
            })?;
            check(&mut harness, "down")?;
        }

        // A pending reveal changes a scroll that the clamped offset alone
        // would report as no movement.
        harness.canopy.with_root_context(|ctx| {
            ctx.scroll_node(list.into(), ScrollOp::To(Point { x: 0, y: 0 }))
        })?;
        harness.canopy.with_root_context(|ctx| {
            ctx.with_widget_mut(list, |_: &mut List<FontBlock>, list_ctx| {
                list_ctx.reveal_area(Rect::new(0, 0, 1, 1), RevealAlign::Nearest);
                Ok(())
            })
        })?;
        check(&mut harness, "up")?;
        Ok(())
    }

    #[test]
    fn input_cursor_and_measurement_use_display_columns() -> Result<()> {
        let mut canopy = canopy::CanopyBuilder::new().build()?;
        let status = canopy.with_root_context(|ctx| ctx.create_detached(Text::new("")))?;
        for (text, columns, width) in [
            ("界a", vec![0, 2, 3], 3),
            ("e\u{301}a", vec![0, 1, 1, 2], 2),
            ("abc", vec![0, 1, 2, 3], 3),
            ("", vec![0], 1),
        ] {
            let mut input = FontGymInput::new(text, Vec::new(), 1, FontEffects::default(), status);
            for (position, column) in columns.into_iter().enumerate() {
                input.cursor = position;
                assert_eq!(input.cursor_column(), column);
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
