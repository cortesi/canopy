//! Panes side by side, with dividers that show their scroll positions, and
//! an optional frame that marks the pane with focus.

use canopy::{
    Context, EventOutcome, NodeId, NodeName, ViewContext, ViewContextExt, Widget, derive_commands,
    error::Result,
    geom::Rect,
    input::Event,
    layout::{Direction, Edges, Layout, ScrollAxis},
    render::Render,
    tree::FocusScope,
};

use crate::{
    border::BoxGlyphs,
    scrollbar::{Scrollbar, ScrollbarGlyphs, edge_track, pane_target},
};

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
/// A framed row also draws a rule above and below the panes and a border
/// left of the first, so each pane sits in a box that shares its sides with
/// its neighbours. The box of the [marked pane](Columns::marked_pane) draws
/// with its own glyphs and `columns/divider/focused`. A [`Pane`](crate::Pane)
/// paints the same ground inside it.
///
/// `Columns` owns the scrollbars of its panes, so an enclosing frame draws none
/// for them. Dividers use `columns/divider`, thumbs `columns/thumb`, and a
/// thumb a drag holds `columns/thumb/active`.
pub struct Columns {
    /// Glyph set the divider and thumb below draw with.
    glyphs: ScrollbarGlyphs,
    /// Thumbs drawn in the dividers.
    scrollbar: Scrollbar,
    /// Glyphs of the frame around each pane, and of the box around the pane
    /// with focus, or `None` for a row without a frame.
    frame: Option<(BoxGlyphs, BoxGlyphs)>,
    /// Panes that stand for focus held by the columns or an ancestor.
    home: Vec<NodeId>,
}

#[derive_commands]
impl Columns {
    /// Construct columns with no panes.
    pub fn new() -> Self {
        Self {
            glyphs: ScrollbarGlyphs::THIN,
            scrollbar: Self::scrollbar(ScrollbarGlyphs::THIN, false),
            frame: None,
            home: Vec::new(),
        }
    }

    /// Build columns that box each pane in `light` and the pane with focus
    /// in `focused`.
    ///
    /// The frame takes a row above and below the panes and a column left of
    /// the first. The dividers become the boxes' shared sides, and the
    /// trailing column the last box's right side. Where two light boxes meet,
    /// the rules join in a tee; beside the focused box, the junction takes
    /// that box's corner.
    #[must_use]
    pub fn with_frame(mut self, light: BoxGlyphs, focused: BoxGlyphs) -> Self {
        self.frame = Some((light, focused));
        self.scrollbar = Self::scrollbar(self.glyphs, true);
        self
    }

    /// Set the panes that stand for focus held by the columns or an
    /// ancestor.
    ///
    /// A host that keeps focus above its panes while one of them takes its
    /// keys names that pane here. While focus is on this widget or an
    /// ancestor, the frame marks the first displayed pane of `panes`.
    pub fn set_home(&mut self, panes: Vec<NodeId>) {
        self.home = panes;
    }

    /// Build columns with replaced scrollbar glyphs.
    ///
    /// Rebuilding the thumbs drops a drag in progress, so configure glyphs
    /// before mounting.
    #[must_use]
    pub fn with_scrollbar_glyphs(mut self, glyphs: ScrollbarGlyphs) -> Self {
        self.glyphs = glyphs;
        self.scrollbar = Self::scrollbar(glyphs, self.frame.is_some());
        self
    }

    /// Build the divider thumb for a glyph set. A frame draws every divider
    /// itself, so the thumb draws no track over the focused box's sides.
    fn scrollbar(glyphs: ScrollbarGlyphs, framed: bool) -> Scrollbar {
        let scrollbar = Scrollbar::vertical("columns/thumb", glyphs.thumb_vertical)
            .with_active("columns/thumb/active");
        if framed {
            scrollbar
        } else {
            scrollbar.with_track("columns/divider", glyphs.track_vertical)
        }
    }

    /// Move focus to another displayed pane by a signed offset, wrapping
    /// around.
    ///
    /// Focus moves to the first node in the pane that accepts focus. A pane
    /// with none leaves focus where it is, and columns without a displayed
    /// pane do nothing.
    /// @param delta Panes to move; negative values move left.
    #[command]
    pub fn focus_column(&mut self, c: &mut dyn Context, delta: i32) -> Result<()> {
        let panes = Self::panes(c, c.node_id());
        if panes.is_empty() {
            return Ok(());
        }
        let current = panes
            .iter()
            .position(|pane| c.is_on_focus_path(*pane))
            .unwrap_or(0);
        let count = i64::try_from(panes.len()).unwrap_or(i64::MAX);
        let next = (i64::try_from(current).unwrap_or(0) + i64::from(delta)).rem_euclid(count);
        let pane = panes[usize::try_from(next).unwrap_or(0)];
        c.focus_first(FocusScope::Node(pane))?;
        Ok(())
    }

    /// Return the pane that the columns at `columns` mark, as their frame
    /// does: the displayed pane on the focus path, or the first displayed home
    /// pane while focus is on the columns or an ancestor.
    ///
    /// Return `None` when no pane is marked, or when `columns` is not a
    /// `Columns`.
    pub fn marked_pane(ctx: &dyn ViewContext, columns: NodeId) -> Option<NodeId> {
        ctx.with_widget(columns, |row: &Self| {
            Ok(row.marked(ctx, columns, &Self::panes(ctx, columns)))
        })
        .ok()
        .flatten()
    }

