//! Large text in bitmap faces, drawn with half blocks or octants at integer
//! scales.

mod face;
#[rustfmt::skip]
mod faces;
mod layout;
#[cfg(test)]
mod tests;

use std::{
    cmp::Ordering,
    time::{Duration, Instant},
};

use canopy::{
    Context, NodeName, ViewContext, Widget,
    error::Result,
    geom::{Point, Rect, Size},
    layout::{Align, Constraint, Layout, MeasureConstraints, Measurement},
    render::Render,
    runtime::{NodeWakeHandle, PollLifetime},
    style::{Style, roles},
};
use canopy_mosaic::symbols::octant;
pub use face::{BigFace, BigWeight};
pub use layout::{BigGrid, BigSize};
use layout::{Column, Resolved, resolve};

use crate::mosaic::{Tier, tier_for};

/// Time that a changed character takes to roll.
const ROLL: Duration = Duration::from_millis(200);

/// Time between two frames of a roll.
const ROLL_FRAME: Duration = Duration::from_millis(33);

/// What a rendering looks like: text rolls only between two renderings that
/// share one geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Geometry {
    /// The face.
    face: BigFace,
    /// The weight.
    weight: BigWeight,
    /// The scale.
    scale: u32,
    /// The grid.
    grid: BigGrid,
    /// Pixel rows of the line box above the baseline.
    above: u32,
    /// Pixel rows of the line box below the baseline.
    below: u32,
}

impl Geometry {
    /// Returns the geometry of resolved text.
    fn of(resolved: &Resolved) -> Self {
        Self {
            face: resolved.laid.face,
            weight: resolved.laid.weight,
            scale: resolved.scale,
            grid: resolved.grid,
            above: resolved.laid.above,
            below: resolved.laid.below,
        }
    }

    /// Returns the pixel rows of one line box.
    fn box_height(self) -> u32 {
        (self.above + self.below).max(1)
    }
}

/// The columns of each line at the last render, and their geometry.
#[derive(Debug, Clone)]
struct Shown {
    /// The geometry.
    geometry: Geometry,
    /// The pixel columns of each line.
    lines: Vec<Vec<Column>>,
}

/// A roll from the columns on screen to the columns of new text.
#[derive(Debug, Clone)]
struct Roll {
    /// What was on screen when the text changed.
    from: Shown,
    /// Start of the roll: the first render after the change.
    started: Option<Instant>,
}

/// Large text in bitmap faces, drawn with half blocks or octants.
///
/// The faces form a ladder: [`BigFace::Compact`], with Canopy's own numerals
/// five pixels high, then the seven sizes of Tamzen. Each draws at any
/// integer scale, where one font pixel is that many columns by that many half
/// rows. With the full mosaic glyphs, each face also draws on the octant
/// grid, where one font pixel is that many half columns by that many quarter
/// rows, and the ladder gains a step between each two half-block scales.
/// The glyphs come from the terminal, as the application set them, unless
/// the text has a tier of its own, set with [`BigText::with_tier`].
/// [`BigSize::Fit`], the default, takes the face and scale that draw
/// the largest capitals in the area; [`BigSize::MaxRows`] caps the rows, and
/// [`BigSize::Exact`] takes one face at one scale. The fit measures the text
/// itself: a line box runs from the cap height down to the baseline, and
/// grows for the ascenders and descenders that the text has. A strut, set
/// with [`BigText::with_strut`], grows the line box for more glyphs, so texts
/// that share a strut share a baseline. When nothing fits, the text takes the
/// smallest rendering and is cut at the right and bottom edges.
/// [`BigWeight::Bold`] is the default weight.
///
/// A newline starts a new line, one blank row below the last. Each line
/// aligns on its own with [`BigText::with_align`], and the block of lines
/// aligns with [`BigText::with_vertical_align`], in half rows.
///
/// The text is a list of runs, each with the style path that paints it, so
/// one value can show a number and a dimmer unit. `BigText` pushes the
/// `big_text` layer, and a plain run paints the `text` part. The `text` part
/// also paints the rest of the area. Gradients span the whole block of text.
///
/// When the text changes and [`ViewContext::motion_active`] is true, the
/// changed characters roll over 200 ms: the old glyph moves up out of its
/// cells, and the new glyph moves in from below. Text rolls only when the old
/// and the new text share a face, weight, scale, and line box; any other
/// change shows at once. The position of the roll comes from
/// [`ViewContext::now`], so a slow repaint skips frames and never slows the
/// roll. A change during a roll starts a new roll from the glyphs on screen.
/// Old and new text align on the right, so a number that gains a digit rolls
/// it in on the left. A hidden widget, or motion that turns off, ends the
/// roll at rest.
pub struct BigText {
    /// Runs of text: the style path that paints each run, and its text.
    runs: Vec<(String, String)>,
    /// How the text chooses its size.
    size: BigSize,
    /// The weight.
    weight: BigWeight,
    /// Horizontal placement of each line in the area.
    align: Align,
    /// Vertical placement of the block of lines in the area.
    vertical: Align,
    /// Characters whose glyphs the line box holds.
    strut: String,
    /// The text's own mosaic tier, or none to follow the terminal's glyphs.
    tier: Option<Tier>,
    /// What the last render showed.
    shown: Option<Shown>,
    /// Roll in progress.
    roll: Option<Roll>,
    /// Handle that starts polling when the text changes.
    wake: Option<NodeWakeHandle>,
}

