//! Rendering benchmarks for canopy-widgets.

use std::hint::black_box;

use canopy::{
    Context, Loader, ViewContext, Widget, derive_commands,
    error::Result,
    layout::{Layout, Sizing},
    render::Render,
    testing::harness::Harness,
};
use canopy_widgets::Text;
use criterion::{Criterion, criterion_group, criterion_main};

/// Key for the text child node.
const KEY_TEXT: &str = "text";

/// Wrapper node used for text render benchmarks.
struct BenchmarkTextWrapper {
    /// Text content to render.
    content: String,
}

#[derive_commands]
impl BenchmarkTextWrapper {
    /// Construct a wrapper with the provided content.
    fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
        }
    }
}

impl Widget for BenchmarkTextWrapper {
    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let text_id = c
            .add_child_keyed(KEY_TEXT, Text::new(self.content.clone()))
            .expect("Failed to attach text");

        c.with_layout(&mut |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })
        .expect("Failed to style root");

        c.with_layout_of(text_id, &mut |layout| {
            layout.width = Sizing::Flex(1);
            layout.height = Sizing::Flex(1);
        })
        .expect("Failed to style text");
        Ok(())
    }
}

impl Loader for BenchmarkTextWrapper {}

/// Benchmark rendering a text node.
fn benchmark_text_rendering(c: &mut Criterion) {
    c.bench_function("text_node_render", |b| {
        // Create a sample text with multiple lines
        let sample_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n\
                          Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.\n\
                          Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.\n\
                          Duis aute irure dolor in reprehenderit in voluptate velit esse cillum.\n\
                          Excepteur sint occaecat cupidatat non proident, sunt in culpa qui.\n\
                          Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n\
                          Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.\n\
                          Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.";

        b.iter(|| {
            // Create a new Text node wrapped in our benchmark wrapper
            let wrapper = BenchmarkTextWrapper::new(sample_text);
            let mut harness = Harness::builder(wrapper)
                .size(80, 24)
                .build()
                .expect("Failed to create harness");

            // Perform the render
            harness.render().expect("Failed to render");

            // Access the buffer to ensure the render is complete
            let buf = harness.buf();
            black_box(buf);
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = benchmark_text_rendering
}
criterion_main!(benches);
