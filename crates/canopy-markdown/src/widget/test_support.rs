//! Helpers that the reader's tests share. They reach the reader's private
//! fields, which the public `testing` module cannot.

use canopy::{
    Context, Widget,
    error::Result,
    input::key::Key,
    testing::harness::{Harness, HarnessBuilder},
};

use super::Markdown;
use crate::executor::ManualExecutor;

/// A harness builder over `root` that runs the reader's default bindings.
/// The caller registers the types it mounts.
pub(super) fn builder<W: Widget + 'static>(root: W) -> HarnessBuilder<W> {
    Harness::builder(root).script("markdown-defaults", "markdown.default_bindings()")
}

/// Render, poll the reader, and render again. Painting alone does not poll.
pub(super) fn service(harness: &mut Harness) {
    harness.render().unwrap();
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            Widget::poll(reader, ctx).map(|_| ())
        })
        .unwrap();
    harness.render().unwrap();
}

/// Run the reader's work until it shows its document, and focus its view
/// unless an inspector covers it.
pub(super) fn settle(harness: &mut Harness, manual: &ManualExecutor) {
    for _ in 0..16 {
        service(harness);
        manual.run_until_idle();
        service(harness);
        if harness.with_root_widget(|reader: &mut Markdown| reader.ready()) {
            service(harness);
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    if reader.inspector.is_none() {
                        ctx.set_focus(reader.navigation_node().unwrap())?;
                    }
                    Ok(())
                })
                .unwrap();
            return;
        }
    }
    let state = harness.with_root_widget(|reader: &mut Markdown| {
        format!(
            "view={:?}, accepted={}, displayed={}, dirty={}, failure={:?}, switching={}",
            reader.active,
            reader.accepted_revision(),
            reader.displayed_revision(),
            reader.submission.is_due(),
            reader.failure,
            reader.switching.is_some()
        )
    });
    panic!("the reader did not settle: {state}");
}

/// Run `f` on the reader.
pub(super) fn reader<R>(
    harness: &mut Harness,
    f: impl FnOnce(&mut Markdown, &mut dyn Context) -> Result<R>,
) -> R {
    harness.with_root_widget_context(f).unwrap()
}

/// Press `key` and service the reader.
pub(super) fn press(harness: &mut Harness, key: &str) {
    harness.key(Key::parse_spec(key).unwrap()).unwrap();
    service(harness);
}
