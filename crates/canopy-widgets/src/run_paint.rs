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
    text::{grapheme_width, tab_width},
};
use unicode_segmentation::UnicodeSegmentation;

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
    for grapheme in text.graphemes(true) {
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
