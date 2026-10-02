//! Big text gym: custom text in `BigText`, with every setting.
//!
//! The field at the top takes the text, where `\n` starts a new line. The
//! gym opens with the keyboard in the field. Esc or Enter moves it to the
//! settings, e moves it back, and Tab moves it either way. The frame that has
//! the keyboard shows in the accent color. In the settings, j and k choose a
//! setting, h and l change it, and H and L change a number by ten. The preview
//! frames the area of the text, so its edges show, and the width and height
//! settings narrow it to try a fit. The ladder view draws the text in every
//! face at the scale setting. The ticker counts the last number of the text up
//! once a second, so its digits roll.

use std::time::Duration;

use canopy::{
    CanopyBuilder, Context, ContextExt, NodeId, NodeName, Register, Setup, TypedId, ViewContext,
    Widget,
    commands::CommandTarget,
    derive_commands,
    error::{Error, Result},
    geom::{Line, Rect, Size},
    layout::{Align, Direction, Edges, Layout, LayoutOverride, ScrollOp, Sizing},
    render::Render,
    style::{Attr, Color, Drift, GradientSpec, GradientStop, Paint, StyleRules, themes::Palette},
};
use canopy_widgets::{
    BigFace, BigSize, BigText, BigWeight, BoxGlyphs, Container, Frame, Input, Scroll, Text,
};

use crate::{fixed_row, flex_row};

/// Text of the gym as it opens.
const DEFAULT_TEXT: &str = "BigText 12.5K";
/// Time between two steps of the ticker.
const TICK: Duration = Duration::from_secs(1);
/// Width of the settings panel, frame included.
const SETTINGS_WIDTH: u32 = 36;
/// Width of the label column of the settings.
const LABEL_WIDTH: u32 = 11;
/// Most scale of the scale setting.
const MOST_SCALE: u32 = 16;
/// Most rows of the max rows setting.
const MOST_ROWS: u32 = 99;
/// Struts to choose from: none, the units of a figure, the descenders, and
/// accented capitals.
const STRUTS: [&str; 4] = ["", "k%", "gjpqy", "ÁÉ|"];
/// Time in which the gradient slides once across the text.
const DRIFT: Duration = Duration::from_secs(6);

/// What the stage shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    /// The text in its framed area.
    Preview,
    /// The text in every face.
    Ladder,
}

/// How the preview chooses its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// [`BigSize::Fit`].
    Fit,
    /// [`BigSize::MaxRows`].
    MaxRows,
    /// [`BigSize::Exact`].
    Exact,
}

/// The run styles of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    /// One run in the plain style.
    Plain,
    /// Numbers in the plain style, and letters and `%` dimmer, as units.
    Units,
    /// One run in a gradient that spans the block.
    Gradient,
}

/// One setting of the gym.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Setting {
    /// [`View`].
    View,
    /// [`Mode`].
    Size,
    /// The face of an exact size.
    Face,
    /// The scale of an exact size and of the ladder.
    Scale,
    /// The rows of a capped fit.
    MaxRows,
    /// [`BigWeight`].
    Weight,
    /// Horizontal alignment.
    Align,
    /// Vertical alignment.
    Vertical,
    /// Width of the area, or the whole stage.
    Width,
    /// Height of the area, or the whole stage.
    Height,
    /// One of [`STRUTS`].
    Strut,
    /// [`Look`].
    Look,
    /// Whether the last number counts up.
    Ticker,
}

/// The settings, in the order of the panel.
const SETTINGS: [Setting; 13] = [
    Setting::View,
    Setting::Size,
    Setting::Face,
    Setting::Scale,
    Setting::MaxRows,
    Setting::Weight,
    Setting::Align,
    Setting::Vertical,
    Setting::Width,
    Setting::Height,
    Setting::Strut,
    Setting::Look,
    Setting::Ticker,
];

impl Setting {
    /// Returns the label of the setting.
    const fn label(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Size => "size",
            Self::Face => "face",
            Self::Scale => "scale",
            Self::MaxRows => "max rows",
            Self::Weight => "weight",
            Self::Align => "align",
            Self::Vertical => "vertical",
            Self::Width => "width",
            Self::Height => "height",
            Self::Strut => "strut",
            Self::Look => "style",
            Self::Ticker => "ticker",
        }
    }
}

