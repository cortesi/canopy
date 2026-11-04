//! Rendered input and reader state around immutable prepared geometry.

use std::{
    cell::RefCell,
    mem,
    ops::Range,
    rc::Rc,
    sync::{Arc, Weak},
};

use canopy::{
    ChangeOutcome, Context, EventOutcome, NodeName, ViewContext, Widget,
    error::Result,
    geom::{Point, Rect, Size},
    input::{Event, NavIntent, mouse},
    layout::{
        CanvasContext, Layout, MeasureConstraints, Measurement, RevealAlign, ScrollAxis,
        ScrollMark, ScrollOp, View,
    },
    render::Render,
    runtime::NodeWakeHandle,
    text::{self, grapheme_width},
    tree::{FocusDirection, FocusScope},
};
use canopy_widgets::view_search;
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    cursor::{self, Frame, Step},
    document::{DocumentSnapshot, Object, ReadingPosition, ReadingRange},
    options::{Follow, Mode},
    presentation::{PaintOverlays, PresentationSnapshot},
    units::{EditUnit, edit_units, unit_at},
};

/// Reader state shared between the rendered surface and its controller.
#[derive(Default)]
pub struct SurfaceState {
    /// The immutable layout currently displayed by this surface.
    pub presentation: Option<Arc<PresentationSnapshot>>,
    /// Semantic identity survives eviction of inactive prepared geometry.
    pub document: Option<Arc<DocumentSnapshot>>,
    /// Selection, search, and object decorations for the displayed revision.
    pub overlays: PaintOverlays,
    /// Whether navigation scrolls the document or moves a reading cursor.
    pub mode: Mode,
    /// Whether publication follows the tail or preserves the reading anchor.
    pub follow: Follow,
    /// Reader-input generation used to supersede pending position restores.
    pub navigation: u64,
    /// Latest allocated viewport width reported to the controller.
    pub width: u32,
    /// Controller wake endpoint for input and geometry changes.
    pub wake: Option<NodeWakeHandle>,
    /// Object requested for inspection by an explicit pointer activation.
    pub pending_open: Option<u64>,
    /// Whether a captured left-button gesture is in progress.
    pub dragging: bool,
    /// Prepared canvas rectangle for the next local reveal request.
    pub reveal: Option<Rect>,
    /// Whether a new query waits to reveal its first match once its matches
    /// publish. Reader input clears it.
    pub search_reveal: bool,
    /// The mode that `Esc` returns to: Scroll, or Interact for a view that
    /// rests in the block cursor.
    pub resting: Mode,
    /// The start of the unit that the block cursor marks. It holds across
    /// the publications of one epoch. Outside Interact, it is the unit that
    /// the cursor marks again if it still shows when Interact starts.
    pub marked: Option<usize>,
    /// A unit for the cursor to mark once a presentation of its epoch
    /// shows.
    pub pending_mark: Option<PendingMark>,
    /// Whether a newer text than the shown presentation's, from an edit,
    /// waits for its presentation. Moves that need geometry do nothing
    /// until it shows.
    pub awaiting: bool,
    /// How the view must scroll for the block cursor, which the controller
    /// does when it next polls.
    pub cursor_scroll: Option<CursorScroll>,
    /// Fixed reading endpoint for the current pointer gesture.
    drag_anchor: Option<ReadingPosition>,
    /// Retained display column for vertical selection movement.
    preferred_column: Option<u32>,
    /// The edit units of the shown document and their rows, built when the
    /// cursor first needs them.
    cursor: Option<CursorMap>,
}

/// A unit for the cursor to mark once a presentation of its epoch shows: as
/// a host asks before a reload shows, or after an edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingMark {
    /// The epoch of the text that `byte` addresses.
    pub epoch: u64,
    /// A source byte; the cursor marks the unit that `unit_at` finds for it.
    pub byte: usize,
    /// The screen row for the unit's first row, or `None` to show the unit
    /// by the smallest move.
    pub row: Option<u32>,
    /// The mode of the view once the unit shows.
    pub mode: Mode,
}

/// How the view must scroll for the block cursor after it painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorScroll {
    /// Show the unit nearest the view, as no unit shows.
    Settle,
    /// Show the marked unit again, as a resize or a new layout hid it.
    Reveal,
}

/// The edit units of a shown document, and the content rows of each in its
/// presentation.
pub type CursorUnits = (Arc<[EditUnit]>, Arc<[Range<u32>]>);

/// The edit units of a shown presentation, and the content rows of each.
struct CursorMap {
    /// The presentation that the rows belong to.
    presentation: Weak<PresentationSnapshot>,
    /// The document that the units belong to.
    document: Weak<DocumentSnapshot>,
    /// The edit units of the document.
    units: Arc<[EditUnit]>,
    /// The content rows of each unit.
    rows: Arc<[Range<u32>]>,
}

/// Return the rows of the content of `unit` in `presentation`: from the first
/// content row of its first block to the last of its last.
fn unit_rows(presentation: &PresentationSnapshot, unit: &EditUnit) -> Range<u32> {
    let blocks = &presentation.document.blocks;
    unit.blocks
        .clone()
        .filter_map(|index| presentation.content_rows(blocks.get(index)?.unit_id))
        .filter(|rows| !rows.is_empty())
        .reduce(|a, b| a.start.min(b.start)..a.end.max(b.end))
        .unwrap_or(0..0)
}

/// Return the document rows that `view` shows.
fn frame(view: &View) -> Frame {
    Frame {
        top: view.scroll.y,
        height: view.content.h,
        canvas: view.canvas.h,
    }
}

impl SurfaceState {
    /// Wake the controller after shared state changes.
    fn notify(&self) -> Result<()> {
        if let Some(wake) = &self.wake {
            wake.wake()?;
        }
        Ok(())
    }

    /// Return to scrolling. The object cursor belongs to Interact, so it
    /// goes too: nothing stays marked, and no action reads an object the
    /// reader no longer shows.
    pub fn leave_interact(&mut self) {
        self.mode = Mode::Scroll;
        self.overlays.active_object = None;
    }

    /// Return the edit units of the shown document and the rows of each,
    /// building them for a new presentation.
    pub fn cursor_map(&mut self) -> Option<CursorUnits> {
        let presentation = self.presentation.clone()?;
        let shown = |map: &CursorMap| Weak::as_ptr(&map.presentation) == Arc::as_ptr(&presentation);
        if !self.cursor.as_ref().is_some_and(shown) {
            let units = self
                .cursor
                .take()
                .filter(|map| Weak::as_ptr(&map.document) == Arc::as_ptr(&presentation.document))
                .map_or_else(
                    || edit_units(&presentation.document).into(),
                    |map| map.units,
                );
            let rows = units
                .iter()
                .map(|unit| unit_rows(&presentation, unit))
                .collect();
            self.cursor = Some(CursorMap {
                presentation: Arc::downgrade(&presentation),
                document: Arc::downgrade(&presentation.document),
                units,
                rows,
            });
        }
        let map = self.cursor.as_ref()?;
        Some((map.units.clone(), map.rows.clone()))
    }

