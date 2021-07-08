pub mod editor;
pub mod frame;
pub mod input;
pub mod panes;
pub mod scroll;
pub mod text;

use crate::{error::CanopyError, geom};
use crossterm::{cursor::MoveTo, style::Print, QueueableCommand};
use std::io::Write;

/// Draw a solid block
pub fn block(w: &mut dyn Write, r: geom::Rect, c: char) -> Result<(), CanopyError> {
    let line = c.to_string().repeat(r.w as usize);
    for n in 0..r.h {
        w.queue(MoveTo(r.tl.x, r.tl.y + n))?;
        w.queue(Print(&line))?;
    }
    Ok(())
}

/// Draw a solid frame
pub fn solid_frame(w: &mut dyn Write, f: geom::Frame, c: char) -> Result<(), CanopyError> {
    block(w, f.top, c)?;
    block(w, f.left, c)?;
    block(w, f.right, c)?;
    block(w, f.bottom, c)?;
    Ok(())
}
