//! Score how well Canopy's block mosaics draw a standard suite of images and
//! text, as a person sees them on a terminal.
//!
//! Each item of the suite has a reference, the picture the eye should see,
//! at the device pixels of a display model. Canopy matches the item in each
//! tier, the display model draws the cells from their Unicode geometry, and
//! S-CIELAB blurs both as the eye does at each viewing condition. Two
//! metrics compare them: the mean CIEDE2000 difference that remains, and
//! the pooled stray, the marks of the display that nothing near in the
//! reference explains. Lower is better. `cargo xtask score` runs it, and
//! `--check` fails when a score regresses from the baseline.

mod color;
mod display;
mod report;
mod scielab;
mod stray;
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

use canopy_mosaic::{Tier, geometry::MATRIX};
use clap::Parser;
use image::{Rgb, RgbImage};

use crate::{
    display::{CELL_H, CELL_W},
    report::{CONDITIONS, Metric, Score},
    scielab::Scielab,
    suite::Spec,
};

/// How far stray looks for a color in the reference: two device pixels each
/// way, which is a matrix pixel across and half of one down. A spur one
/// matrix pixel long strays.
const REACH: [usize; 2] = [CELL_W / MATRIX, CELL_W / MATRIX];

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
    /// Fail when a group or the suite scores worse than the baseline in any
    /// tier or metric.
    #[arg(long, conflicts_with = "update")]
    check: bool,
    /// Print the cells of the full tier with the most stray at the near
    /// condition, worst first. With `--sheets`, also write a sheet of them.
    #[arg(long, value_name = "N", default_value_t = 0)]
    worst: usize,
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

/// A cell of the full tier and its stray.
struct Worst {
    /// The sum of the pooled stray of the cell's device pixels: each
    /// pixel's stray to the power [`stray::POOLING`].
    stray: f64,
    /// The item.
    item: String,
    /// The cell's column and row.
    at: (usize, usize),
    /// The character and its pens.
    cell: display::Shown,
    /// The neighborhood of the cell, [`TILE`] cells across and down, in
    /// the reference, the display, and the stray: linear sRGB.
    tile: [Vec<[f32; 3]>; 3],
}

/// Cells across and down the neighborhood of a cell in a sheet of the
/// worst cells.
const TILE: [usize; 2] = [5, 3];

/// The neighborhood of the cell at `(col, row)`, [`TILE`] cells around it,
/// cut from an image `cols` cells across. Pixels beyond the image are
/// black.
fn neighborhood(pixels: &[[f32; 3]], cols: usize, (col, row): (usize, usize)) -> Vec<[f32; 3]> {
    let width = cols * CELL_W;
    let height = pixels.len() / width;
    let (w, h) = (TILE[0] * CELL_W, TILE[1] * CELL_H);
    let x0 = (col * CELL_W) as isize - (TILE[0] / 2 * CELL_W) as isize;
    let y0 = (row * CELL_H) as isize - (TILE[1] / 2 * CELL_H) as isize;
    (0..w * h)
        .map(|i| {
            let (x, y) = (x0 + (i % w) as isize, y0 + (i / w) as isize);
            if (0..width as isize).contains(&x) && (0..height as isize).contains(&y) {
                pixels[y as usize * width + x as usize]
            } else {
                [0.0; 3]
            }
        })
        .collect()
}

/// Write a sheet of the worst cells to `tmp/score/worst.png`: for each, its
/// neighborhood in the reference, the display, and the stray, the cell
/// framed in red.
fn write_worst_sheet(worst: &[Worst]) {
    let dir = suite::root().join("tmp/score");
    fs::create_dir_all(&dir).expect("sheet directory");
    let (w, h) = (TILE[0] * CELL_W, TILE[1] * CELL_H);
    let gap = 8;
    let (tile_w, tile_h) = (3 * w + 2 * gap, h);
    let per_row = 4;
    let rows = worst.len().div_ceil(per_row);
    let mut sheet = RgbImage::from_pixel(
        (per_row * tile_w + (per_row - 1) * 3 * gap) as u32,
        (rows * tile_h + rows.saturating_sub(1) * 3 * gap) as u32,
        Rgb([40, 40, 40]),
    );
    let frame = |x: usize, y: usize| {
        let (cx, cy) = (TILE[0] / 2 * CELL_W, TILE[1] / 2 * CELL_H);
        let on_x = x + 1 == cx || x == cx + CELL_W;
        let on_y = y + 1 == cy || y == cy + CELL_H;
        (on_x && (cy - 1..=cy + CELL_H).contains(&y))
            || (on_y && (cx - 1..=cx + CELL_W).contains(&x))
    };
    for (index, cell) in worst.iter().enumerate() {
        let left = index % per_row * (tile_w + 3 * gap);
        let top = index / per_row * (tile_h + 3 * gap);
        for (panel, pixels) in cell.tile.iter().enumerate() {
            for (i, p) in pixels.iter().enumerate() {
                let (x, y) = (i % w, i / w);
                let rgb = if frame(x, y) {
                    [255, 0, 0]
                } else {
                    p.map(color::encode)
                };
                sheet.put_pixel(
                    (left + panel * (w + gap) + x) as u32,
                    (top + y) as u32,
                    Rgb(rgb),
                );
            }
        }
    }
    sheet.save(dir.join("worst.png")).expect("write the sheet");
}

