//! Chart gym: the chart widgets and primitives of `canopy_widgets`, live, in
//! every theme.
//!
//! Four pages share one live model:
//!
//! - The widgets page shows stats tiles of `BigText`, `Meter`, and `Sparkline`,
//!   and more sparklines and meters below them.
//! - The text page shows every glyph of `BigText`, runs in their own styles,
//!   and a live clock.
//! - The primitives page paints stacked bars, eighth-block ramps, gradient
//!   bars, a mirrored column chart, and a braille line with
//!   `canopy_widgets::chart`.
//! - The columns page shows a `ColumnChart` of the attempts, with markers, a
//!   context window line, a cursor, and a hover, and a line that inspects the
//!   attempt under them.
//!
//! `Tab` shows the next page, `p` pauses, `t` shows the next theme, and `m`
//! mutes every other attempt of the column charts. On the columns page, `h`
//! and `l` move the cursor, `H` and `L` move it between labels, `[` and `]`
//! move it to the first and the newest attempt, and `z` fits every attempt
//! to the width or shows one a cell. Each page scrolls with the
//! navigation keys and the wheel, so a short terminal reaches all of it.

use std::{collections::VecDeque, f64::consts::TAU, time::Duration};

use canopy::{
    CanopyBuilder, Context, ContextExt, NodeId, NodeName, Register, Setup, TypedId, ViewContext,
    Widget,
    commands::CommandTarget,
    derive_commands,
    error::{Error, Result},
    geom::{Line, Point, Rect, Size},
    layout::{Align, Direction, Edges, Layout, LayoutOverride, MeasureConstraints, Measurement},
    render::Render,
    style::{
        Attr, Color, GradientSpec, GradientStop, Mix, Paint, StyleRules,
        themes::{self, Palette},
    },
};
use canopy_widgets::{
    BigText, BoxGlyphs, ColumnChart, Container, Frame, Meter, Scroll, Sparkline, Tabs, Text,
    chart::{self, Base, Braille, Column, Marker, Scale, Segment, Tint},
};

use crate::{fixed_row, flex_row};

