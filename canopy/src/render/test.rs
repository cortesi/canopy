use super::Backend;
use crate::{cursor, style::Color, Point, Rect, Result};
use std::sync::{Arc, Mutex};

/// A handle to a vector that contains the result of the render.
pub struct TestBuf {
    pub text: Vec<String>,
}

impl TestBuf {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

impl Default for TestBuf {
    fn default() -> Self {
        TestBuf { text: vec![] }
    }
}

/// A render backend for testing, which logs render outcomes.
pub struct TestRender {
    pub text: Arc<Mutex<TestBuf>>,
}

impl TestRender {
    /// Create returns a `TestBuf` protected by a mutex, and a `TestRender`
    /// instance. The `TestBuf` can be used to access the result of the render
    /// for testing.
    pub fn create() -> (Arc<Mutex<TestBuf>>, Self) {
        let tb = Arc::new(Mutex::new(TestBuf::default()));
        let tb2 = tb.clone();
        (tb, TestRender { text: tb2 })
    }
}

impl Backend for TestRender {
    fn reset(&mut self) -> Result<()> {
        self.text.lock()?.text.clear();
        Ok(())
    }

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
        self.text.lock()?.text.push(txt.trim().into());
        Ok(())
    }

    fn exit(&mut self, _code: i32) -> ! {
        unreachable!()
    }
}
