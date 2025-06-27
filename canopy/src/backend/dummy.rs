use crate::{
    geom::Point,
    render::RenderBackend,
    style::Style,
    Result,
};

/// A dummy render backend that discards all output.
/// This is useful for tests where we want to inspect the TermBuf directly.
pub struct DummyBackend;

impl DummyBackend {
    pub fn new() -> Self {
        DummyBackend
    }
}

impl RenderBackend for DummyBackend {
    fn style(&mut self, _style: Style) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, _loc: Point, _txt: &str) -> Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn exit(&mut self, code: i32) -> ! {
        std::process::exit(code)
    }

    fn reset(&mut self) -> Result<()> {
        Ok(())
    }
}