    /// Return the displayed panes of the columns at `columns` in order.
    fn panes(ctx: &dyn ViewContext, columns: NodeId) -> Vec<NodeId> {
        ctx.children_of(columns)
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

    /// Return the marked pane of `panes`, the displayed panes of these
    /// columns at `columns`.
    fn marked(&self, ctx: &dyn ViewContext, columns: NodeId, panes: &[NodeId]) -> Option<NodeId> {
        if let Some(pane) = panes.iter().find(|pane| ctx.is_on_focus_path(**pane)) {
            return Some(*pane);
        }
        let focused = ctx.focused_node()?;
        let mut node = Some(columns);
        while let Some(current) = node {
            if current == focused {
                return self.home.iter().find(|pane| panes.contains(pane)).copied();
            }
            node = ctx.parent_of(current);
        }
        None
    }

    /// Draw the boxes around `panes` in `light`, and around the pane with
    /// focus in `focused`.
    fn render_frame(
        &self,
        rndr: &mut Render,
        ctx: &dyn ViewContext,
        panes: &[NodeId],
        (light, focused): (BoxGlyphs, BoxGlyphs),
    ) -> Result<()> {
        let own = ctx.view().outer;
        if own.h < 2 {
            return Ok(());
        }
        let focus = self.marked(ctx, ctx.node_id(), panes);
        // Each displayed pane's columns, in this widget's outer coordinates,
        // and whether it holds focus.
        let spans: Vec<(u32, u32, bool)> = panes
            .iter()
            .filter_map(|pane| {
                let outer = ctx.view_of(*pane)?.outer;
                let x = u32::try_from(i64::from(outer.tl.x) - i64::from(own.tl.x)).ok()?;
                Some((x, outer.w, focus == Some(*pane)))
            })
            .collect();
        let Some(&(last_x, last_w, _)) = spans.last() else {
            return Ok(());
        };
        let look = |lit: bool| {
            if lit {
                ("columns/divider/focused", focused)
            } else {
                ("columns/divider", light)
            }
        };
        let bottom = own.h - 1;
        for &(x, w, lit) in &spans {
            let (style, glyphs) = look(lit);
            rndr.fill(style, Rect::new(x, 0, w, 1), glyphs.horizontal)?;
            rndr.fill(style, Rect::new(x, bottom, w, 1), glyphs.horizontal)?;
        }
        // A side lies left of each pane and right of the last, between the
        // panes on either side of it.
        for side in 0..=spans.len() {
            let left = side.checked_sub(1).map(|pane| spans[pane].2);
            let right = spans.get(side).map(|span| span.2);
            let x = match spans.get(side) {
                Some(&(x, _, _)) => x.saturating_sub(1),
                None => last_x + last_w,
            };
            if x >= own.w {
                continue;
            }
            let (style, glyphs) = look(left == Some(true) || right == Some(true));
            rndr.fill(
                style,
                Rect::new(x, 1, 1, bottom.saturating_sub(1)),
                glyphs.vertical,
            )?;
            let (top, low) = junctions(glyphs, left, right);
            rndr.fill(style, Rect::new(x, 0, 1, 1), top)?;
            rndr.fill(style, Rect::new(x, bottom, 1, 1), low)?;
        }
        Ok(())
    }

    /// Return each divider track paired with the node it scrolls.
    fn tracks(ctx: &dyn ViewContext) -> Result<Vec<(NodeId, Rect)>> {
        let owner = ctx.node_id();
        let mut tracks = Vec::new();
        for pane in Self::panes(ctx, owner) {
            let Some(divider) = Self::divider(ctx, pane) else {
                continue;
            };
            let Some(target) = pane_target(ctx, owner, pane, ScrollAxis::Vertical)? else {
                continue;
            };
            if let Some(track) = edge_track(ctx, &target, pane, ScrollAxis::Vertical, divider) {
                tracks.push((target.node, track));
            }
        }
        Ok(tracks)
    }
}

/// Return the glyphs where a side meets the top and bottom rules. `glyphs`
/// draw the side, and `left` and `right` give whether the box on that side
/// has focus, or `None` where no pane lies.
///
/// Beside the focused box the junction takes that box's corner and drops
/// the light rule's arm, which would draw in the box's colour.
fn junctions(glyphs: BoxGlyphs, left: Option<bool>, right: Option<bool>) -> (char, char) {
    match (left, right) {
        (Some(true), Some(true)) | (Some(false), Some(false)) => ('┬', '┴'),
        (_, Some(true)) | (None, _) => (glyphs.topleft, glyphs.bottomleft),
        (Some(true) | Some(false), _) => (glyphs.topright, glyphs.bottomright),
    }
}

impl Default for Columns {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Columns {
    fn layout(&self) -> Layout {
        let padding = if self.frame.is_some() {
            Edges::all(1)
        } else {
            Edges::new(0, 1, 0, 0)
        };
        Layout::fill()
            .direction(Direction::Row)
            .gap(1)
            .padding(padding)
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let panes = Self::panes(ctx, ctx.node_id());
        if let Some(frame) = self.frame {
            self.render_frame(rndr, ctx, &panes, frame)?;
        } else {
            // The last pane's divider is the trailing column, which shows
            // only a track.
            for pane in panes.iter().take(panes.len().saturating_sub(1)) {
                if let Some(divider) = Self::divider(ctx, *pane) {
                    rndr.fill("columns/divider", divider, self.glyphs.track_vertical)?;
                }
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
