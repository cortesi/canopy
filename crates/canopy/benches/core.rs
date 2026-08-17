//! Core layout, rendering, and terminal buffer benchmarks.

use std::hint::black_box;

use canopy::{
    Canopy, Context, NodeId, TermBuf, ViewContext, Widget, command, derive_commands,
    error::Result,
    geom::{Line, Point, Size},
    layout::{Layout, MeasureConstraints, Measurement},
    render::{Render, RenderBackend},
    state::NodeName,
    style::{AttrSet, Color, ResolvedStyle},
};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

/// Viewport size used for tree layout and rendering.
const SCREEN: Size = Size { w: 120, h: 40 };
/// Depth used for the synthetic benchmark widget tree.
const TREE_DEPTH: usize = 4;
/// Fanout used for the synthetic benchmark widget tree.
const TREE_FANOUT: usize = 4;

/// Widget used to build benchmark trees without depending on example apps.
struct BenchNode {
    /// Stable node name.
    name: NodeName,
    /// Layout returned by the widget.
    layout: Layout,
    /// Optional text rendered by leaf nodes.
    label: Option<&'static str>,
}

impl BenchNode {
    /// Build a container node.
    fn branch(index: usize) -> Self {
        Self {
            name: NodeName::convert(&format!("branch_{index}")),
            layout: Layout::column().gap(1),
            label: None,
        }
    }

    /// Build a leaf node.
    fn leaf(index: usize) -> Self {
        Self {
            name: NodeName::convert(&format!("leaf_{index}")),
            layout: Layout::default().fixed_width(18).fixed_height(1),
            label: Some("leaf \u{754c} \u{1f642}"),
        }
    }
}

impl Widget for BenchNode {
    fn layout(&self) -> Layout {
        self.layout
    }

    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        if self.label.is_some() {
            constraints.clamp(Size::new(18, 1))
        } else {
            constraints.wrap()
        }
    }

    fn render(&mut self, frame: &mut Render<'_>, _ctx: &dyn ViewContext) -> Result<()> {
        if let Some(label) = self.label {
            frame.text("default", Line::new(0, 0, 18), label)?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        self.name.clone()
    }
}

/// Command target placed after the synthetic tree to exercise full subtree resolution.
struct CommandLeaf;

#[derive_commands]
impl CommandLeaf {
    /// No-op command used to measure target resolution.
    #[command]
    fn resolve(&self, _ctx: &mut dyn Context) {}
}

impl Widget for CommandLeaf {}

/// Render backend that counts output operations without touching a terminal.
#[derive(Default)]
struct CountingBackend {
    /// Number of bytes passed to text output.
    text_bytes: usize,
    /// Number of character shift operations.
    char_shifts: usize,
    /// Number of line shift operations.
    line_shifts: usize,
}