/// Every setting of the gym, and the text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    /// The text as typed, with `\n` for a new line.
    text: String,
    /// What the stage shows.
    view: View,
    /// How the preview chooses its size.
    mode: Mode,
    /// Index of the face in [`BigFace::ALL`].
    face: usize,
    /// Scale of an exact size and of the ladder.
    scale: u32,
    /// Rows of a capped fit.
    max_rows: u32,
    /// The weight.
    weight: BigWeight,
    /// Horizontal alignment.
    align: Align,
    /// Vertical alignment.
    vertical: Align,
    /// Width of the area, or `None` for the whole stage.
    width: Option<u32>,
    /// Height of the area, or `None` for the whole stage.
    height: Option<u32>,
    /// Index of the strut in [`STRUTS`].
    strut: usize,
    /// The run styles.
    look: Look,
    /// Whether the last number counts up.
    ticker: bool,
}

impl State {
    /// Returns the settings as the gym opens.
    fn new() -> Self {
        Self {
            text: DEFAULT_TEXT.to_owned(),
            view: View::Preview,
            mode: Mode::Fit,
            face: 0,
            scale: 1,
            max_rows: 6,
            weight: BigWeight::Bold,
            align: Align::Start,
            vertical: Align::Start,
            width: None,
            height: None,
            strut: 0,
            look: Look::Plain,
            ticker: false,
        }
    }

    /// Returns the face of an exact size.
    fn face(&self) -> BigFace {
        BigFace::ALL[self.face % BigFace::ALL.len()]
    }

    /// Returns the size of the preview.
    fn size(&self) -> BigSize {
        match self.mode {
            Mode::Fit => BigSize::Fit,
            Mode::MaxRows => BigSize::MaxRows(self.max_rows),
            Mode::Exact => BigSize::Exact {
                face: self.face(),
                scale: self.scale,
            },
        }
    }

    /// Returns whether a setting changes what the stage shows.
    fn applies(&self, setting: Setting) -> bool {
        let preview = self.view == View::Preview;
        match setting {
            Setting::Size | Setting::Width | Setting::Height => preview,
            Setting::Face => preview && self.mode == Mode::Exact,
            Setting::Scale => !preview || self.mode == Mode::Exact,
            Setting::MaxRows => preview && self.mode == Mode::MaxRows,
            _ => true,
        }
    }

    /// Returns the value of a setting as the panel shows it.
    fn value(&self, setting: Setting) -> String {
        let extent =
            |value: Option<u32>| value.map_or_else(|| "full".to_owned(), |n| n.to_string());
        match setting {
            Setting::View => match self.view {
                View::Preview => "preview",
                View::Ladder => "ladder",
            }
            .to_owned(),
            Setting::Size => match self.mode {
                Mode::Fit => "fit",
                Mode::MaxRows => "max rows",
                Mode::Exact => "exact",
            }
            .to_owned(),
            Setting::Face => face_name(self.face()).to_owned(),
            Setting::Scale => format!("×{}", self.scale),
            Setting::MaxRows => self.max_rows.to_string(),
            Setting::Weight => weight_name(self.weight).to_owned(),
            Setting::Align => align_name(self.align).to_owned(),
            Setting::Vertical => align_name(self.vertical).to_owned(),
            Setting::Width => extent(self.width),
            Setting::Height => extent(self.height),
            Setting::Strut => match STRUTS[self.strut % STRUTS.len()] {
                "" => "none".to_owned(),
                strut => strut.to_owned(),
            },
            Setting::Look => match self.look {
                Look::Plain => "plain",
                Look::Units => "dim units",
                Look::Gradient => "gradient",
            }
            .to_owned(),
            Setting::Ticker => if self.ticker { "on" } else { "off" }.to_owned(),
        }
    }

