//! Keyed child reconciliation benchmarks for canopy-widgets.

use std::hint::black_box;

use canopy::{CanopyBuilder, Widget};
use canopy_widgets::KeyedChildren;
use criterion::{Criterion, criterion_group, criterion_main};

/// Leaf row with no behavior.
struct Row;

impl Widget for Row {}

/// Benchmark stable and rolling key sets against an existing 1,000-row tree.
fn bench_keyed_reconciliation(c: &mut Criterion) {
    for (name, step) in [
        ("keyed_reconcile_unchanged_1000", 0),
        ("keyed_reconcile_rolling_1000", 1),
    ] {
        let mut app = CanopyBuilder::new()
            .build()
            .expect("an empty application builds");
        let mut items = KeyedChildren::<usize, Row>::new();
        app.with_root_context(|ctx| items.reconcile(ctx, 0..1_000, |_| Ok(Row), |_, _, _| Ok(())))
            .expect("keyed benchmark tree should build");
        let mut start = 0;
        c.bench_function(name, |b| {
            b.iter(|| {
                start += step;
                app.with_root_context(|ctx| {
                    items.reconcile(
                        ctx,
                        black_box(start..start + 1_000),
                        |_| Ok(Row),
                        |_, _, _| Ok(()),
                    )
                })
                .expect("keyed reconciliation should succeed")
            });
        });
    }
}

criterion_group!(benches, bench_keyed_reconciliation);
criterion_main!(benches);
