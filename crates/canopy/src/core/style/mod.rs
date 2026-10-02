pub mod animation;
/// Color helpers.
mod color;
/// Style effects system.
pub mod effects;
pub mod themes;

use std::{
    collections::HashMap,
    f64::consts::TAU,
    iter,
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) use animation::MotionClocks;
pub use animation::{Animation, AnimationStart, Easing, Pause, Repeat};
pub use color::{Color, Mix, hex_byte};

use crate::geom;

/// Shared part names that widgets paint beneath their own layer.
///
/// A widget pushes its node name as a layer and paints these bare roles, so
/// `button/text` and `input/text` are both the `TEXT` part.
pub mod roles {
    /// Ordinary text.
    pub const TEXT: &str = "text";
    /// The ground a widget fills before painting its parts.
    pub const BACKGROUND: &str = "background";
    /// Box chrome drawn around a widget.
    pub const BORDER: &str = "border";
    /// A key name, such as an accelerator or a binding hint.
    pub const KEY: &str = "key";
    /// A fixed prompt before editable text.
    pub const PROMPT: &str = "prompt";
    /// A title in a widget's chrome.
    pub const TITLE: &str = "title";
    /// A scrollbar thumb.
    pub const THUMB: &str = "thumb";

    /// Paint a retained selection according to which control takes the keys.
    /// Pass actual focus, or the composite widget's active part. Inactive
    /// selections remain visible without claiming keyboard focus.
    pub const fn selection(active: bool) -> &'static str {
        if active {
            "selection"
        } else {
            "selection/dimmed"
        }
    }
}

/// A style replacement requested during a turn and applied before the next
/// render.
#[derive(Clone, Debug)]
pub(crate) enum StyleChange {
    /// Switch to a theme, reapplying every widget style rule set.
    Theme(themes::Palette),
    /// Replace the whole style map.
    Map(StyleMap),
}

/// Style rules a widget crate or application derives from the palette.
pub(crate) type WidgetStyles = Box<dyn Fn(&themes::Palette, StyleRules<'_>)>;

/// Independent widget states mapped onto the existing style layer stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetState {
    /// The widget or its descendant holds focus.
    Focused,
    /// The widget is selected independently of focus.
    Selected,
    /// The widget's command is disabled.
    Disabled,
    /// The widget is pressed or explicitly active.
    Pressed,
}

impl WidgetState {
    /// Return the existing string layer for this state.
    pub const fn layer(self) -> &'static str {
        match self {
            Self::Focused => "focused",
            Self::Selected => "selected",
            Self::Disabled => "disabled",
            Self::Pressed => "active",
        }
    }
}

/// A text attribute.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Attr {
    /// Bold text.
    Bold,
    /// Crossed out text.
    CrossedOut,
    /// Dim text.
    Dim,
    /// Italic text.
    Italic,
    /// Overlined text.
    Overline,
    /// Underlined text.
    Underline,
}

/// A set of active text attributes.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct AttrSet {
    /// Bold flag.
    pub bold: bool,
    /// Crossed out flag.
    pub crossedout: bool,
    /// Dim flag.
    pub dim: bool,
    /// Italic flag.
    pub italic: bool,
    /// Overline flag.
    pub overline: bool,
    /// Underline flag.
    pub underline: bool,
}

impl Default for AttrSet {
    /// Construct an empty set of text attributes.
    fn default() -> Self {
        Self {
            bold: false,
            crossedout: false,
            dim: false,
            italic: false,
            overline: false,
            underline: false,
        }
    }
}

impl AttrSet {
    /// Construct a set of text attributes with a single attribute turned on.
    pub fn new(attr: Attr) -> Self {
        Self::default().with(attr)
    }
    /// A helper for progressive construction of attribute sets.
    #[must_use]
    pub fn with(mut self, attr: Attr) -> Self {
        match attr {
            Attr::Bold => self.bold = true,
            Attr::Dim => self.dim = true,
            Attr::Italic => self.italic = true,
            Attr::CrossedOut => self.crossedout = true,
            Attr::Underline => self.underline = true,
            Attr::Overline => self.overline = true,
        };
        self
    }

    /// Return the lowercase name of each attribute, as scripts, snapshots,
    /// and captures name it, with whether it is on.
    pub(crate) fn named(self) -> [(&'static str, bool); 6] {
        [
            ("bold", self.bold),
            ("crossedout", self.crossedout),
            ("dim", self.dim),
            ("italic", self.italic),
            ("overline", self.overline),
            ("underline", self.underline),
        ]
    }

    /// Turn on the attribute that `name` names, and return whether it names
    /// one.
    pub(crate) fn set_named(&mut self, name: &str) -> bool {
        let attr = match name {
            "bold" => Attr::Bold,
            "crossedout" => Attr::CrossedOut,
            "dim" => Attr::Dim,
            "italic" => Attr::Italic,
            "overline" => Attr::Overline,
            "underline" => Attr::Underline,
            _ => return false,
        };
        *self = self.with(attr);
        true
    }
}

/// A gradient stop in a paint specification.
#[derive(Debug, Clone, PartialEq)]
pub struct GradientStop {
    /// Offset along the gradient (0.0-1.0).
    pub offset: f32,
    /// Color at this stop.
    pub color: Color,
}

impl GradientStop {
    /// Construct a gradient stop, clamping the offset to 0.0-1.0.
    pub fn new(offset: f32, color: Color) -> Self {
        Self {
            offset: offset.clamp(0.0, 1.0),
            color,
        }
    }
}

/// How a gradient moves across its rectangle. Every drift keeps step with the
/// motion epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Drift {
    /// Slide across the rectangle once in each period, wrapping around.
    Slide(Duration),
    /// Swing to and fro: in each period the middle of the gradient travels
    /// to one edge of the rectangle, to the other edge, and back. The swing
    /// slows at each edge, like a pendulum, and a gradient whose ends match
    /// shows a crest that runs back and forth.
    Sweep(Duration),
}

impl Drift {
    /// Return the period of the drift.
    fn period(self) -> Duration {
        match self {
            Self::Slide(period) | Self::Sweep(period) => period,
        }
    }

    /// Return the gradient position that shows at `ratio` after `phase` of
    /// a period, both 0.0 to 1.0.
    fn shift(self, ratio: f64, phase: f64) -> f64 {
        match self {
            Self::Slide(_) => (ratio - phase).rem_euclid(1.0),
            // The offset starts at rest, swings half the rectangle to the
            // right, then half to the left, and returns.
            Self::Sweep(_) => ratio - (phase * TAU).sin() / 2.0,
        }
    }
}