impl BigText {
    /// Constructs big text of one run in the `text` part. It fits its area,
    /// in bold, at the start of the area.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            runs: vec![(roles::TEXT.to_owned(), text.into())],
            size: BigSize::default(),
            weight: BigWeight::default(),
            align: Align::Start,
            vertical: Align::Start,
            strut: String::new(),
            tier: None,
            shown: None,
            roll: None,
            wake: None,
        }
    }

    /// Replaces the text with runs, each painted by its own style path.
    #[must_use]
    pub fn with_runs<S, T>(mut self, runs: impl IntoIterator<Item = (S, T)>) -> Self
    where
        S: Into<String>,
        T: Into<String>,
    {
        self.set_runs(runs);
        self
    }

    /// Sets how the text chooses its size.
    #[must_use]
    pub fn with_size(mut self, size: BigSize) -> Self {
        self.size = size;
        self
    }

    /// Sets the weight.
    #[must_use]
    pub fn with_weight(mut self, weight: BigWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Places each line at the start, the center, or the end of the area.
    #[must_use]
    pub fn with_align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Places the block of lines at the top, the middle, or the bottom of the
    /// area. A middle that falls between two half rows takes the upper one.
    #[must_use]
    pub fn with_vertical_align(mut self, align: Align) -> Self {
        self.vertical = align;
        self
    }

    /// Sets a strut: characters whose glyphs the line box holds, as if the
    /// text had them. The strut draws nothing and takes no width. Texts
    /// that share a strut and a rendering share a line box, so they stand on
    /// one baseline, and a change between them can roll.
    #[must_use]
    pub fn with_strut(mut self, strut: impl Into<String>) -> Self {
        self.strut = strut.into();
        self
    }

    /// Draws with the symbols of `tier`, whatever glyphs the terminal has.
    /// The full tier adds the octant grid to the ladder.
    #[must_use]
    pub fn with_tier(mut self, tier: Tier) -> Self {
        self.tier = Some(tier);
        self
    }

    /// Draws with the symbols of `tier`, or with none, the symbols that the
    /// terminal's glyphs allow.
    pub fn set_tier(&mut self, tier: Option<Tier>) {
        self.tier = tier;
    }

    /// Returns the text's own mosaic tier, if it has one.
    pub fn tier(&self) -> Option<Tier> {
        self.tier
    }

    /// Sets how the text chooses its size.
    pub fn set_size(&mut self, size: BigSize) {
        self.size = size;
    }

    /// Sets the weight.
    pub fn set_weight(&mut self, weight: BigWeight) {
        self.weight = weight;
    }

    /// Places each line at the start, the center, or the end of the area.
    pub fn set_align(&mut self, align: Align) {
        self.align = align;
    }

    /// Places the block of lines at the top, the middle, or the bottom of the
    /// area.
    pub fn set_vertical_align(&mut self, align: Align) {
        self.vertical = align;
    }

    /// Sets a strut: characters whose glyphs the line box holds.
    pub fn set_strut(&mut self, strut: impl Into<String>) {
        self.strut = strut.into();
    }

    /// Replaces the text with one run in the `text` part.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.set_runs([(roles::TEXT, text.into())]);
    }

    /// Replaces the text with runs, each painted by its own style path.
    pub fn set_runs<S, T>(&mut self, runs: impl IntoIterator<Item = (S, T)>)
    where
        S: Into<String>,
        T: Into<String>,
    {
        let before = self.text();
        self.runs = runs
            .into_iter()
            .map(|(style, text)| (style.into(), text.into()))
            .collect();
        // Text that never showed has nothing to roll from.
        if self.text() != before
            && let Some(shown) = &self.shown
        {
            self.roll = Some(Roll {
                from: shown.clone(),
                started: None,
            });
            if let Some(wake) = &self.wake {
                // An expired handle means that the widget left the tree, and
                // its next mount takes a new one.
                let _outcome = wake.wake();
            }
        }
    }

    /// Returns the text of all runs.
    pub fn text(&self) -> String {
        self.runs.iter().map(|(_, text)| text.as_str()).collect()
    }

    /// The tier that the text draws with when it does not know the
    /// terminal's glyphs: its own, or the default tier.
    fn own_tier(&self) -> Tier {
        self.tier.unwrap_or_default()
    }

    /// Returns the size in cells that the text takes in an area of `area`,
    /// in its own tier.
    pub fn size_in(&self, area: Size) -> Size {
        self.resolve(Some(area.w), Some(area.h), self.own_tier())
            .size()
    }

    /// Returns the face and the scale on its grid that draw the text in an
    /// area of `area`, in its own tier.
    pub fn face_in(&self, area: Size) -> (BigFace, u32) {
        let resolved = self.resolve(Some(area.w), Some(area.h), self.own_tier());
        (resolved.laid.face, resolved.scale)
    }

    /// Returns the grid that draws the text in an area of `area`, in its own
    /// tier.
    pub fn grid_in(&self, area: Size) -> BigGrid {
        self.resolve(Some(area.w), Some(area.h), self.own_tier())
            .grid
    }

    /// Returns the cell rows that draw the text in an area of `area`, placed
    /// at the start of the area, without trailing spaces, in its own tier.
    pub fn rows_in(&self, area: Size) -> Vec<String> {
        let resolved = self.resolve(Some(area.w), Some(area.h), self.own_tier());
        let lines = Self::columns(&resolved);
        let size = resolved.size();
        let placement = Placement::new(&resolved, &lines, size, Align::Start, Align::Start);
        (0..size.h)
            .map(|y| {
                let row: String = (0..size.w)
                    .map(|x| placement.cell(&lines, x, y).map_or(' ', |(ch, _)| ch))
                    .collect();
                row.trim_end().to_owned()
            })
            .collect()
    }

    /// Returns whether a roll is in progress.
    #[cfg(test)]
    fn rolling(&self) -> bool {
        self.roll.is_some()
    }

    /// Returns the lines of the runs, each a list of characters with the
    /// index of their run.
    fn lines(&self) -> Vec<Vec<(usize, char)>> {
        let mut lines = vec![Vec::new()];
        for (index, (_, text)) in self.runs.iter().enumerate() {
            for ch in text.chars() {
                if ch == '\n' {
                    lines.push(Vec::new());
                } else if let Some(line) = lines.last_mut() {
                    line.push((index, ch));
                }
            }
        }
        lines
    }

    /// Choose the face, scale, and grid for an area of at most `width`
    /// columns and `height` rows, in `tier`. `None` is unbounded. Only the
    /// full tier has the octants of the octant grid.
    fn resolve(&self, width: Option<u32>, height: Option<u32>, tier: Tier) -> Resolved {
        let grids: &[BigGrid] = if tier == Tier::Full {
            &[BigGrid::Half, BigGrid::Octant]
        } else {
            &[BigGrid::Half]
        };
        resolve(
            &self.lines(),
            &self.strut,
            self.weight,
            self.size,
            (width, height),
            grids,
        )
    }

    /// Returns the pixel columns of each line of resolved text.
    fn columns(resolved: &Resolved) -> Vec<Vec<Column>> {
        resolved
            .laid
            .lines
            .iter()
            .map(|line| resolved.laid.columns(line))
            .collect()
    }

    /// Returns the columns to draw at `now`, and ends a roll that finished,
    /// that motion no longer shows, or whose geometry changed.
    fn frame(&mut self, now: Instant, motion: bool, resolved: &Resolved) -> Vec<Vec<Column>> {
        let target = Self::columns(resolved);
        let Some(roll) = &mut self.roll else {
            return target;
        };
        let geometry = Geometry::of(resolved);
        let started = *roll.started.get_or_insert(now);
        let elapsed = now.saturating_duration_since(started);
        if !motion || elapsed >= ROLL || roll.from.geometry != geometry {
            self.roll = None;
            return target;
        }
        let progress = elapsed.as_secs_f32() / ROLL.as_secs_f32();
        target
            .iter()
            .enumerate()
            .map(|(index, line)| {
                roll_line(
                    roll.from.lines.get(index).map_or(&[][..], Vec::as_slice),
                    line,
                    progress,
                    geometry.box_height(),
                )
            })
            .collect()
    }
}