    /// Changes a setting by `delta` steps. `stage` is the area of the whole
    /// stage, which a width or height without a value takes.
    fn adjust(&mut self, setting: Setting, delta: i32, stage: Size) {
        match setting {
            Setting::View => {
                self.view = cycle(&[View::Preview, View::Ladder], self.view, delta);
            }
            Setting::Size => {
                self.mode = cycle(&[Mode::Fit, Mode::MaxRows, Mode::Exact], self.mode, delta);
            }
            Setting::Face => {
                self.face = step_index(self.face, BigFace::ALL.len(), delta);
            }
            Setting::Scale => self.scale = step_number(self.scale, delta, MOST_SCALE),
            Setting::MaxRows => self.max_rows = step_number(self.max_rows, delta, MOST_ROWS),
            Setting::Weight => {
                self.weight = cycle(&[BigWeight::Bold, BigWeight::Regular], self.weight, delta);
            }
            Setting::Align => self.align = cycle(&ALIGNS, self.align, delta),
            Setting::Vertical => self.vertical = cycle(&ALIGNS, self.vertical, delta),
            Setting::Width => self.width = step_extent(self.width, delta, stage.w),
            Setting::Height => self.height = step_extent(self.height, delta, stage.h),
            Setting::Strut => self.strut = step_index(self.strut, STRUTS.len(), delta),
            Setting::Look => {
                self.look = cycle(
                    &[Look::Plain, Look::Units, Look::Gradient],
                    self.look,
                    delta,
                );
            }
            Setting::Ticker => self.ticker = !self.ticker,
        }
    }

    /// Returns the runs of the text in the run styles.
    fn runs(&self) -> Vec<(String, String)> {
        let text = lines(&self.text);
        match self.look {
            Look::Plain => vec![("number".to_owned(), text)],
            Look::Gradient => vec![("gradient".to_owned(), text)],
            Look::Units => {
                let mut runs: Vec<(String, String)> = Vec::new();
                for ch in text.chars() {
                    let style = if ch.is_alphabetic() || ch == '%' {
                        "unit"
                    } else {
                        "number"
                    };
                    match runs.last_mut() {
                        Some((last, run)) if last == style => run.push(ch),
                        _ => runs.push((style.to_owned(), ch.to_string())),
                    }
                }
                runs
            }
        }
    }

    /// Gives `big` the text and every setting but its size.
    fn configure(&self, big: &mut BigText) {
        big.set_runs(self.runs());
        big.set_weight(self.weight);
        big.set_align(self.align);
        big.set_vertical_align(self.vertical);
        big.set_strut(STRUTS[self.strut % STRUTS.len()]);
    }

    /// Returns big text of the state at `size`.
    fn big(&self, size: BigSize) -> BigText {
        let mut big = BigText::new("").with_size(size);
        self.configure(&mut big);
        big
    }
}

/// The three alignments, in the order of the settings.
const ALIGNS: [Align; 3] = [Align::Start, Align::Center, Align::End];

/// Returns the item `delta` steps from `current` in `items`, wrapping.
fn cycle<T: Copy + PartialEq>(items: &[T], current: T, delta: i32) -> T {
    let index = items.iter().position(|item| *item == current).unwrap_or(0);
    items[step_index(index, items.len(), delta)]
}

/// Returns the index `delta` steps from `index` among `len`, wrapping.
fn step_index(index: usize, len: usize, delta: i32) -> usize {
    let len = i64::try_from(len).unwrap_or(1).max(1);
    let index = i64::try_from(index).unwrap_or(0);
    usize::try_from((index + i64::from(delta)).rem_euclid(len)).unwrap_or(0)
}

/// Returns `value` moved by `delta`, from 1 to `most`.
fn step_number(value: u32, delta: i32, most: u32) -> u32 {
    let next = i64::from(value) + i64::from(delta);
    u32::try_from(next.clamp(1, i64::from(most))).unwrap_or(1)
}

/// Returns an extent moved by `delta`. `None` is the whole extent `full`, and
/// a step that reaches it returns to `None`.
fn step_extent(value: Option<u32>, delta: i32, full: u32) -> Option<u32> {
    let next = i64::from(value.unwrap_or(full)) + i64::from(delta);
    (next < i64::from(full)).then(|| u32::try_from(next.max(1)).unwrap_or(1))
}

/// Returns typed text with each `\n` as a new line.
fn lines(text: &str) -> String {
    text.replace("\\n", "\n")
}

