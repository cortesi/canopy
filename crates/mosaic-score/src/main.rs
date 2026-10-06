//! Score how well Canopy's block mosaics draw a standard suite of images and
//! text, as a person sees them on a terminal.
//!
//! Each item of the suite has a reference, the picture the eye should see,
//! at the device pixels of a display model. Canopy matches the item in each
//! tier, the display model draws the cells from their Unicode geometry, and
//! S-CIELAB compares the two at each viewing condition: it blurs both as the
//! eye does at that distance, and measures the mean CIEDE2000 difference
//! that remains. Lower is better. `cargo xtask score` runs it.

mod color;
mod display;
mod report;
mod scielab;
mod suite;

use std::{
    cmp::Reverse,
    fs,
    path::PathBuf,
    process::ExitCode,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Instant,
};

use canopy_mosaic::Tier;
use clap::Parser;
use image::{Rgb, RgbImage};

use crate::{
    display::{CELL_H, CELL_W},
    report::{CONDITIONS, Score},
    scielab::Scielab,
    suite::Spec,
};

/// Command line interface.
#[derive(Parser)]
#[command(name = "mosaic-score")]
struct Args {
    /// Score only the items whose names contain this text.
    #[arg(long)]
    filter: Option<String>,
    /// Write the scores as the new baseline.
    #[arg(long, conflicts_with = "filter")]
    update: bool,
    /// Write a contact sheet of each item to `tmp/score`: its reference
    /// above its display in each tier.
    #[arg(long)]
    sheets: bool,
    /// Print the score at each viewing condition.
    #[arg(long)]
    detail: bool,
    /// Worker threads, by default one to each core.
    #[arg(long)]
    threads: Option<usize>,
}

/// The baseline of the suite.
fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baseline.txt")
}

/// Halve an image of linear pixels by area and encode it to sRGB.
fn half_srgb(pixels: &[[f32; 3]], width: usize) -> (usize, Vec<[u8; 3]>) {
    let (w, h) = (width / 2, pixels.len() / width / 2);
    let out = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w * 2, i / w * 2);
            let at = |dx: usize, dy: usize| pixels[(y + dy) * width + x + dx];
            let quad = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
            [0, 1, 2].map(|c| color::encode(quad.iter().map(|p| p[c]).sum::<f32>() / 4.0))
        })
        .collect();
    (w, out)
}

/// Write a contact sheet: the reference, then the display of each tier.
fn write_sheet(name: &str, width: usize, panels: &[Vec<[f32; 3]>]) {
    let dir = suite::root().join("tmp/score");
    fs::create_dir_all(&dir).expect("sheet directory");
    let halves: Vec<_> = panels.iter().map(|p| half_srgb(p, width)).collect();
    let (w, h) = (halves[0].0, halves[0].1.len() / halves[0].0);
    let gap = 8;
    let mut sheet = RgbImage::from_pixel(
        w as u32,
        (h * halves.len() + gap * (halves.len() - 1)) as u32,
        Rgb([40, 40, 40]),
    );
    for (index, (_, pixels)) in halves.iter().enumerate() {
        let top = index * (h + gap);
        for (i, p) in pixels.iter().enumerate() {
            sheet.put_pixel((i % w) as u32, (top + i / w) as u32, Rgb(*p));
        }
    }
    sheet
        .save(dir.join(format!("{name}.png")))
        .expect("write the sheet");
}

/// Score one item in every tier and condition.
fn score(spec: &Spec, filters: &[Scielab], sheets: bool) -> Vec<Score> {
    let item = spec.build();
    let width = item.width();
    let references: Vec<_> = filters
        .iter()
        .map(|f| f.lab(&item.reference, width))
        .collect();
    let mut scores = Vec::new();
    let mut panels = vec![item.reference.clone()];
    for tier in Tier::ALL {
        let shown = display::draw(&item.shown(tier), item.cols);
        for ((condition, filter), reference) in CONDITIONS.iter().zip(filters).zip(&references) {
            let lab = filter.lab(&shown, width);
            scores.push(Score {
                item: spec.name.clone(),
                group: spec.group,
                tier,
                condition: condition.name,
                value: scielab::mean_difference(reference, &lab),
            });
        }
        if sheets {
            panels.push(shown);
        }
    }
    if sheets {
        write_sheet(&spec.name, width, &panels);
    }
    scores
}

/// Run the suite.
fn main() -> ExitCode {
    let args = Args::parse();
    let mut specs: Vec<Spec> = suite::suite()
        .into_iter()
        .filter(|spec| {
            args.filter
                .as_ref()
                .is_none_or(|filter| spec.name.contains(filter.as_str()))
        })
        .collect();
    if specs.is_empty() {
        eprintln!("no items match");
        return ExitCode::FAILURE;
    }
    specs.sort_by_key(|spec| Reverse(spec.cost()));
    let filters: Vec<Scielab> = CONDITIONS
        .iter()
        .map(|c| Scielab::new(c.cols_per_degree * CELL_W as f64))
        .collect();
    debug_assert_eq!(CELL_H, 2 * CELL_W, "cells are twice as tall as wide");
    let threads = args
        .threads
        .or_else(|| thread::available_parallelism().ok().map(usize::from))
        .unwrap_or(1)
        .max(1);
    let started = Instant::now();
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    thread::scope(|scope| {
        for _ in 0..threads.min(specs.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(spec) = specs.get(index) else {
                        break;
                    };
                    let scores = score(spec, &filters, args.sheets);
                    results.lock().expect("results").extend(scores);
                }
            });
        }
    });
    let scores = results.into_inner().expect("results");
    let path = baseline_path();
    let baseline = match report::read_baseline(&path) {
        Ok(baseline) => baseline,
        Err(error) => {
            eprintln!("cannot read {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    print!(
        "{}",
        report::report(&scores, baseline.as_ref(), args.detail)
    );
    println!(
        "\n{} items in {:.1}s on {threads} threads",
        specs.len(),
        started.elapsed().as_secs_f64()
    );
    if args.sheets {
        println!("sheets in {}", suite::root().join("tmp/score").display());
    }
    if args.update {
        if let Err(error) = report::write_baseline(&path, &scores) {
            eprintln!("cannot write {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
        println!("wrote {}", path.display());
    }
    ExitCode::SUCCESS
}