/// Where resolved text falls in an area: each line's first grid column, and
/// the grid row where the block starts.
struct Placement {
    /// Grid pixels of one font pixel, on each axis.
    scale: u32,
    /// The grid.
    grid: BigGrid,
    /// Grid rows of one line box.
    line_rows: u32,
    /// Grid rows from the top of one line to the top of the next.
    pitch: u32,
    /// Grid row where the block of lines starts.
    top: u32,
    /// First grid column of each line.
    lefts: Vec<u32>,
}

impl Placement {
    /// Place resolved text in an area of `area`.
    fn new(
        resolved: &Resolved,
        lines: &[Vec<Column>],
        area: Size,
        align: Align,
        vertical: Align,
    ) -> Self {
        let (scale, grid) = (resolved.scale, resolved.grid);
        let block = resolved.laid.grid_rows(scale, grid);
        let room = area.h.saturating_mul(grid.down());
        let top = match vertical {
            Align::Start => 0,
            Align::Center => room.saturating_sub(block) / 2,
            Align::End => room.saturating_sub(block),
        };
        let columns = area.w.saturating_mul(grid.across());
        let lefts = lines
            .iter()
            .map(|line| {
                let width = u32::try_from(line.len())
                    .unwrap_or(u32::MAX)
                    .saturating_mul(scale);
                match align {
                    Align::Start => 0,
                    Align::Center => columns.saturating_sub(width) / 2,
                    Align::End => columns.saturating_sub(width),
                }
            })
            .collect();
        Self {
            scale,
            grid,
            line_rows: resolved.laid.line_rows(scale),
            pitch: resolved.laid.pitch(scale, grid),
            top,
            lefts,
        }
    }