/// A theme: its name and the function that builds its palette.
type Theme = (&'static str, fn() -> Palette);

/// Themes that `t` moves through, in order.
const THEMES: [Theme; 5] = [
    ("default dark", themes::default_dark),
    ("solarized dark", themes::solarized_dark),
    ("solarized light", themes::solarized_light),
    ("gruvbox dark", themes::gruvbox_dark),
    ("dracula", themes::dracula),
];

/// Time between two steps of the live data.
const STEP: Duration = Duration::from_millis(100);
/// Steps between two new attempts of the column chart.
const STEPS_PER_ATTEMPT: u64 = 2;
/// Attempts that the model keeps. A wider view shows fewer columns.
const HISTORY: usize = 400;
/// Attempts that the model starts with, so that the charts open full.
const FIRST_ATTEMPTS: u64 = 240;
/// Attempts between two compactions of the context.
const COMPACTION_EVERY: u64 = 37;
/// Context window of the attempts, in the units of the model.
const WINDOW: f64 = 272.0;
/// Steps of one sweep of the sweep bar, up and down.
const SWEEP_STEPS: u64 = 96;
/// Cells of the sweep bar: half a sweep fills it one eighth a step.
const SWEEP_CELLS: u32 = 6;
/// Values that each sparkline starts with, so that it opens full.
const FIRST_VALUES: u64 = 240;
/// First step of the values that the sparklines start with. The steps lie
/// far from the live ones, so the live values do not repeat them.
const FIRST_VALUES_SEED: u64 = 1_000_000;

/// Width of the label column of the bars and the widget rows.
const LABEL_WIDTH: u32 = 14;
/// Widest bar of the primitives page.
const BAR_WIDTH: u32 = 48;
/// Narrowest bar of the primitives page.
const MIN_BAR_WIDTH: u32 = 8;
/// Rows above the axis of the column chart.
const UPPER_ROWS: u32 = 6;
/// Rows below the axis of the column chart.
const LOWER_ROWS: u32 = 2;
/// Rows of the braille line of the primitives page.
const BRAILLE_ROWS: u32 = 3;
/// Rows of a stats tile: a border, big text, a meter, a sparkline, a border.
const TILE_ROWS: u32 = 7;
/// Rows of the chart of the columns page.
const TIMELINE_ROWS: u32 = 16;
/// Attempts between two labels of the chart of the columns page.
const LABEL_EVERY: u64 = 40;
/// Rows of the primitives page: its top margin, the two groups of bars, the
/// ramps, the gradient bars, the column chart, and the braille line, each
/// with its heading and the blank row after it.
const PRIMITIVES_ROWS: u32 =
    1 + 10 + 3 + 5 + (1 + UPPER_ROWS + 1 + LOWER_ROWS + 1) + (1 + BRAILLE_ROWS);

/// One bar series: its parts drift around a base, and its total grows.
struct Series {
    /// Label of the series.
    label: &'static str,
    /// Base cache read percent.
    read: f64,
    /// Base cache write percent.
    write: f64,
    /// How far the cache read percent swings around its base.
    swing: f64,
    /// Total at step zero.
    total: f64,
    /// Growth of the total each step.
    growth: f64,
}

impl Series {
    /// Returns the cache read, cache write, and fresh percents at `step`.
    fn parts(&self, step: u64, index: usize) -> [f64; 3] {
        let phase = step as f64 * 0.05 + index as f64 * 2.1;
        let read = (self.read + self.swing * phase.sin()).clamp(0.0, 100.0);
        let write = self.write * (1.0 + 0.5 * (phase * 1.7).cos());
        [read, write, (100.0 - read - write).max(0.0)]
    }

    /// Returns the total at `step`.
    fn total(&self, step: u64) -> f64 {
        self.total + self.growth * step as f64
    }
}

/// The bar series.
const SERIES: [Series; 3] = [
    Series {
        label: "alpha",
        read: 88.0,
        write: 2.1,
        swing: 5.0,
        total: 3_182_523.0,
        growth: 9_000.0,
    },
    Series {
        label: "beta",
        read: 78.0,
        write: 7.0,
        swing: 9.0,
        total: 1_010_562.0,
        growth: 4_000.0,
    },
    Series {
        label: "gamma",
        read: 18.0,
        write: 1.0,
        swing: 12.0,
        total: 207_510.0,
        growth: 1_500.0,
    },
];

/// One attempt of the model: the parts of its input and its output.
#[derive(Debug, Clone, Copy)]
struct Sample {
    /// Number of the attempt, from zero.
    index: u64,
    /// Cache read input.
    read: f64,
    /// Cache write input.
    write: f64,
    /// Fresh input.
    fresh: f64,
    /// Reasoning output.
    reasoning: f64,
    /// Other output.
    other: f64,
}

impl Sample {
    /// Returns the whole input.
    fn input(&self) -> f64 {
        self.read + self.write + self.fresh
    }

    /// Returns the whole output.
    fn output(&self) -> f64 {
        self.reasoning + self.other
    }

    /// Returns the percent of the input that a cache served.
    fn hit(&self) -> f64 {
        self.read / self.input().max(f64::EPSILON) * 100.0
    }
}

/// The live data of the gym.
struct Live {
    /// Steps so far.
    step: u64,
    /// The latest attempts, oldest first.
    attempts: VecDeque<Sample>,
    /// Attempts so far, including the ones that left the history.
    count: u64,
    /// Context of the attempts, which grows and then compacts.
    context: f64,
    /// Output of every attempt so far.
    output: f64,
    /// Reasoning within that output.
    reasoning: f64,
}

impl Live {
    /// Returns the model at step zero, with its first attempts.
    fn new() -> Self {
        let mut live = Self {
            step: 0,
            attempts: VecDeque::with_capacity(HISTORY),
            count: 0,
            context: 20.0,
            output: 0.0,
            reasoning: 0.0,
        };
        for _ in 0..FIRST_ATTEMPTS {
            live.push_attempt();
        }
        live
    }

    /// Moves one step on, and returns the attempt that the step added, if it
    /// added one.
    fn advance(&mut self) -> Option<Sample> {
        self.step += 1;
        self.step
            .is_multiple_of(STEPS_PER_ATTEMPT)
            .then(|| self.push_attempt())
    }

    /// Adds the next attempt, and drops the oldest past the history. The
    /// context grows with each attempt, and each compaction starts it small
    /// and mostly fresh.
    fn push_attempt(&mut self) -> Sample {
        let index = self.count;
        self.count += 1;
        let compacted = index > 0 && index.is_multiple_of(COMPACTION_EVERY);
        let (fresh, write) = if compacted {
            self.context = 30.0;
            (self.context * 0.9, self.context * 0.05)
        } else {
            self.context += 2.0 + 3.0 * jitter(index);
            (1.5 + 2.0 * jitter(index + 7), 0.8 * jitter(index + 13))
        };
        let output = 1.0 + 6.0 * jitter(index + 29).powi(3);
        let reasoning = output * 0.6 * jitter(index + 41);
        let sample = Sample {
            index,
            read: (self.context - fresh - write).max(0.0),
            write,
            fresh,
            reasoning,
            other: output - reasoning,
        };
        self.output += output;
        self.reasoning += reasoning;
        self.attempts.push_back(sample);
        if self.attempts.len() > HISTORY {
            self.attempts.pop_front();
        }
        sample
    }

    /// Returns the latest attempt.
    fn latest(&self) -> Sample {
        *self
            .attempts
            .back()
            .expect("the model starts with attempts")
    }

    /// Returns the total of the bar series.
    fn tokens(&self) -> f64 {
        SERIES.iter().map(|series| series.total(self.step)).sum()
    }

    /// Returns the cache read, cache write, and fresh percents of all series,
    /// weighted by their totals.
    fn composition(&self) -> [f64; 3] {
        let total = self.tokens().max(f64::EPSILON);
        let mut parts = [0.0; 3];
        for (index, series) in SERIES.iter().enumerate() {
            let weight = series.total(self.step) / total;
            for (part, value) in parts.iter_mut().zip(series.parts(self.step, index)) {
                *part += value * weight;
            }
        }
        parts
    }
}

/// The widgets of one stats tile.
struct Tile {
    /// The big value.
    value: TypedId<BigText>,
    /// The meter below the value.
    meter: TypedId<Meter>,
    /// The sparkline at the bottom.
    spark: TypedId<Sparkline>,
}

/// The widgets that the live data updates.
struct Nodes {
    /// The line above the pages: the theme and whether the data moves.
    title: TypedId<Text>,
    /// The pages.
    tabs: TypedId<Tabs>,
    /// The tokens tile.
    tokens: Tile,
    /// The cache hit tile.
    cache: Tile,
    /// The output tile.
    output: Tile,
    /// The context tile.
    context: Tile,
    /// A sparkline of bars with gaps.
    bars: TypedId<Sparkline>,
    /// A sparkline of bars three rows high.
    tall: TypedId<Sparkline>,
    /// A sparkline that draws a braille line.
    line: TypedId<Sparkline>,
    /// A meter of one value with a label.
    level: TypedId<Meter>,
    /// A meter of stacked values.
    stacked: TypedId<Meter>,
    /// A meter with a gradient fill.
    heat: TypedId<Meter>,
    /// A clock of the time the data has moved.
    clock: TypedId<BigText>,
    /// The primitives page.
    primitives: TypedId<Primitives>,
    /// The column chart of the attempts.
    timeline: TypedId<ColumnChart>,
    /// The line that inspects the attempt under the cursor or the hover.
    inspector: TypedId<Text>,
}

/// The chart gym: the pages and the live data that drives them.
pub struct ChartGym {
    /// Index of the theme on screen, in [`THEMES`].
    theme: usize,
    /// Whether every other attempt of the column chart is muted.
    muted: bool,
    /// Whether the live data stands still.
    paused: bool,
    /// The live data.
    live: Live,
    /// First attempt in the columns last given to the timeline.
    charted_first: Option<u64>,
    /// The widgets that the live data updates, once mounted.
    nodes: Option<Nodes>,
}

impl Default for ChartGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl ChartGym {
    /// Construct the chart gym in the default theme.
    pub fn new() -> Self {
        Self {
            theme: 0,
            muted: false,
            paused: false,
            live: Live::new(),
            charted_first: None,
            nodes: None,
        }
    }

    /// Show the next theme.
    #[command]
    pub fn next_theme(&mut self, c: &mut dyn Context) -> Result<()> {
        self.theme = (self.theme + 1) % THEMES.len();
        c.set_theme((THEMES[self.theme].1)());
        self.show(c)
    }

    /// Mute or unmute every other attempt of the column charts.
    #[command]
    pub fn toggle_mute(&mut self, c: &mut dyn Context) -> Result<()> {
        self.muted = !self.muted;
        self.timeline(c)?;
        self.show(c)
    }

    /// Move the cursor of the column chart by `delta` attempts.
    /// @param delta Attempts to move, negative for older ones.
    #[command]
    pub fn timeline_by(&self, c: &mut dyn Context, delta: i32) -> Result<()> {
        let chart = self.nodes()?.timeline;
        c.with_widget_mut(chart, |chart: &mut ColumnChart, ctx| {
            chart.cursor_by(ctx, delta)
        })
    }

    /// Move the cursor of the column chart to the next label, or to the
    /// previous one when `delta` is negative.
    /// @param delta Direction of the move.
    #[command]
    pub fn timeline_label(&self, c: &mut dyn Context, delta: i32) -> Result<()> {
        let chart = self.nodes()?.timeline;
        c.with_widget_mut(chart, |chart: &mut ColumnChart, ctx| {
            chart.cursor_label(ctx, delta)
        })
    }

    /// Move the cursor of the column chart to the first attempt.
    #[command]
    pub fn timeline_first(&self, c: &mut dyn Context) -> Result<()> {
        let chart = self.nodes()?.timeline;
        c.with_widget_mut(chart, |chart: &mut ColumnChart, ctx| {
            chart.cursor_first(ctx)
        })
    }

    /// Move the cursor of the column chart to the newest attempt, which it
    /// then follows.
    #[command]
    pub fn timeline_newest(&self, c: &mut dyn Context) -> Result<()> {
        let chart = self.nodes()?.timeline;
        c.with_widget_mut(chart, |chart: &mut ColumnChart, ctx| {
            chart.cursor_newest(ctx)
        })
    }

    /// Fit every attempt of the column chart to its width, or show one
    /// attempt a cell.
    #[command]
    pub fn toggle_fit(&self, c: &mut dyn Context) -> Result<()> {
        let chart = self.nodes()?.timeline;
        c.with_widget_mut(chart, |chart: &mut ColumnChart, _| {
            chart.set_fit(!chart.fit());
            Ok(())
        })?;
        self.inspect(c)
    }

    /// Describe the attempt under the cursor or the hover of the column
    /// chart, which posts this after each move.
    #[command]
    pub fn timeline_moved(&self, c: &mut dyn Context) -> Result<()> {
        self.inspect(c)
    }

    /// Pause or resume the live data.
    #[command]
    pub fn toggle_pause(&mut self, c: &mut dyn Context) -> Result<()> {
        self.paused = !self.paused;
        if !self.paused {
            c.request_poll()?;
        }
        self.show(c)
    }

    /// Show the next page.
    #[command]
    pub fn next_page(&self, c: &mut dyn Context) -> Result<()> {
        let tabs = self.nodes()?.tabs;
        c.with_widget_mut(tabs, |tabs: &mut Tabs, ctx| tabs.cycle(ctx, 1))
    }

    /// Show one page: 0 for widgets, 1 for text, 2 for primitives, and 3
    /// for columns.
    /// @param index Index of the page.
    #[command]
    pub fn show_page(&self, c: &mut dyn Context, index: usize) -> Result<()> {
        let tabs = self.nodes()?.tabs;
        c.with_widget_mut(tabs, |tabs: &mut Tabs, ctx| tabs.select(ctx, index))
    }

    /// Returns the mounted widgets.
    fn nodes(&self) -> Result<&Nodes> {
        self.nodes
            .as_ref()
            .ok_or_else(|| Error::Invalid("the chart gym is not mounted".into()))
    }

    /// Fills the sparklines with the values before step zero, so that they
    /// open full.
    fn fill(&self, c: &mut dyn Context) -> Result<()> {
        let nodes = self.nodes()?;
        let attempts = self.live.attempts.iter();
        let before = (0..FIRST_VALUES).map(|back| step_values(FIRST_VALUES_SEED + back));
        let (rates, bars, waves): (Vec<_>, Vec<_>, Vec<_>) = before.fold(
            (Vec::new(), Vec::new(), Vec::new()),
            |(mut rates, mut bars, mut waves), (rate, bar, wave)| {
                rates.push(rate);
                bars.push(bar);
                waves.push(wave);
                (rates, bars, waves)
            },
        );
        replace_values(c, nodes.tokens.spark, rates)?;
        replace_values(c, nodes.bars, bars)?;
        replace_values(c, nodes.line, waves)?;
        let hits = attempts.clone().map(|sample| Some(sample.hit()));
        replace_values(c, nodes.cache.spark, hits.collect())?;
        let outputs = attempts.clone().map(|sample| Some(sample.output()));
        replace_values(c, nodes.output.spark, outputs.collect())?;
        let inputs = attempts
            .map(|sample| Some(sample.input()))
            .collect::<Vec<_>>();
        replace_values(c, nodes.context.spark, inputs.clone())?;
        replace_values(c, nodes.tall, inputs)
    }

    /// Adds the values of one step to the sparklines, then shows the state.
    fn feed(&mut self, c: &mut dyn Context, attempt: Option<Sample>) -> Result<()> {
        let nodes = self.nodes()?;
        let step = self.live.step;
        let (rate, bar, wave) = step_values(step);
        push(c, nodes.tokens.spark, rate)?;
        push(c, nodes.bars, bar)?;
        push(c, nodes.line, wave)?;
        if let Some(sample) = attempt {
            push(c, nodes.cache.spark, sample.hit())?;
            push(c, nodes.output.spark, sample.output())?;
            push(c, nodes.context.spark, sample.input())?;
            push(c, nodes.tall, sample.input())?;
            self.timeline(c)?;
        }
        self.show(c)
    }

    /// Shows the attempts in the column chart. A new set of columns clears
    /// the hover, so the chart changes only when an attempt arrives.
    fn timeline(&mut self, c: &mut dyn Context) -> Result<()> {
        let timeline = self.nodes()?.timeline;
        let attempts = &self.live.attempts;
        let first = attempts.front().map(|sample| sample.index);
        let previous_first = self.charted_first;
        let columns = attempts
            .iter()
            .map(|sample| Column {
                upper: composition([sample.read, sample.write, sample.fresh]),
                lower: vec![
                    Segment::new(sample.reasoning, "hue/0/reasoning"),
                    Segment::new(sample.other, "hue/0/fresh"),
                ],
                reference: Some(WINDOW),
                marker: marker(sample.index),
                muted: self.muted && sample.index % 2 == 1,
            })
            .collect();
        let peak = attempts.iter().map(Sample::input).fold(0.0, f64::max);
        // The window line shows when the context comes near it.
        let upper = if peak >= WINDOW / 2.0 {
            Scale::linear(peak.max(WINDOW))
        } else {
            Scale::nice(peak)
        };
        let lower = Scale::nice(attempts.iter().map(Sample::output).fold(0.0, f64::max));
        let labels = attempts
            .iter()
            .enumerate()
            .filter(|(_, sample)| sample.index.is_multiple_of(LABEL_EVERY))
            .map(|(position, sample)| (position, format!("#{}", sample.index)))
            .collect();
        c.with_widget_mut(timeline, |chart: &mut ColumnChart, _| {
            let selected = previous_first
                .filter(|_| !chart.following())
                .and_then(|previous| chart.cursor().map(|cursor| previous + cursor as u64));
            chart.set_columns(columns, upper, lower);
            chart.set_labels(labels);
            if let (Some(first), Some(selected)) = (first, selected) {
                let position = selected.saturating_sub(first) as usize;
                chart.set_cursor(Some(position));
            }
            Ok(())
        })?;
        self.charted_first = first;
        self.inspect(c)
    }

    /// Describes the attempt under the hover, or else under the cursor, of
    /// the column chart.
    fn inspect(&self, c: &mut dyn Context) -> Result<()> {
        let nodes = self.nodes()?;
        let (cursor, hover, fit) =
            c.with_widget_mut(nodes.timeline, |chart: &mut ColumnChart, _| {
                let group = |column: Option<usize>| column.and_then(|column| chart.group(column));
                Ok((group(chart.cursor()), group(chart.hover()), chart.fit()))
            })?;
        let (what, group) = match (hover, cursor) {
            (Some(hover), _) => ("hover", Some(hover)),
            (None, Some(cursor)) => ("cursor", Some(cursor)),
            (None, None) => ("", None),
        };
        let mode = if fit { "fit" } else { "one a cell" };
        let attempts = &self.live.attempts;
        let named = group.as_ref().and_then(|group| {
            let first = attempts.get(group.start)?;
            let last = attempts.get(group.end - 1)?;
            Some(if group.len() > 1 {
                format!("#{}–#{}", first.index, last.index)
            } else {
                format!("#{}", last.index)
            })
        });
        let text = match group
            .and_then(|group| attempts.get(group.end - 1))
            .zip(named)
        {
            Some((sample, named)) => format!(
                "{mode} · {what} {named} · in {:.1}k = {:.1}k read + {:.1}k written + \
                 {:.1}k fresh · out {:.1}k = {:.1}k reasoning + {:.1}k other · {:.0}% hit",
                sample.input(),
                sample.read,
                sample.write,
                sample.fresh,
                sample.output(),
                sample.reasoning,
                sample.other,
                sample.hit(),
            ),
            None => format!("{mode} · no attempt under the cursor"),
        };
        c.with_widget_mut(nodes.inspector, |inspector: &mut Text, _| {
            inspector.set_text(text);
            Ok(())
        })
    }

    /// Shows the state of the live data in every widget that is not a
    /// sparkline.
    fn show(&self, c: &mut dyn Context) -> Result<()> {
        let nodes = self.nodes()?;
        let state = if self.paused { "paused" } else { "live" };
        let title = format!("chart gym · {} · {state}", THEMES[self.theme].0);
        c.with_widget_mut(nodes.title, |text: &mut Text, _| {
            text.set_text(title);
            Ok(())
        })?;
        let live = &self.live;
        let tokens = live.tokens();
        set_value(c, nodes.tokens.value, compact(tokens))?;
        let shares = SERIES
            .iter()
            .enumerate()
            .map(|(index, series)| {
                Segment::new(series.total(live.step), format!("hue/{index}/fresh"))
            })
            .collect();
        set_meter(c, nodes.tokens.meter, shares, tokens, None)?;
        let parts = live.composition();
        set_value(c, nodes.cache.value, (format!("{:.1}", parts[0]), "%"))?;
        set_meter(c, nodes.cache.meter, composition(parts), 100.0, None)?;
        set_value(c, nodes.output.value, compact(live.output * 1_000.0))?;
        let output = vec![
            Segment::new(live.reasoning, "hue/0/reasoning"),
            Segment::new(live.output - live.reasoning, "hue/0/fresh"),
        ];
        set_meter(c, nodes.output.meter, output, live.output, None)?;
        let use_of_window = live.latest().input() / WINDOW * 100.0;
        set_value(c, nodes.context.value, (format!("{use_of_window:.0}"), "%"))?;
        let heat = vec![Segment::new(use_of_window, "heat")];
        set_meter(c, nodes.context.meter, heat, 100.0, None)?;
        let level = 50.0 + 45.0 * (live.step as f64 * 0.04).sin();
        // The labels share one width, so the bars end together.
        let label = format!("{:>8}", format!("{level:.0}%"));
        set_meter(
            c,
            nodes.level,
            vec![Segment::new(level, "fill")],
            100.0,
            Some(label.clone()),
        )?;
        let hit = format!("{:>8}", format!("{:.0}% hit", parts[0]));
        set_meter(c, nodes.stacked, composition(parts), 100.0, Some(hit))?;
        set_meter(
            c,
            nodes.heat,
            vec![Segment::new(level, "heat")],
            100.0,
            Some(label),
        )?;
        let seconds = live.step / 10;
        let clock = format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        );
        c.with_widget_mut(nodes.clock, |text: &mut BigText, _| {
            text.set_text(clock);
            Ok(())
        })?;
        let scene = Scene {
            step: live.step,
            muted: self.muted,
            attempts: live.attempts.iter().copied().collect(),
        };
        c.with_widget_mut(nodes.primitives, |primitives: &mut Primitives, _| {
            primitives.scene = scene;
            Ok(())
        })
    }

    /// Adds the widgets page below `page` and returns its widgets, apart
    /// from the ones on other pages.
    fn widgets_page(c: &mut dyn Context, page: NodeId) -> Result<WidgetsPage> {
        let row = c.add_child(page, Container::row().with_name("tiles"))?;
        c.set_layout_override(row.into(), fixed_row(TILE_ROWS))?;
        let tokens = tile(c, row.into(), "tokens", Sparkline::bars())?;
        let cache = tile(
            c,
            row.into(),
            "cache hit",
            Sparkline::bars().with_max(100.0),
        )?;
        let output = tile(c, row.into(), "output", Sparkline::line())?;
        let context = tile(c, row.into(), "context", Sparkline::bars())?;
        heading(c, page, "sparklines")?;
        let bars = labeled(c, page, "bars, gaps", Sparkline::bars(), 1)?;
        let tall = labeled(c, page, "bars, 3 rows", Sparkline::bars().with_rows(3), 3)?;
        let line = labeled(c, page, "line, 3 rows", Sparkline::line().with_rows(3), 3)?;
        heading(c, page, "meters")?;
        let level = labeled(c, page, "value", Meter::new(), 1)?;
        let stacked = labeled(c, page, "stacked", Meter::new(), 1)?;
        let heat = labeled(c, page, "gradient", Meter::new(), 1)?;
        Ok(WidgetsPage {
            tiles: [tokens, cache, output, context],
            bars,
            tall,
            line,
            level,
            stacked,
            heat,
        })
    }

    /// Adds the column chart page below `page` and returns its chart and its
    /// inspector line. The chart posts its moves to the gym at `gym`.
    fn columns_page(
        c: &mut dyn Context,
        page: NodeId,
        gym: NodeId,
    ) -> Result<(TypedId<ColumnChart>, TypedId<Text>)> {
        heading(c, page, "column chart: input above the axis, output below")?;
        let moved = Self::spec_timeline_moved()
            .call()
            .with_target(CommandTarget::Exact(gym));
        let chart = ColumnChart::new()
            .with_format(|value| {
                if value <= 0.0 {
                    return "0".to_owned();
                }
                let (number, unit) = compact(value * 1_000.0);
                format!("{number}{}", unit.to_lowercase())
            })
            .with_command(moved);
        let chart = c.add_child(page, chart)?;
        let rows = LayoutOverride::new()
            .flex_horizontal(1)
            .fixed_height(TIMELINE_ROWS);
        c.set_layout_override(chart.into(), rows)?;
        let inspector = c.add_child(page, Text::new(""))?;
        c.set_layout_override(inspector.into(), fixed_row(2))?;
        Ok((chart, inspector))
    }

    /// Adds the text page below `page` and returns its live clock.
    fn text_page(c: &mut dyn Context, page: NodeId) -> Result<TypedId<BigText>> {
        heading(c, page, "every glyph")?;
        big(c, page, BigText::new("ABCDEFGHIJKLM\nNOPQRSTUVWXYZ"))?;
        big(c, page, BigText::new("0123456789 +-×÷=%<>≤≥"))?;
        big(c, page, BigText::new(".,:;!?'\"()[]/\\_#^~|°·—…"))?;
        heading(c, page, "runs in their own styles")?;
        let runs = BigText::new("").with_runs([
            ("text", "4.40"),
            ("unit", "M"),
            ("text", "  85.5"),
            ("unit", "%"),
            ("text", "  131"),
            ("unit", "K"),
        ]);
        big(c, page, runs)?;
        heading(c, page, "a live clock, centered")?;
        big(c, page, BigText::new("00:00:00").with_align(Align::Center))
    }
}