/// A gradient paint specification.
#[derive(Debug, Clone, PartialEq)]
pub struct GradientSpec {
    /// Gradient angle in degrees (0 = left to right, 90 = top to bottom).
    pub angle_deg: f32,
    /// Ordered list of gradient stops.
    pub stops: Vec<GradientStop>,
    /// The space that mixes colors between stops.
    pub mix: Mix,
    /// How the gradient moves across its rectangle. `None` holds it still.
    pub drift: Option<Drift>,
    /// When a drifting gradient holds at rest.
    pub pause: Pause,
}

impl GradientSpec {
    /// Construct a gradient from explicit stops.
    pub fn with_stops(angle_deg: f32, mut stops: Vec<GradientStop>) -> Self {
        if stops.is_empty() {
            stops.push(GradientStop::new(0.0, Color::White));
            stops.push(GradientStop::new(1.0, Color::White));
        }
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        Self {
            angle_deg,
            stops,
            mix: Mix::Oklab,
            drift: None,
            pause: Pause::Idle,
        }
    }

    /// Replace the mixing space.
    #[must_use]
    pub fn with_mix(mut self, mix: Mix) -> Self {
        self.mix = mix;
        self
    }

    /// Move the gradient across its rectangle.
    #[must_use]
    pub fn with_drift(mut self, drift: Drift) -> Self {
        self.drift = Some(drift);
        self
    }

    /// Replace the pause.
    #[must_use]
    pub fn with_pause(mut self, pause: Pause) -> Self {
        self.pause = pause;
        self
    }

    /// Map all colors in this gradient through a transform.
    #[must_use]
    pub fn map_colors(&self, f: impl Fn(Color) -> Color) -> Self {
        let stops = self
            .stops
            .iter()
            .map(|stop| GradientStop::new(stop.offset, f(stop.color)))
            .collect();
        Self {
            stops,
            ..self.clone()
        }
    }

    /// Resolve a gradient color at a point within a rectangle, at rest.
    pub fn color_at(&self, rect: geom::Rect, point: geom::Point) -> Color {
        self.color_for_ratio(self.ratio_at(rect, point))
    }

    /// Resolve a gradient color at a point at the clocks' time, drifted by
    /// the time since the motion epoch.
    pub(crate) fn color_in_motion(
        &self,
        rect: geom::Rect,
        point: geom::Point,
        clocks: &MotionClocks,
    ) -> Color {
        let ratio = self.ratio_at(rect, point);
        let Some(drift) = self.moving_drift().filter(|_| !self.pause.holds(clocks)) else {
            return self.color_for_ratio(ratio);
        };
        let period = drift.period().as_nanos();
        let elapsed = clocks
            .now
            .saturating_duration_since(clocks.epoch)
            .as_nanos();
        let phase = (elapsed % period) as f64 / period as f64;
        self.color_for_ratio(drift.shift(f64::from(ratio), phase) as f32)
    }

    /// Return the drift, unless the gradient holds still: it has none, or
    /// its period is zero.
    fn moving_drift(&self) -> Option<Drift> {
        self.drift.filter(|drift| !drift.period().is_zero())
    }

    /// Return the position of a point along the gradient, 0.0 to 1.0.
    fn ratio_at(&self, rect: geom::Rect, point: geom::Point) -> f32 {
        if rect.w == 0 || rect.h == 0 {
            return 0.0;
        }

        let width = rect.w as f32;
        let height = rect.h as f32;
        let angle = self.angle_deg.to_radians();
        let dir_x = angle.cos();
        let dir_y = angle.sin();
        let corners = [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)];
        let (min_dot, max_dot) = corners.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(min_dot, max_dot), (x, y)| {
                let dot = dir_x * x + dir_y * y;
                (min_dot.min(dot), max_dot.max(dot))
            },
        );

        let local_x = point.x.saturating_sub(rect.tl.x) as f32 + 0.5;
        let local_y = point.y.saturating_sub(rect.tl.y) as f32 + 0.5;
        let dot = dir_x * local_x + dir_y * local_y;
        if (max_dot - min_dot).abs() < f32::EPSILON {
            0.0
        } else {
            ((dot - min_dot) / (max_dot - min_dot)).clamp(0.0, 1.0)
        }
    }

    /// Blend between gradient stops for a normalized ratio.
    fn color_for_ratio(&self, ratio: f32) -> Color {
        if self.stops.len() == 1 {
            return self.stops[0].color;
        }
        let ratio = ratio.clamp(0.0, 1.0);
        let mut prev = &self.stops[0];
        for stop in &self.stops[1..] {
            if ratio <= stop.offset {
                let span = (stop.offset - prev.offset).max(f32::EPSILON);
                let local = (ratio - prev.offset) / span;
                return prev.color.mix(stop.color, local, self.mix);
            }
            prev = stop;
        }
        self.stops.last().expect("gradient stops exist").color
    }
}

/// A paint definition for a style channel.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    /// Solid color fill.
    Solid(Color),
    /// Gradient fill.
    Gradient(GradientSpec),
    /// A color that changes over time.
    Animated(Arc<Animation>),
}

impl Paint {
    /// Construct a solid paint.
    pub fn solid(color: Color) -> Self {
        Self::Solid(color)
    }

    /// Construct a gradient paint.
    pub fn gradient(spec: GradientSpec) -> Self {
        Self::Gradient(spec)
    }

    /// Construct an animated paint.
    pub fn animated(animation: Animation) -> Self {
        Self::Animated(Arc::new(animation))
    }

    /// Return the solid color if this paint is solid.
    pub fn solid_color(&self) -> Option<Color> {
        match self {
            Self::Solid(color) => Some(*color),
            Self::Gradient(_) | Self::Animated(_) => None,
        }
    }

    /// Resolve the paint at a location, at rest.
    pub fn resolve(&self, rect: geom::Rect, point: geom::Point) -> Color {
        match self {
            Self::Solid(color) => *color,
            Self::Gradient(spec) => spec.color_at(rect, point),
            Self::Animated(animation) => animation.rest(),
        }
    }

    /// Resolve the paint at a location at the clocks' time.
    pub(crate) fn resolve_in_motion(
        &self,
        rect: geom::Rect,
        point: geom::Point,
        clocks: &MotionClocks,
    ) -> Color {
        match self {
            Self::Solid(color) => *color,
            Self::Gradient(spec) => spec.color_in_motion(rect, point, clocks),
            Self::Animated(animation) => animation.color(clocks),
        }
    }