/// Returns `text` with its last number one higher: `12.5K` gives `12.6K`,
/// and `199` gives `200`. Text without a digit gains a `0`.
fn count_up(text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    let Some(last) = chars.iter().rposition(char::is_ascii_digit) else {
        return format!("{text}0");
    };
    let mut index = last;
    loop {
        if chars[index] == '9' {
            chars[index] = '0';
            match index.checked_sub(1) {
                Some(before) if chars[before].is_ascii_digit() => index = before,
                _ => {
                    chars.insert(index, '1');
                    break;
                }
            }
        } else {
            chars[index] = char::from(chars[index] as u8 + 1);
            break;
        }
    }
    chars.into_iter().collect()
}

/// Returns the name of a face.
const fn face_name(face: BigFace) -> &'static str {
    match face {
        BigFace::Compact => "compact",
        BigFace::Tamzen5x9 => "Tamzen 5×9",
        BigFace::Tamzen6x12 => "Tamzen 6×12",
        BigFace::Tamzen7x13 => "Tamzen 7×13",
        BigFace::Tamzen7x14 => "Tamzen 7×14",
        BigFace::Tamzen8x15 => "Tamzen 8×15",
        BigFace::Tamzen8x16 => "Tamzen 8×16",
        BigFace::Tamzen10x20 => "Tamzen 10×20",
    }
}

/// Returns the name of a weight.
const fn weight_name(weight: BigWeight) -> &'static str {
    match weight {
        BigWeight::Bold => "bold",
        BigWeight::Regular => "regular",
    }
}

/// Returns the name of an alignment.
const fn align_name(align: Align) -> &'static str {
    match align {
        Align::Start => "start",
        Align::Center => "center",
        Align::End => "end",
    }
}

/// The settings panel: a row for each setting, and what the preview draws.
struct Settings {
    /// The settings that the panel shows.
    state: State,
    /// Index of the chosen setting.
    selected: usize,
    /// The big text of the preview, whose area the panel reports.
    preview: Option<TypedId<BigText>>,
}

impl Settings {
    /// Returns the lines that report what the preview draws in its area.
    fn report(&self, ctx: &dyn ViewContext) -> Vec<(&'static str, String)> {
        if self.state.view == View::Ladder {
            return vec![("draws", format!("every face at ×{}", self.state.scale))];
        }
        let Some(area) = self
            .preview
            .and_then(|preview| ctx.view_of(preview.into()))
            .map(|view| view.content_size())
        else {
            return Vec::new();
        };
        let big = self.state.big(self.state.size());
        let (face, scale) = big.face_in(area);
        let size = big.size_in(area);
        let lines = lines(&self.state.text).lines().count().max(1);
        vec![
            ("draws", format!("{} ×{scale}", face_name(face))),
            ("text", format!("{}×{}", size.w, size.h)),
            ("area", format!("{}×{}", area.w, area.h)),
            ("lines", lines.to_string()),
        ]
    }
}

impl Widget for Settings {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        let area = ctx.view().view_rect_local();
        r.fill("", area, ' ')?;
        let width = area.w;
        let value_width = width.saturating_sub(LABEL_WIDTH);
        let mut y = area.tl.y;
        for (index, setting) in SETTINGS.iter().enumerate() {
            let selected = index == self.selected;
            let (label, value) = if selected && ctx.is_focused() {
                ("selected", "selected")
            } else if self.state.applies(*setting) {
                ("label", "value")
            } else {
                ("label", "unused")
            };
            let marker = if selected { "›" } else { " " };
            r.fill(label, Rect::new(area.tl.x, y, width, 1), ' ')?;
            r.text(
                label,
                Line::new(area.tl.x, y, LABEL_WIDTH),
                &format!("{marker} {}", setting.label()),
            )?;
            r.text(
                value,
                Line::new(area.tl.x + LABEL_WIDTH, y, value_width),
                &self.state.value(*setting),
            )?;
            y += 1;
        }
        y += 1;
        for (label, value) in self.report(ctx) {
            r.text(
                "label",
                Line::new(area.tl.x, y, LABEL_WIDTH),
                &format!("  {label}"),
            )?;
            r.text(
                "report",
                Line::new(area.tl.x + LABEL_WIDTH, y, value_width),
                &value,
            )?;
            y += 1;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("big_gym_settings")
    }
}

/// One step of the ladder: its heading and its text.
struct Step {
    /// The heading.
    heading: TypedId<Text>,
    /// The text.
    big: TypedId<BigText>,
}

