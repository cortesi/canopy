//! Input latency profile of the gyms: the time from a key to its emitted
//! frame, turn and emission apart, on a large screen.
//!
//! Run in release with `cargo test --release -p gyms perf -- --ignored
//! --nocapture`. `GYMS_PERF` selects scenarios by name, and `GYMS_PERF_ROUNDS`
//! repeats each script in a fresh app, for a profiler.

use std::{
    env, iter,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use canopy::{
    error::Result,
    geom::{Point, Size},
    input::{Event, key::Key},
    render::RenderBackend,
    runtime::TurnInput,
    style::ResolvedStyle,
    testing::ManualClock,
};

use crate::Demo;

/// Screen of a large terminal window.
const SCREEN: Size = Size { w: 200, h: 60 };

/// Backend that estimates the bytes a terminal would read, and shifts as
/// the Crossterm backend does.
#[derive(Default)]
struct Counting {
    /// Estimated bytes written: text, and the escapes that position, style,
    /// and shift it.
    bytes: usize,
}

impl RenderBackend for Counting {
    fn style(&mut self, _style: &ResolvedStyle) -> Result<()> {
        // A reset, and two true colors.
        self.bytes += 42;
        Ok(())
    }

    fn text(&mut self, _loc: Point, text: &str) -> Result<()> {
        // A cursor move, and the text.
        self.bytes += 8 + text.len();
        Ok(())
    }

    fn supports_char_shift(&self) -> bool {
        true
    }

    fn shift_chars(&mut self, _loc: Point, _count: i32) -> Result<()> {
        self.bytes += 8;
        Ok(())
    }

    fn supports_line_shift(&self) -> bool {
        true
    }

    fn shift_lines(&mut self, _top: u32, _bottom: u32, _count: i32) -> Result<()> {
        self.bytes += 16;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Return the path of a workspace file.
fn workspace_file(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// Return `count` copies of each key spec, in order.
fn keys(script: &[(&str, usize)]) -> Vec<Key> {
    script
        .iter()
        .flat_map(|(spec, count)| {
            let key = Key::parse_spec(spec).expect("key spec");
            iter::repeat_n(key, *count)
        })
        .collect()
}

/// Return the scenarios: a name, a demo, and the keys to press.
fn scenarios() -> Vec<(&'static str, Demo, Vec<Key>)> {
    let source = workspace_file("crates/canopy/src/core/script/base_api.rs");
    vec![
        (
            "list",
            Demo::List,
            keys(&[("Down", 200), ("PageDown", 20), ("Tab", 6), ("Up", 50)]),
        ),
        (
            "pager",
            Demo::Pager {
                file: source.clone(),
            },
            keys(&[("Down", 200), ("PageDown", 30), ("Right", 20)]),
        ),
        (
            "cedit",
            Demo::Cedit { file: source },
            keys(&[("j", 200), ("Ctrl+d", 20), ("i", 1), ("x", 100), ("Esc", 1)]),
        ),
        (
            "markdown",
            Demo::Markdown,
            keys(&[("j", 150), ("PageDown", 20), ("k", 50)]),
        ),
        (
            "table",
            Demo::Table,
            keys(&[("Down", 150), ("Right", 30), ("PageDown", 10)]),
        ),
        ("frame", Demo::Frame, keys(&[("Down", 150), ("Tab", 10)])),
        ("text", Demo::Text, keys(&[("Down", 100), ("Right", 30)])),
        ("editor", Demo::Editor, keys(&[("i", 1), ("x", 150)])),
        ("focus", Demo::Focus, keys(&[("Tab", 60), ("Right", 30)])),
    ]
}

/// Return the mean, median, 95th percentile, and maximum of `times`, in
/// microseconds.
fn stats(times: &mut [Duration]) -> (f64, f64, f64, f64) {
    times.sort_unstable();
    let micros = |d: Duration| d.as_secs_f64() * 1e6;
    let mean = times.iter().copied().map(micros).sum::<f64>() / times.len() as f64;
    let at = |q: f64| micros(times[((times.len() - 1) as f64 * q) as usize]);
    (mean, at(0.5), at(0.95), at(1.0))
}

#[test]
#[ignore = "a profile for release builds, not a check"]
fn input_latency() -> Result<()> {
    let only = env::var("GYMS_PERF").ok();
    let rounds: usize = env::var("GYMS_PERF_ROUNDS")
        .ok()
        .and_then(|rounds| rounds.parse().ok())
        .unwrap_or(1);
    println!(
        "{:<10} {:>6} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>8}",
        "scenario", "keys", "mean", "p50", "p95", "max", "emit", "frames", "bytes"
    );
    for (name, demo, script) in scenarios() {
        if only
            .as_deref()
            .is_some_and(|only| !only.split(',').any(|n| n == name))
        {
            continue;
        }
        let mut backend = Counting::default();
        let mut totals = Vec::new();
        let mut emits = Duration::ZERO;
        let mut frames = 0;
        // Each round presses the script in a fresh app.
        for _ in 0..rounds {
            let mut canopy = demo.build(false)?;
            // A still clock keeps timed work out of the frames, so they
            // repeat.
            canopy.set_clock_for_testing(Arc::new(ManualClock::new()))?;
            canopy.set_screen_size(SCREEN)?;
            canopy.turn(TurnInput::Prepare)?;
            canopy.emit_for_testing(&mut Counting::default())?;
            for key in &script {
                let began = Instant::now();
                let outcome = canopy.turn(TurnInput::Events(vec![Event::Key(*key)]))?;
                let turned = Instant::now();
                if outcome.frame.is_some() {
                    frames += 1;
                    canopy.emit_for_testing(&mut backend)?;
                }
                let done = Instant::now();
                emits += done - turned;
                totals.push(done - began);
            }
        }
        let presses = totals.len();
        let (mean, p50, p95, max) = stats(&mut totals);
        println!(
            "{name:<10} {presses:>6} {mean:>9.1} {p50:>9.1} {p95:>9.1} {max:>9.1} {:>9.1} {frames:>9} {:>8}",
            emits.as_secs_f64() * 1e6 / presses as f64,
            backend.bytes / frames.max(1),
        );
    }
    Ok(())
}