    /// Return whether the paint changes over time.
    pub fn moves(&self) -> bool {
        match self {
            Self::Solid(_) => false,
            Self::Gradient(spec) => spec.moving_drift().is_some(),
            Self::Animated(_) => true,
        }
    }

    /// Bind the start of an animation that starts when shown, and return
    /// whether the paint still moves at `clocks.now`.
    pub(crate) fn show(&self, clocks: &MotionClocks) -> bool {
        match self {
            Self::Solid(_) => false,
            Self::Gradient(spec) => spec.moving_drift().is_some(),
            Self::Animated(animation) => {
                animation.bind_shown(clocks.now);
                !animation.finished(clocks)
            }
        }
    }

    /// Return the next time the paint can change after the clocks' time.
    pub(crate) fn next_change(&self, clocks: &MotionClocks, sample: Duration) -> Option<Instant> {
        match self {
            Self::Solid(_) => None,
            Self::Gradient(spec) => spec
                .moving_drift()
                .filter(|_| !spec.pause.holds(clocks))
                .map(|_| clocks.now + sample),
            Self::Animated(animation) => animation.next_change(clocks, sample),
        }
    }

    /// Map colors within this paint.
    #[must_use]
    pub fn map_colors(&self, f: impl Fn(Color) -> Color) -> Self {
        match self {
            Self::Solid(color) => Self::Solid(f(*color)),
            Self::Gradient(spec) => Self::Gradient(spec.map_colors(f)),
            Self::Animated(animation) => Self::Animated(Arc::new(animation.map_colors(f))),
        }
    }
}

impl From<Color> for Paint {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

impl From<GradientSpec> for Paint {
    fn from(spec: GradientSpec) -> Self {
        Self::Gradient(spec)
    }
}

impl From<Animation> for Paint {
    fn from(animation: Animation) -> Self {
        Self::animated(animation)
    }
}

/// A resolved style specification stored in terminal buffers.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ResolvedStyle {
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Text attributes.
    pub attrs: AttrSet,
}

impl ResolvedStyle {
    /// Construct a resolved style from components.
    pub fn new(fg: Color, bg: Color, attrs: AttrSet) -> Self {
        Self { fg, bg, attrs }
    }
}

/// How much of a cell a glyph covers, for antialiased drawing.
///
/// A covered cell mixes its style's background toward its foreground: by
/// `fg` for the glyph and by `bg` for the cell ground, each 0 to 255.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Coverage {
    /// Glyph coverage.
    pub fg: u8,
    /// Ground coverage.
    pub bg: u8,
}

impl Coverage {
    /// Apply the coverage to a resolved style.
    pub fn apply(self, style: ResolvedStyle) -> ResolvedStyle {
        let weight = |coverage: u8| f32::from(coverage) / 255.0;
        ResolvedStyle::new(
            style.bg.mix(style.fg, weight(self.fg), Mix::Rgb),
            style.bg.mix(style.fg, weight(self.bg), Mix::Rgb),
            style.attrs,
        )
    }
}

/// A paint-based style specification.
#[derive(Debug, PartialEq, Clone)]
pub struct Style {
    /// Foreground paint.
    pub fg: Paint,
    /// Background paint.
    pub bg: Paint,
    /// Text attributes.
    pub attrs: AttrSet,
}

impl Style {
    /// Resolve the style at a location within a rectangle.
    pub fn resolve_at(&self, rect: geom::Rect, point: geom::Point) -> ResolvedStyle {
        ResolvedStyle::new(
            self.fg.resolve(rect, point),
            self.bg.resolve(rect, point),
            self.attrs,
        )
    }

    /// Return whether either paint changes over time.
    pub fn moves(&self) -> bool {
        self.fg.moves() || self.bg.moves()
    }

    /// Resolve the style to a solid variant if both paints are solid.
    pub fn resolve_solid(&self) -> Option<ResolvedStyle> {
        Some(ResolvedStyle::new(
            self.fg.solid_color()?,
            self.bg.solid_color()?,
            self.attrs,
        ))
    }
}

/// A possibly partial style specification, which is stored in a StyleManager.
/// Partial styles are completely resolved during the style resolution process.
#[derive(Default, Debug, PartialEq, Clone)]
pub struct PartialStyle {
    /// Optional foreground paint.
    pub fg: Option<Paint>,
    /// Optional background paint.
    pub bg: Option<Paint>,
    /// Optional attributes.
    pub attrs: Option<AttrSet>,
}

impl PartialStyle {
    /// Create an empty partial style, which inherits every component.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the foreground paint.
    #[must_use]
    pub fn fg(mut self, paint: impl Into<Paint>) -> Self {
        self.fg = Some(paint.into());
        self
    }

    /// Set the background paint.
    #[must_use]
    pub fn bg(mut self, paint: impl Into<Paint>) -> Self {
        self.bg = Some(paint.into());
        self
    }

    /// Add one attribute to the attributes set so far.
    #[must_use]
    pub fn attr(mut self, attr: Attr) -> Self {
        self.attrs = Some(self.attrs.unwrap_or_default().with(attr));
        self
    }

    /// Set every attribute at once; an empty set stops attributes inheriting.
    #[must_use]
    pub fn attrs(mut self, attrs: AttrSet) -> Self {
        self.attrs = Some(attrs);
        self
    }

    /// Resolve the partial style into a full style.
    fn resolve(&self) -> Style {
        Style {
            fg: self.fg.clone().expect("foreground paint is set"),
            bg: self.bg.clone().expect("background paint is set"),
            attrs: self.attrs.expect("attributes are set"),
        }
    }

    /// Merge two partial styles. Components set on `self` win.
    fn join(&self, other: &Self) -> Self {
        Self {
            fg: self.fg.clone().or_else(|| other.fg.clone()),
            bg: self.bg.clone().or_else(|| other.bg.clone()),
            attrs: self.attrs.or(other.attrs),
        }
    }

    /// Return true if all components are set.
    fn is_complete(&self) -> bool {
        self.fg.is_some() && self.bg.is_some() && self.attrs.is_some()
    }
}

/// Split a style path into its non-empty components.
fn path_segments(path: &str) -> impl Iterator<Item = &str> {
    path.split('/').filter(|part| !part.is_empty())
}

