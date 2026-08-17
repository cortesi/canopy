use crate::{Canopy, error::Result, geom::Point, render::RenderBackend, style::ResolvedStyle};

/// A render backend for testing, which logs the text it is asked to draw.
#[derive(Default)]
pub struct TestRender {
    /// Captured text fragments, in draw order.
    pub text: Vec<String>,
}

impl TestRender {
    /// Construct a backend with an empty capture buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Render a node tree into the capture buffer.
    pub fn render(&mut self, c: &mut Canopy) -> Result<()> {
        c.render(self)?;
        Ok(())
    }

    /// Return true if no text has been captured.
    pub fn buf_empty(&self) -> bool {
        self.text.is_empty()
    }
}

impl RenderBackend for TestRender {
    fn reset(&mut self) -> Result<()> {
        self.text.clear();
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn style(&mut self, _s: &ResolvedStyle) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, _loc: Point, txt: &str) -> Result<()> {
        let txt = txt.trim();
        if !txt.is_empty() {
            self.text.push(txt.into());
        }
        Ok(())
    }
}
