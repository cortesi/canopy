//! Proportional scrollbars that widgets draw and drive.
//!
//! A [`Scrollbar`] measures one axis of a scrolling view. Its thumb spans the
//! visible fraction of the canvas and sits at the scroll offset. A widget draws
//! the scrollbar into a track in its own outer coordinates and passes mouse
//! events to it. A press on the track centers the thumb on the pointer, and a
//! drag keeps the thumb under the pointer.

use canopy::{
    Context, EventOutcome, Render, View,
    error::Result,
    event::mouse,
    geom::{Point, Rect},
};

/// The axis a scrollbar measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    /// Rows, with a track that runs top to bottom.
    Vertical,
    /// Columns, with a track that runs left to right.
    Horizontal,
}

/// A proportional scrollbar for one axis of a view.
#[derive(Clone, Copy, Debug)]
pub struct Scrollbar {
    /// Axis the scrollbar measures.
    axis: Axis,
    /// Style path and glyph of the thumb.
    thumb: (&'static str, char),
    /// Style path and glyph of the track around the thumb, when drawn.
    track: Option<(&'static str, char)>,
    /// Pointer offset within the thumb while a drag is in progress.
    grip: Option<u32>,
}

impl Scrollbar {
    /// Construct a vertical scrollbar whose thumb uses `style` and `glyph`.
    pub const fn vertical(style: &'static str, glyph: char) -> Self {
        Self {
            axis: Axis::Vertical,
            thumb: (style, glyph),
            track: None,
            grip: None,
        }
    }

    /// Construct a horizontal scrollbar whose thumb uses `style` and `glyph`.
    pub const fn horizontal(style: &'static str, glyph: char) -> Self {
        Self {
            axis: Axis::Horizontal,
            thumb: (style, glyph),
            track: None,
            grip: None,
        }
    }

    /// Also draw the track around the thumb with `style` and `glyph`.
    #[must_use]
    pub const fn with_track(mut self, style: &'static str, glyph: char) -> Self {
        self.track = Some((style, glyph));
        self
    }

    /// Return the parts of `track` before, under, and after the thumb, or
    /// `None` when `view` shows its whole canvas along this axis.
    fn parts(&self, view: &View, track: Rect) -> Option<(Rect, Rect, Rect)> {
        let parts = match self.axis {
            Axis::Vertical => view.vactive(track),
            Axis::Horizontal => view.hactive(track),
        };
        parts.ok().flatten()
    }

    /// Draw the scrollbar of `view` into `track`.
    pub fn render(&self, render: &mut Render, view: &View, track: Rect) -> Result<()> {
        let Some((before, thumb, after)) = self.parts(view, track) else {
            return Ok(());
        };
        if let Some((style, glyph)) = self.track {
            render.fill(style, before, glyph)?;
            render.fill(style, after, glyph)?;
        }
        render.fill(self.thumb.0, thumb, self.thumb.1)
    }

    /// Handle a mouse event for the scrollbar of `view`, drawn in `track`.
    ///
    /// `track` is in the outer coordinates of the widget that receives the
    /// event. A press on the track captures the mouse for that widget and the
    /// release frees it. `scroll_to` moves the node that owns `view` to an
    /// `(x, y)` scroll offset. Returns [`EventOutcome::Handle`] for events the
    /// scrollbar uses.
    pub fn handle_mouse(
        &mut self,
        context: &mut dyn Context,
        event: &mouse::MouseEvent,
        view: &View,
        track: Rect,
        scroll_to: impl FnOnce(&mut dyn Context, u32, u32) -> Result<()>,
    ) -> Result<EventOutcome> {
        // A captured drag can leave the widget, so clamp it to the widget's
        // edges.
        let pointer = context
            .view()
            .viewport_to_outer(event.location)?
            .clamped_point();
        match event.action {
            mouse::Action::Down if event.button == mouse::Button::Left => {
                let Some((_, thumb, _)) = self.parts(view, track) else {
                    return Ok(EventOutcome::Ignore);
                };
                if !track.contains_point(pointer) {
                    return Ok(EventOutcome::Ignore);
                }
                let (position, thumb_start, thumb_len) = self.along(pointer, track, thumb);
                let on_thumb = thumb.contains_point(pointer);
                let grip = if on_thumb {
                    position.saturating_sub(thumb_start)
                } else {
                    thumb_len / 2
                };
                self.grip = Some(grip);
                context.capture_mouse()?;
                if !on_thumb {
                    let start = position.saturating_sub(grip);
                    self.scroll(context, view, track, thumb_len, start, scroll_to)?;
                }
                Ok(EventOutcome::Handle)
            }
            mouse::Action::Drag => {
                let Some(grip) = self.grip else {
                    return Ok(EventOutcome::Ignore);
                };
                if let Some((_, thumb, _)) = self.parts(view, track) {
                    let (position, _, thumb_len) = self.along(pointer, track, thumb);
                    let start = position.saturating_sub(grip);
                    self.scroll(context, view, track, thumb_len, start, scroll_to)?;
                }
                Ok(EventOutcome::Handle)
            }
            mouse::Action::Up if event.button == mouse::Button::Left && self.grip.is_some() => {
                self.grip = None;
                context.release_mouse()?;
                Ok(EventOutcome::Handle)
            }
            _ => Ok(EventOutcome::Ignore),
        }
    }

    /// Return the pointer position, thumb start, and thumb length along this
    /// axis, measured from the start of `track`.
    fn along(&self, pointer: Point, track: Rect, thumb: Rect) -> (u32, u32, u32) {
        match self.axis {
            Axis::Vertical => (
                pointer.y.saturating_sub(track.tl.y),
                thumb.tl.y.saturating_sub(track.tl.y),
                thumb.h,
            ),
            Axis::Horizontal => (
                pointer.x.saturating_sub(track.tl.x),
                thumb.tl.x.saturating_sub(track.tl.x),
                thumb.w,
            ),
        }
    }

    /// Scroll `view` so its thumb starts `thumb_start` cells into `track`.
    fn scroll(
        &self,
        context: &mut dyn Context,
        view: &View,
        track: Rect,
        thumb_len: u32,
        thumb_start: u32,
        scroll_to: impl FnOnce(&mut dyn Context, u32, u32) -> Result<()>,
    ) -> Result<()> {
        match self.axis {
            Axis::Vertical => {
                let offset = offset_for_thumb_start(
                    thumb_start,
                    track.h,
                    thumb_len,
                    view.canvas.h,
                    view.content.h,
                );
                scroll_to(context, view.scroll.x, offset)
            }
            Axis::Horizontal => {
                let offset = offset_for_thumb_start(
                    thumb_start,
                    track.w,
                    thumb_len,
                    view.canvas.w,
                    view.content.w,
                );
                scroll_to(context, offset, view.scroll.y)
            }
        }
    }
}

/// Return the scroll offset whose thumb starts `thumb_start` cells into the
/// track.
///
/// This inverts the thumb placement of [`View::vactive`], so a dragged thumb
/// stays under the pointer. A thumb at the end of the track reaches the last
/// offset, which the rounded-up thumb length could otherwise leave out.
fn offset_for_thumb_start(
    thumb_start: u32,
    track_len: u32,
    thumb_len: u32,
    canvas_len: u32,
    view_len: u32,
) -> u32 {
    let max_offset = canvas_len.saturating_sub(view_len);
    if track_len == 0 || max_offset == 0 {
        return 0;
    }
    if thumb_start >= track_len.saturating_sub(thumb_len) {
        return max_offset;
    }
    let offset = (u64::from(thumb_start) * u64::from(canvas_len)).div_ceil(u64::from(track_len));
    u32::try_from(offset).unwrap_or(u32::MAX).min(max_offset)
}

#[cfg(test)]
mod tests {
    use canopy::geom::{RectI32, Size};
    use proptest::prelude::*;

    use super::*;

    /// Return the thumb of a vertical track for a view scrolled to `offset`.
    fn thumb(track_len: u32, canvas_len: u32, view_len: u32, offset: u32) -> Rect {
        let view = View::new(
            RectI32::new(0, 0, 1, view_len),
            RectI32::new(0, 0, 1, view_len),
            Point { x: 0, y: offset },
            Size::new(1, canvas_len),
        );
        Scrollbar::vertical("thumb", '#')
            .parts(&view, Rect::new(0, 0, 1, track_len))
            .expect("the canvas is taller than the view")
            .1
    }

    proptest! {
        #[test]
        fn a_dragged_thumb_stays_under_the_pointer_and_reaches_the_end(
            track_len in 1u32..40,
            view_len in 1u32..40,
            extra in 1u32..200,
            start in 0u32..40,
        ) {
            // Once the canvas is at least as long as the track, each thumb
            // start has an offset that places the thumb exactly there.
            let canvas_len = (view_len + extra).max(track_len);
            let thumb_len = thumb(track_len, canvas_len, view_len, 0).h;
            let last_start = track_len - thumb_len;
            let start = start.min(last_start);

            let offset = offset_for_thumb_start(start, track_len, thumb_len, canvas_len, view_len);
            prop_assert!(offset <= canvas_len - view_len);
            prop_assert_eq!(thumb(track_len, canvas_len, view_len, offset).tl.y, start);
            if start == last_start {
                prop_assert_eq!(offset, canvas_len - view_len);
            }
        }
    }

    #[test]
    fn a_view_that_fits_has_no_scrollbar_and_no_offset() {
        let view = View::new(
            RectI32::new(0, 0, 1, 5),
            RectI32::new(0, 0, 1, 5),
            Point::ZERO,
            Size::new(1, 5),
        );
        assert!(
            Scrollbar::vertical("thumb", '#')
                .parts(&view, Rect::new(0, 0, 1, 5))
                .is_none()
        );
        assert_eq!(offset_for_thumb_start(3, 5, 5, 5, 5), 0);
        assert_eq!(offset_for_thumb_start(0, 0, 0, 30, 5), 0);
    }
}
