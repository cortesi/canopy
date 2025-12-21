use std::process::exit;

use crate::{error::Result, geom::Point, render::RenderBackend, style::Style};

/// A dummy render backend that discards all output.
/// This is useful for tests where we want to inspect the TermBuf directly.
pub struct NopBackend;

impl NopBackend {
    /// Construct a no-op backend.
    pub fn new() -> Self {
        Self
    }
}

impl RenderBackend for NopBackend {
    fn style(&mut self, _style: &Style) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, _loc: Point, _txt: &str) -> Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn exit(&mut self, code: i32) -> ! {
        exit(code)
    }

    fn reset(&mut self) -> Result<()> {
        Ok(())
    }
}