/// Return the canonical map key for a style path: non-empty components joined
/// by `/`.
fn canonical_path(path: &str) -> String {
    let mut key = String::with_capacity(path.len());
    for part in path_segments(path) {
        if !key.is_empty() {
            key.push('/');
        }
        key.push_str(part);
    }
    key
}

/// Map of style paths to partial styles, keyed by canonical path.
#[derive(Clone, Debug)]
pub struct StyleMap {
    /// Path-to-style map.
    styles: HashMap<String, PartialStyle>,
}

impl StyleMap {
    /// Resolve the style at `path`, as a widget with no pushed layers
    /// would see it.
    pub fn resolve(&self, path: &str) -> Style {
        StyleManager::default().get(self, path)
    }

    /// Construct a style map with defaults.
    pub fn new() -> Self {
        let mut cs = Self {
            styles: HashMap::new(),
        };
        cs.insert_style(
            "/",
            PartialStyle {
                fg: Some(Paint::Solid(Color::White)),
                bg: Some(Paint::Solid(Color::Black)),
                attrs: Some(AttrSet::default()),
            },
        );
        cs
    }

    /// Begin a fluent rule-building chain.
    ///
    /// # Example
    ///
    /// ```
    /// use canopy::style::{StyleMap, solarized};
    ///
    /// let mut style_map = StyleMap::new();
    /// style_map
    ///     .rules()
    ///     .fg("red/text", Color::Red)
    ///     .fg("blue/text", Color::Blue)
    ///     .apply();
    /// ```
    pub fn rules(&mut self) -> StyleRules<'_> {
        StyleRules {
            map: self,
            prefix: None,
            pending: Vec::new(),
        }
    }

    /// Iterate over every rule as its canonical path and partial style.
    ///
    /// Paths omit the leading `/`, so the root rule's path is empty. The order
    /// is unspecified.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &PartialStyle)> {
        self.styles
            .iter()
            .map(|(path, style)| (path.as_str(), style))
    }

    /// Insert a partial style at a path.
    fn insert_style(&mut self, path: &str, style: PartialStyle) {
        self.styles
            .entry(canonical_path(path))
            .and_modify(|existing| *existing = style.join(existing))
            .or_insert(style);
    }
}

impl Default for StyleMap {
    fn default() -> Self {
        Self::new()
    }
}

/// A fluent builder for adding style rules to a StyleMap.
///
/// Created via [`StyleMap::rules()`]. Collects path/style pairs and commits
/// them on [`.apply()`](StyleRules::apply).
#[must_use = "call .apply() to commit rules"]
pub struct StyleRules<'a> {
    /// The target style map.
    map: &'a mut StyleMap,
    /// Optional path prefix for subsequent rules.
    prefix: Option<String>,
    /// Accumulated rules to be committed.
    pending: Vec<(String, PartialStyle)>,
}

impl<'a> StyleRules<'a> {
    /// Set the foreground paint for a path.
    ///
    /// If a rule already exists for this path, the foreground paint is merged
    /// with the existing style.
    pub fn fg(mut self, path: &str, paint: impl Into<Paint>) -> Self {
        let full_path = self.make_path(path);
        self.merge_pending(full_path, PartialStyle::new().fg(paint));
        self
    }

    /// Set the background paint for a path.
    ///
    /// If a rule already exists for this path, the background paint is merged
    /// with the existing style.
    pub fn bg(mut self, path: &str, paint: impl Into<Paint>) -> Self {
        let full_path = self.make_path(path);
        self.merge_pending(full_path, PartialStyle::new().bg(paint));
        self
    }

    /// Add a single attribute for a path.
    ///
    /// If a rule already exists for this path, the attribute is merged
    /// with the existing style.
    pub fn attr(mut self, path: &str, attr: Attr) -> Self {
        let full_path = self.make_path(path);
        self.merge_pending(full_path, PartialStyle::new().attrs(AttrSet::new(attr)));
        self
    }

    /// Apply a complete style to a path.
    ///
    /// If a rule already exists for this path, the style is merged
    /// with the existing style (new values take precedence).
    pub fn style(mut self, path: &str, style: impl Into<PartialStyle>) -> Self {
        let full_path = self.make_path(path);
        self.merge_pending(full_path, style.into());
        self
    }

    /// Apply a complete style to multiple paths.
    ///
    /// If a rule already exists for any path, the style is merged
    /// with the existing style (new values take precedence).
    pub fn style_all(mut self, paths: &[&str], style: impl Into<PartialStyle>) -> Self {
        let partial = style.into();
        for path in paths {
            let full_path = self.make_path(path);
            self.merge_pending(full_path, partial.clone());
        }
        self
    }

    /// Merge a style into the pending rules.
    ///
    /// If a rule with the same path exists, merge the new style into it.
    /// Otherwise, add a new pending rule.
    fn merge_pending(&mut self, path: String, style: PartialStyle) {
        if let Some((_, existing)) = self.pending.iter_mut().find(|(p, _)| p == &path) {
            *existing = style.join(existing);
        } else {
            self.pending.push((path, style));
        }
    }

    /// Set a path prefix for all subsequent rules.
    ///
    /// Can be called multiple times; each call replaces the previous prefix.
    pub fn prefix(mut self, prefix: &str) -> Self {
        self.prefix = Some(prefix.to_string());
        self
    }

    /// Clear the current prefix.
    pub fn no_prefix(mut self) -> Self {
        self.prefix = None;
        self
    }

    /// Commit all pending rules to the StyleMap.
    pub fn apply(self) {
        for (path, style) in self.pending {
            self.map.insert_style(&path, style);
        }
    }

    /// Combine the current prefix with a path suffix.
    fn make_path(&self, path: &str) -> String {
        match &self.prefix {
            Some(prefix) if !prefix.is_empty() && !path.is_empty() => {
                format!("{}/{}", prefix, path)
            }
            Some(prefix) if !prefix.is_empty() => prefix.clone(),
            _ => path.to_string(),
        }
    }
}