    /// Return the index of the unit that the block cursor marks.
    pub fn marked_index(&mut self) -> Option<usize> {
        let byte = self.marked?;
        unit_at(&self.cursor_map()?.0, byte)
    }

    /// Return the unit that the block cursor marks, and its rows, while
    /// Interact lasts.
    pub fn marked_unit(&mut self) -> Option<(EditUnit, Range<u32>)> {
        if self.mode != Mode::Interact {
            return None;
        }
        let index = self.marked_index()?;
        let (units, rows) = self.cursor_map()?;
        Some((units.get(index)?.clone(), rows.get(index)?.clone()))
    }

    /// Mark unit `index`. A move to another unit drops the object cursor,
    /// unless `keep_object`, as when the object cursor carries the block
    /// cursor.
    pub fn mark(&mut self, index: usize, keep_object: bool) {
        let Some(start) = self
            .cursor_map()
            .and_then(|(units, _)| Some(units.get(index)?.range.start))
        else {
            return;
        };
        let moved = self.marked_index() != Some(index);
        if moved && !keep_object {
            self.overlays.active_object = None;
        }
        self.marked = Some(start);
    }

    /// Return the index of the unit that holds block `index` of the shown
    /// document.
    pub fn unit_of_block(&mut self, index: usize) -> Option<usize> {
        let (units, _) = self.cursor_map()?;
        let unit = units.partition_point(|unit| unit.blocks.end <= index);
        units
            .get(unit)
            .is_some_and(|found| found.blocks.contains(&index))
            .then_some(unit)
    }

    /// Return the index of the unit of the reading unit `unit_id`.
    pub fn unit_of_reading(&mut self, unit_id: u64) -> Option<usize> {
        let block = self
            .presentation
            .as_ref()?
            .document
            .unit_position(unit_id)?;
        self.unit_of_block(block)
    }

    /// Keep the block cursor on a unit with a row in view, as `frame` shows
    /// it: the marked unit, or else the first unit in view, or else the
    /// nearest unit, which the view must scroll to. Return that step, if the
    /// cursor moves.
    fn settle(&mut self, frame: Frame) -> Option<Step> {
        if self.mode != Mode::Interact || frame.height == 0 {
            return None;
        }
        let (_, rows) = self.cursor_map()?;
        let step = cursor::settle(&rows, self.marked_index(), frame)?;
        self.mark(step.unit, false);
        Some(step)
    }

    /// Supersede any queued reveal after explicit reader navigation.
    fn navigated(&mut self) {
        self.navigation = self.navigation.wrapping_add(1);
        self.reveal = None;
        self.search_reveal = false;
    }

    /// Record a moved viewport as navigation, pausing follow only when the
    /// offset leaves the tail.
    fn scrolled(&mut self, view: &View) {
        self.navigated();
        if view.scroll.y < view.canvas.h.saturating_sub(view.content.h) {
            self.follow = Follow::Paused;
        }
    }

    /// Whether selection or object navigation, rather than the viewport,
    /// consumes a navigation intent.
    pub fn claims(&self, nav: NavIntent) -> bool {
        self.overlays.selection.is_some() && !matches!(nav, NavIntent::PageUp | NavIntent::PageDown)
            || self.mode == Mode::Interact
                && matches!(
                    nav,
                    NavIntent::Up | NavIntent::Down | NavIntent::First | NavIntent::Last
                )
    }
}

/// Focusable reading surface that paints and hits prepared document geometry.
pub struct RenderedSurface {
    /// Shared publication and reader state.
    state: Rc<RefCell<SurfaceState>>,
    /// Input uses the geometry that produced the visible frame.
    painted: Option<Weak<PresentationSnapshot>>,
    /// Last painted or explicitly scrolled offset and viewport size.
    observed_scroll: Option<(Point, Size)>,
    /// Unpainted reading restore with its navigation generation and source
    /// fallback.
    pending_reading: Option<(u64, ReadingPosition, usize, u32)>,
}

impl RenderedSurface {
    /// Construct the surface around controller-owned shared reader state.
    pub fn new(state: Rc<RefCell<SurfaceState>>) -> Self {
        Self {
            state,
            painted: None,
            observed_scroll: None,
            pending_reading: None,
        }
    }

    /// Reconcile against state captured now, after any intervening input.
    pub fn publish(
        &mut self,
        ctx: &mut dyn Context,
        next: Arc<PresentationSnapshot>,
    ) -> Result<bool> {
        let navigation = self.state.borrow().navigation;
        // The block cursor's unit holds its place on screen, so a resize or
        // an append keeps it where the reader looks.
        let anchor = self
            .pending_reading
            .filter(|pending| pending.0 == navigation)
            .map(|(_, position, source, row)| (position, source, row))
            .or_else(|| self.marked_anchor(ctx))
            .or_else(|| self.reading_anchor(ctx));
        let mut state = self.state.borrow_mut();
        let mut cleared = false;
        let old_document = state
            .presentation
            .as_ref()
            .map(|p| p.document.clone())
            .or_else(|| state.document.clone());
        let same_epoch = old_document
            .as_ref()
            .is_some_and(|old| old.source.epoch == next.document.source.epoch);
        if same_epoch {
            if let Some(selection) = &state.overlays.selection {
                let unchanged = old_document
                    .as_ref()
                    .is_some_and(|old| same_projection(old, &next.document, selection));
                if !unchanged {
                    state.overlays.selection = None;
                    cleared = true;
                }
            }
            if state
                .overlays
                .active_object
                .is_some_and(|id| !next.document.objects.iter().any(|o| o.id == id))
            {
                let source = old_document
                    .as_ref()
                    .and_then(|document| {
                        document
                            .objects
                            .iter()
                            .find(|o| Some(o.id) == state.overlays.active_object)
                    })
                    .map_or(0, |o| o.source.start);
                state.overlays.active_object = next
                    .document
                    .objects
                    .iter()
                    .find(|o| o.source.start >= source)
                    .or_else(|| next.document.objects.last())
                    .map(|o| o.id);
            }
            if state
                .drag_anchor
                .is_some_and(|p| !valid_position(&next.document, p))
                || cleared
            {
                state.drag_anchor = None;
                state.dragging = false;
                ctx.release_mouse()?;
            }
        } else {
            cleared = state.overlays.selection.is_some();
            // The images that the host supplied outlive a new text, as the
            // reader's answers do: it does not ask for them again. A new
            // document drops both.
            let images = mem::take(&mut state.overlays.images);
            state.overlays = PaintOverlays {
                images,
                ..PaintOverlays::default()
            };
            state.mode = state.resting;
            state.marked = None;
            state.drag_anchor = None;
            state.dragging = false;
            state.preferred_column = None;
            state.pending_open = None;
            state.navigation = state.navigation.wrapping_add(1);
            ctx.release_mouse()?;
        }
        let resolved = if same_epoch && state.follow == Follow::Paused {
            anchor.and_then(|(position, source, viewport_row)| {
                let position =
                    next.document
                        .resolve_anchor(position.unit_id, source, position.offset)?;
                let source = next.document.unit(position.unit_id)?.source.start;
                Some((position, source, viewport_row))
            })
        } else {
            None
        };
        let target = if state.follow == Follow::Following {
            Some((next.height().saturating_sub(1), None))
        } else if same_epoch {
            resolved.map(|(position, _, viewport_row)| {
                (
                    next.row_for_unit(position.unit_id, position.offset),
                    Some(viewport_row),
                )
            })
        } else {
            Some((0, Some(0)))
        };
        self.pending_reading = resolved.map(|(position, source, viewport_row)| {
            (state.navigation, position, source, viewport_row)
        });
        let epoch = next.document.source.epoch;
        state.document = Some(next.document.clone());
        state.presentation = Some(next);
        if !same_epoch {
            state.awaiting = false;
        }
        let mut target = target;
        // A unit that a host asked for, or that an edit changed, before its
        // presentation showed. An edit keeps its unit on its screen row.
        if let Some(pending) = state.pending_mark
            && pending.epoch == epoch
        {
            state.pending_mark = None;
            state.marked = Some(pending.byte);
            state.mode = pending.mode;
            let rows = state
                .cursor_map()
                .and_then(|(units, rows)| rows.get(unit_at(&units, pending.byte)?).cloned());
            if let Some(rows) = rows
                && !same_epoch
                && state.follow == Follow::Paused
            {
                target = Some((rows.start, pending.row));
            }
        }
        if let Some((row, viewport_row)) = target {
            state.reveal = Some(Rect::new(0, row, 1, 1));
            ctx.reveal_anchor(viewport_row.map_or(RevealAlign::Nearest, RevealAlign::Top));
        }
        self.observed_scroll = None;
        Ok(cleared)
    }