impl RenderBackend for CountingBackend {
    fn style(&mut self, _style: &ResolvedStyle) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, _loc: Point, text: &str) -> Result<()> {
        self.text_bytes += text.len();
        Ok(())
    }

    fn supports_char_shift(&self) -> bool {
        true
    }

    fn shift_chars(&mut self, _loc: Point, _count: i32) -> Result<()> {
        self.char_shifts += 1;
        Ok(())
    }

    fn supports_line_shift(&self) -> bool {
        true
    }

    fn shift_lines(&mut self, _top: u32, _bottom: u32, _count: i32) -> Result<()> {
        self.line_shifts += 1;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Build a deterministic tree for layout and render benchmarks.
fn build_tree() -> Result<Canopy> {
    let mut app = Canopy::new();
    populate_tree(&mut app)?;
    app.set_root_size(SCREEN)?;
    Ok(app)
}

/// Populate an app with the deterministic benchmark tree and return its top node.
fn populate_tree(app: &mut Canopy) -> Result<NodeId> {
    let mut next_index = 1;
    app.with_root_context(|context| {
        let root_child: NodeId = context.create_detached(BenchNode::branch(0))?.into();
        context.set_children(vec![root_child])?;
        add_children(context, root_child, TREE_DEPTH, &mut next_index)?;
        Ok(root_child)
    })
}

/// Build a benchmark tree whose command target is visited after the main subtree.
fn build_command_tree() -> Result<Canopy> {
    let mut app = Canopy::new();
    app.add_commands::<CommandLeaf>()?;
    let root_child = populate_tree(&mut app)?;
    app.with_context(root_child, |context| {
        let command_leaf: NodeId = context.create_detached(CommandLeaf)?.into();
        context.attach(root_child, command_leaf)
    })?;
    app.set_root_size(SCREEN)?;
    Ok(app)
}

/// Add a fixed fanout subtree below `parent`.
fn add_children(
    context: &mut dyn Context,
    parent: NodeId,
    depth: usize,
    next_index: &mut usize,
) -> Result<()> {
    let mut children = Vec::with_capacity(TREE_FANOUT);

    for _ in 0..TREE_FANOUT {
        let index = *next_index;
        *next_index += 1;
        let child = if depth == 1 {
            context.create_detached(BenchNode::leaf(index))
        } else {
            context.create_detached(BenchNode::branch(index))
        }?
        .into();

        if depth > 1 {
            add_children(context, child, depth - 1, next_index)?;
        }

        children.push(child);
    }

    context.set_children_of(parent, children)
}

/// Return the solid style used in terminal buffer benchmarks.
fn style() -> ResolvedStyle {
    ResolvedStyle::new(Color::White, Color::Black, AttrSet::default())
}

/// Return a populated terminal buffer for diff benchmarks.
fn filled_buffer() -> TermBuf {
    let style = style();
    let mut buf = TermBuf::new((160, 60), ' ', style).expect("test render target should allocate");
    for y in 0..60 {
        let text = format!("row {y:02} abc \u{754c} \u{1f642}");
        buf.text(&style, Line::new(0, y, 160), &text)
            .expect("test buffer mutation should succeed");
    }
    buf
}

/// Benchmark layout recomputation for a large tree.
fn bench_layout(c: &mut Criterion) {
    c.bench_function("layout_large_tree", |b| {
        let mut app = build_tree().expect("benchmark tree should build");
        b.iter(|| {
            app.set_root_size(black_box(SCREEN))
                .expect("layout should succeed");
        });
    });
}

/// Benchmark one transactional tree attachment against an established tree.
fn bench_tree_edit(c: &mut Criterion) {
    c.bench_function("tree_edit_attach", |b| {
        b.iter_batched(
            || {
                let mut app = build_tree().expect("benchmark tree should build");
                let child = app
                    .create_detached(BenchNode::leaf(usize::MAX))
                    .expect("detached benchmark node should build");
                (app, child)
            },
            |(mut app, child)| {
                let root = app.root_id();
                app.with_root_context(|context| {
                    context.apply_tree_edit(&mut |context| context.attach(root, child.into()))
                })
                .expect("tree edit should succeed");
                black_box(app)
            },
            BatchSize::SmallInput,
        );
    });
}

/// Benchmark diff rendering from an empty buffer to a populated buffer.
fn bench_render_diffing(c: &mut Criterion) {
    c.bench_function("render_diffing", |b| {
        let previous =
            TermBuf::new((160, 60), '\0', style()).expect("test render target should allocate");
        let current = filled_buffer();
        let mut backend = CountingBackend::default();
        b.iter(|| {
            current
                .diff(black_box(&previous), black_box(&mut backend))
                .expect("diff render should succeed");
            black_box((backend.text_bytes, backend.char_shifts, backend.line_shifts));
        });
    });
}

/// Benchmark command target resolution through the synthetic subtree.
fn bench_command_resolution(c: &mut Criterion) {
    c.bench_function("command_resolution", |b| {
        let app = build_command_tree().expect("command benchmark tree should build");
        b.iter(|| black_box(app.command_availability_from_node(black_box(app.root_id()))));
    });
}

/// Benchmark first-run startup script finalization, compilation, and execution.
fn bench_script_startup(c: &mut Criterion) {
    c.bench_function("script_startup", |b| {
        b.iter_batched(
            || {
                let mut app = Canopy::new();
                app.register_startup_script(
                    "benchmark",
                    "function setup() local values = { 1, 2, 3, 4 }; assert(#values == 4) end",
                )
                .expect("startup script should register");
                app
            },
            |mut app| {
                black_box(
                    app.run_startup_scripts()
                        .expect("startup script should execute"),
                );
            },
            BatchSize::SmallInput,
        );
    });
}

/// Benchmark writing wide graphemes into terminal buffers.
fn bench_text_buffer(c: &mut Criterion) {
    c.bench_function("text_buffer_wide_lines", |b| {
        b.iter_batched(
            || {
                let style = style();
                let text = "abc \u{754c} \u{1f642} xyz ".repeat(16);
                (
                    TermBuf::new((160, 60), ' ', style)
                        .expect("benchmark render target should allocate"),
                    style,
                    text,
                )
            },
            |(mut buf, style, text)| {
                for y in 0..60 {
                    buf.text(&style, Line::new(0, y, 160), black_box(&text))
                        .expect("benchmark text write should succeed");
                }
                black_box(buf)
            },
            BatchSize::SmallInput,
        );
    });
}

/// Benchmark rendering a large widget tree into a backend.
fn bench_large_tree_render(c: &mut Criterion) {
    c.bench_function("large_tree_render", |b| {
        let mut app = build_tree().expect("benchmark tree should build");
        let mut backend = CountingBackend::default();
        b.iter(|| {
            app.render(black_box(&mut backend))
                .expect("render should succeed");
            black_box((backend.text_bytes, backend.char_shifts, backend.line_shifts));
        });
    });
}

criterion_group!(
    benches,
    bench_tree_edit,
    bench_layout,
    bench_render_diffing,
    bench_command_resolution,
    bench_script_startup,
    bench_text_buffer,
    bench_large_tree_render
);
criterion_main!(benches);
