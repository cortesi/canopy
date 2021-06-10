pub mod editor;
pub mod frame;
pub mod panes;
pub mod scroll;
pub mod text;

use crate::geom;
use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    style::{Color, Print, SetForegroundColor},
    QueueableCommand,
};
use std::io::Write;

/// Draw a solid block
pub fn block(w: &mut dyn Write, r: geom::Rect, col: Color, c: char) -> Result<()> {
    let line = c.to_string().repeat(r.w as usize);
    w.queue(SetForegroundColor(col))?;
    for n in 0..r.h {
        w.queue(MoveTo(r.tl.x, r.tl.y + n))?;
        w.queue(Print(&line))?;
    }
    Ok(())
}

/// Draw a solid frame
pub fn solid_frame(w: &mut dyn Write, f: geom::Frame, col: Color, c: char) -> Result<()> {
    block(w, f.top, col, c)?;
    block(w, f.left, col, c)?;
    block(w, f.right, col, c)?;
    block(w, f.bottom, col, c)?;
    Ok(())
}