/// A hierarchical style manager.
///
/// `Style` objects are entered into the manager with '/'-separated paths. For
/// example:
///
///   / white, black
///   /frame -> grey, None
///   /frame/selected -> blue, None
///
/// The first entry with the empty path is the global default. Every
/// `StyleMap` is guaranteed to have a default Style object with non-None
/// foreground and background colors, so style resolution always succeeds.
///
/// `Style` objects also contain text attributes.
///
/// During rendering, a node may push a name onto the stack of layers tracked by
/// the `Style` object. Layers are maintained for a node and all its
/// descendants, and `Canopy` manages popping layers back off the stack at the
/// appropriate time during rendering.
///
/// When a colour is resolved, each prefix of the path, longest first, is
/// looked up under the whole layer stack, then under the stack with its outer
/// layers dropped one at a time, then with its inner layers dropped, and
/// finally under no layers.
///
/// So given a layer stack ["foo", "bar"], and an attempt to look up "text",
/// we try the following lookups in order: ["foo/bar/text", "bar/text",
/// "foo/text", "text", "foo/bar", "bar", "foo", ""].
#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct StyleManager {
    /// Current render level.
    level: usize,
    /// Active layer names.
    layers: Vec<String>,
    /// Render levels corresponding to layers.
    layer_levels: Vec<usize>,
}

impl Default for StyleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleManager {
    /// Construct a style manager in the reset state.
    pub fn new() -> Self {
        Self {
            level: 0,
            layers: vec![],
            layer_levels: vec![0],
        }
    }

    /// Increment the render level.
    pub fn push(&mut self) {
        self.level += 1
    }

    /// Decrement the render level and pop any layers at this level.
    pub fn pop(&mut self) {
        if self.level != 0 {
            while self.layer_levels.last() == Some(&self.level) {
                self.layers.pop();
                self.layer_levels.pop();
            }
            self.level -= 1;
        }
    }

    /// Push onto the layer stack with the current render level.
    pub fn push_layer(&mut self, name: &str) {
        self.layers.push(name.to_owned());
        self.layer_levels.push(self.level);
    }

    /// Resolve a style path.
    pub fn get(&self, smap: &StyleMap, path: &str) -> Style {
        let path: Vec<&str> = path_segments(path).collect();
        self.resolve(smap, &self.layers, &path)
    }