    /// Returns the line and the pixel row of grid row `row` of the area, or
    /// `None` for a row outside every line box.
    fn pixel_row(&self, row: u32) -> Option<(usize, u32)> {
        let from_top = row.checked_sub(self.top)?;
        let within = from_top % self.pitch;
        (within < self.line_rows).then(|| ((from_top / self.pitch) as usize, within / self.scale))
    }

    /// Returns the run of the font pixel at grid column `x`, row `y`, and
    /// whether it has ink, or `None` outside the text.
    fn pixel(&self, lines: &[Vec<Column>], x: u32, y: u32) -> Option<(usize, bool)> {
        let (line, row) = self.pixel_row(y)?;
        let column = x.checked_sub(*self.lefts.get(line)?)? / self.scale;
        let column = lines.get(line)?.get(column as usize)?;
        Some((column.run, column.ink(row)))
    }

    /// Returns the character at cell `x`, `y` of the area and the run that
    /// paints it, or `None` for a cell without ink.
    fn cell(&self, lines: &[Vec<Column>], x: u32, y: u32) -> Option<(char, usize)> {
        match self.grid {
            BigGrid::Half => {
                // At least one blank row separates two lines, so a cell
                // never holds pixels of two lines.
                let upper = self.pixel(lines, x, y.saturating_mul(2));
                let lower = self.pixel(lines, x, y.saturating_mul(2).saturating_add(1));
                let run = upper.or(lower)?.0;
                let ink = |pixel: Option<(usize, bool)>| pixel.is_some_and(|(_, ink)| ink);
                let ch = match (ink(upper), ink(lower)) {
                    (true, true) => '█',
                    (true, false) => '▀',
                    (false, true) => '▄',
                    (false, false) => return None,
                };
                Some((ch, run))
            }
            BigGrid::Octant => {
                // The run with the most ink in the cell paints it. On a tie,
                // the run that comes first in the text does.
                let mut bits = 0u8;
                let mut ink: Vec<(usize, u32)> = Vec::new();
                for row in 0..4 {
                    for col in 0..2 {
                        let gx = x.saturating_mul(2).saturating_add(col);
                        let gy = y.saturating_mul(4).saturating_add(row);
                        if let Some((run, true)) = self.pixel(lines, gx, gy) {
                            bits |= 1 << (row * 2 + col);
                            match ink.iter_mut().find(|(r, _)| *r == run) {
                                Some((_, count)) => *count += 1,
                                None => ink.push((run, 1)),
                            }
                        }
                    }
                }
                let run = ink
                    .iter()
                    .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))?
                    .0;
                Some((octant(bits), run))
            }
        }
    }
}

