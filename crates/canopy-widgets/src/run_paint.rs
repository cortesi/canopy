//! A shared painter for one line of styled, tab-expanded, horizontally
//! clipped text.
//!
//! [`paint_run`] is the grapheme walk the Editor's line loop and
//! [`crate::diff_view::DiffView`]'s code lines both need: it expands tabs,
//! clips to a horizontal window, and paints each visible cell with the style
//! its caller resolves. It carries no highlighting of its own; a caller
//! layers that on by resolving each grapheme's style itself, merging any
//! highlight spans it holds.

use canopy::{
    error::Result,
    geom::{Point, Rect},
    render::Render,
    style::Style,
    text::{self, grapheme_width, tab_width},
};

/// Convert a `usize` cell count to the coordinate type, saturating.
pub fn column(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Paint one line's visible graphemes.
///
/// `tab_col` is the column tab stops measure from: `text`'s own display
/// column, unaffected by scrolling or a fixed gutter. `screen_col` is the
/// column clipping and painting measure from; it advances in lockstep with
/// `tab_col` by the same per-grapheme cell counts, but can start from a
/// different origin, such as past a gutter that `tab_col` does not count.
/// `start_char` is the char index `text`'s first grapheme starts at, in
/// whatever larger text `style_at`'s ranges are reported against.
///
/// A grapheme paints only while its `screen_col` range overlaps
/// `[window_start, window_start + window_width)`; a visible grapheme paints
/// at `screen_x + (its screen_col - window_start)`, row `screen_y`.
/// `style_at` receives the renderer and a visible grapheme's char range,
/// `[g_start, g_end)`, and returns the style to paint it with.
#[expect(clippy::too_many_arguments, reason = "one render step")]
pub fn paint_run(
    render: &mut Render,
    text: &str,
    tab_col: u32,
    screen_col: u32,
    start_char: usize,
    tab_stop: usize,
    window_start: u32,
    window_width: u32,
    screen_x: u32,
    screen_y: u32,
    line_rect: Rect,
    mut style_at: impl FnMut(&Render, usize, usize) -> Style,
) -> Result<()> {
    let mut tab_col = tab_col;
    let mut screen_col = screen_col;
    let mut char_index = start_char;
    for grapheme in text::graphemes(text) {
        let chars = grapheme.chars().count();
        let g_start = char_index;
        let g_end = char_index.saturating_add(chars);
        let cells = if grapheme == "\t" {
            column(tab_width(tab_col as usize, tab_stop))
        } else {
            column(grapheme_width(grapheme))
        };
        let start_screen = screen_col;
        let end_screen = screen_col.saturating_add(cells);
        tab_col = tab_col.saturating_add(cells);
        screen_col = end_screen;
        char_index = g_end;
        if end_screen <= window_start {
            continue;
        }
        let draw = start_screen.saturating_sub(window_start);
        if draw >= window_width {
            break;
        }
        let style = style_at(render, g_start, g_end);
        let point = Point {
            x: screen_x.saturating_add(draw),
            y: screen_y,
        };
        if grapheme == "\t" {
            for offset in 0..cells {
                let position = draw.saturating_add(offset);
                if position >= window_width {
                    break;
                }
                let cell = Point {
                    x: screen_x.saturating_add(position),
                    y: screen_y,
                };
                render.put_cell(style.resolve_at(line_rect, cell), cell, ' ')?;
            }
        } else {
            render.put_grapheme(style.resolve_at(line_rect, point), point, grapheme)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use canopy::{ViewContext, Widget, testing::harness::Harness};

    use super::*;

    /// Paints one run on row 0, and records the char range of each grapheme
    /// it styles.
    struct Run {
        /// The text of the run.
        text: &'static str,
        /// First visible screen column.
        window_start: u32,
        /// Visible screen columns.
        window_width: u32,
        /// Cell the window starts at.
        screen_x: u32,
        /// The char ranges `style_at` received, shared with the test.
        styled: Rc<RefCell<Vec<(usize, usize)>>>,
    }

    impl Widget for Run {
        fn render(&mut self, render: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            let styled = Rc::clone(&self.styled);
            paint_run(
                render,
                self.text,
                0,
                0,
                0,
                4,
                self.window_start,
                self.window_width,
                self.screen_x,
                0,
                Rect::new(0, 0, 12, 1),
                move |render, start, end| {
                    styled.borrow_mut().push((start, end));
                    render.resolve_style("text")
                },
            )
        }
    }

    /// The first row a `Run` paints, and the ranges it styles.
    fn painted(
        text: &'static str,
        window_start: u32,
        window_width: u32,
        screen_x: u32,
    ) -> Result<(String, Vec<(usize, usize)>)> {
        let styled = Rc::default();
        let mut harness = Harness::builder(Run {
            text,
            window_start,
            window_width,
            screen_x,
            styled: Rc::clone(&styled),
        })
        .size(12, 1)
        .build()?;
        styled.borrow_mut().clear();
        harness.render()?;
        let row = harness.tbuf().lines().remove(0);
        Ok((row, styled.take()))
    }

    #[test]
    fn a_run_expands_tabs_to_the_next_stop() -> Result<()> {
        let (row, styled) = painted("a\tb", 0, 12, 1)?;
        assert_eq!(row, " a   b      ");
        assert_eq!(styled, [(0, 1), (1, 2), (2, 3)]);
        Ok(())
    }

    #[test]
    fn a_run_paints_only_the_graphemes_in_its_window() -> Result<()> {
        let (row, styled) = painted("abcdef", 2, 3, 0)?;
        assert_eq!(row, "cde         ");
        assert_eq!(styled, [(2, 3), (3, 4), (4, 5)]);
        Ok(())
    }

    #[test]
    fn a_tab_stops_at_the_window_edge() -> Result<()> {
        let (row, _) = painted("\tz", 0, 2, 0)?;
        // The tab takes four cells, and the window shows two of them.
        assert_eq!(row, "            ");
        let (row, _) = painted("\tz", 0, 5, 0)?;
        assert_eq!(row, "    z       ");
        Ok(())
    }

    #[test]
    fn column_saturates() {
        assert_eq!(column(7), 7);
        assert_eq!(column(usize::MAX), u32::MAX);
    }
}