    /// Resolve a style using a path and a layer specification, ignoring
    /// `self.layers`.
    ///
    /// Probes path prefixes from longest to shortest. Within each, it probes
    /// the whole layer stack, then the stack with outer layers dropped, then
    /// the stack with inner layers dropped, and finally no layers. So a
    /// component's own rules apply wherever it is mounted, and a context rule
    /// such as `dialog/button/border` still beats `button/border`. The first
    /// probe that sets a component wins.
    fn resolve(&self, smap: &StyleMap, layers: &[String], path: &[&str]) -> Style {
        let n = layers.len();
        let inner = (1..n).map(|start| start..n);
        let outer = (1..n).rev().map(|end| 0..end);
        let windows: Vec<_> = iter::once(0..n)
            .chain(inner)
            .chain(outer)
            .chain((n > 0).then_some(0..0))
            .collect();
        let mut ret = PartialStyle::default();
        let mut key = String::new();
        for suffix in (0..=path.len()).rev() {
            for window in &windows {
                key.clear();
                let parts = layers[window.clone()]
                    .iter()
                    .map(String::as_str)
                    .chain(path[..suffix].iter().copied());
                for part in parts {
                    if !key.is_empty() {
                        key.push('/');
                    }
                    key.push_str(part);
                }
                if let Some(c) = smap.styles.get(key.as_str()) {
                    ret = ret.join(c);
                    if ret.is_complete() {
                        return ret.resolve();
                    }
                }
            }
        }
        ret.resolve()
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::*;

    #[test]
    fn stock_roles_preserve_fallback_across_decorative_levels() {
        let mut map = StyleMap::new();
        map.rules()
            .fg("button/active/text", Color::Red)
            .fg("button/active/border", Color::Blue)
            .fg("input/text", Color::Green)
            .apply();
        let mut manager = StyleManager::new();
        manager.push_layer("button");
        manager.push_layer(WidgetState::Pressed.layer());
        let label = manager.get(&map, roles::TEXT);
        let border = manager.get(&map, roles::BORDER);
        manager.push();
        manager.push();
        assert_eq!(manager.get(&map, roles::TEXT), label);
        assert_eq!(manager.get(&map, roles::BORDER), border);
        let mut input = StyleManager::new();
        input.push_layer("input");
        input.push_layer(WidgetState::Focused.layer());
    }

    #[test]
    fn disabled_selected_and_focused_layers_remain_distinct() {
        let mut map = StyleMap::new();
        map.rules()
            .fg("button/text", Color::White)
            .fg("button/focused/text", Color::Blue)
            .fg("button/focused/selected/text", Color::Green)
            .fg("button/focused/selected/disabled/text", Color::Red)
            .apply();
        let mut manager = StyleManager::new();
        manager.push_layer("button");
        let ordinary = manager.get(&map, roles::TEXT);
        manager.push_layer(WidgetState::Focused.layer());
        let focused = manager.get(&map, roles::TEXT);
        manager.push_layer(WidgetState::Selected.layer());
        let selected = manager.get(&map, roles::TEXT);
        manager.push_layer(WidgetState::Disabled.layer());
        let disabled = manager.get(&map, roles::TEXT);
        assert_ne!(ordinary, focused);
        assert_ne!(focused, selected);
        assert_ne!(selected, disabled);
    }

    #[test]
    fn partial_style_chains_a_reusable_rule() {
        let selected = PartialStyle::new()
            .fg(Color::White)
            .bg(Color::Blue)
            .attr(Attr::Bold);

        let mut style_map = StyleMap::new();
        style_map.rules().style("item/selected", selected).apply();

        let manager = StyleManager::new();
        let resolved = manager.get(&style_map, "item/selected");
        assert_eq!(resolved.fg, Paint::solid(Color::White));
        assert_eq!(resolved.bg, Paint::solid(Color::Blue));
        assert_eq!(resolved.attrs, AttrSet::new(Attr::Bold));
    }

    #[test]
    fn rules_chain_sets_one_path_per_call() {
        let mut style_map = StyleMap::new();
        style_map
            .rules()
            .style(
                "",
                PartialStyle::new()
                    .fg(Color::White)
                    .bg(Color::Black)
                    .attrs(AttrSet::default()),
            )
            .fg("red/text", Color::Red)
            .fg("blue/text", Color::Blue)
            .apply();

        let manager = StyleManager::new();
        assert_eq!(
            manager.get(&style_map, "red/text"),
            solid_style(Color::Red, Color::Black)
        );
        assert_eq!(
            manager.get(&style_map, "blue/text"),
            solid_style(Color::Blue, Color::Black)
        );
    }

    /// Render a theme's complete rule set as sorted `path fg bg attrs` lines.
    fn dump_theme(name: &str, map: &StyleMap) -> String {
        let mut lines: Vec<String> = map
            .styles
            .iter()
            .map(|(path, style)| {
                format!(
                    "/{path} fg={:?} bg={:?} attrs={:?}",
                    style.fg, style.bg, style.attrs
                )
            })
            .collect();
        lines.sort();
        format!("# {name}\n{}\n", lines.join("\n"))
    }

    #[test]
    fn the_focus_ground_leans_from_the_background_toward_the_accent() {
        for palette in [
            themes::default_dark(),
            themes::solarized_dark(),
            themes::solarized_light(),
            themes::dracula(),
            themes::gruvbox_dark(),
        ] {
            let focus = palette.focus_bg();
            assert_ne!(focus, palette.bg, "the focus ground stands out");
            assert_ne!(focus, palette.accent, "the focus ground stays a ground");
            assert_eq!(focus, palette.bg.mix(palette.accent, 0.13, Mix::Rgb));
        }
    }

    /// In every built-in theme, a button's key keeps one color in every state
    /// and reads on each face as well as the theme's text reads on its panel,
    /// up to WCAG AA. So does the label on the faces of focus and a press;
    /// on the resting face it keeps the contrast the theme gives it.
    #[test]
    fn button_labels_and_keys_read_on_every_face() {
        for (name, palette) in [
            ("default_dark", themes::default_dark()),
            ("solarized_dark", themes::solarized_dark()),
            ("solarized_light", themes::solarized_light()),
            ("dracula", themes::dracula()),
            ("gruvbox_dark", themes::gruvbox_dark()),
        ] {
            let map = palette.style_map();
            let target = palette.fg.contrast_ratio(palette.panel_bg).min(4.5);
            let colors = |path: &str| {
                let style = map.resolve(path);
                (
                    style.fg.solid_color().expect("solid fg"),
                    style.bg.solid_color().expect("solid bg"),
                    style.attrs,
                )
            };
            let (rest_key, ..) = colors("/button/face/key");
            for state in ["", "focused/", "active/"] {
                let (text, face, _) = colors(&format!("/button/{state}face/text"));
                let (key, key_face, attrs) = colors(&format!("/button/{state}face/key"));
                assert_eq!(face, key_face, "{name} {state}: one face");
                assert_eq!(key, rest_key, "{name} {state}: one key color");
                assert!(attrs.bold && !attrs.underline, "{name} {state}: key marks");
                let parts = if state.is_empty() {
                    vec![("key", key)]
                } else {
                    vec![("text", text), ("key", key)]
                };
                for (part, color) in parts {
                    let ratio = color.contrast_ratio(face);
                    assert!(
                        ratio + 0.005 >= target,
                        "{name} {state}{part}: {ratio:.2} under {target:.2}"
                    );
                }
            }
        }
    }

    /// Render every built-in theme in a stable order.
    fn dump_all_themes() -> String {
        [
            ("default_dark", themes::default_dark().style_map()),
            ("solarized_dark", themes::solarized_dark().style_map()),
            ("solarized_light", themes::solarized_light().style_map()),
            ("dracula", themes::dracula().style_map()),
            ("gruvbox_dark", themes::gruvbox_dark().style_map()),
        ]
        .iter()
        .map(|(name, map)| dump_theme(name, map))
        .collect()
    }

    /// Compare every built-in theme with its capture.
    ///
    /// Set `UPDATE_GOLDEN` to rewrite the capture from the themes instead of
    /// editing it, so a palette change and its capture cannot disagree.
    #[test]
    fn built_in_themes_match_the_golden_rule_sets() {
        let dumped = dump_all_themes();
        if env::var_os("UPDATE_GOLDEN").is_some() {
            let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/core/style/themes.golden");
            fs::write(path, &dumped).expect("theme capture is writable");
        }
        assert_eq!(dumped, include_str!("themes.golden"));
    }

    fn solid_style(fg: Color, bg: Color) -> Style {
        Style {
            fg: Paint::solid(fg),
            bg: Paint::solid(bg),
            attrs: AttrSet::default(),
        }
    }

    #[test]
    fn style_canonical_path() {
        assert_eq!(canonical_path("/one/two"), "one/two");
        assert_eq!(canonical_path("one/two"), "one/two");
        assert_eq!(canonical_path("//one///two/"), "one/two");
        assert!(canonical_path("").is_empty());
        assert!(canonical_path("/").is_empty());
    }

    #[test]
    fn style_resolve() {
        let mut smap = StyleMap::new();
        smap.rules()
            .style(
                "",
                PartialStyle::new()
                    .fg(Color::White)
                    .bg(Color::Black)
                    .attrs(AttrSet::default()),
            )
            .fg("one", Color::Red)
            .fg("one/two", Color::Blue)
            .fg("one/two/target", Color::Green)
            .fg("frame/border", Color::Yellow)
            .apply();

        let c = StyleManager::new();

        assert_eq!(
            c.resolve(
                &smap,
                &["one".to_string(), "two".to_string()],
                &["target", "voing"]
            ),
            solid_style(Color::Green, Color::Black)
        );

        assert_eq!(
            c.resolve(
                &smap,
                &["one".to_string(), "two".to_string()],
                &["two", "voing"]
            ),
            solid_style(Color::Blue, Color::Black)
        );

        assert_eq!(
            c.resolve(&smap, &["one".to_string(), "two".to_string()], &["target"]),
            solid_style(Color::Green, Color::Black)
        );
        assert_eq!(
            c.resolve(
                &smap,
                &["one".to_string(), "two".to_string()],
                &["nonexistent"]
            ),
            solid_style(Color::Blue, Color::Black)
        );
        assert_eq!(
            c.resolve(&smap, &["somelayer".to_string()], &["nonexistent"]),
            solid_style(Color::White, Color::Black)
        );
        assert_eq!(
            c.resolve(
                &smap,
                &["one".to_string(), "two".to_string()],
                &["frame", "border"]
            ),
            solid_style(Color::Yellow, Color::Black)
        );
        assert_eq!(
            c.resolve(&smap, &["frame".to_string()], &["border"]),
            solid_style(Color::Yellow, Color::Black)
        );
    }
    #[test]
    fn style_layers_basic() {
        let mut c = StyleManager::new();
        assert!(c.layers.is_empty());
        assert_eq!(c.layer_levels, vec![0]);
        assert_eq!(c.level, 0);

        // A nop at this level
        c.pop();
        assert_eq!(c.level, 0);

        c.push();
        c.push_layer("foo");
        assert_eq!(c.level, 1);
        assert_eq!(c.layers, vec!["foo"]);
        assert_eq!(c.layer_levels, vec![0, 1]);
    }

    #[test]
    fn component_rules_apply_under_any_outer_layer() {
        let mut map = StyleMap::new();
        map.rules()
            .fg("editor/text", Color::Red)
            .fg("host/text", Color::Blue)
            .fg("host/selection", Color::Green)
            .fg("dialog/button/border", Color::Yellow)
            .fg("button/border", Color::Magenta)
            .apply();
        let mut manager = StyleManager::new();
        manager.push_layer("host");
        manager.push_layer("editor");
        assert_eq!(
            manager.get(&map, "text").fg,
            Paint::Solid(Color::Red),
            "the component's rule beats the host's"
        );
        assert_eq!(
            manager.get(&map, "selection").fg,
            Paint::Solid(Color::Green),
            "the host's rule fills what the component leaves unset"
        );
        let mut dialog = StyleManager::new();
        dialog.push_layer("dialog");
        dialog.push_layer("button");
        assert_eq!(
            dialog.get(&map, "border").fg,
            Paint::Solid(Color::Yellow),
            "a context rule beats the component's"
        );
    }

    #[test]
    fn style_layers_nested() {
        let mut c = StyleManager::new();
        c.push();
        c.push_layer("foo");

        c.push();
        c.push();
        c.push_layer("bar");
        assert_eq!(c.level, 3);
        assert_eq!(c.layers, vec!["foo", "bar"]);
        assert_eq!(c.layer_levels, vec![0, 1, 3]);

        c.push();
        assert_eq!(c.level, 4);

        c.pop();
        assert_eq!(c.level, 3);
        assert_eq!(c.layers, vec!["foo", "bar"]);
        assert_eq!(c.layer_levels, vec![0, 1, 3]);

        c.pop();
        assert_eq!(c.level, 2);
        assert_eq!(c.layers, vec!["foo"]);
        assert_eq!(c.layer_levels, vec![0, 1]);

        c.pop();
        assert_eq!(c.level, 1);
        assert_eq!(c.layers, vec!["foo"]);
        assert_eq!(c.layer_levels, vec![0, 1]);

        c.pop();
        assert_eq!(c.level, 0);
        assert!(c.layers.is_empty());
        assert_eq!(c.layer_levels, vec![0]);

        c.pop();
        assert_eq!(c.level, 0);
        assert!(c.layers.is_empty());
        assert_eq!(c.layer_levels, vec![0]);
    }

    #[test]
    fn style_rules_merge_same_path() {
        let mut smap = StyleMap::new();

        // Setting fg then bg on the same path should merge them
        smap.rules()
            .fg("test/path", Color::Red)
            .bg("test/path", Color::Blue)
            .apply();

        let c = StyleManager::new();
        let resolved = c.resolve(&smap, &[], &["test", "path"]);

        assert_eq!(resolved.fg.solid_color(), Some(Color::Red));
        assert_eq!(resolved.bg.solid_color(), Some(Color::Blue));
    }

    #[test]
    fn pop_pops_all_layers_at_level() {
        let mut sm = StyleManager::default();
        sm.push();

        sm.push_layer("button");
        sm.push_layer("selected");

        sm.pop();

        assert!(sm.layers.is_empty());
        assert_eq!(sm.layer_levels, vec![0]);
    }

    #[test]
    fn style_rules_later_overrides_earlier() {
        let mut smap = StyleMap::new();

        // Later fg call should override earlier fg call
        smap.rules()
            .fg("test", Color::Red)
            .fg("test", Color::Green)
            .apply();

        let c = StyleManager::new();
        let resolved = c.resolve(&smap, &[], &["test"]);

        assert_eq!(resolved.fg.solid_color(), Some(Color::Green));
    }

    #[test]
    fn stylemap_default_is_complete() {
        let smap = StyleMap::default();
        let c = StyleManager::new();
        let resolved = c.get(&smap, "");
        assert_eq!(resolved.fg.solid_color(), Some(Color::White));
        assert_eq!(resolved.bg.solid_color(), Some(Color::Black));
        assert_eq!(resolved.attrs, AttrSet::default());
    }

    #[test]
    fn gradient_resolves_left_to_right() {
        let start = Color::Rgb { r: 0, g: 0, b: 0 };
        let end = Color::Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        let spec = GradientSpec::with_stops(
            0.0,
            vec![GradientStop::new(0.0, start), GradientStop::new(1.0, end)],
        );
        let rect = geom::Rect::new(0, 0, 10, 1);

        let left = spec.color_at(rect, geom::Point { x: 0, y: 0 });
        let right = spec.color_at(rect, geom::Point { x: 9, y: 0 });

        assert_eq!(left, start.mix(end, 0.05, Mix::Oklab));
        assert_eq!(right, start.mix(end, 0.95, Mix::Oklab));
    }

    #[test]
    fn gradient_resolves_top_to_bottom() {
        let start = Color::Rgb { r: 0, g: 0, b: 0 };
        let end = Color::Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        let spec = GradientSpec::with_stops(
            90.0,
            vec![GradientStop::new(0.0, start), GradientStop::new(1.0, end)],
        );
        let rect = geom::Rect::new(0, 0, 1, 10);

        let top = spec.color_at(rect, geom::Point { x: 0, y: 0 });
        let bottom = spec.color_at(rect, geom::Point { x: 0, y: 9 });

        assert_eq!(top, start.mix(end, 0.05, Mix::Oklab));
        assert_eq!(bottom, start.mix(end, 0.95, Mix::Oklab));
    }

    #[test]
    fn gradient_interpolates_multiple_stops() {
        let red = Color::Rgb { r: 255, g: 0, b: 0 };
        let green = Color::Rgb { r: 0, g: 255, b: 0 };
        let blue = Color::Rgb { r: 0, g: 0, b: 255 };
        let spec = GradientSpec::with_stops(
            0.0,
            vec![
                GradientStop::new(1.0, blue),
                GradientStop::new(0.0, red),
                GradientStop::new(0.5, green),
            ],
        );
        let rect = geom::Rect::new(0, 0, 10, 1);
        let point = geom::Point { x: 4, y: 0 };

        let width = rect.w as f32;
        let angle = spec.angle_deg.to_radians();
        let dir_x = angle.cos();
        let dir_y = angle.sin();
        let corners = [(0.0, 0.0), (width, 0.0), (0.0, 1.0), (width, 1.0)];
        let (min_dot, max_dot) = corners.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(min_dot, max_dot), (x, y)| {
                let dot = dir_x * x + dir_y * y;
                (min_dot.min(dot), max_dot.max(dot))
            },
        );
        let local_x = point.x as f32 + 0.5;
        let dot = dir_x * local_x + dir_y * 0.5;
        let ratio = ((dot - min_dot) / (max_dot - min_dot)).clamp(0.0, 1.0);
        let expected = if ratio <= 0.5 {
            red.mix(green, ratio / 0.5, Mix::Oklab)
        } else {
            green.mix(blue, (ratio - 0.5) / 0.5, Mix::Oklab)
        };

        assert_eq!(spec.color_at(rect, point), expected);
    }