impl Widget for BigText {
    /// Big text measures its size: the rendering that the offered area
    /// allows. A layout override can fix either side.
    fn layout(&self) -> Layout {
        Layout::column()
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let bound = |constraint| match constraint {
            Constraint::Exact(n) | Constraint::AtMost(n) => Some(n),
            Constraint::Unbounded => None,
        };
        let (width, height) = (bound(c.width), bound(c.height));
        let tier = self.tier.unwrap_or_else(|| tier_for(c.glyphs));
        let text = self.resolve(width, height, tier).size();
        // A fit fills each bounded axis, and a capped fit fills a bounded
        // width. An unbounded axis takes the text, so text in a scrolling
        // area keeps a size.
        let size = match self.size {
            BigSize::Fit => Size::new(width.unwrap_or(text.w), height.unwrap_or(text.h)),
            BigSize::MaxRows(_) => Size::new(width.unwrap_or(text.w), text.h),
            BigSize::Exact { .. } => text,
        };
        c.clamp(size)
    }

    fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        render.push_layer("big_text");
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        render.fill(roles::TEXT, area, ' ')?;
        let tier = self.tier.unwrap_or_else(|| tier_for(render.glyphs()));
        let resolved = self.resolve(Some(area.w), Some(area.h), tier);
        let lines = self.frame(ctx.now(), ctx.motion_active(), &resolved);
        let placement = Placement::new(&resolved, &lines, area.size(), self.align, self.vertical);
        // Gradients span the block of text, not each row of it.
        let size = resolved.size();
        let (across, down) = (resolved.grid.across(), resolved.grid.down());
        let left = placement.lefts.iter().copied().min().unwrap_or(0);
        let block = Rect::new(
            area.tl.x.saturating_add(left / across),
            area.tl.y.saturating_add(placement.top / down),
            size.w.saturating_add(u32::from(left % across != 0)),
            size.h
                .saturating_add(u32::from(!placement.top.is_multiple_of(down))),
        );
        let styles: Vec<Style> = self
            .runs
            .iter()
            .map(|(path, _)| render.resolve_style(path))
            .collect();
        for y in 0..area.h {
            for x in 0..area.w {
                if let Some((ch, run)) = placement.cell(&lines, x, y)
                    && let Some(style) = styles.get(run)
                {
                    let point = Point {
                        x: area.tl.x.saturating_add(x),
                        y: area.tl.y.saturating_add(y),
                    };
                    render.put_styled(style, block, point, ch)?;
                }
            }
        }
        self.shown = Some(Shown {
            geometry: Geometry::of(&resolved),
            lines,
        });
        Ok(())
    }

    fn poll(&mut self, ctx: &mut dyn Context) -> Result<Option<Duration>> {
        let Some(roll) = &mut self.roll else {
            return Ok(None);
        };
        let area = ctx.view().view_rect_local();
        let hidden = area.w == 0 || area.h == 0;
        if hidden || !ctx.motion_active() {
            self.roll = None;
            return Ok(None);
        }
        // The poll repaints, and the render after the last frame ends the
        // roll. A widget out of view renders no frame, so the poll starts the
        // clock of the roll too.
        let started = *roll.started.get_or_insert(ctx.now());
        let ended = ctx.now().saturating_duration_since(started) >= ROLL;
        Ok((!ended).then_some(ROLL_FRAME))
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.wake = Some(ctx.wake_handle(PollLifetime::Node)?);
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("big_text")
    }
}

/// Returns one line of a roll at `progress`, from 0 to 1, for line boxes of
/// `height` pixel rows. The old columns align with the new ones on the
/// right. A column whose character and pixels match stays, and every other
/// column moves the old pixels up and the new ones in from below, one blank
/// row apart. A column without an old column rolls in from blank.
fn roll_line(from: &[Column], to: &[Column], progress: f32, height: u32) -> Vec<Column> {
    // Ease out, so a roll settles gently on its new glyph.
    let eased = 1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(3);
    let window = height.saturating_add(1);
    let shift = ((eased * window as f32).floor() as u32).min(window);
    let skip = from.len().saturating_sub(to.len());
    let lead = to.len().saturating_sub(from.len());
    to.iter()
        .enumerate()
        .map(|(index, new)| {
            let old = index
                .checked_sub(lead)
                .and_then(|index| from.get(index + skip));
            if old.is_some_and(|old| old.pixels == new.pixels && old.slot == new.slot) {
                return *new;
            }
            let old = old.map_or(0, |old| old.pixels);
            let mut pixels = 0;
            for row in 0..height.min(u64::BITS) {
                let source = row + shift;
                let ink = match source.cmp(&height) {
                    Ordering::Less => old >> source & 1 == 1,
                    Ordering::Equal => false,
                    Ordering::Greater => new.ink(source - height - 1),
                };
                if ink {
                    pixels |= 1 << row;
                }
            }
            Column { pixels, ..*new }
        })
        .collect()
}