    /// Enter the block cursor without changing the follow policy. It marks
    /// the unit it marked before, if that unit still shows, or else the first
    /// unit with a row in view.
    pub fn enter_interact(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.state.borrow_mut().mode = Mode::Interact;
        self.sync_cursor(ctx)?;
        self.state.borrow().notify()
    }

    /// Keep the block cursor on a unit with a row in view, and scroll to it
    /// when no unit shows. After a resize hid the marked unit, scroll to
    /// that unit instead.
    pub fn sync_cursor(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let view = ctx.view();
        let shown = frame(&view);
        let top = {
            let mut state = self.state.borrow_mut();
            match state.cursor_scroll.take() {
                Some(CursorScroll::Reveal) => state
                    .marked_unit()
                    .map(|(_, rows)| rows)
                    .filter(|rows| !rows.is_empty() && !shown.shows(rows))
                    .map(|rows| cursor::reveal(&rows, shown, rows.start >= shown.top)),
                _ => state.settle(shown).map(|step| step.top),
            }
        };
        if let Some(top) = top
            && top != view.scroll.y
        {
            self.scroll_to(ctx, top);
        }
        Ok(())
    }

    /// Mark the unit that holds source byte `byte`, or else the next unit,
    /// or else the one before, enter the block cursor, and show the unit.
    pub fn select_block_at(&mut self, ctx: &mut dyn Context, byte: usize) -> Result<()> {
        let view = ctx.view();
        let step = {
            let mut state = self.state.borrow_mut();
            state.mode = Mode::Interact;
            let Some((units, rows)) = state.cursor_map() else {
                return Ok(());
            };
            let Some(unit) = unit_at(&units, byte) else {
                return Ok(());
            };
            state.mark(unit, false);
            state.follow = Follow::Paused;
            state.navigated();
            rows.get(unit)
                .filter(|rows| !rows.is_empty())
                .map(|rows| cursor::reveal(rows, frame(&view), true))
        };
        if let Some(top) = step {
            self.scroll_to(ctx, top);
        }
        self.state.borrow().notify()
    }

    /// Scroll the view so its first row is `top`, as the block cursor's own
    /// move, and keep the observed offset.
    fn scroll_to(&mut self, ctx: &mut dyn Context, top: u32) {
        let x = ctx.view().scroll.x;
        ctx.scroll(ScrollOp::To(Point { x, y: top }));
        let view = ctx.view();
        self.observed_scroll = Some((view.scroll, view.content_size()));
    }

    /// Move the block cursor by a navigation intent: to the next or previous
    /// unit, or the first or last, or through a tall unit.
    fn block_step(&mut self, ctx: &mut dyn Context, intent: NavIntent) -> Result<()> {
        let view = frame(&ctx.view());
        let step = {
            let mut state = self.state.borrow_mut();
            // The rows of an edited text are not laid out yet.
            if state.awaiting {
                return Ok(());
            }
            let Some((_, rows)) = state.cursor_map() else {
                return Ok(());
            };
            let marked = state.marked_index();
            match (intent, marked) {
                (NavIntent::Down, Some(index)) => cursor::down(&rows, index, view),
                (NavIntent::Up, Some(index)) => cursor::up(&rows, index, view),
                (NavIntent::First, _) => cursor::first(&rows, view),
                (NavIntent::Last, _) => cursor::last(&rows, view),
                _ => cursor::settle(&rows, marked, view),
            }
        };
        let Some(step) = step else {
            return Ok(());
        };
        {
            let mut state = self.state.borrow_mut();
            state.mark(step.unit, false);
            state.follow = Follow::Paused;
            state.navigated();
        }
        self.scroll_to(ctx, step.top);
        self.state.borrow().notify()
    }

    /// Capture the reading position of the first row of the marked unit in
    /// view, while Interact lasts, with its row in the view.
    fn marked_anchor(&self, ctx: &dyn ViewContext) -> Option<(ReadingPosition, usize, u32)> {
        let view = ctx.view().view_rect();
        let (_, rows) = self.state.borrow_mut().marked_unit()?;
        let from = rows.start.max(view.tl.y);
        let end = rows.end.min(view.tl.y.saturating_add(view.h));
        if from >= end {
            return None;
        }
        self.anchor_at(
            Point {
                x: view.tl.x,
                y: from,
            },
            end,
        )
        .map(|(position, source, row)| (position, source, row + (from - view.tl.y)))
    }

    /// Start selection at the object boundary or current reading anchor.
    pub fn start_selection(&self, ctx: &dyn ViewContext) -> Result<()> {
        let reading = self.reading_anchor(ctx).map(|a| a.0);
        let mut state = self.state.borrow_mut();
        // The block cursor's unit starts it, at its first reading position.
        let marked = state.marked_unit().and_then(|(unit, _)| {
            let block = state
                .presentation
                .as_ref()?
                .document
                .blocks
                .get(unit.blocks.start)?;
            Some(ReadingPosition {
                unit_id: block.unit_id,
                offset: 0,
            })
        });
        state.mode = Mode::Interact;
        if state.overlays.selection.is_none() {
            let object_position = state.presentation.as_ref().and_then(|p| {
                let object = p
                    .document
                    .objects
                    .iter()
                    .find(|o| Some(o.id) == state.overlays.active_object)?;
                object_position(&p.document, object)
            });
            if let Some(position) = object_position.or(marked).or(reading) {
                state.overlays.selection = Some(ReadingRange {
                    start: position,
                    end: position,
                });
            }
        }
        state.follow = Follow::Paused;
        state.preferred_column = None;
        state.navigated();
        state.notify()
    }