/// The live widgets of the widgets page.
struct WidgetsPage {
    /// The stats tiles: tokens, cache hit, output, and context.
    tiles: [Tile; 4],
    /// A sparkline of bars with gaps.
    bars: TypedId<Sparkline>,
    /// A sparkline of bars three rows high.
    tall: TypedId<Sparkline>,
    /// A sparkline that draws a braille line.
    line: TypedId<Sparkline>,
    /// A meter of one value with a label.
    level: TypedId<Meter>,
    /// A meter of stacked values.
    stacked: TypedId<Meter>,
    /// A meter with a gradient fill.
    heat: TypedId<Meter>,
}

impl Widget for ChartGym {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let node = c.node_id();
        let title = c.add_child(node, Text::new(""))?;
        c.set_layout_override(title.into(), fixed_row(1))?;
        let tabs = c.add_child(node, Tabs::new())?;
        c.set_layout_override(tabs.into(), flex_row(1))?;
        // Each page scrolls, so a short terminal reaches all of it. The page
        // inside takes focus, which puts its scroll on the focus route of the
        // navigation keys.
        let (widgets, text, primitives, columns) =
            c.with_widget_mut(tabs, |tabs: &mut Tabs, ctx| {
                let widgets = tabs.add_tab(ctx, "widgets", Scroll::vertical())?;
                let text = tabs.add_tab(ctx, "text", Scroll::vertical())?;
                let primitives = tabs.add_tab(ctx, "primitives", Scroll::vertical())?;
                let columns = tabs.add_tab(ctx, "columns", Scroll::vertical())?;
                Ok((widgets, text, primitives, columns))
            })?;
        let page = |name: &str| {
            Container::new(
                Layout::column()
                    .flex_horizontal(1)
                    .padding(Edges::new(1, 2, 1, 2))
                    .gap(1),
            )
            .with_name(name)
            .focusable()
        };
        let widgets = c.add_child(widgets, page("widgets"))?;
        let text = c.add_child(text, page("text"))?;
        let primitives = c.add_child(primitives, Primitives::new())?;
        let columns = c.add_child(columns, page("columns"))?;
        let page = Self::widgets_page(c, widgets.into())?;
        let clock = Self::text_page(c, text.into())?;
        let (timeline, inspector) = Self::columns_page(c, columns.into(), node)?;
        let [tokens, cache, output, context] = page.tiles;
        self.nodes = Some(Nodes {
            title,
            tabs,
            tokens,
            cache,
            output,
            context,
            bars: page.bars,
            tall: page.tall,
            line: page.line,
            level: page.level,
            stacked: page.stacked,
            heat: page.heat,
            clock,
            primitives,
            timeline,
            inspector,
        });
        self.fill(c)?;
        self.timeline(c)?;
        self.show(c)?;
        c.set_focus(widgets.into())?;
        Ok(())
    }

    fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        r.push_layer("chart_gym");
        r.fill("", ctx.view().view_rect_local(), ' ')
    }

    fn poll(&mut self, c: &mut dyn Context) -> Result<Option<Duration>> {
        if self.paused {
            return Ok(None);
        }
        let attempt = self.live.advance();
        self.feed(c, attempt)?;
        Ok(Some(STEP))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("chart_gym")
    }
}

