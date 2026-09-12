//! Published snapshots and emitted frames have separate baselines.

use super::Canopy;
use crate::{
    ContextExt, TermBuf, ViewContext, Widget,
    commands::ArgValue,
    error::{Error, Result},
    geom::{Line, Point, Size},
    layout::Layout,
    render::{Render, RenderBackend},
    style::{AttrSet, Color, ResolvedStyle},
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    Reset,
    Style,
    Text,
    Shift,
    Flush,
}

#[derive(Default)]
struct FailingBackend {
    capture: TestRender,
    fail_at: Option<Failure>,
    shifts: usize,
}

impl FailingBackend {
    fn check_failure(&mut self, operation: Failure) -> Result<()> {
        if self.fail_at == Some(operation) {
            self.fail_at = None;
            Err(Error::RunLoop("injected backend failure".into()))
        } else {
            Ok(())
        }
    }
}

impl RenderBackend for FailingBackend {
    fn reset(&mut self) -> Result<()> {
        self.capture.reset()?;
        self.check_failure(Failure::Reset)
    }

    fn style(&mut self, style: &ResolvedStyle) -> Result<()> {
        self.capture.style(style)?;
        self.check_failure(Failure::Style)
    }

    fn text(&mut self, point: Point, text: &str) -> Result<()> {
        self.capture.text(point, text)?;
        self.check_failure(Failure::Text)
    }

    fn supports_char_shift(&self) -> bool {
        true
    }

    fn shift_chars(&mut self, _point: Point, _count: i32) -> Result<()> {
        self.shifts += 1;
        self.check_failure(Failure::Shift)
    }

    fn flush(&mut self) -> Result<()> {
        self.check_failure(Failure::Flush)
    }
}

#[test]
fn failed_output_repaints_even_when_the_next_frame_reverts() -> Result<()> {
    for operation in [
        Failure::Reset,
        Failure::Style,
        Failure::Text,
        Failure::Flush,
    ] {
        let mut canopy = app()?;
        let mut backend = FailingBackend::default();
        canopy.render(&mut backend)?;
        paint(&mut canopy, 'b')?;
        canopy.flush()?;

        backend.fail_at = Some(operation);
        assert!(matches!(
            canopy.emit_frame(&mut backend),
            Err(Error::RunLoop(message)) if message == "injected backend failure"
        ));
        assert_eq!(canopy.buf().unwrap().screen_text(), "b");

        // The failed output may already have painted 'b'. A diff against the
        // last successful 'a' would leave that cell unchanged on the terminal.
        paint(&mut canopy, 'a')?;
        canopy.flush()?;
        canopy.emit_frame(&mut backend)?;
        assert_eq!(backend.capture.text, ["a"], "failed at {operation:?}");
        assert_eq!(canopy.emitted_buf.as_ref().unwrap().screen_text(), "a");
        canopy.emit_frame(&mut backend)?;
        assert!(backend.capture.text.is_empty());
    }
    Ok(())
}

fn frame(text: &str) -> Result<TermBuf> {
    let style = ResolvedStyle::new(Color::White, Color::Black, AttrSet::default());
    let width = text.len() as u32;
    let mut frame = TermBuf::new(Size::new(width, 1), ' ', style)?;
    frame.text(&style, Line::new(0, 0, width), text)?;
    Ok(frame)
}

#[test]
fn failed_shift_is_not_repeated_on_retry() -> Result<()> {
    let mut canopy = Canopy::new();
    let mut backend = FailingBackend::default();
    canopy.termbuf = Some(frame("abcdef")?);
    canopy.emit_frame(&mut backend)?;
    canopy.termbuf = Some(frame("Zabcde")?);

    // A backend can apply a shift before reporting a write failure. Repeating
    // that relative operation would shift the existing content twice.
    backend.fail_at = Some(Failure::Shift);
    assert!(canopy.emit_frame(&mut backend).is_err());
    assert_eq!(backend.shifts, 1);
    canopy.emit_frame(&mut backend)?;
    assert_eq!(backend.shifts, 1);
    assert_eq!(backend.capture.text, ["Zabcde"]);
    canopy.emit_frame(&mut backend)?;
    assert!(backend.capture.text.is_empty());
    Ok(())
}