    /// Clear the text selection first, then the object cursor, and then,
    /// for a view that rests in Scroll, leave the block cursor. Release
    /// capture.
    pub fn escape(&self, ctx: &mut dyn Context) -> Result<bool> {
        let mut state = self.state.borrow_mut();
        let handled = if state.overlays.selection.take().is_some()
            || state.overlays.active_object.take().is_some()
        {
            true
        } else if state.mode == Mode::Interact && state.resting == Mode::Scroll {
            state.leave_interact();
            true
        } else {
            state.drag_anchor.is_some()
        };
        if !handled {
            return Ok(false);
        }
        state.dragging = false;
        state.drag_anchor = None;
        state.preferred_column = None;
        ctx.release_mouse()?;
        state.notify()?;
        Ok(handled)
    }

    /// Apply a navigation intent to selection, object cursor, or viewport.
    pub fn navigate(&mut self, ctx: &mut dyn Context, intent: NavIntent) -> Result<()> {
        let (claimed, selection) = {
            let state = self.state.borrow();
            (state.claims(intent), state.overlays.selection.is_some())
        };
        if !claimed {
            return self.scroll(ctx, intent.scroll_op());
        }
        if selection {
            return self.move_selection(ctx, intent);
        }
        self.block_step(ctx, intent)
    }

    /// Visit an adjacent object in the block cursor, and carry the cursor to
    /// the object's unit, or traverse focus at a boundary if
    /// `leave_at_boundary`. With no object cursor, start from the marked
    /// unit: forward at its first object, backward at its last.
    pub fn object_next(
        &self,
        ctx: &mut dyn Context,
        delta: i32,
        leave_at_boundary: bool,
    ) -> Result<()> {
        let mut state = self.state.borrow_mut();
        let Some(presentation) = state.presentation.clone() else {
            return Ok(());
        };
        state.mode = Mode::Interact;
        let objects = &presentation.document.objects;
        let current = objects
            .iter()
            .position(|o| Some(o.id) == state.overlays.active_object);
        let marked = state.marked_unit().map(|(unit, _)| unit.range);
        let target = match (current, marked) {
            (Some(index), _) if delta < 0 => index.checked_sub(1),
            (Some(index), _) => (index + 1 < objects.len()).then_some(index + 1),
            (None, Some(range)) if delta < 0 => {
                objects.iter().rposition(|o| o.source.start <= range.end)
            }
            (None, Some(range)) => objects.iter().position(|o| o.source.start >= range.start),
            (None, None) if delta < 0 => objects.len().checked_sub(1),
            (None, None) => (!objects.is_empty()).then_some(0),
        };
        if let Some(index) = target {
            let object = &objects[index];
            state.overlays.active_object = Some(object.id);
            if let Some(unit) = state.unit_of_reading(object.unit_id) {
                state.mark(unit, true);
            }
            state.follow = Follow::Paused;
            state.navigated();
            drop(state);
            return self.reveal_object(ctx);
        }
        drop(state);
        if leave_at_boundary {
            ctx.focus_move(
                FocusScope::Root,
                if delta < 0 {
                    FocusDirection::Prev
                } else {
                    FocusDirection::Next
                },
            )?;
        }
        self.state.borrow().notify()
    }

    /// Queue a local reveal of the selected object, including its card label.
    fn reveal_object(&self, ctx: &mut dyn Context) -> Result<()> {
        let mut state = self.state.borrow_mut();
        if let Some(presentation) = state.presentation.as_ref()
            && let Some(object) = presentation
                .document
                .objects
                .iter()
                .find(|o| Some(o.id) == state.overlays.active_object)
        {
            state.reveal = Some(Rect::new(
                0,
                object_row(presentation, object).unwrap_or(0),
                1,
                1,
            ));
            ctx.reveal_anchor(RevealAlign::Nearest);
        }
        state.notify()
    }

    /// Capture the first useful reading position within the current viewport.
    fn reading_anchor(&self, ctx: &dyn ViewContext) -> Option<(ReadingPosition, usize, u32)> {
        let view = ctx.view().view_rect();
        self.anchor_at(view.tl, view.tl.y.saturating_add(view.h))
    }

    /// Find a useful prepared reading position within a bounded row interval.
    fn anchor_at(&self, scroll: Point, end: u32) -> Option<(ReadingPosition, usize, u32)> {
        let state = self.state.borrow();
        let presentation = state.presentation.as_ref()?;
        for y in scroll.y..end.min(presentation.height()) {
            let row = presentation.row(y)?;
            let offset = row
                .runs
                .iter()
                .find_map(|run| run.reading.as_ref().map(|range| range.start));
            if let Some(offset) = offset.or_else(|| row.atomic.then_some(0)) {
                let position = ReadingPosition {
                    unit_id: row.unit_id,
                    offset,
                };
                // A card label has no reading bytes. Its first preview position
                // lives below the label, so retain that relative row too.
                let reading_row = presentation.row_for_unit(position.unit_id, position.offset);
                return Some((
                    position,
                    row.source.start,
                    y - scroll.y + reading_row.saturating_sub(y),
                ));
            }
        }
        None
    }

    /// Capture the prepared reading position at the first useful viewport row.
    /// A Point permits the controller to query this with its read-only context
    /// while supplying the surface's own scroll, rather than the owner's view.
    pub fn viewport_anchor(&self, scroll: Point) -> Option<(ReadingPosition, u32)> {
        self.anchor_at(scroll, u32::MAX)
            .map(|(position, _, row)| (position, row))
    }

    /// Restore a same-epoch reading identity, retaining its row in the
    /// viewport. Return false when the controller must use the source
    /// neighborhood instead.
    pub fn restore_viewport_anchor(
        &mut self,
        ctx: &mut dyn Context,
        position: ReadingPosition,
        viewport_row: u32,
    ) -> Result<bool> {
        let mut state = self.state.borrow_mut();
        let Some(presentation) = state.presentation.as_ref() else {
            return Ok(false);
        };
        if !valid_position(&presentation.document, position) {
            return Ok(false);
        }
        let source = presentation
            .document
            .unit(position.unit_id)
            .map_or(0, |unit| unit.source.start);
        let row = presentation.row_for_unit(position.unit_id, position.offset);
        state.reveal = Some(Rect::new(0, row, 1, 1));
        self.pending_reading = Some((state.navigation, position, source, viewport_row));
        self.observed_scroll = None;
        ctx.reveal_anchor(RevealAlign::Top(viewport_row));
        Ok(true)
    }