impl Register for ChartGym {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        Ok(())
    }
}

/// Returns the values that one step adds to the step sparklines: a token
/// rate, a bar or a gap, and a point of a wave.
fn step_values(step: u64) -> (Option<f64>, Option<f64>, Option<f64>) {
    let growth: f64 = SERIES.iter().map(|series| series.growth).sum();
    let rate = growth * (0.6 + 0.8 * jitter(step.wrapping_add(101)));
    let bar = (!step.is_multiple_of(11)).then(|| 2.0 + 6.0 * jitter(step.wrapping_add(211)));
    let wave = (step as f64 * 0.21).sin() * 4.0 + 5.0 + jitter(step.wrapping_add(307));
    (Some(rate), bar, Some(wave))
}

/// Returns the segments of cache read, cache write, and fresh percents.
fn composition([read, write, fresh]: [f64; 3]) -> Vec<Segment> {
    vec![
        Segment::new(read, "hue/0/read"),
        Segment::new(write, "hue/0/write"),
        Segment::new(fresh, "hue/0/fresh"),
    ]
}

/// Adds a heading below `page`.
fn heading(c: &mut dyn Context, page: NodeId, text: &str) -> Result<()> {
    let heading = c.add_child(page, Text::new(text))?;
    c.set_layout_override(heading.into(), fixed_row(1))
}