/// The mounted nodes of the gym.
struct Nodes {
    /// The text field.
    input: TypedId<Input>,
    /// The settings panel.
    settings: TypedId<Settings>,
    /// The stage beside the settings.
    stage: NodeId,
    /// The frame of the preview area.
    frame: TypedId<Frame>,
    /// The big text of the preview.
    preview: TypedId<BigText>,
    /// The scroll of the ladder.
    ladder: NodeId,
    /// The steps of the ladder, one for each face.
    steps: Vec<Step>,
}

/// The big text gym.
pub struct BigGym {
    /// Every setting, and the text.
    state: State,
    /// Index of the chosen setting.
    selected: usize,
    /// The mounted nodes.
    nodes: Option<Nodes>,
}

impl Default for BigGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl BigGym {
    /// Constructs the gym with its default text and settings.
    pub fn new() -> Self {
        Self {
            state: State::new(),
            selected: 0,
            nodes: None,
        }
    }

    /// Replaces the text. The text field posts this after every edit.
    /// @param value The text, with `\n` for a new line.
    #[command]
    pub fn set_text(&mut self, c: &mut dyn Context, value: String) -> Result<()> {
        self.state.text = value;
        self.apply(c)
    }

    /// Chooses the setting `delta` rows below the chosen one.
    /// @param delta Rows to move, negative to move up.
    #[command]
    pub fn select(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        self.selected = step_index(self.selected, SETTINGS.len(), delta);
        self.apply(c)
    }

    /// Changes the chosen setting by `delta` steps.
    /// @param delta Steps to change by, negative to go back.
    #[command]
    pub fn adjust(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        let setting = SETTINGS[self.selected];
        let stage = self.stage_area(c)?;
        self.state.adjust(setting, delta, stage);
        if setting == Setting::Ticker && self.state.ticker {
            c.request_poll()?;
        }
        self.apply(c)
    }

    /// Scrolls the ladder by `delta` pages.
    /// @param delta Pages to scroll, negative to scroll up.
    #[command]
    pub fn scroll_ladder(&self, c: &mut dyn Context, delta: i32) -> Result<()> {
        let ladder = self.nodes()?.ladder;
        c.scroll_node(ladder, ScrollOp::pages(delta))?;
        Ok(())
    }

    /// Gives the keyboard to the text field.
    #[command]
    pub fn focus_text(&self, c: &mut dyn Context) -> Result<()> {
        let input = self.nodes()?.input;
        c.set_focus(input.into())?;
        Ok(())
    }

    /// Gives the keyboard to the settings.
    #[command]
    pub fn focus_settings(&self, c: &mut dyn Context) -> Result<()> {
        let settings = self.nodes()?.settings;
        c.set_focus(settings.into())?;
        Ok(())
    }

    /// Returns the mounted nodes.
    fn nodes(&self) -> Result<&Nodes> {
        self.nodes
            .as_ref()
            .ok_or_else(|| Error::Invalid("the big text gym is not mounted".into()))
    }

    /// Returns the area of the whole stage, inside the frame of the preview.
    fn stage_area(&self, c: &dyn Context) -> Result<Size> {
        let stage = self.nodes()?.stage;
        let size = c
            .view_of(stage)
            .map_or(Size::new(0, 0), |view| view.content_size());
        Ok(Size::new(
            size.w.saturating_sub(2),
            size.h.saturating_sub(2),
        ))
    }