    /// Queue a local reveal using a source-byte neighborhood as fallback.
    pub fn reveal_source(
        &mut self,
        ctx: &mut dyn Context,
        byte: usize,
        viewport_row: u32,
    ) -> Result<()> {
        let mut state = self.state.borrow_mut();
        if let Some(presentation) = state.presentation.as_ref() {
            state.reveal = Some(Rect::new(0, presentation.row_for_source(byte), 1, 1));
            ctx.reveal_anchor(RevealAlign::Top(viewport_row));
            self.observed_scroll = None;
            self.pending_reading = None;
        }
        Ok(())
    }

    /// Scroll locally and pause follow only when the actual offset leaves the
    /// tail.
    fn scroll(&mut self, ctx: &mut dyn Context, operation: ScrollOp) -> Result<()> {
        let before = ctx.view().scroll;
        ctx.scroll(operation);
        let view = ctx.view();
        {
            let mut state = self.state.borrow_mut();
            if before != view.scroll {
                state.scrolled(&view);
            }
        }
        self.observed_scroll = Some((view.scroll, view.content_size()));
        self.sync_cursor(ctx)?;
        self.state.borrow().notify()
    }

    /// Move the active endpoint by grapheme or displayed row and reveal it.
    fn move_selection(&self, ctx: &mut dyn Context, intent: NavIntent) -> Result<()> {
        let mut state = self.state.borrow_mut();
        let Some(presentation) = state.presentation.clone() else {
            return Ok(());
        };
        let Some(selection) = state.overlays.selection.clone() else {
            return Ok(());
        };
        let current = selection.end;
        let Some(unit_index) = presentation.document.unit_position(current.unit_id) else {
            return Ok(());
        };
        let units = &presentation.document.units;
        let unit = &units[unit_index];
        let mut endpoint = current;
        match intent {
            NavIntent::Left => {
                if current.offset > 0 {
                    endpoint.offset = if unit.atomic {
                        0
                    } else {
                        unit.text[..current.offset]
                            .grapheme_indices(true)
                            .next_back()
                            .map_or(0, |(byte, _)| byte)
                    };
                } else if let Some(previous) = unit_index.checked_sub(1).map(|i| &units[i]) {
                    endpoint = ReadingPosition {
                        unit_id: previous.id,
                        offset: previous.text.len(),
                    };
                }
                state.preferred_column = None;
            }
            NavIntent::Right => {
                if current.offset < unit.text.len() {
                    endpoint.offset = if unit.atomic {
                        unit.text.len()
                    } else {
                        current.offset
                            + unit.text[current.offset..]
                                .graphemes(true)
                                .next()
                                .map_or(0, str::len)
                    };
                } else if let Some(next) = units.get(unit_index + 1) {
                    endpoint = ReadingPosition {
                        unit_id: next.id,
                        offset: 0,
                    };
                }
                state.preferred_column = None;
            }
            NavIntent::Up | NavIntent::Down => {
                let (row, column) = display_position(&presentation, current);
                let column = *state.preferred_column.get_or_insert(column);
                let mut candidate = i64::from(row) + if intent == NavIntent::Up { -1 } else { 1 };
                while candidate >= 0 && candidate < i64::from(presentation.height()) {
                    if let Some(mut position) = presentation
                        .hit(Point {
                            x: column.min(presentation.width().saturating_sub(1)),
                            y: candidate as u32,
                        })
                        .and_then(|hit| hit.reading)
                    {
                        if let Some(unit) = presentation.document.unit(position.unit_id)
                            && unit.atomic
                            && intent == NavIntent::Down
                        {
                            position.offset = unit.text.len();
                        }
                        if position != current {
                            endpoint = position;
                            break;
                        }
                    }
                    candidate += if intent == NavIntent::Up { -1 } else { 1 };
                }
            }
            NavIntent::First => {
                if let Some(unit) = units.first() {
                    endpoint = ReadingPosition {
                        unit_id: unit.id,
                        offset: 0,
                    };
                }
                state.preferred_column = None;
            }
            NavIntent::Last => {
                if let Some(unit) = units.last() {
                    endpoint = ReadingPosition {
                        unit_id: unit.id,
                        offset: unit.text.len(),
                    };
                }
                state.preferred_column = None;
            }
            NavIntent::PageUp | NavIntent::PageDown => unreachable!(),
        }
        state.overlays.selection = Some(ReadingRange {
            start: selection.start,
            end: endpoint,
        });
        state.follow = Follow::Paused;
        state.navigated();
        let (row, column) = display_position(&presentation, endpoint);
        state.reveal = Some(Rect::new(column, row, 1, 1));
        ctx.reveal_anchor(RevealAlign::Nearest);
        state.notify()
    }