/// Adds large text below `page`, as tall as its lines.
fn big(c: &mut dyn Context, page: NodeId, text: BigText) -> Result<TypedId<BigText>> {
    let rows = BigText::size_of(&text.text()).h;
    let node = c.add_child(page, text)?;
    c.set_layout_override(node.into(), fixed_row(rows))?;
    Ok(node)
}

/// Adds a row of a label and `widget`, `rows` high, below `page`, and returns
/// the widget.
fn labeled<W: Widget + 'static>(
    c: &mut dyn Context,
    page: NodeId,
    label: &str,
    widget: W,
    rows: u32,
) -> Result<TypedId<W>> {
    let row = c.add_child(page, Container::row())?;
    c.set_layout_override(row.into(), fixed_row(rows))?;
    let text = c.add_child(row, Text::new(label))?;
    let fixed = LayoutOverride::new()
        .fixed_width(LABEL_WIDTH)
        .fixed_height(rows);
    c.set_layout_override(text.into(), fixed)?;
    let widget = c.add_child(row, widget)?;
    let flex = LayoutOverride::new().flex_horizontal(1).fixed_height(rows);
    c.set_layout_override(widget.into(), flex)?;
    Ok(widget)
}

/// Adds a stats tile titled `title` to `row`: a frame around big text, a
/// meter, and `spark`.
fn tile(c: &mut dyn Context, row: NodeId, title: &str, spark: Sparkline) -> Result<Tile> {
    let frame = c.add_child(
        row,
        Frame::new().with_glyphs(BoxGlyphs::ROUND).with_title(title),
    )?;
    c.set_layout_override(
        frame.into(),
        LayoutOverride::new().flex_horizontal(1).flex_vertical(1),
    )?;
    let column = c.add_child(
        frame,
        Container::new(
            Layout::fill()
                .direction(Direction::Column)
                .padding(Edges::new(0, 1, 0, 1)),
        ),
    )?;
    let value = c.add_child(column, BigText::new(""))?;
    c.set_layout_override(value.into(), fixed_row(3))?;
    let meter = c.add_child(column, Meter::new())?;
    c.set_layout_override(meter.into(), fixed_row(1))?;
    let spark = c.add_child(column, spark)?;
    c.set_layout_override(spark.into(), fixed_row(1))?;
    Ok(Tile {
        value,
        meter,
        spark,
    })
}

