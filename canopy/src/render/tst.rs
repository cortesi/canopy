use super::Backend;
use crate::{cursor, style::Color, Point, Rect, Result};

pub struct TestRender {
    renders: Vec<String>,
}

impl TestRender {
    pub fn new() -> Self {
        TestRender { renders: vec![] }
    }
}

impl Backend for TestRender {
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn show_cursor(&mut self, c: cursor::Cursor) -> Result<()> {
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<()> {
        Ok(())
    }

    fn fg(&mut self, c: Color) -> Result<()> {
        Ok(())
    }

    fn bg(&mut self, c: Color) -> Result<()> {
        Ok(())
    }

    fn fill(&mut self, r: Rect, c: char) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
        Ok(())
    }
}