    /// Gives the preview, the ladder, and the settings panel the state.
    fn apply(&self, c: &mut dyn Context) -> Result<()> {
        let state = &self.state;
        let nodes = self.nodes()?;
        let preview = state.view == View::Preview;
        c.set_hidden(nodes.frame.into(), !preview)?;
        c.set_hidden(nodes.ladder, preview)?;
        c.with_widget_mut(nodes.preview, |big: &mut BigText, _| {
            state.configure(big);
            big.set_size(state.size());
            Ok(())
        })?;
        // The frame takes two columns and two rows around the area. A set
        // extent takes no share of the flexible space.
        let mut area = LayoutOverride::new();
        area = match state.width {
            Some(width) => LayoutOverride {
                width: Some(Sizing::Measure),
                ..area.fixed_width(width + 2)
            },
            None => area.flex_horizontal(1),
        };
        area = match state.height {
            Some(height) => LayoutOverride {
                height: Some(Sizing::Measure),
                ..area.fixed_height(height + 2)
            },
            None => area.flex_vertical(1),
        };
        c.set_layout_override(nodes.frame.into(), area)?;
        for (step, face) in nodes.steps.iter().zip(BigFace::ALL) {
            let size = BigSize::Exact {
                face,
                scale: state.scale,
            };
            let drawn = state.big(size).size_in(Size::new(u32::MAX, u32::MAX));
            c.with_widget_mut(step.big, |big: &mut BigText, _| {
                state.configure(big);
                big.set_size(size);
                Ok(())
            })?;
            c.set_layout_override(step.big.into(), fixed_row(drawn.h.max(1)))?;
            let heading = format!(
                "{} · {} · ×{} · {}×{}",
                face_name(face),
                weight_name(state.weight),
                state.scale,
                drawn.w,
                drawn.h
            );
            c.with_widget_mut(step.heading, |text: &mut Text, _| {
                text.set_text(heading);
                Ok(())
            })?;
        }
        let selected = self.selected;
        let state = state.clone();
        c.with_widget_mut(nodes.settings, |settings: &mut Settings, _| {
            settings.state = state;
            settings.selected = selected;
            Ok(())
        })
    }
}

impl Widget for BigGym {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let node = c.node_id();
        let owner = CommandTarget::Exact(node);
        let field = c.add_child(
            node,
            Frame::new()
                .with_glyphs(BoxGlyphs::ROUND)
                .with_title("text: \\n starts a line"),
        )?;
        c.set_layout_override(field.into(), fixed_row(3))?;
        let input = c.add_child(
            field,
            Input::new(self.state.text.clone())
                .with_name("big_gym_text")
                .with_on_change(Self::spec_set_text().call().with_target(owner)),
        )?;
        let body = c.add_child(
            node,
            Container::new(Layout::fill().direction(Direction::Row).gap(1)),
        )?;
        c.set_layout_override(body.into(), flex_row(1))?;
        let panel = c.add_child(
            body,
            Frame::new()
                .with_glyphs(BoxGlyphs::ROUND)
                .with_title("settings"),
        )?;
        // The panel keeps its width, and takes no share of the flexible
        // width.
        c.set_layout_override(
            panel.into(),
            LayoutOverride {
                width: Some(Sizing::Measure),
                ..LayoutOverride::new()
                    .fixed_width(SETTINGS_WIDTH)
                    .flex_vertical(1)
            },
        )?;
        let settings = c.add_child(
            panel,
            Settings {
                state: self.state.clone(),
                selected: self.selected,
                preview: None,
            },
        )?;
        let stage = c.add_child(
            body,
            Container::new(Layout::fill().direction(Direction::Column)),
        )?;
        c.set_layout_override(
            stage.into(),
            LayoutOverride::new().flex_horizontal(1).flex_vertical(1),
        )?;
        let frame = c.add_child(
            stage,
            Frame::new()
                .with_glyphs(BoxGlyphs::ROUND)
                .with_title("preview"),
        )?;
        let preview = c.add_child(frame, BigText::new(""))?;
        c.set_layout_override(
            preview.into(),
            LayoutOverride::new().flex_horizontal(1).flex_vertical(1),
        )?;
        let ladder = c.add_child(stage, Scroll::vertical())?;
        c.set_layout_override(
            ladder.into(),
            LayoutOverride::new().flex_horizontal(1).flex_vertical(1),
        )?;
        let page = c.add_child(
            ladder,
            Container::new(
                Layout::column()
                    .flex_horizontal(1)
                    .padding(Edges::new(0, 1, 1, 1))
                    .gap(1),
            )
            .with_name("big_gym_ladder"),
        )?;
        let mut steps = Vec::new();
        for _ in BigFace::ALL {
            let heading = c.add_child(page, Text::new("").with_style("heading"))?;
            c.set_layout_override(heading.into(), fixed_row(1))?;
            let big = c.add_child(page, BigText::new(""))?;
            steps.push(Step { heading, big });
        }
        c.with_widget_mut(settings, |settings: &mut Settings, _| {
            settings.preview = Some(preview);
            Ok(())
        })?;
        self.nodes = Some(Nodes {
            input,
            settings,
            stage: stage.into(),
            frame,
            preview,
            ladder: ladder.into(),
            steps,
        });
        self.apply(c)?;
        c.set_focus(input.into())?;
        Ok(())
    }

    fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("big_gym");
        r.fill("", ctx.view().view_rect_local(), ' ')
    }

    fn poll(&mut self, c: &mut dyn Context) -> Result<Option<Duration>> {
        if !self.state.ticker {
            return Ok(None);
        }
        self.state.text = count_up(&self.state.text);
        let input = self.nodes()?.input;
        let text = self.state.text.clone();
        c.with_widget_mut(input, |input: &mut Input, _| {
            input.set_value(text);
            Ok(())
        })?;
        self.apply(c)?;
        Ok(Some(TICK))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("big_gym")
    }
}

