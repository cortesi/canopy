//! Published snapshots and successfully emitted frames have separate baselines.

use super::Canopy;
use crate::{
    ViewContext, Widget,
    commands::ArgValue,
    error::{Error, Result},
    geom::{Point, Size},
    layout::Layout,
    render::{Render, RenderBackend},
    style::ResolvedStyle,
    testing::backend::TestRender,
};

struct Paint(char);

impl Widget for Paint {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        render.fill("", ctx.view().outer_rect_local(), self.0)
    }
}

fn app() -> Result<Canopy> {
    let mut canopy = Canopy::new();
    canopy.replace_root(Paint('a'))?;
    canopy.set_root_size(Size::new(1, 1))?;
    canopy.finalize_api()?;
    Ok(canopy)
}

fn paint(canopy: &mut Canopy, character: char) -> Result<()> {
    canopy.with_root_context(|ctx| {
        let root = ctx.node_id();
        ctx.with_widget_mut(root, |widget: &mut Paint, _| {
            widget.0 = character;
            Ok(())
        })
    })
}

#[test]
fn script_snapshot_refresh_preserves_backend_diff_baseline() -> Result<()> {
    let mut canopy = app()?;
    let mut backend = TestRender::new();
    canopy.render(&mut backend)?;
    assert_eq!(backend.text, ["a"]);

    paint(&mut canopy, 'b')?;
    assert_eq!(
        canopy.eval_script("return canopy.screen_text()")?,
        ArgValue::String("b".into())
    );
    assert_eq!(canopy.buf().unwrap().screen_text(), "b");
    assert_eq!(canopy.emitted_buf.as_ref().unwrap().screen_text(), "a");

    canopy.emit_frame(&mut backend)?;
    assert_eq!(backend.text, ["b"]);
    canopy.emit_frame(&mut backend)?;
    assert!(backend.text.is_empty());
    Ok(())
}

#[derive(Default)]
struct FlushFailure {
    capture: TestRender,
    fail_flush: bool,
}

impl RenderBackend for FlushFailure {
    fn reset(&mut self) -> Result<()> {
        self.capture.reset()
    }

    fn style(&mut self, style: &ResolvedStyle) -> Result<()> {
        self.capture.style(style)
    }

    fn text(&mut self, point: Point, text: &str) -> Result<()> {
        self.capture.text(point, text)
    }

    fn flush(&mut self) -> Result<()> {
        if self.fail_flush {
            Err(Error::RunLoop("injected backend flush failure".into()))
        } else {
            Ok(())
        }
    }
}

#[test]
fn failed_flush_keeps_successful_baseline_for_retry() -> Result<()> {
    let mut canopy = app()?;
    let mut backend = FlushFailure::default();
    canopy.render(&mut backend)?;
    paint(&mut canopy, 'b')?;
    canopy.flush()?;

    backend.fail_flush = true;
    assert!(
        matches!(canopy.emit_frame(&mut backend), Err(Error::RunLoop(message)) if message == "injected backend flush failure")
    );
    assert_eq!(backend.capture.text, ["b"]);
    assert_eq!(canopy.emitted_buf.as_ref().unwrap().screen_text(), "a");
    assert_eq!(canopy.buf().unwrap().screen_text(), "b");

    backend.fail_flush = false;
    canopy.emit_frame(&mut backend)?;
    assert_eq!(backend.capture.text, ["b"]);
    assert_eq!(canopy.emitted_buf.as_ref().unwrap().screen_text(), "b");
    canopy.emit_frame(&mut backend)?;
    assert!(backend.capture.text.is_empty());
    Ok(())
}