/// The stray of each cell of a stray map, `cols` cells across.
fn cell_strays(map: &[f64], cols: usize) -> Vec<f64> {
    let width = cols * CELL_W;
    let mut sums = vec![0.0; map.len() / (CELL_W * CELL_H)];
    for (i, v) in map.iter().enumerate() {
        let (x, y) = (i % width, i / width);
        sums[y / CELL_H * cols + x / CELL_W] += v;
    }
    sums
}

/// Score one item in every tier and condition, with its cells of most
/// stray when `worst` asks for them.
fn score(spec: &Spec, filters: &[Scielab], sheets: bool, worst: usize) -> (Vec<Score>, Vec<Worst>) {
    let item = spec.build();
    let width = item.width();
    let references: Vec<_> = filters
        .iter()
        .map(|f| f.lab(&item.reference, width))
        .collect();
    let mut scores = Vec::new();
    let mut worst_cells = Vec::new();
    let mut panels = vec![item.reference.clone()];
    for tier in Tier::ALL {
        let cells = item.shown(tier);
        let shown = display::draw(&cells, item.cols);
        let explained = stray::explained(&item.reference, &shown, width, REACH);
        for (index, ((condition, filter), reference)) in
            CONDITIONS.iter().zip(filters).zip(&references).enumerate()
        {
            let lab = filter.lab(&shown, width);
            let map = scielab::differences(&lab, &filter.lab(&explained, width));
            for (metric, value) in [
                (
                    Metric::Difference,
                    scielab::mean_difference(reference, &lab),
                ),
                (Metric::Stray, stray::pooled(&map)),
            ] {
                scores.push(Score {
                    item: spec.name.clone(),
                    group: spec.group,
                    tier,
                    condition: condition.name,
                    metric,
                    value,
                });
            }
            if worst > 0 && tier == Tier::Full && index == 0 {
                let powered: Vec<f64> = map.iter().map(|v| v.powi(stray::POOLING)).collect();
                let mut strays: Vec<(usize, f64)> = cell_strays(&powered, item.cols)
                    .into_iter()
                    .enumerate()
                    .collect();
                strays.sort_by(|a, b| b.1.total_cmp(&a.1));
                // Stray shows as red light, full at a difference of 20.
                let heat: Vec<[f32; 3]> = map
                    .iter()
                    .map(|v| [(*v as f32 / 20.0).min(1.0), 0.0, 0.0])
                    .collect();
                worst_cells.extend(strays.into_iter().take(worst).map(|(i, stray)| {
                    let at = (i % item.cols, i / item.cols);
                    Worst {
                        stray,
                        item: spec.name.clone(),
                        at,
                        cell: cells[i],
                        tile: [&item.reference, &shown, &heat]
                            .map(|pixels| neighborhood(pixels, item.cols, at)),
                    }
                }));
            }
        }
        if sheets {
            panels.push(shown);
        }
    }
    if sheets {
        write_sheet(&spec.name, width, &panels);
    }
    (scores, worst_cells)
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
    let results = Mutex::new((Vec::new(), Vec::new()));
    thread::scope(|scope| {
        for _ in 0..threads.min(specs.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(spec) = specs.get(index) else {
                        break;
                    };
                    let (scores, worst) = score(spec, &filters, args.sheets, args.worst);
                    let mut results = results.lock().expect("results");
                    results.0.extend(scores);
                    results.1.extend(worst);
                }
            });
        }
    });
    let (scores, mut worst) = results.into_inner().expect("results");
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
    if args.worst > 0 {
        worst.sort_by(|a, b| b.stray.total_cmp(&a.stray));
        worst.truncate(args.worst);
        if args.sheets {
            write_worst_sheet(&worst);
        }
        println!("\ncells of the full tier with the most stray, near");
        for w in &worst {
            println!(
                "{:>10.1}  {:<24} {:>3},{:<3} {} U+{:04X} fg {:?} bg {:?}",
                w.stray,
                w.item,
                w.at.0,
                w.at.1,
                w.cell.ch,
                u32::from(w.cell.ch),
                w.cell.fg,
                w.cell.bg
            );
        }
    }
    if args.check {
        let Some(baseline) = &baseline else {
            eprintln!("no baseline at {}", path.display());
            return ExitCode::FAILURE;
        };
        let found = report::regressions(&scores, baseline);
        if !found.is_empty() {
            eprintln!("\nscores regressed from the baseline:");
            for line in found {
                eprintln!("  {line}");
            }
            return ExitCode::FAILURE;
        }
        println!("no regressions from the baseline");
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