impl Register for BigGym {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        Input::register(setup)?;
        Ok(())
    }
}

/// Build the gym styles from a palette.
fn styles(p: &Palette, rules: StyleRules<'_>) {
    let gradient = GradientSpec::with_stops(
        25.0,
        vec![
            GradientStop::new(0.0, p.yellow),
            GradientStop::new(0.35, p.orange),
            GradientStop::new(0.7, p.magenta),
            GradientStop::new(1.0, p.yellow),
        ],
    )
    .with_drift(Drift::Slide(DRIFT));
    let selected: Color = p.accent;
    rules
        .prefix("big_gym")
        .fg("number", p.fg)
        .fg("unit", p.muted_fg)
        .fg("gradient", Paint::gradient(gradient))
        .fg("heading", p.muted_fg)
        .fg("label", p.muted_fg)
        .fg("value", p.fg)
        .fg("unused", p.faint_fg)
        .fg("report", p.cyan)
        .fg("frame/focused", p.accent)
        .fg("selected", p.bg)
        .bg("selected", selected)
        .attr("selected", Attr::Bold)
        .apply();
}

/// Bindings of the gym. The settings keys act while the settings have the
/// keyboard, so the text field takes them as text. Esc and Enter leave the
/// field before it sees them, so its help lists them.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()
canopy.keymap({
    path = "big_gym",
    { key = "Tab", description = "Move between the text and the settings", action = command.root.focus("next") },
    { key = "BackTab", description = "Move between the text and the settings", action = command.root.focus("prev") },
})
canopy.keymap({
    path = "big_gym_text",
    phase = "before_widget",
    { key = { "Esc", "Enter" }, description = "Back to the settings", action = command.big_gym.focus_settings() },
})
canopy.keymap({
    path = "big_gym_settings",
    { key = "e", description = "Edit the text", action = command.big_gym.focus_text() },
    { key = { "j", "Down" }, description = "Next setting", action = command.big_gym.select(1) },
    { key = { "k", "Up" }, description = "Previous setting", action = command.big_gym.select(-1) },
    { key = { "l", "Right" }, description = "Change the setting", action = command.big_gym.adjust(1) },
    { key = { "h", "Left" }, description = "Change the setting back", action = command.big_gym.adjust(-1) },
    { key = "L", description = "Add ten", action = command.big_gym.adjust(10) },
    { key = "H", description = "Take ten", action = command.big_gym.adjust(-10) },
    { key = { "PageDown", "Space" }, description = "Page down the ladder", action = command.big_gym.scroll_ladder(1) },
    { key = "PageUp", description = "Page up the ladder", action = command.big_gym.scroll_ladder(-1) },
})
"#;

/// Queue this demo's styles and bindings.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder
        .configure(|setup| {
            setup.widget_styles(styles);
            Ok(())
        })
        .script("biggym", DEFAULT_BINDINGS)
}

#[cfg(test)]
mod tests {
    use canopy::{ViewContextExt, input::key::Key, testing::harness::Harness};

    use super::*;
    use crate::tests::{Mount, root_harness};

    /// Returns a harness of the gym at `size`.
    fn gym(size: Size) -> Result<Harness> {
        root_harness(
            BigGym::new(),
            |builder| binding_setup(builder.configure(BigGym::register)),
            size,
            Mount::Wrap,
        )
    }

    /// Returns the screen as text, one line a row.
    fn screen(harness: &Harness) -> String {
        harness.tbuf().lines().join("\n")
    }

