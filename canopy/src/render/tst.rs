use super::Backend;
use crate::{cursor, style::Color, Point, Rect, Result};

pub struct TestRender {
    pub text: Vec<String>,
}

impl TestRender {
    pub fn new() -> Self {
        TestRender { text: vec![] }
    }
    pub fn clear(&mut self) {
        self.text = vec![];
    }
}

impl Backend for TestRender {
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn show_cursor(&mut self, _c: cursor::Cursor) -> Result<()> {
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<()> {
        Ok(())
    }

    fn fg(&mut self, _c: Color) -> Result<()> {
        Ok(())
    }

    fn bg(&mut self, _c: Color) -> Result<()> {
        Ok(())
    }

    fn fill(&mut self, _r: Rect, _c: char) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, _loc: Point, txt: &str) -> Result<()> {
        self.text.push(txt.trim().into());
        Ok(())
    }
}
