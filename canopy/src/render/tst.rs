use super::Backend;
use crate::{cursor, style::Color, Point, Rect, Result};
use std::sync::{Arc, Mutex};

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

pub struct TestRender {
    pub text: Arc<Mutex<TestBuf>>,
}

impl TestRender {
    pub fn create() -> (Arc<Mutex<TestBuf>>, Self) {
        let tb = Arc::new(Mutex::new(TestBuf::default()));
        let tb2 = tb.clone();
        (tb, TestRender { text: tb2 })
    }

    pub fn clear(&mut self) -> Result<()> {
        self.text.lock()?.text.clear();
        Ok(())
    }
}

impl PartialEq<Vec<String>> for TestRender {
    fn eq(&self, other: &Vec<String>) -> bool {
        self.text.lock().unwrap().text == *other
    }
}

impl Backend for TestRender {
    fn reset(&mut self) -> Result<()> {
        self.clear()
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
