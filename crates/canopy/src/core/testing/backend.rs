use crate::{error::Result, geom::Point, render::RenderBackend, style::ResolvedStyle};

/// A render backend for testing, which logs the text it is asked to draw.
#[derive(Default)]
pub struct TestRender {
    /// Captured text fragments, in draw order.
    pub text: Vec<String>,
    /// Location and style of every text write since the last reset.
    pub writes: Vec<(Point, ResolvedStyle)>,
    /// Where the last emission parked the hidden terminal cursor.
    pub parked: Option<Point>,
    /// Style that applies to the next write.
    style: Option<ResolvedStyle>,
}

impl TestRender {
    /// Construct a backend with an empty capture buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return true if no text has been captured.
    pub fn buf_empty(&self) -> bool {
        self.text.is_empty()
    }
}

impl RenderBackend for TestRender {
    fn reset(&mut self) -> Result<()> {
        self.text.clear();
        self.writes.clear();
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn style(&mut self, s: &ResolvedStyle) -> Result<()> {
        self.style = Some(*s);
        Ok(())
    }

    fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
        if let Some(style) = self.style {
            self.writes.push((loc, style));
        }
        let txt = txt.trim();
        if !txt.is_empty() {
            self.text.push(txt.into());
        }
        Ok(())
    }

    fn park_cursor(&mut self, location: Option<Point>) -> Result<()> {
        if location.is_some() {
            self.parked = location;
        }
        Ok(())
    }
}
