//! Editor rendering benchmarks for canopy-widgets.

use std::hint::black_box;

use canopy::{
    Context, ContextExt, Register, Setup, Widget, derive_commands,
    error::Result,
    geom::{Point, Size},
    layout::{Layout, ScrollOp},
    testing::harness::Harness,
};
use canopy_widgets::{
    editor::{EditMode, Editor, EditorConfig, Interaction, LineNumbers, WrapMode},
    highlight::{Highlighter, SyntectHighlighter},
};
use criterion::{Criterion, criterion_group, criterion_main};

canopy::slot!(EditorSlot: Editor);

/// Wrapper node used for editor render benchmarks.
struct BenchmarkEditorWrapper {
    /// Text content to render.
    text: String,
    /// Optional syntax for the preview.
    syntax: Option<&'static str>,
}

#[derive_commands]
impl BenchmarkEditorWrapper {
    /// Construct a wrapper with the provided content.
    fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            syntax: None,
        }
    }
}

impl Widget for BenchmarkEditorWrapper {
    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let config = EditorConfig::new()
            .with_mode(EditMode::Text)
            .with_wrap(WrapMode::Soft)
            .with_line_numbers(LineNumbers::Absolute);
        let mut editor = Editor::with_config(self.text.clone(), config);
        if let Some(syntax) = self.syntax {
            editor.set_highlighter(Some(Box::new(SyntectHighlighter::new(syntax))));
        }
        let editor_id = c
            .add_slot::<EditorSlot>(c.node_id(), editor)
            .expect("Failed to attach editor");

        c.set_layout_override(c.node_id(), Layout::fill().into())
            .expect("Failed to style root");

        c.set_layout_override(editor_id.into(), Layout::fill().into())
            .expect("Failed to style editor");
        Ok(())
    }
}

/// Scroll a preview-sized document in both directions after loading it.
fn benchmark_preview_scrolling(c: &mut Criterion) {
    let text: String = (0..2_000)
        .map(|line| format!("Line {line}: long document content with **bold** text, a [link](https://example.com), and more words to wrap.\n"))
        .collect();
    for (name, syntax) in [("plain", None), ("markdown", Some("md"))] {
        let wrapper = BenchmarkEditorWrapper {
            text: text.clone(),
            syntax,
        };
        let mut harness = Harness::builder(wrapper)
            .size(80, 24)
            .build()
            .expect("Failed to create harness");
        harness.render().expect("Failed to render");
        harness
            .with_unique::<Editor, _>(|editor, _| {
                editor.set_config(
                    EditorConfig::new()
                        .with_interaction(Interaction::Display)
                        .with_wrap(WrapMode::Soft),
                );
                Ok(())
            })
            .expect("editor missing");
        harness.render().expect("Failed to render");
        let mut row = 1_000u32;
        let mut down = true;
        c.bench_function(&format!("preview_scroll_{name}"), |b| {
            b.iter(|| {
                harness
                    .with_unique::<Editor, _>(|_, ctx| {
                        ctx.scroll(ScrollOp::To(Point { x: 0, y: row }));
                        Ok(())
                    })
                    .expect("editor missing");
                harness.render().expect("Failed to render");
                black_box(harness.buf());
                if row == 1_040 {
                    down = false;
                }
                if row == 1_000 {
                    down = true;
                }
                row = if down { row + 1 } else { row - 1 };
            });
        });
    }
}

impl Register for BenchmarkEditorWrapper {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Editor>()?;
        Ok(())
    }
}

/// Lay a document out again on each render, by alternating its width. Each
/// relayout breaks every line at its line break opportunities.
fn benchmark_relayout(c: &mut Criterion) {
    let text: String = (0..2_000)
        .map(|line| format!("Line {line}: prose of ordinary words to wrap, some 漢字 text, and a-long-hyphenated-run.\n"))
        .collect();
    let wrapper = BenchmarkEditorWrapper { text, syntax: None };
    let mut harness = Harness::builder(wrapper)
        .size(80, 24)
        .build()
        .expect("Failed to create harness");
    harness.render().expect("Failed to render");
    let mut wide = true;
    c.bench_function("editor_relayout", |b| {
        b.iter(|| {
            wide = !wide;
            let width = if wide { 80 } else { 79 };
            harness
                .canopy
                .set_screen_size(Size::new(width, 24))
                .expect("resize");
            harness.render().expect("Failed to render");
            black_box(harness.buf());
        });
    });
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
        let wrapper = BenchmarkEditorWrapper::new(sample_text);
        let mut harness = Harness::builder(wrapper)
            .register::<BenchmarkEditorWrapper>()
            .size(80, 24)
            .build()
            .expect("Failed to create harness");
        // The first render also runs the on-start hooks, so warm up outside the
        // measured loop.
        harness.render().expect("Failed to render");

        b.iter(|| {
            harness.render().expect("Failed to render");
            black_box(harness.buf());
        });
    });
}

/// Measure preparation independently of lazy syntax parsing and grammar
/// loading.
fn benchmark_highlight_preparation(c: &mut Criterion) {
    let source = "let value = 42; // a source line\n".repeat(100_000);
    let highlighter = SyntectHighlighter::new("rs");
    highlighter.prepare(&source);
    c.bench_function("highlight_prepare_large_source", |b| {
        b.iter(|| highlighter.prepare(black_box(&source)));
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = benchmark_editor_rendering, benchmark_highlight_preparation, benchmark_preview_scrolling, benchmark_relayout
}
criterion_main!(benches);