    /// Route wheel movement, object activation, and captured text selection.
    fn pointer(
        &mut self,
        event: &mouse::MouseEvent,
        ctx: &mut dyn Context,
    ) -> Result<EventOutcome> {
        if let Some(delta) = event.action.scroll_delta() {
            let before = ctx.view().scroll;
            self.scroll(ctx, ScrollOp::By(delta.x, delta.y))?;
            return Ok(if before != ctx.view().scroll {
                EventOutcome::Handle
            } else {
                EventOutcome::Ignore
            });
        }
        if event.button != mouse::Button::Left {
            return Ok(EventOutcome::Ignore);
        }
        if event.action == mouse::Action::Up {
            let mut state = self.state.borrow_mut();
            let handled = state.drag_anchor.take().is_some();
            state.dragging = false;
            ctx.release_mouse()?;
            state.notify()?;
            return Ok(if handled {
                EventOutcome::Handle
            } else {
                EventOutcome::Ignore
            });
        }
        if !matches!(event.action, mouse::Action::Down | mouse::Action::Drag) {
            return Ok(EventOutcome::Ignore);
        }
        let Some(presentation) = self.painted.as_ref().and_then(Weak::upgrade) else {
            return Ok(EventOutcome::Ignore);
        };
        let point =
            if event.action == mouse::Action::Drag && self.state.borrow().drag_anchor.is_some() {
                let mut point = ctx
                    .view()
                    .viewport_to_content(event.location)?
                    .clamped_point();
                point.x = point.x.min(presentation.width().saturating_sub(1));
                point.y = point.y.min(presentation.height().saturating_sub(1));
                Some(point)
            } else {
                ctx.view().content_point(event.location)
            };
        // A press on a row of a unit moves the block cursor to that unit.
        let mut marked = false;
        if event.action == mouse::Action::Down
            && let Some(point) = point
        {
            let mut state = self.state.borrow_mut();
            if state.mode == Mode::Interact
                && !state.awaiting
                && let Some(unit) = state
                    .cursor_map()
                    .and_then(|(_, rows)| cursor::at_row(&rows, point.y))
            {
                state.mark(unit, false);
                state.follow = Follow::Paused;
                state.navigated();
                ctx.set_focus(ctx.node_id())?;
                state.notify()?;
                marked = true;
            }
        }
        let ignore = if marked {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        };
        let Some(hit) = point.and_then(|point| presentation.hit(point)) else {
            return Ok(ignore);
        };
        let Some(position) = hit.reading else {
            return Ok(ignore);
        };
        let mut state = self.state.borrow_mut();
        if !state.presentation.as_ref().is_some_and(|p| {
            p.document.source.epoch == presentation.document.source.epoch
                && valid_position(&p.document, position)
        }) {
            return Ok(ignore);
        }
        if event.action == mouse::Action::Down {
            ctx.set_focus(ctx.node_id())?;
            state.drag_anchor = Some(position);
            state.dragging = false;
            state.preferred_column = None;
            if let Some(object) = hit.object {
                let activate =
                    state.mode == Mode::Interact && state.overlays.active_object == Some(object);
                state.mode = Mode::Interact;
                state.overlays.active_object = Some(object);
                if let Some(unit) = state.unit_of_reading(position.unit_id) {
                    state.mark(unit, true);
                }
                state.overlays.selection = None;
                state.follow = Follow::Paused;
                state.navigated();
                if activate {
                    state.pending_open = Some(object);
                    state.drag_anchor = None;
                    ctx.release_mouse()?;
                }
            }
            if state.drag_anchor.is_some() {
                ctx.capture_mouse()?;
                state.dragging = true;
            }
        } else if let Some(anchor) = state.drag_anchor {
            let mut end = position;
            if let Some(unit) = presentation.document.unit(position.unit_id)
                && unit.atomic
            {
                let key = |p: ReadingPosition| {
                    presentation
                        .document
                        .unit_position(p.unit_id)
                        .map(|i| (i, p.offset))
                };
                if key(position) >= key(anchor) {
                    end.offset = unit.text.len();
                }
            }
            state.overlays.selection = Some(ReadingRange { start: anchor, end });
            state.dragging = true;
            state.mode = Mode::Interact;
            state.follow = Follow::Paused;
            state.navigated();
            let (row, column) = display_position(&presentation, end);
            state.reveal = Some(Rect::new(column, row, 1, 1));
            ctx.reveal_anchor(RevealAlign::Nearest);
        } else {
            return Ok(EventOutcome::Ignore);
        }
        state.notify()?;
        Ok(EventOutcome::Handle)
    }
}

impl Widget for RenderedSurface {
    fn name(&self) -> NodeName {
        NodeName::convert("markdown_rendered")
    }
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        constraints.clamp(Size::new(1, 1))
    }
    fn canvas(&self, content: Size, _ctx: &CanvasContext) -> Size {
        self.state
            .borrow()
            .presentation
            .as_ref()
            .map_or(content, |p| {
                Size::new(content.w.max(p.width()), content.h.max(p.height()))
            })
    }
    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let mut state = self.state.borrow_mut();
        if state.width != view.content.w {
            state.width = view.content.w;
            state.notify()?;
        }
        let resized = self
            .observed_scroll
            .is_some_and(|(_, size)| size != view.content_size());
        if let Some((before, size)) = self.observed_scroll
            && size == view.content_size()
            && before != view.scroll
        {
            // A scroll seen only as the surface paints may be a restore's,
            // so it leaves a pending search reveal alone.
            let search_reveal = state.search_reveal;
            state.scrolled(&view);
            state.search_reveal = search_reveal;
            state.notify()?;
        }
        self.observed_scroll = Some((view.scroll, view.content_size()));
        if !ctx.has_mouse_capture() && state.drag_anchor.is_some() {
            state.drag_anchor = None;
            state.dragging = false;
        }
        // The block cursor keeps a unit in view. A resize or a new layout
        // keeps the marked unit, and the controller scrolls to it, unless the
        // view follows the tail; when no unit shows, it scrolls to the
        // nearest.
        let shown = frame(&view);
        let hidden = state
            .marked_unit()
            .is_some_and(|(_, rows)| !rows.is_empty() && !shown.shows(&rows));
        let laid_out = state.presentation.as_ref().is_some_and(|presentation| {
            self.painted.as_ref().map(Weak::as_ptr) != Some(Arc::as_ptr(presentation))
        });
        let revealing = state.cursor_scroll == Some(CursorScroll::Reveal);
        if (resized || laid_out) && hidden && shown.height > 0 && state.follow == Follow::Paused {
            state.cursor_scroll = Some(CursorScroll::Reveal);
            state.notify()?;
        } else if !revealing
            && let Some(step) = state.settle(shown)
            && step.top != view.scroll.y
        {
            state.cursor_scroll = Some(CursorScroll::Settle);
            state.notify()?;
        }
        state.overlays.block_rows = state.marked_unit().map(|(_, rows)| rows);
        if let Some(presentation) = state.presentation.as_ref() {
            presentation.paint(render, view.view_rect(), &state.overlays)?;
            self.painted = Some(Arc::downgrade(presentation));
            if view.content.w > 0 && view.content.h > 0 {
                self.pending_reading = None;
            }
        }
        Ok(())
    }
    fn accepts_intent(&self, intent: &str, ctx: &dyn ViewContext) -> bool {
        NavIntent::from_name(intent).is_some_and(|nav| {
            self.state.borrow().claims(nav)
                || ctx.scroll_outcome(ctx.node_id(), nav.scroll_op())
                    == Some(ChangeOutcome::Changed)
        })
    }
    fn on_intent(&mut self, intent: &str, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Some(nav) = NavIntent::from_name(intent)
            && self.accepts_intent(intent, ctx)
        {
            self.navigate(ctx, nav)?;
            return Ok(EventOutcome::Handle);
        }
        Ok(EventOutcome::Ignore)
    }
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        match event {
            Event::Mouse(event) => self.pointer(event, ctx),
            Event::FocusLost => {
                let mut state = self.state.borrow_mut();
                state.dragging = false;
                state.drag_anchor = None;
                ctx.release_mouse()?;
                Ok(EventOutcome::Ignore)
            }
            _ => Ok(EventOutcome::Ignore),
        }
    }
    fn reveal_anchor(&self, _content: Size) -> Option<Rect> {
        self.state.borrow().reveal
    }
    fn scroll_marks(&self, axis: ScrollAxis, _content: Size) -> Vec<ScrollMark> {
        let state = self.state.borrow();
        let Some(presentation) = state.presentation.as_ref() else {
            return Vec::new();
        };
        if axis != ScrollAxis::Vertical {
            return Vec::new();
        }
        // Matches arrive in reading order, so their rows ascend.
        view_search::marks(
            state
                .overlays
                .search
                .iter()
                .map(|range| presentation.row_for_unit(range.start.unit_id, range.start.offset)),
        )
    }
    fn on_unmount(&mut self, ctx: &mut dyn Context) {
        let mut state = self.state.borrow_mut();
        state.dragging = false;
        state.drag_anchor = None;
        drop(ctx.release_mouse());
    }
}

