#[cfg(test)]
use crate::geom::Expanse;
use crate::{
    cursor,
    geom::Point,
    render::RenderBackend,
    style::{Style, StyleManager},
    Canopy, Node, Result,
};
use std::sync::{Arc, Mutex};

/// A handle to a vector that contains the result of the render.
#[derive(Default)]
pub struct TestBuf {
    pub text: Vec<String>,
}

impl TestBuf {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
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

    pub fn render(&mut self, c: &mut Canopy, e: &mut dyn Node) -> Result<()> {
        c.render(self, e)?;
        Ok(())
    }

    pub fn styleman(&self) -> StyleManager {
        StyleManager::default()
    }

    pub fn buf_text(&self) -> Vec<String> {
        self.text.lock().unwrap().text.clone()
    }

    pub fn buf_empty(&self) -> bool {
        self.text.lock().unwrap().text.is_empty()
    }
}

impl RenderBackend for TestRender {
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

    fn style(&mut self, _s: Style) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, _loc: Point, txt: &str) -> Result<()> {
        let txt = txt.trim();
        if !txt.is_empty() {
            self.text.lock()?.text.push(txt.trim().into());
        }
        Ok(())
    }

    fn exit(&mut self, _code: i32) -> ! {
        unreachable!()
    }
}

/// A simple in-memory canvas for verifying render placement in tests.
#[cfg(test)]
#[derive(Default)]
pub struct CanvasBuf {
    size: Expanse,
    pub cells: Vec<Vec<char>>,
    /// Track which cells have been written to during a render.
    pub painted: Vec<Vec<bool>>,
}

#[cfg(test)]
impl CanvasBuf {
    fn new(size: Expanse) -> Self {
        CanvasBuf {
            size,
            cells: vec![vec![' '; size.w as usize]; size.h as usize],
            painted: vec![vec![false; size.w as usize]; size.h as usize],
        }
    }

    fn clear(&mut self) {
        for row in &mut self.cells {
            for c in row.iter_mut() {
                *c = ' ';
            }
        }
        for row in &mut self.painted {
            for c in row.iter_mut() {
                *c = false;
            }
        }
    }
}

#[cfg(test)]
pub struct CanvasRender {
    pub canvas: Arc<Mutex<CanvasBuf>>,
}

#[cfg(test)]
impl CanvasRender {
    pub fn create(size: Expanse) -> (Arc<Mutex<CanvasBuf>>, Self) {
        let buf = Arc::new(Mutex::new(CanvasBuf::new(size)));
        let buf2 = buf.clone();
        (buf, CanvasRender { canvas: buf2 })
    }
}

#[cfg(test)]
impl RenderBackend for CanvasRender {
    fn reset(&mut self) -> Result<()> {
        self.canvas.lock()?.clear();
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

    fn style(&mut self, _s: Style) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
        let mut buf = self.canvas.lock()?;
        for (i, ch) in txt.chars().enumerate() {
            let x = loc.x as usize + i;
            let y = loc.y as usize;
            if x < buf.size.w as usize && y < buf.size.h as usize {
                buf.cells[y][x] = ch;
                buf.painted[y][x] = true;
            }
        }
        Ok(())
    }

    fn exit(&mut self, _code: i32) -> ! {
        unreachable!()
    }
}
