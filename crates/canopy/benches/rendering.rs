use canopy::{
    Canopy, Context, Layout, Loader, Node, NodeState, Render, Result, StatefulNode,
    derive_commands, geom::Expanse, tutils::harness::Harness, widgets::Text,
};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

// Simple wrapper to provide Loader implementation for Text widget
#[derive(StatefulNode)]
struct BenchmarkTextWrapper {
    state: NodeState,
    text: Text,
}

#[derive_commands]
impl BenchmarkTextWrapper {
    fn new(content: &str) -> Self {
        BenchmarkTextWrapper {
            state: NodeState::default(),
            text: Text::new(content),
        }
    }
}

impl Node for BenchmarkTextWrapper {
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.text)
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        l.fill(self, sz)?;
        let vp = self.vp();
        l.fit(&mut self.text, vp)
    }

    fn render(&mut self, _c: &dyn Context, _r: &mut Render) -> Result<()> {
        Ok(())
    }
}

impl Loader for BenchmarkTextWrapper {
    fn load(_c: &mut Canopy) {
        // No commands to register for this simple wrapper
    }
}

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
