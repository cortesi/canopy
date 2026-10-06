//! Image view benchmarks: a 1024×683 image in a 200×60 view, with a cache
//! that holds, a cache that misses, and a buffer fill of the same area to
//! compare a warm render with.

use std::hint::black_box;

use canopy::{
    geom::{Rect, Size},
    render::TermBuf,
    style::{AttrSet, Color, ResolvedStyle},
    testing::harness::Harness,
};
use canopy_widgets::{ImageView, mosaic::Tier};
use criterion::{Criterion, criterion_group, criterion_main};
use image::{Rgba, RgbaImage};

/// Columns of the view.
const COLS: u32 = 200;
/// Rows of the view.
const ROWS: u32 = 60;

/// A source image with gradients and a hard-edged disc.
fn image() -> RgbaImage {
    RgbaImage::from_fn(1024, 683, |x, y| {
        let (fx, fy) = (x as f32 / 1024.0, y as f32 / 683.0);
        let disc = ((fx - 0.5).powi(2) + (fy - 0.5).powi(2)).sqrt() < 0.3;
        if disc {
            Rgba([230, 90, 60, 255])
        } else {
            Rgba([(fx * 200.0) as u8, (fy * 150.0) as u8, 60, 255])
        }
    })
}

/// A harness with an image view of `tier` at the full size of the view.
fn harness(tier: Tier) -> Harness {
    let view = ImageView::new(&image()).with_tier(tier);
    let mut harness = Harness::builder(view)
        .size(COLS, ROWS)
        .build()
        .expect("harness");
    harness.render().expect("render");
    harness
}

/// Benchmark renders that hold the cache and renders that miss it.
fn bench(c: &mut Criterion) {
    for tier in [Tier::Blocks, Tier::Full] {
        let mut warm = harness(tier);
        c.bench_function(&format!("image_view_warm_{}", tier.name()), |b| {
            b.iter(|| {
                warm.render().expect("render");
                black_box(warm.buf());
            });
        });
        let mut cold = harness(tier);
        let mut flip = false;
        c.bench_function(&format!("image_view_cold_{}", tier.name()), |b| {
            b.iter(|| {
                // A change of tier misses the cache.
                flip = !flip;
                let next = if flip { Tier::Halves } else { tier };
                cold.with_unique(|view: &mut ImageView, _| {
                    view.set_tier(Some(next));
                    Ok(())
                })
                .expect("view");
                cold.render().expect("render");
                black_box(cold.buf());
            });
        });
    }
    let style = ResolvedStyle::new(Color::White, Color::Black, AttrSet::default());
    let mut buf = TermBuf::new(Size::new(COLS, ROWS), ' ', style).expect("buffer");
    c.bench_function("termbuf_fill_200x60", |b| {
        b.iter(|| {
            for y in 0..ROWS {
                for x in 0..COLS {
                    buf.fill(&style, Rect::new(x, y, 1, 1), '▀').expect("fill");
                }
            }
            black_box(&buf);
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = bench
}
criterion_main!(benches);
