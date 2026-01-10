//! Rendering benchmarks for canopy-widgets.

use std::hint::black_box;

use canopy::{
    Context, Loader, ReadContext, Widget, derive_commands, error::Result, key, layout::Layout,
    render::Render, testing::harness::Harness,
};
use canopy_widgets::Text;
use criterion::{Criterion, criterion_group, criterion_main};

key!(TextSlot: Text);

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
    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let text_id = c
            .add_keyed::<TextSlot>(Text::new(self.content.clone()))
            .expect("Failed to attach text");

        c.set_layout(Layout::fill()).expect("Failed to style root");

        c.set_layout_of(text_id, Layout::fill())
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