/// Adds one value to a sparkline.
fn push(
    c: &mut dyn Context,
    spark: TypedId<Sparkline>,
    value: impl Into<Option<f64>>,
) -> Result<()> {
    let value = value.into();
    c.with_widget_mut(spark, |spark: &mut Sparkline, _| {
        spark.push(value);
        Ok(())
    })
}

/// Replaces the values of a sparkline.
fn replace_values(
    c: &mut dyn Context,
    spark: TypedId<Sparkline>,
    values: Vec<Option<f64>>,
) -> Result<()> {
    c.with_widget_mut(spark, |spark: &mut Sparkline, _| {
        spark.set_values(values);
        Ok(())
    })
}

/// Shows a number and its unit in big text, the unit dimmer.
fn set_value(
    c: &mut dyn Context,
    text: TypedId<BigText>,
    (number, unit): (String, &str),
) -> Result<()> {
    let runs = [("text", number), ("unit", unit.to_owned())];
    c.with_widget_mut(text, |text: &mut BigText, _| {
        text.set_runs(runs);
        Ok(())
    })
}

/// Shows segments on a meter with a top and an optional label.
fn set_meter(
    c: &mut dyn Context,
    meter: TypedId<Meter>,
    segments: Vec<Segment>,
    max: f64,
    label: Option<String>,
) -> Result<()> {
    c.with_widget_mut(meter, |meter: &mut Meter, _| {
        meter.set_max(max);
        meter.set_segments(segments);
        meter.set_label(label);
        Ok(())
    })
}

/// Returns `value` with three significant digits, and its unit.
fn compact(value: f64) -> (String, &'static str) {
    let (scaled, unit) = if value >= 999_500.0 {
        (value / 1_000_000.0, "M")
    } else if value >= 999.5 {
        (value / 1_000.0, "K")
    } else {
        (value, "")
    };
    let number = if scaled >= 99.95 {
        format!("{scaled:.0}")
    } else if scaled >= 9.995 {
        format!("{scaled:.1}")
    } else {
        format!("{scaled:.2}")
    };
    (number, unit)
}

/// The state of the live data that the primitives page paints.
#[derive(Default)]
struct Scene {
    /// Steps so far.
    step: u64,
    /// Whether every other attempt of the column chart is muted.
    muted: bool,
    /// The latest attempts, oldest first.
    attempts: Vec<Sample>,
}

/// The primitives page: every painter of `canopy_widgets::chart`.
struct Primitives {
    /// The state that the page paints.
    scene: Scene,
}

impl Primitives {
    /// Constructs the page before the first state arrives.
    fn new() -> Self {
        Self {
            scene: Scene::default(),
        }
    }