/// Check that an endpoint addresses an existing unit and UTF-8 boundary.
fn valid_position(document: &DocumentSnapshot, position: ReadingPosition) -> bool {
    document.unit(position.unit_id).is_some_and(|unit| {
        position.offset <= unit.text.len() && unit.text.is_char_boundary(position.offset)
    })
}

/// Compare borrowed projection pieces, including separators. This scans
/// selected bytes without allocating another copy of potentially large card
/// payloads.
fn same_projection(old: &DocumentSnapshot, next: &DocumentSnapshot, range: &ReadingRange) -> bool {
    /// Borrow selected projection slices and their inter-unit separators.
    fn pieces<'a>(
        document: &'a DocumentSnapshot,
        range: &ReadingRange,
    ) -> Option<impl Iterator<Item = &'a str>> {
        if !valid_position(document, range.start) || !valid_position(document, range.end) {
            return None;
        }
        let a = document.unit_position(range.start.unit_id)?;
        let b = document.unit_position(range.end.unit_id)?;
        let (start, end) = if (a, range.start.offset) <= (b, range.end.offset) {
            ((a, range.start.offset), (b, range.end.offset))
        } else {
            ((b, range.end.offset), (a, range.start.offset))
        };
        Some((start.0..=end.0).flat_map(move |index| {
            let unit = &document.units[index];
            let from = if index == start.0 && !unit.atomic {
                start.1
            } else {
                0
            };
            let to = if index == end.0 && !unit.atomic {
                end.1
            } else {
                unit.text.len()
            };
            [
                if index > start.0 {
                    document.separator(index)
                } else {
                    ""
                },
                &unit.text[from..to],
            ]
        }))
    }
    let (Some(old), Some(next)) = (pieces(old, range), pieces(next, range)) else {
        return false;
    };
    old.flat_map(str::bytes).eq(next.flat_map(str::bytes))
}

/// Locate an object's inline reading span, or the start of its card unit.
fn object_position(document: &DocumentSnapshot, object: &Object) -> Option<ReadingPosition> {
    let unit = document.unit(object.unit_id)?;
    let offset = unit
        .spans
        .iter()
        .find(|span| span.object == Some(object.id))
        .map_or(0, |span| span.range.start);
    Some(ReadingPosition {
        unit_id: unit.id,
        offset,
    })
}

/// Find the first prepared row of an object or its inline reading span.
fn object_row(presentation: &PresentationSnapshot, object: &Object) -> Option<u32> {
    let position = object_position(&presentation.document, object)?;
    let unit = presentation.document.unit(position.unit_id)?;
    let mut row = presentation.row_for_unit(unit.id, position.offset);
    if unit.atomic {
        while row > 0
            && presentation
                .row(row - 1)
                .is_some_and(|previous| previous.unit_id == unit.id)
        {
            row -= 1;
        }
    }
    Some(row)
}

/// Map a reading endpoint to its prepared display row and cell column.
fn display_position(presentation: &PresentationSnapshot, position: ReadingPosition) -> (u32, u32) {
    let y = presentation.row_for_unit(position.unit_id, position.offset);
    let Some(row) = presentation.row(y) else {
        return (y, 0);
    };
    let mut end = 0;
    for run in &row.runs {
        let Some(range) = &run.reading else {
            continue;
        };
        let mut x = run.x;
        for (byte, grapheme) in text::grapheme_indices(&run.text) {
            let offset = if run.text.len() == range.len() {
                range.start + byte
            } else {
                range.start
            };
            if offset >= position.offset {
                return (y, x);
            }
            x = x.saturating_add(grapheme_width(grapheme) as u32);
        }
        end = x;
    }
    (y, end)
}

#[cfg(test)]
mod tests {
    use canopy::{geom::PointI32, input::key, testing::harness::Harness};

    use super::*;
    use crate::{
        document::{ParseLimits, parse_document},
        presentation::{GeometryKey, Supplied},
        source_store::SourceStore,
    };

