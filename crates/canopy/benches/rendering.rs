use criterion::{Criterion, criterion_group, criterion_main};

fn benchmark_text_rendering(_c: &mut Criterion) {}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = benchmark_text_rendering
}
criterion_main!(benches);