    /// Sends key specs in order.
    fn keys(harness: &mut Harness, specs: &[&str]) -> Result<()> {
        for spec in specs {
            harness.key(Key::parse_spec(spec)?)?;
        }
        Ok(())
    }

    #[test]
    fn keys_move_the_keyboard_between_the_text_and_the_settings() -> Result<()> {
        let mut harness = gym(Size::new(100, 30))?;
        let focus = |harness: &Harness| {
            harness.canopy.with_root_view(|ctx| {
                let focused = ctx.focused_node();
                let input = ctx.unique_descendant::<Input>(ctx.root_id());
                let settings = ctx.unique_descendant::<Settings>(ctx.root_id());
                match (input, settings) {
                    (Ok(Some(input)), _) if focused == Some(input.into()) => "text",
                    (_, Ok(Some(settings))) if focused == Some(settings.into()) => "settings",
                    _ => "elsewhere",
                }
            })
        };
        assert_eq!(focus(&harness), "text", "the gym opens in the field");
        for (spec, expected) in [
            ("Esc", "settings"),
            ("e", "text"),
            ("Enter", "settings"),
            ("Tab", "text"),
            ("Tab", "settings"),
        ] {
            keys(&mut harness, &[spec])?;
            assert_eq!(focus(&harness), expected, "after {spec}");
        }
        Ok(())
    }

    #[test]
    fn the_last_number_counts_up() {
        assert_eq!(count_up("12.5K"), "12.6K");
        assert_eq!(count_up("199"), "200");
        assert_eq!(count_up("x9y"), "x10y");
        assert_eq!(count_up("a1 b9"), "a1 b10");
        assert_eq!(count_up("Verber"), "Verber0");
    }

    #[test]
    fn an_escaped_newline_starts_a_line() {
        assert_eq!(lines("a\\nb"), "a\nb");
    }

    #[test]
    fn steps_wrap_and_clamp() {
        assert_eq!(step_index(0, 3, -1), 2);
        assert_eq!(step_number(1, -10, 16), 1);
        assert_eq!(step_number(10, 10, 16), 16);
        assert_eq!(step_extent(None, -1, 40), Some(39));
        assert_eq!(step_extent(Some(39), 1, 40), None);
        assert_eq!(step_extent(Some(3), -10, 40), Some(1));
    }

    #[test]
    fn units_take_their_own_run() {
        let state = State {
            text: "12.5K 87%".to_owned(),
            look: Look::Units,
            ..State::new()
        };
        let styles: Vec<_> = state.runs().into_iter().map(|(style, _)| style).collect();
        assert_eq!(styles, ["number", "unit", "number", "unit"]);
    }

    #[test]
    fn settings_keys_change_the_preview() -> Result<()> {
        let mut harness = gym(Size::new(100, 30))?;
        assert!(screen(&harness).contains("draws"), "{}", screen(&harness));
        keys(&mut harness, &["Esc"])?;
        // Size: fit, max rows, exact. Exact takes the compact face at ×1.
        keys(&mut harness, &["j", "l", "l"])?;
        harness.render()?;
        let text = screen(&harness);
        assert!(text.contains("exact"), "{text}");
        assert!(text.contains("compact ×1"), "{text}");
        // Face: Tamzen 5×9, then scale ×2.
        keys(&mut harness, &["j", "l", "j", "l"])?;
        harness.render()?;
        let text = screen(&harness);
        assert!(text.contains("Tamzen 5×9 ×2"), "{text}");
        Ok(())
    }

    #[test]
    fn the_field_takes_text_and_the_ladder_draws_every_face() -> Result<()> {
        let mut harness = gym(Size::new(100, 40))?;
        // The gym opens with the keyboard in the field.
        keys(
            &mut harness,
            &[
                "End",
                "Backspace",
                "Backspace",
                "Backspace",
                "Backspace",
                "Backspace",
            ],
        )?;
        keys(&mut harness, &["j"])?;
        harness.render()?;
        assert!(
            screen(&harness).contains("BigText j"),
            "{}",
            screen(&harness)
        );
        // Back to the settings, and the ladder view.
        keys(&mut harness, &["Enter", "l"])?;
        harness.render()?;
        let text = screen(&harness);
        assert!(text.contains("every face at ×1"), "{text}");
        assert!(text.contains("compact · bold · ×1"), "{text}");
        Ok(())
    }
}