    /// Paint one section heading at `y`, and return the next row.
    fn heading(r: &mut Render<'_>, x: u32, y: u32, width: u32, text: &str) -> Result<u32> {
        r.text("heading", Line::new(x, y, width), text)?;
        Ok(y + 1)
    }

    /// Paint the composition bars and the bars on a shared scale.
    fn bars(&self, r: &mut Render<'_>, x: u32, mut y: u32, width: u32) -> Result<u32> {
        let step = self.scene.step;
        let bar = width
            .saturating_sub(LABEL_WIDTH + 36)
            .clamp(MIN_BAR_WIDTH, BAR_WIDTH);
        let values = x + LABEL_WIDTH + bar + 2;
        let value_line = |y| Line::new(values, y, width.saturating_sub(values - x));
        y = Self::heading(r, x, y, width, "stacked bars: each bar is its own whole")?;
        for (index, series) in SERIES.iter().enumerate() {
            let [read, write, fresh] = series.parts(step, index);
            r.text("label", Line::new(x, y, LABEL_WIDTH), series.label)?;
            let segments = [
                Segment::new(read, format!("hue/{index}/read")),
                Segment::new(write, format!("hue/{index}/write")),
                Segment::new(fresh, format!("hue/{index}/fresh")),
            ];
            let line = Line::new(x + LABEL_WIDTH, y, bar);
            chart::hbar(r, line, &Scale::linear(100.0), &segments, Some("track"))?;
            let text = format!("read {read:.1}% · write {write:.1}% · fresh {fresh:.1}%");
            r.text("value", value_line(y), &text)?;
            y += 1;
        }
        y += 1;
        y = Self::heading(r, x, y, width, "stacked bars: one shared scale")?;
        let top = SERIES
            .iter()
            .map(|series| series.total(step))
            .fold(0.0, f64::max);
        let scale = Scale::nice(top);
        for (index, series) in SERIES.iter().enumerate() {
            let total = series.total(step);
            r.text("label", Line::new(x, y, LABEL_WIDTH), series.label)?;
            let segments = series
                .parts(step, index)
                .into_iter()
                .zip(["read", "write", "fresh"])
                .map(|(part, name)| {
                    Segment::new(total * part / 100.0, format!("hue/{index}/{name}"))
                })
                .collect::<Vec<_>>();
            let line = Line::new(x + LABEL_WIDTH, y, bar);
            chart::hbar(r, line, &scale, &segments, None)?;
            let text = format!("{} of {}", grouped(total), grouped(scale.max()));
            r.text("value", value_line(y), &text)?;
            y += 1;
        }
        Ok(y + 1)
    }

    /// Paint one bar and one column for each eighth of a cell, and a bar that
    /// sweeps up and down one eighth a step.
    fn ramps(&self, r: &mut Render<'_>, x: u32, mut y: u32, width: u32) -> Result<u32> {
        y = Self::heading(r, x, y, width, "eighths: bars, columns, and a sweep")?;
        let scale = Scale::linear(8.0);
        for eighths in 1..=8 {
            let at = x + (eighths - 1) * 2;
            let segments = [Segment::new(f64::from(eighths), "hue/0/fresh")];
            chart::hbar(r, Line::new(at, y, 1), &scale, &segments, Some("track"))?;
            let at = x + 18 + (eighths - 1) * 2;
            let rect = Rect::new(at, y, 1, 1);
            chart::column(r, rect, Base::Bottom, &scale, &segments, Tint::None)?;
        }
        let half = SWEEP_STEPS / 2;
        let phase = self.scene.step % SWEEP_STEPS;
        let rise = if phase < half {
            phase
        } else {
            SWEEP_STEPS - phase
        };
        let sweep = [Segment::new(rise as f64, "hue/1/fresh")];
        let line = Line::new(x + 36, y, SWEEP_CELLS);
        chart::hbar(r, line, &Scale::linear(half as f64), &sweep, Some("track"))?;
        Ok(y + 2)
    }

    /// Paint gradient bars, which swing between empty and full.
    fn gradients(&self, r: &mut Render<'_>, x: u32, mut y: u32, width: u32) -> Result<u32> {
        let bar = width
            .saturating_sub(LABEL_WIDTH + 8)
            .clamp(MIN_BAR_WIDTH, BAR_WIDTH);
        y = Self::heading(r, x, y, width, "gradient: the paint spans the whole bar")?;
        for index in 0..3 {
            let phase = self.scene.step as f64 * 0.04 + f64::from(index) * 2.1;
            let level = 50.0 + 45.0 * phase.sin();
            r.text(
                "label",
                Line::new(x, y, LABEL_WIDTH),
                &format!("{level:.0}%"),
            )?;
            let segments = [Segment::new(level, "heat")];
            let line = Line::new(x + LABEL_WIDTH, y, bar);
            chart::hbar(r, line, &Scale::linear(100.0), &segments, Some("track"))?;
            y += 1;
        }
        Ok(y + 1)
    }

    /// Paint the mirrored column chart of the latest attempts: input above
    /// the axis and output below it, each on its own scale.
    fn columns(&self, r: &mut Render<'_>, x: u32, mut y: u32, width: u32) -> Result<u32> {
        y = Self::heading(r, x, y, width, "columns: input grows up, output grows down")?;
        let attempts = &self.scene.attempts;
        let shown = &attempts[attempts.len() - attempts.len().min(width as usize)..];
        let upper = Scale::nice(shown.iter().map(Sample::input).fold(0.0, f64::max));
        let lower = Scale::nice(shown.iter().map(Sample::output).fold(0.0, f64::max));
        for (column, sample) in (0_u32..).zip(shown) {
            let mute = if self.scene.muted && sample.index % 2 == 1 {
                0.75
            } else {
                0.0
            };
            let input = [
                Segment::new(sample.read, "hue/0/read"),
                Segment::new(sample.write, "hue/0/write"),
                Segment::new(sample.fresh, "hue/0/fresh"),
            ];
            let rect = Rect::new(x + column, y, 1, UPPER_ROWS);
            chart::column(r, rect, Base::Bottom, &upper, &input, Tint::Mute(mute))?;
            let output = [
                Segment::new(sample.reasoning, "hue/0/reasoning"),
                Segment::new(sample.other, "hue/0/fresh"),
            ];
            let rect = Rect::new(x + column, y + UPPER_ROWS + 1, 1, LOWER_ROWS);
            chart::column(r, rect, Base::Top, &lower, &output, Tint::Mute(mute))?;
        }
        let axis = "─".repeat(width as usize);
        r.text("axis", Line::new(x, y + UPPER_ROWS, width), &axis)?;
        Ok(y + UPPER_ROWS + 1 + LOWER_ROWS + 1)
    }

