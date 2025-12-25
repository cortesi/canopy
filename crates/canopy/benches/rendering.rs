//! Rendering benchmarks for canopy.

use std::{hint::black_box, time::Duration};

use canopy::{
    Context, Loader, NodeId, ViewContext, derive_commands, error::Result, geom::Rect,
    render::Render, testing::harness::Harness, widget::Widget, widgets::Text,
};
use criterion::{Criterion, criterion_group, criterion_main};
use taffy::style::{Dimension, Display, FlexDirection, Style};

/// Wrapper node used for text render benchmarks.
struct BenchmarkTextWrapper {
    /// Text content to render.
    content: String,
    /// Text node id.
    text_id: Option<NodeId>,
}

#[derive_commands]
impl BenchmarkTextWrapper {
    /// Construct a wrapper with the provided content.
    fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
            text_id: None,
        }
    }

    /// Ensure the text child node is created and styled.
    fn ensure_tree(&mut self, c: &mut dyn Context) {
        if self.text_id.is_some() {
            return;
        }

        let text_id = c.add(Box::new(Text::new(self.content.clone())));
        c.set_children(c.node_id(), vec![text_id])
            .expect("Failed to attach text");

        let mut update_root = |style: &mut Style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Column;
        };
        c.with_style(c.node_id(), &mut update_root)
            .expect("Failed to style root");

        let mut grow = |style: &mut Style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        };
        c.with_style(text_id, &mut grow)
            .expect("Failed to style text");

        self.text_id = Some(text_id);
    }
}

impl Widget for BenchmarkTextWrapper {
    fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c);
        None
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
