use crate::{geom, style::Color, style::Style, Result};

pub mod term;

pub trait Backend {
    fn fg(&mut self, c: Color) -> Result<()>;
    fn bg(&mut self, c: Color) -> Result<()>;
    fn fill(&mut self, r: geom::Rect, c: char) -> Result<()>;
    fn text(&mut self, loc: geom::Point, txt: &str) -> Result<()>;
}

pub struct Render<'a> {
    backend: &'a mut dyn Backend,
    style: Style,
}

impl<'a> Render<'a> {
    pub fn new(backend: &mut dyn Backend, style: Style) -> Render {
        Render { backend, style }
    }

    /// Fill a rectangle with a specified character.
    pub fn fill(&mut self, color: &str, r: geom::Rect, c: char) -> Result<()> {
        self.backend.fg(self.style.fg(color))?;
        self.backend.fill(r, c)
    }

    /// Print text in the first line of the specified rectangle. If the text is
    /// wider than the rectangle, it will be truncated; if it is shorter, it
    /// will be padded.
    pub fn text(&mut self, color: &str, r: geom::Rect, txt: &str) -> Result<()> {
        self.backend.fg(self.style.fg(color))?;
        self.backend.bg(self.style.bg(color))?;
        if txt.len() >= r.w as usize {
            self.backend.text(r.tl, &txt[..r.w as usize])
        } else {
            self.backend.text(r.tl, txt)?;
            self.backend
                .text(r.tl, &" ".repeat(r.w as usize - txt.len()))
        }
    }
    pub fn push(&mut self) {
        self.style.push();
    }
    pub fn pop(&mut self) {
        self.style.pop();
    }
}