    /// Paint a damped wave that travels to the right, as a braille line.
    fn wave(&self, r: &mut Render<'_>, x: u32, mut y: u32, width: u32) -> Result<u32> {
        y = Self::heading(r, x, y, width, "braille: two by four dots a cell")?;
        let mut canvas = Braille::new(Size::new(width, BRAILLE_ROWS));
        let dots = canvas.dots();
        let middle = f64::from(dots.h - 1) / 2.0;
        let phase = self.scene.step as f64 * 0.12;
        let point = |dot: u32| {
            let t = f64::from(dot) / f64::from(dots.w.max(1));
            let value = (t * TAU * 3.0 - phase).sin() * (-t * 1.5).exp();
            // The wave stays within the canvas, so the row fits.
            let row = (middle - value * middle).round() as u32;
            Point { x: dot, y: row }
        };
        for dot in 1..dots.w {
            canvas.line(point(dot - 1), point(dot));
        }
        canvas.paint(r, Point { x, y }, "line")?;
        Ok(y + BRAILLE_ROWS)
    }
}

impl Widget for Primitives {
    fn layout(&self) -> Layout {
        Layout::column().flex_horizontal(1)
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.clamp(Size::new(MIN_BAR_WIDTH, PRIMITIVES_ROWS))
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, r: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        let area = ctx.view().view_rect_local();
        r.fill("", area, ' ')?;
        let x = area.tl.x + 2;
        let width = area.w.saturating_sub(4);
        if width == 0 || self.scene.attempts.is_empty() {
            return Ok(());
        }
        let y = area.tl.y + 1;
        let y = self.bars(r, x, y, width)?;
        let y = self.ramps(r, x, y, width)?;
        let y = self.gradients(r, x, y, width)?;
        let y = self.columns(r, x, y, width)?;
        self.wave(r, x, y, width)?;
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("primitives")
    }
}

/// Returns `value` as a whole number with its digits in groups of three.
fn grouped(value: f64) -> String {
    let digits = format!("{:.0}", value.max(0.0));
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

/// Returns the marker of attempt `index`: a compaction, a failure, or a
/// retry, at steady intervals.
fn marker(index: u64) -> Option<Marker> {
    if index > 0 && index.is_multiple_of(COMPACTION_EVERY) {
        Some(Marker::new('◆', "marker/compaction").with_rank(2))
    } else if index % 29 == 11 {
        Some(Marker::new('✕', "marker/failed").with_rank(3))
    } else if index % 29 == 12 {
        Some(Marker::new('↻', "marker/retry").with_rank(1))
    } else {
        None
    }
}

/// Returns a steady pseudo-random number from 0 to 1 for `seed`.
fn jitter(seed: u64) -> f64 {
    (seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 40 & 0x3ff) as f64 / 1023.0
}

/// Build the gym styles from a palette: each hue has a bright fresh tint, a
/// middle write tint, and a dim read tint, mixed toward the background.
fn styles(p: &Palette, rules: StyleRules<'_>) {
    let tint = |color: Color, t: f32| color.mix(p.bg, t, Mix::Oklab);
    let heat = GradientSpec::with_stops(
        0.0,
        vec![
            GradientStop::new(0.0, p.green),
            GradientStop::new(0.6, p.yellow),
            GradientStop::new(1.0, p.red),
        ],
    );
    let mut rules = rules
        .prefix("chart_gym")
        .fg("", p.fg)
        .bg("", p.bg)
        .fg("heading", p.fg)
        .attr("heading", Attr::Bold)
        .fg("label", p.muted_fg)
        .fg("value", p.muted_fg)
        .fg("unit", p.muted_fg)
        .fg("axis", p.frame)
        .fg("track", tint(p.faint_fg, 0.7))
        .fg("line", p.cyan)
        .fg("heat", Paint::gradient(heat))
        .fg("marker/compaction", p.violet)
        .fg("marker/failed", p.red)
        .fg("marker/retry", p.yellow);
    for (index, hue) in [p.violet, p.cyan, p.orange].into_iter().enumerate() {
        rules = rules
            .fg(&format!("hue/{index}/fresh"), hue)
            .fg(&format!("hue/{index}/write"), tint(hue, 0.35))
            .fg(&format!("hue/{index}/reasoning"), tint(hue, 0.4))
            .fg(&format!("hue/{index}/read"), tint(hue, 0.62));
    }
    rules.apply();
}

/// Bindings of the chart gym.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()
canopy.keymap({
    path = "chart_gym",
    { key = "Tab", description = "Next page", action = command.chart_gym.next_page() },
    { key = "p", description = "Pause or resume", action = command.chart_gym.toggle_pause() },
    { key = "t", description = "Next theme", action = command.chart_gym.next_theme() },
    { key = "m", description = "Mute every other attempt", action = command.chart_gym.toggle_mute() },
    { key = { "h", "Left" }, description = "Previous attempt", action = command.chart_gym.timeline_by(-1) },
    { key = { "l", "Right" }, description = "Next attempt", action = command.chart_gym.timeline_by(1) },
    { key = "H", description = "Previous label", action = command.chart_gym.timeline_label(-1) },
    { key = "L", description = "Next label", action = command.chart_gym.timeline_label(1) },
    { key = "[", description = "First attempt", action = command.chart_gym.timeline_first() },
    { key = "]", description = "Newest attempt", action = command.chart_gym.timeline_newest() },
    { key = "z", description = "Fit the attempts or show one a cell", action = command.chart_gym.toggle_fit() },
    { key = { "j", "Down" }, description = "Scroll down", action = "canopy.nav.down" },
    { key = { "k", "Up" }, description = "Scroll up", action = "canopy.nav.up" },
    { key = { "PageDown", "Space" }, description = "Page down", action = "canopy.nav.page_down" },
    { key = "PageUp", description = "Page up", action = "canopy.nav.page_up" },
    { key = { "g", "Home" }, description = "Top", action = "canopy.nav.first" },
    { key = { "G", "End" }, description = "Bottom", action = "canopy.nav.last" },
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
        .script("chartgym", DEFAULT_BINDINGS)
}
