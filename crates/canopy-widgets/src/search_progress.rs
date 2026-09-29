//! One row under search results that says what the search found.

use std::time::{Duration, Instant};

use canopy::{
    Context, NodeName, ViewContext, Widget,
    error::Result,
    geom::Size,
    layout::{CanvasContext, Layout, MeasureConstraints, Measurement},
    render::Render,
    runtime::{NodeWakeHandle, PollLifetime},
    style::roles,
};

use crate::Spinner;

/// Narrowest width that the row asks for, so that it never widens the pane
/// that holds it.
const MIN_WIDTH: u32 = 12;

/// One row under a list of search results: a spinner while the search runs,
/// and what the search found, such as `12 matches`.
///
/// The row runs no search. Its owner says what the search found, and whether
/// it runs on, with [`SearchProgress::set`]. A host puts the row under the
/// results, outside their scrolling, so the count stays in sight while the
/// results move. While the search runs, the spinner turns before the text,
/// and the row repaints itself as long as motion is active. At rest, as in
/// headless runs, the spinner shows its first frame.
///
/// The row pushes the `search_progress` layer and paints `text`.
pub struct SearchProgress {
    /// Node name, which bindings and automation match.
    name: NodeName,
    /// What the search found.
    text: String,
    /// Whether the search runs.
    running: bool,
    /// When the spinner started to turn, after the first poll of a run.
    since: Option<Instant>,
    /// Wake handle that starts the polls of a run, after the row mounts.
    wake: Option<NodeWakeHandle>,
}

impl Default for SearchProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchProgress {
    /// Construct an empty row for a search that does not run.
    pub fn new() -> Self {
        Self {
            name: NodeName::convert("search_progress"),
            text: String::new(),
            running: false,
            since: None,
            wake: None,
        }
    }

    /// Name the node, so bindings and automation can tell rows apart. The
    /// style layer stays `search_progress`.
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = NodeName::convert(name);
        self
    }

    /// Show `text`, and turn the spinner while `running` says that the
    /// search runs.
    pub fn set(&mut self, text: impl Into<String>, running: bool) {
        self.text = text.into();
        if running && !self.running {
            self.since = None;
            if let Some(wake) = &self.wake {
                // An expired handle means that the row left the tree, and its
                // next mount takes a new one.
                let _outcome = wake.wake();
            }
        }
        self.running = running;
    }

    /// Return the text, without the spinner.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Return whether the search runs.
    pub fn running(&self) -> bool {
        self.running
    }

    /// Return the glyph before the text: a spinner frame while the search
    /// runs, and a blank otherwise.
    fn lead(&self, ctx: &dyn ViewContext) -> char {
        match self.since {
            _ if !self.running => ' ',
            Some(since) if ctx.motion_active() => {
                Spinner::DOTS.frame(ctx.now().saturating_duration_since(since))
            }
            _ => Spinner::DOTS.step(0),
        }
    }
}

impl Widget for SearchProgress {
    fn layout(&self) -> Layout {
        Layout::column().flex_horizontal(1).fixed_height(1)
    }

    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        constraints.clamp(Size::new(MIN_WIDTH, 1))
    }

    fn canvas(&self, view: Size, _ctx: &CanvasContext) -> Size {
        view
    }

    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        render.push_layer("search_progress");
        let area = ctx.view().view_rect_local();
        render.fill(roles::TEXT, area, ' ')?;
        if area.h == 0 {
            return Ok(());
        }
        let text = format!("{} {}", self.lead(ctx), self.text);
        render.text(roles::TEXT, area.line(0)?, &text)
    }

    fn poll(&mut self, ctx: &mut dyn Context) -> Result<Option<Duration>> {
        if !self.running || !ctx.motion_active() {
            return Ok(None);
        }
        self.since.get_or_insert(ctx.now());
        Ok(Some(Spinner::DOTS.period()))
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.wake = Some(ctx.wake_handle(PollLifetime::Node)?);
        Ok(())
    }

    fn name(&self) -> NodeName {
        self.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use canopy::{
        ContextExt, TypedId,
        layout::Direction,
        testing::{ManualClock, harness::Harness},
    };

    use super::*;

    /// A host with the row at its top.
    #[derive(Default)]
    struct Host {
        /// The row, after the host mounts.
        row: Option<TypedId<SearchProgress>>,
    }

    impl Widget for Host {
        fn layout(&self) -> Layout {
            Layout::fill().direction(Direction::Column)
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let row = ctx.add_child(
                ctx.node_id(),
                SearchProgress::new().with_name("find_status"),
            )?;
            self.row = Some(row);
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("host")
        }
    }

    /// Sets the row of the host, and renders.
    fn set(harness: &mut Harness, text: &str, running: bool) -> Result<()> {
        harness.with_unique(|row: &mut SearchProgress, _| {
            row.set(text, running);
            Ok(())
        })?;
        harness.render()
    }

    /// Returns the first glyph of the row.
    fn lead(harness: &Harness) -> Option<char> {
        harness.tbuf().lines()[0].chars().next()
    }

    #[test]
    fn the_row_shows_a_spinner_only_while_the_search_runs() -> Result<()> {
        let mut harness = Harness::builder(Host::default()).size(20, 2).build()?;
        harness.render()?;
        assert_eq!(harness.find_nodes("**/find_status")?.len(), 1);
        set(&mut harness, "3 matches", true)?;
        let row = harness.tbuf().lines()[0].clone();
        assert!(
            row.starts_with(&format!("{} 3 matches", Spinner::DOTS.step(0))),
            "a running search shows the spinner at rest: {row:?}"
        );
        set(&mut harness, "4 matches", false)?;
        let row = harness.tbuf().lines()[0].clone();
        assert!(row.starts_with("  4 matches"), "{row:?}");
        Ok(())
    }

    #[test]
    fn with_motion_the_spinner_turns_with_the_clock_until_the_search_ends() -> Result<()> {
        let clock = Arc::new(ManualClock::new());
        let mut harness = Harness::builder(Host::default())
            .size(20, 2)
            .clock(Arc::clone(&clock))
            .motion(true)
            .build()?;
        harness.render()?;
        set(&mut harness, "1 match", true)?;
        assert_eq!(lead(&harness), Some(Spinner::DOTS.step(0)));
        harness.wait_until(Duration::from_secs(10), |harness| {
            clock.advance(Spinner::DOTS.period())?;
            Ok(lead(harness) == Some(Spinner::DOTS.step(2)))
        })?;
        set(&mut harness, "2 matches", false)?;
        assert_eq!(lead(&harness), Some(' '));
        Ok(())
    }
}
