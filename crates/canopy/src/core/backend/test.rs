use std::sync::{Arc, Mutex};

use crate::{
    Canopy, Node, Result,
    geom::{Expanse, Point},
    render::RenderBackend,
    style::{Style, StyleManager},
};

/// A handle to a vector that contains the result of the render.
#[derive(Default)]
pub struct TestBuf {
    /// Captured text fragments.
    pub text: Vec<String>,
}

impl TestBuf {
    /// Return true if no text has been captured.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Return true if any captured line contains the provided substring.
    pub fn contains(&self, s: &str) -> bool {
        self.text.iter().any(|l| l.contains(s))
    }
}

/// A render backend for testing, which logs render outcomes.
pub struct TestRender {
    /// Shared buffer of captured text.
    pub text: Arc<Mutex<TestBuf>>,
}

impl TestRender {
    /// Create returns a `TestBuf` protected by a mutex, and a `TestRender`
    /// instance. The `TestBuf` can be used to access the result of the render
    /// for testing.
    pub fn create() -> (Arc<Mutex<TestBuf>>, Self) {
        let tb = Arc::new(Mutex::new(TestBuf::default()));
        let tb2 = tb.clone();
        (tb, Self { text: tb2 })
    }

    /// Render a node tree into the test buffer.
    pub fn render(&mut self, c: &mut Canopy, e: &mut dyn Node) -> Result<()> {
        c.render(self, e)?;
        Ok(())
    }

    /// Return the default style manager used in tests.
    pub fn styleman(&self) -> StyleManager {
        StyleManager::default()
    }

    /// Return captured text lines.
    pub fn buf_text(&self) -> Vec<String> {
        self.text.lock().unwrap().text.clone()
    }

    /// Return true if no text has been captured.
    pub fn buf_empty(&self) -> bool {
        self.text.lock().unwrap().text.is_empty()
    }

    /// Return true if any captured line contains the substring.
    pub fn contains_text(&self, txt: &str) -> bool {
        self.text.lock().unwrap().contains(txt)
    }
}

impl RenderBackend for TestRender {
    fn reset(&mut self) -> Result<()> {
        self.text.lock().unwrap().text.clear();
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn style(&mut self, _s: &Style) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, _loc: Point, txt: &str) -> Result<()> {
        let txt = txt.trim();
        if !txt.is_empty() {
            self.text.lock().unwrap().text.push(txt.trim().into());
        }
        Ok(())
    }

    fn exit(&mut self, _code: i32) -> ! {
        unreachable!()
    }
}

/// A simple in-memory canvas for verifying render placement in tests.
#[derive(Default)]
pub struct CanvasBuf {
    /// Canvas size.
    size: Expanse,
    /// Character cells.
    pub cells: Vec<Vec<char>>,
    /// Track which cells have been written to during a render.
    pub painted: Vec<Vec<bool>>,
}

impl CanvasBuf {
    /// Construct a new canvas buffer.
    fn new(size: Expanse) -> Self {
        Self {
            size,
            cells: vec![vec![' '; size.w as usize]; size.h as usize],
            painted: vec![vec![false; size.w as usize]; size.h as usize],
        }
    }

    /// Clear all cell contents and paint markers.
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

/// A render backend that draws into an in-memory canvas.
pub struct CanvasRender {
    /// Shared canvas buffer for render output.
    pub canvas: Arc<Mutex<CanvasBuf>>,
}

impl CanvasRender {
    /// Create a new canvas render backend.
    pub fn create(size: Expanse) -> (Arc<Mutex<CanvasBuf>>, Self) {
        let buf = Arc::new(Mutex::new(CanvasBuf::new(size)));
        let buf2 = buf.clone();
        (buf, Self { canvas: buf2 })
    }
}

impl RenderBackend for CanvasRender {
    fn reset(&mut self) -> Result<()> {
        self.canvas.lock().unwrap().clear();
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn style(&mut self, _s: &Style) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
        let mut buf = self.canvas.lock().unwrap();
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