    #[test]
    fn a_drifting_gradient_slides_and_wraps() {
        use std::time::{Duration, Instant};
        let spec = GradientSpec::with_stops(
            0.0,
            vec![
                GradientStop::new(0.0, Color::Black),
                GradientStop::new(1.0, Color::White),
            ],
        )
        .with_drift(Drift::Slide(Duration::from_secs(1)));
        let rect = geom::Rect::new(0, 0, 10, 1);
        let point = geom::Point { x: 7, y: 0 };
        let t0 = Instant::now();
        let at = |ms: u64| MotionClocks {
            now: t0 + Duration::from_millis(ms),
            ..MotionClocks::at(t0)
        };
        assert_eq!(
            spec.color_in_motion(rect, point, &at(0)),
            spec.color_at(rect, point)
        );
        let shifted = geom::Point { x: 2, y: 0 };
        assert_eq!(
            spec.color_in_motion(rect, point, &at(500)),
            spec.color_at(rect, shifted)
        );
        assert_eq!(
            spec.color_in_motion(rect, point, &at(1000)),
            spec.color_at(rect, point)
        );
        let paused = MotionClocks {
            paused: true,
            ..at(500)
        };
        assert_eq!(
            spec.color_in_motion(rect, point, &paused),
            spec.color_at(rect, point)
        );
    }