    fn prepare(
        store: &SourceStore,
        old: Option<&PresentationSnapshot>,
    ) -> Arc<PresentationSnapshot> {
        let document = Arc::new(parse_document(
            &store.snapshot(),
            &ParseLimits::default(),
            old.map(|p| p.document.as_ref()),
        ));
        Arc::new(PresentationSnapshot::prepare(
            document,
            GeometryKey {
                width: 20,
                ..Default::default()
            },
            old,
            &Supplied::default(),
        ))
    }
    fn mounted(text: &str) -> Result<(Harness, Rc<RefCell<SurfaceState>>)> {
        let store = SourceStore::static_text(text).unwrap();
        let state = Rc::new(RefCell::new(SurfaceState {
            presentation: Some(prepare(&store, None)),
            ..Default::default()
        }));
        let mut harness = Harness::builder(RenderedSurface::new(state.clone()))
            .size(20, 5)
            .build()?;
        harness.render()?;
        Ok((harness, state))
    }
    fn with<R>(
        h: &mut Harness,
        f: impl FnOnce(&mut RenderedSurface, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        h.with_root_widget_context(f)
    }
    fn pointer(action: mouse::Action, x: i32, y: i32) -> mouse::MouseEvent {
        mouse::MouseEvent {
            action,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32 { x, y },
        }
    }

    #[test]
    fn scroll_declines_boundaries_and_interact_consumes_them() -> Result<()> {
        let (mut h, state) = mounted("[one](https://one.test)\n\n[two](https://two.test)\n\n")?;
        with(&mut h, |surface, ctx| {
            assert!(!surface.accepts_intent(NavIntent::Up.name(), ctx));
            surface.enter_interact(ctx)?;
            assert!(surface.accepts_intent(NavIntent::Up.name(), ctx));
            let object = state.borrow().overlays.active_object;
            surface.navigate(ctx, NavIntent::Up)?;
            assert_eq!(state.borrow().overlays.active_object, object);
            assert!(surface.escape(ctx)?);
            assert_eq!(state.borrow().mode, Mode::Scroll);
            assert_eq!(
                state.borrow().overlays.active_object,
                None,
                "leaving Interact drops the object cursor, so nothing stays marked"
            );
            assert!(!surface.escape(ctx)?, "a second escape has nothing to end");
            Ok(())
        })
    }

    #[test]
    fn selection_moves_by_grapheme_and_escape_keeps_interact() -> Result<()> {
        let (mut h, state) = mounted("á🙂z\n")?;
        with(&mut h, |surface, ctx| {
            surface.start_selection(ctx)?;
            surface.navigate(ctx, NavIntent::Right)?;
            assert_eq!(
                state
                    .borrow()
                    .overlays
                    .selection
                    .as_ref()
                    .unwrap()
                    .end
                    .offset,
                "á".len()
            );
            surface.navigate(ctx, NavIntent::Right)?;
            assert_eq!(
                state
                    .borrow()
                    .overlays
                    .selection
                    .as_ref()
                    .unwrap()
                    .end
                    .offset,
                "á🙂".len()
            );
            surface.escape(ctx)?;
            assert!(state.borrow().overlays.selection.is_none());
            assert_eq!(state.borrow().mode, Mode::Interact);
            surface.escape(ctx)?;
            assert_eq!(state.borrow().mode, Mode::Scroll);
            Ok(())
        })
    }

    #[test]
    fn publication_uses_latest_selection_and_detects_changed_projection() -> Result<()> {
        let mut store = SourceStore::stream();
        store.append("alpha\n\nbeta\n\n").unwrap();
        let old = prepare(&store, None);
        let mut next_document = parse_document(
            &store.snapshot(),
            &ParseLimits::default(),
            Some(&old.document),
        );
        let id = next_document.units[0].id;
        let state = Rc::new(RefCell::new(SurfaceState {
            presentation: Some(old.clone()),
            ..Default::default()
        }));
        let mut h = Harness::builder(RenderedSurface::new(state.clone()))
            .size(20, 5)
            .build()?;
        h.render()?;
        // The selection is changed after preparation began, before publication.
        state.borrow_mut().overlays.selection = Some(ReadingRange {
            start: ReadingPosition {
                unit_id: id,
                offset: 0,
            },
            end: ReadingPosition {
                unit_id: id,
                offset: 5,
            },
        });
        with(&mut h, |surface, ctx| {
            assert!(!surface.publish(
                ctx,
                Arc::new(PresentationSnapshot::prepare(
                    Arc::new(next_document.clone()),
                    old.geometry,
                    Some(&old),
                    &Supplied::default()
                ))
            )?);
            next_document.units[0].text = "other".into();
            assert!(surface.publish(
                ctx,
                Arc::new(PresentationSnapshot::prepare(
                    Arc::new(next_document),
                    old.geometry,
                    None,
                    &Supplied::default()
                ))
            )?);
            assert!(state.borrow().overlays.selection.is_none());
            Ok(())
        })
    }

    #[test]
    fn pointer_drag_captures_and_second_object_activation_requests_inspector() -> Result<()> {
        let (mut h, state) = mounted("[one](https://one.test)\n\nplain text\n")?;
        let point = {
            let state = state.borrow();
            let p = state.presentation.as_ref().unwrap();
            (0..p.height())
                .find_map(|y| {
                    (0..p.width()).find_map(|x| {
                        p.hit(Point { x, y })
                            .filter(|hit| hit.object.is_some())
                            .map(|_| (x, y))
                    })
                })
                .unwrap()
        };
        h.mouse(pointer(mouse::Action::Down, point.0 as i32, point.1 as i32))?;
        assert_eq!(state.borrow().mode, Mode::Interact);
        assert!(state.borrow().pending_open.is_none());
        h.mouse(pointer(mouse::Action::Up, point.0 as i32, point.1 as i32))?;
        h.mouse(pointer(mouse::Action::Down, point.0 as i32, point.1 as i32))?;
        assert!(state.borrow().pending_open.is_some());
        with(&mut h, |_, ctx| {
            assert!(!ctx.has_mouse_capture());
            Ok(())
        })?;
        state.borrow_mut().pending_open = None;
        h.mouse(pointer(mouse::Action::Down, 2, 2))?;
        h.mouse(pointer(mouse::Action::Drag, 6, 2))?;
        assert!(state.borrow().dragging);
        assert!(state.borrow().overlays.selection.is_some());
        h.mouse(pointer(mouse::Action::Up, 6, 2))?;
        assert!(!state.borrow().dragging);
        with(&mut h, |_, ctx| {
            assert!(!ctx.has_mouse_capture());
            Ok(())
        })
    }

    #[test]
    fn entering_interact_and_blocked_scroll_do_not_pause_follow() -> Result<()> {
        let (mut h, state) = mounted("[one](https://one.test)\n")?;
        state.borrow_mut().follow = Follow::Following;
        with(&mut h, |surface, ctx| {
            surface.enter_interact(ctx)?;
            assert_eq!(state.borrow().follow, Follow::Following);
            surface.navigate(ctx, NavIntent::PageDown)?;
            assert_eq!(state.borrow().follow, Follow::Following);
            surface.start_selection(ctx)?;
            assert_eq!(state.borrow().follow, Follow::Paused);
            Ok(())
        })
    }

    #[test]
    fn viewport_identity_restores_inside_a_formatted_wrapped_paragraph() -> Result<()> {
        let text = format!("**{}**\n", "long formatted paragraph ".repeat(30));
        let (mut h, _) = mounted(&text)?;
        let (anchor, offset) = with(&mut h, |surface, ctx| {
            ctx.scroll(ScrollOp::To(Point { x: 0, y: 8 }));
            let scroll = ctx.view().scroll;
            let anchor = surface.viewport_anchor(scroll).unwrap();
            assert!(anchor.0.offset > 0);
            Ok((anchor, scroll.y))
        })?;
        with(&mut h, |surface, ctx| {
            ctx.scroll(ScrollOp::To(Point { x: 0, y: u32::MAX }));
            assert!(surface.restore_viewport_anchor(ctx, anchor.0, anchor.1)?);
            Ok(())
        })?;
        h.render()?;
        with(&mut h, |surface, ctx| {
            assert_eq!(ctx.view().scroll.y, offset);
            assert_eq!(surface.viewport_anchor(ctx.view().scroll), Some(anchor));
            assert!(!surface.restore_viewport_anchor(
                ctx,
                ReadingPosition {
                    unit_id: u64::MAX,
                    offset: 0
                },
                0
            )?);
            Ok(())
        })
    }

    #[test]
    fn layout_eviction_preserves_semantic_selection_and_interaction() -> Result<()> {
        let (mut h, state) = mounted("[one](https://one.test)\n\nalpha\n")?;
        let old = state.borrow().presentation.clone().unwrap();
        with(&mut h, |surface, ctx| {
            surface.publish(ctx, old.clone())?;
            surface.enter_interact(ctx)?;
            surface.start_selection(ctx)?;
            surface.navigate(ctx, NavIntent::Right)?;
            Ok(())
        })?;
        let (selection, object, navigation) = {
            let mut state = state.borrow_mut();
            state.presentation = None;
            (
                state.overlays.selection.clone(),
                state.overlays.active_object,
                state.navigation,
            )
        };
        let document = state.borrow().document.clone().unwrap();
        let next = Arc::new(PresentationSnapshot::prepare(
            document.clone(),
            GeometryKey {
                width: 12,
                ..old.geometry
            },
            None,
            &Supplied::default(),
        ));
        with(&mut h, |surface, ctx| {
            assert!(!surface.publish(ctx, next)?);
            let state = state.borrow();
            assert_eq!(state.mode, Mode::Interact);
            assert_eq!(state.overlays.selection, selection);
            assert_eq!(state.overlays.active_object, object);
            assert_eq!(state.navigation, navigation);
            assert!(Arc::ptr_eq(state.document.as_ref().unwrap(), &document));
            Ok(())
        })
    }
}
