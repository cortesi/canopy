//! Editor rendering benchmarks for canopy.

use std::{hint::black_box, time::Duration};

use canopy::{
    Canopy, Context, Loader, NodeId, ViewContext, derive_commands,
    error::Result,
    layout::Layout,
    render::Render,
    testing::harness::Harness,
    widget::Widget,
    widgets::editor::{EditMode, Editor, EditorConfig, LineNumbers, WrapMode},
};
use criterion::{Criterion, criterion_group, criterion_main};

/// Wrapper node used for editor render benchmarks.
struct BenchmarkEditorWrapper {
    /// Text content to render.
    text: String,
    /// Editor node id.
    editor_id: Option<NodeId>,
}

#[derive_commands]
impl BenchmarkEditorWrapper {
    /// Construct a wrapper with the provided content.
    fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            editor_id: None,
        }
    }

    /// Ensure the editor child node is created and laid out.
    fn ensure_tree(&mut self, c: &mut dyn Context) {
        if self.editor_id.is_some() {
            return;
        }

        let config = EditorConfig::new()
            .with_mode(EditMode::Text)
            .with_wrap(WrapMode::Soft)
            .with_line_numbers(LineNumbers::Absolute);
        let editor = Editor::with_config(self.text.clone(), config);
        let editor_id = c.add_child(editor).expect("Failed to attach editor");

        c.with_layout(&mut |layout| {
            *layout = Layout::fill();
        })
        .expect("Failed to style root");

        c.with_layout_of(editor_id, &mut |layout| {
            *layout = Layout::fill();
        })
        .expect("Failed to style editor");

        self.editor_id = Some(editor_id);
    }
}

impl Widget for BenchmarkEditorWrapper {
    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c);
        None
    }
}

impl Loader for BenchmarkEditorWrapper {
    fn load(c: &mut Canopy) {
        c.add_commands::<Editor>();
    }
}

/// Benchmark rendering an editor node.
fn benchmark_editor_rendering(c: &mut Criterion) {
    let sample_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n\
        Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.\n\
        Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.\n\
        Duis aute irure dolor in reprehenderit in voluptate velit esse cillum.\n\
        Excepteur sint occaecat cupidatat non proident, sunt in culpa qui.\n\
        Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n\
        Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.\n\
        Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.\n\
        Duis aute irure dolor in reprehenderit in voluptate velit esse cillum.";

    c.bench_function("editor_render", |b| {
        b.iter(|| {
            let wrapper = BenchmarkEditorWrapper::new(sample_text);
            let mut harness = Harness::builder(wrapper)
                .size(80, 24)
                .build()
                .expect("Failed to create harness");

            harness.render().expect("Failed to render");
            let buf = harness.buf();
            black_box(buf);
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = benchmark_editor_rendering
}
criterion_main!(benches);