    #[test]
    fn a_drift_of_no_period_holds_still() {
        use std::time::{Duration, Instant};
        let paint = Paint::gradient(
            GradientSpec::with_stops(0.0, Vec::new())
                .with_drift(Drift::Sweep(Duration::ZERO))
                .with_pause(Pause::Never),
        );
        assert!(!paint.moves());
        let clocks = MotionClocks::at(Instant::now());
        assert!(!paint.show(&clocks));
        assert_eq!(paint.next_change(&clocks, Duration::from_millis(33)), None);
    }

    #[test]
    fn a_sweeping_gradient_swings_to_and_fro() {
        use std::time::{Duration, Instant};
        let spec = GradientSpec::with_stops(
            0.0,
            vec![
                GradientStop::new(0.0, Color::Black),
                GradientStop::new(1.0, Color::White),
            ],
        )
        .with_drift(Drift::Sweep(Duration::from_secs(1)))
        .with_pause(Pause::Never);
        let rect = geom::Rect::new(0, 0, 10, 1);
        let left = geom::Point { x: 2, y: 0 };
        let right = geom::Point { x: 7, y: 0 };
        let t0 = Instant::now();
        let at = |ms: u64| MotionClocks {
            now: t0 + Duration::from_millis(ms),
            ..MotionClocks::at(t0)
        };
        assert_eq!(
            spec.color_in_motion(rect, right, &at(0)),
            spec.color_at(rect, right)
        );
        // A quarter period swings the gradient half the rectangle right, and
        // three quarters swing it half the rectangle left.
        assert_eq!(
            spec.color_in_motion(rect, right, &at(250)),
            spec.color_at(rect, left)
        );
        assert_eq!(
            spec.color_in_motion(rect, left, &at(750)),
            spec.color_at(rect, right)
        );
        assert_eq!(
            spec.color_in_motion(rect, right, &at(500)),
            spec.color_at(rect, right)
        );
        // A busy gradient swings on while the operator is idle.
        let paused = MotionClocks {
            paused: true,
            ..at(250)
        };
        assert_eq!(
            spec.color_in_motion(rect, right, &paused),
            spec.color_at(rect, left)
        );
        assert!(
            Paint::gradient(spec)
                .next_change(&paused, Duration::from_millis(33))
                .is_some()
        );
    }

    #[test]
    fn style_resolves_gradient_paints() {
        let fg_spec = GradientSpec::with_stops(
            0.0,
            vec![
                GradientStop::new(0.0, Color::White),
                GradientStop::new(1.0, Color::Black),
            ],
        );
        let bg_spec = GradientSpec::with_stops(
            90.0,
            vec![
                GradientStop::new(0.0, Color::Red),
                GradientStop::new(1.0, Color::Blue),
            ],
        );
        let style = Style {
            fg: Paint::gradient(fg_spec.clone()),
            bg: Paint::gradient(bg_spec.clone()),
            attrs: AttrSet::default(),
        };
        let rect = geom::Rect::new(0, 0, 4, 4);
        let point = geom::Point { x: 1, y: 2 };
        let resolved = style.resolve_at(rect, point);

        assert_eq!(resolved.fg, fg_spec.color_at(rect, point));
        assert_eq!(resolved.bg, bg_spec.color_at(rect, point));
    }
    #[test]
    fn committed_style_rules_merge_canonical_components() {
        let mut map = StyleMap::new();
        map.rules().fg("/", Color::Red).apply();
        let manager = StyleManager::new();
        let root = manager.get(&map, "").resolve_solid().unwrap();
        assert_eq!(root.fg, Color::Red);
        assert_eq!(root.bg, Color::Black);
        assert_eq!(root.attrs, AttrSet::default());

        map.rules()
            .fg("/item", Color::Blue)
            .attr("/item", Attr::Bold)
            .apply();
        map.rules().bg("item/", Color::Green).apply();
        let item = manager.get(&map, "item").resolve_solid().unwrap();
        assert_eq!(item.fg, Color::Blue);
        assert_eq!(item.bg, Color::Green);
        assert_eq!(item.attrs, AttrSet::new(Attr::Bold));

        map.rules()
            .fg("//item/", Color::Yellow)
            .bg("item", Color::Red)
            .style("/item//", PartialStyle::new().attrs(AttrSet::default()))
            .apply();
        let item = manager.get(&map, "item").resolve_solid().unwrap();
        assert_eq!(item.fg, Color::Yellow);
        assert_eq!(item.bg, Color::Red);
        assert_eq!(item.attrs, AttrSet::default());
    }
}
