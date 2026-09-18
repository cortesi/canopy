//! Panes side by side, with dividers that show their scroll positions.

use canopy::{
    Context, EventOutcome, NodeId, NodeName, Render, ViewContext, ViewContextExt, Widget,
    derive_commands,
    error::Result,
    event::Event,
    geom::Rect,
    layout::{Direction, Edges, Layout},
};

use crate::scrollbar::{Axis, Scrollbar, ScrollbarGlyphs, THIN, edge_track, pane_target};

/// Panes side by side, each followed by a divider.
///
/// Children are panes with normal child sizing. `Columns` places them in a
/// row, with a one-cell gap after each displayed pane and one trailing column.
/// That cell holds a divider across the pane's rows. When a node in a pane
/// overflows vertically and reaches the pane's right edge, the divider beside
/// the node's visible rows becomes a track that shows and controls its
/// position; headers and footers in the pane keep plain divider lines. The
/// trailing column stays blank while the last pane fits. Horizontal overflow
/// scrolls through wheel input without a track.
///
/// `Columns` owns the scrollbars of its panes, so an enclosing frame draws none
/// for them. Dividers use `columns/divider`, thumbs `columns/thumb`, and a
/// thumb a drag holds `columns/thumb/active`.
pub struct Columns {
    /// Glyph set the divider and thumb below draw with.
    glyphs: ScrollbarGlyphs,
    /// Thumbs drawn in the dividers.
    scrollbar: Scrollbar,
}

#[derive_commands]
impl Columns {
    /// Construct columns with no panes.
    pub fn new() -> Self {
        Self {
            glyphs: THIN,
            scrollbar: Self::scrollbar(THIN),
        }
    }

    /// Build columns with replaced scrollbar glyphs.
    ///
    /// Rebuilding the thumbs drops a drag in progress, so configure glyphs
    /// before mounting.
    pub fn with_scrollbar_glyphs(mut self, glyphs: ScrollbarGlyphs) -> Self {
        self.glyphs = glyphs;
        self.scrollbar = Self::scrollbar(glyphs);
        self
    }

    /// Build the divider thumb for a glyph set.
    fn scrollbar(glyphs: ScrollbarGlyphs) -> Scrollbar {
        Scrollbar::vertical("columns/thumb", glyphs.thumb_vertical)
            .with_active("columns/thumb/active")
            .with_track("columns/divider", glyphs.track_vertical)
    }

    /// Return the scrollbar glyphs these columns draw with.
    pub fn scrollbar_glyphs(&self) -> ScrollbarGlyphs {
        self.glyphs
    }

    /// Move focus to another displayed pane by a signed offset, wrapping
    /// around.
    ///
    /// Focus moves to the pane's first focusable leaf, or to its first leaf
    /// when none accepts focus. Columns without a displayed pane do nothing.
    /// @param delta Panes to move; negative values move left.
    #[command]
    pub fn focus_column(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        let panes = Self::panes(c);
        if panes.is_empty() {
            return Ok(());
        }
        let current = panes
            .iter()
            .position(|pane| c.is_on_focus_path_of(*pane))
            .unwrap_or(0);
        let count = i64::try_from(panes.len()).unwrap_or(i64::MAX);
        let next = (i64::try_from(current).unwrap_or(0) + i64::from(delta)).rem_euclid(count);
        let pane = panes[usize::try_from(next).unwrap_or(0)];
        let target = c
            .focusable_leaves(pane)
            .first()
            .copied()
            .or_else(|| c.first_leaf(pane));
        if let Some(target) = target {
            c.set_focus(target)?;
        }
        Ok(())
    }

    /// Return the displayed panes in order.
    fn panes(ctx: &dyn ViewContext) -> Vec<NodeId> {
        ctx.children()
            .into_iter()
            .filter(|pane| ctx.view_of(*pane).is_some_and(|view| !view.is_empty()))
            .collect()
    }

    /// Return the divider cells after a pane, in this widget's outer
    /// coordinates.
    fn divider(ctx: &dyn ViewContext, pane: NodeId) -> Option<Rect> {
        let own = ctx.view().outer;
        let pane = ctx.view_of(pane)?.outer;
        let x = u32::try_from(pane.right() - i64::from(own.tl.x)).ok()?;
        let y = u32::try_from(i64::from(pane.tl.y) - i64::from(own.tl.y)).ok()?;
        (x < own.w && y < own.h).then(|| Rect::new(x, y, 1, pane.h.min(own.h - y)))
    }

    /// Return each divider track paired with the node it scrolls.
    fn tracks(ctx: &dyn ViewContext) -> Result<Vec<(NodeId, Rect)>> {
        let owner = ctx.node_id();
        let mut tracks = Vec::new();
        for pane in Self::panes(ctx) {
            let Some(divider) = Self::divider(ctx, pane) else {
                continue;
            };
            let Some(target) = pane_target(ctx, owner, pane, Axis::Vertical)? else {
                continue;
            };
            if let Some(track) = edge_track(ctx, &target, pane, Axis::Vertical, divider) {
                tracks.push((target.node, track));
            }
        }
        Ok(tracks)
    }
}

impl Default for Columns {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Columns {
    fn layout(&self) -> Layout {
        Layout::fill()
            .direction(Direction::Row)
            .gap(1)
            .padding(Edges::new(0, 1, 0, 0))
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let panes = Self::panes(ctx);
        // The last pane's divider is the trailing column, which shows only a
        // track.
        for pane in panes.iter().take(panes.len().saturating_sub(1)) {
            if let Some(divider) = Self::divider(ctx, *pane) {
                rndr.fill("columns/divider", divider, self.glyphs.track_vertical)?;
            }
        }
        let tracks = Self::tracks(ctx)?;
        self.scrollbar.render(rndr, ctx, &tracks)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Mouse(mouse) = event else {
            return Ok(EventOutcome::Ignore);
        };
        let tracks = Self::tracks(ctx)?;
        self.scrollbar.handle_mouse(ctx, mouse, &tracks)
    }

    fn owns_scrollbars(&self) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("columns")
    }
}
