//! Benchmarks for matching and resampling a full-screen view of 200×60 cells.

use std::hint::black_box;

use canopy_mosaic::{
    RasterView, SymbolSet, Tier, coverage_into,
    raster::{Image, LinearRaster, Placement, composite, resample},
    rgb_into,
};
use criterion::{Criterion, criterion_group, criterion_main};

/// Columns of the view.
const COLS: usize = 200;
/// Rows of the view.
const ROWS: usize = 60;
/// Source image width.
const SOURCE_W: usize = 1024;
/// Source image height.
const SOURCE_H: usize = 683;

/// A source image with gradients, a hard-edged disc, stripes, and noise.
fn source() -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut noise = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 24) as f32
    };
    let mut data = Vec::with_capacity(SOURCE_W * SOURCE_H * 4);
    for y in 0..SOURCE_H {
        for x in 0..SOURCE_W {
            let (fx, fy) = (x as f32 / SOURCE_W as f32, y as f32 / SOURCE_H as f32);
            let d = ((fx - 0.5).powi(2) + (fy - 0.5).powi(2)).sqrt();
            let disc = if d < 0.3 { 1.0 } else { 0.0 };
            let stripe = if ((x + y) / 9) % 2 == 0 { 40.0 } else { 0.0 };
            let n = noise();
            data.extend([
                (fx * 200.0 + disc * 50.0 + n) as u8,
                (fy * 150.0 + stripe + n) as u8,
                (disc * 220.0 + (1.0 - disc) * 30.0 + n) as u8,
                255,
            ]);
        }
    }
    data
}

/// The placement that fits the source to the view, cells being 1:2.
fn placement() -> Placement {
    Placement {
        origin: [0.0, 0.0],
        step: [
            SOURCE_W as f32 / (COLS * 8) as f32,
            SOURCE_H as f32 / (ROWS * 8) as f32,
        ],
    }
}

/// The opaque raster of the view.
fn raster(data: &[u8]) -> Vec<[u8; 3]> {
    let image = Image::new(data, SOURCE_W, SOURCE_H).expect("image");
    let mut linear = LinearRaster::new();
    resample(&image, placement(), COLS * 8, ROWS * 8, &mut linear);
    let mut out = Vec::new();
    composite(&linear, &vec![[0, 0, 0]; COLS * ROWS], &mut out);
    out
}

/// Benchmark matching each tier, and resampling.
fn bench(c: &mut Criterion) {
    let data = source();
    let pixels = raster(&data);
    let coverage: Vec<u8> = pixels.iter().map(|p| p[1]).collect();
    let view = RasterView::new(&pixels, COLS * 8, ROWS * 8, COLS * 8).expect("raster");
    let coverage_view = RasterView::new(&coverage, COLS * 8, ROWS * 8, COLS * 8).expect("raster");
    let mut group = c.benchmark_group("match_200x60");
    group.sample_size(20);
    for tier in Tier::ALL {
        let set = SymbolSet::tier(tier);
        let mut out = Vec::new();
        group.bench_function(format!("rgb_{}", tier.name()), |b| {
            b.iter(|| {
                rgb_into(&view, set, &mut out);
                black_box(&out);
            });
        });
        let mut out = Vec::new();
        group.bench_function(format!("coverage_{}", tier.name()), |b| {
            b.iter(|| {
                coverage_into(&coverage_view, set, &mut out);
                black_box(&out);
            });
        });
    }
    group.finish();

    let image = Image::new(&data, SOURCE_W, SOURCE_H).expect("image");
    let mut linear = LinearRaster::new();
    let mut opaque = Vec::new();
    let grounds = vec![[0, 0, 0]; COLS * ROWS];
    c.bench_function("resample_composite_1024x683_to_200x60", |b| {
        b.iter(|| {
            resample(&image, placement(), COLS * 8, ROWS * 8, &mut linear);
            composite(&linear, &grounds, &mut opaque);
            black_box(&opaque);
        });
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
