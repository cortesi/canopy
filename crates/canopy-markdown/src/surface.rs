//! Rendered input and reader state around immutable prepared geometry.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Weak},
};

use canopy::{
    ChangeOutcome, Context, EventOutcome, NodeName, ViewContext, Widget,
    error::Result,
    geom::{Point, Rect, Size},
    input::{Event, NavIntent, mouse},
    layout::{CanvasContext, Layout, MeasureConstraints, Measurement, RevealAlign, ScrollOp, View},
    render::Render,
    runtime::NodeWakeHandle,
    text::grapheme_width,
    tree::{FocusDirection, FocusScope},
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    document::{DocumentSnapshot, Object, ReadingPosition, ReadingRange},
    options::{Follow, Mode},
    presentation::{PaintOverlays, PresentationSnapshot},
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
    /// Fixed reading endpoint for the current pointer gesture.
    drag_anchor: Option<ReadingPosition>,
    /// Retained display column for vertical selection movement.
    preferred_column: Option<u32>,
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

    /// Supersede any queued reveal after explicit reader navigation.
    fn navigated(&mut self) {
        self.navigation = self.navigation.wrapping_add(1);
        self.reveal = None;
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
        let anchor = self
            .pending_reading
            .filter(|pending| pending.0 == navigation)
            .map(|(_, position, source, row)| (position, source, row))
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
            state.overlays = PaintOverlays::default();
            state.mode = Mode::Scroll;
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
        state.document = Some(next.document.clone());
        state.presentation = Some(next);
        if let Some((row, viewport_row)) = target {
            state.reveal = Some(Rect::new(0, row, 1, 1));
            ctx.reveal_anchor(viewport_row.map_or(RevealAlign::Nearest, RevealAlign::Top));
        }
        self.observed_scroll = None;
        Ok(cleared)
    }

    /// Enter object navigation without changing the follow policy.
    pub fn enter_interact(&self, ctx: &dyn ViewContext) -> Result<()> {
        let mut state = self.state.borrow_mut();
        state.mode = Mode::Interact;
        if let Some(presentation) = state.presentation.clone() {
            let visible = ctx.view().view_rect();
            let objects = &presentation.document.objects;
            let remembered_visible = objects
                .iter()
                .find(|o| Some(o.id) == state.overlays.active_object)
                .and_then(|o| object_row(&presentation, o))
                .is_some_and(|row| {
                    row >= visible.tl.y && row < visible.tl.y.saturating_add(visible.h)
                });
            if !remembered_visible {
                state.overlays.active_object = objects
                    .iter()
                    .find(|o| object_row(&presentation, o).is_some_and(|row| row >= visible.tl.y))
                    .or_else(|| presentation.document.objects.last())
                    .map(|o| o.id);
            }
        }
        state.notify()
    }

    /// Start selection at the object boundary or current reading anchor.
    pub fn start_selection(&self, ctx: &dyn ViewContext) -> Result<()> {
        let reading = self.reading_anchor(ctx).map(|a| a.0);
        let mut state = self.state.borrow_mut();
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
            if let Some(position) = object_position.or(reading) {
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

    /// Clear selection first, otherwise return to scrolling; release capture.
    pub fn escape(&self, ctx: &mut dyn Context) -> Result<bool> {
        let mut state = self.state.borrow_mut();
        let selected = state.overlays.selection.take().is_some();
        let handled = selected
            || state.mode == Mode::Interact
            || state.drag_anchor.is_some()
            || state.overlays.active_object.is_some();
        if !handled {
            return Ok(false);
        }
        if !selected {
            state.leave_interact();
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
        let selection = self.state.borrow().overlays.selection.is_some();
        if selection && !matches!(intent, NavIntent::PageUp | NavIntent::PageDown) {
            return self.move_selection(ctx, intent);
        }
        if self.state.borrow().mode == Mode::Interact
            && matches!(
                intent,
                NavIntent::Up | NavIntent::Down | NavIntent::First | NavIntent::Last
            )
        {
            if matches!(intent, NavIntent::First | NavIntent::Last) {
                let mut state = self.state.borrow_mut();
                let target = state.presentation.as_ref().and_then(|p| {
                    if intent == NavIntent::First {
                        p.document.objects.first()
                    } else {
                        p.document.objects.last()
                    }
                    .map(|o| o.id)
                });
                if target == state.overlays.active_object {
                    return Ok(());
                }
                state.overlays.active_object = target;
                state.follow = Follow::Paused;
                state.navigated();
                drop(state);
                return self.reveal_object(ctx);
            }
            return self.object_next(ctx, if intent == NavIntent::Up { -1 } else { 1 }, false);
        }
        self.scroll(ctx, nav_scroll(intent))
    }

    /// Visit an adjacent object, optionally traversing focus at a boundary.
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
        let objects = &presentation.document.objects;
        let current = objects
            .iter()
            .position(|o| Some(o.id) == state.overlays.active_object);
        let target = match current {
            Some(index) if delta < 0 => index.checked_sub(1),
            Some(index) => (index + 1 < objects.len()).then_some(index + 1),
            None if delta < 0 => objects.len().checked_sub(1),
            None => (!objects.is_empty()).then_some(0),
        };
        if let Some(index) = target {
            state.overlays.active_object = Some(objects[index].id);
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
        let mut state = self.state.borrow_mut();
        if before != view.scroll {
            state.scrolled(&view);
        }
        self.observed_scroll = Some((view.scroll, view.content_size()));
        state.notify()
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
        let Some(hit) = point.and_then(|point| presentation.hit(point)) else {
            return Ok(EventOutcome::Ignore);
        };
        let Some(position) = hit.reading else {
            return Ok(EventOutcome::Ignore);
        };
        let mut state = self.state.borrow_mut();
        if !state.presentation.as_ref().is_some_and(|p| {
            p.document.source.epoch == presentation.document.source.epoch
                && valid_position(&p.document, position)
        }) {
            return Ok(EventOutcome::Ignore);
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
        if let Some((before, size)) = self.observed_scroll
            && size == view.content_size()
            && before != view.scroll
        {
            state.scrolled(&view);
            state.notify()?;
        }
        self.observed_scroll = Some((view.scroll, view.content_size()));
        if !ctx.has_mouse_capture() && state.drag_anchor.is_some() {
            state.drag_anchor = None;
            state.dragging = false;
        }
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
                || ctx.scroll_outcome(ctx.node_id(), nav_scroll(nav))
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
    fn on_unmount(&mut self, ctx: &mut dyn Context) {
        let mut state = self.state.borrow_mut();
        state.dragging = false;
        state.drag_anchor = None;
        drop(ctx.release_mouse());
    }
}

/// Translate navigation into explicit local viewport operations.
pub fn nav_scroll(intent: NavIntent) -> ScrollOp {
    match intent {
        NavIntent::Up => ScrollOp::By(0, -1),
        NavIntent::Down => ScrollOp::By(0, 1),
        NavIntent::Left => ScrollOp::By(-1, 0),
        NavIntent::Right => ScrollOp::By(1, 0),
        NavIntent::PageUp => ScrollOp::pages(-1),
        NavIntent::PageDown => ScrollOp::pages(1),
        NavIntent::First => ScrollOp::To(Point::ZERO),
        NavIntent::Last => ScrollOp::To(Point { x: 0, y: u32::MAX }),
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
        for (byte, grapheme) in run.text.grapheme_indices(true) {
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
