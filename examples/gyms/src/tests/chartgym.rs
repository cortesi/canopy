use std::{collections::BTreeSet, sync::Arc, time::Duration};

use canopy::{
    Register,
    error::Result,
    geom::{Point, Size},
    testing::{ManualClock, harness::Harness},
};
use canopy_widgets::{BigText, Root};

use super::{Mount, root_harness};
use crate::{
    chartgym::{ChartGym, binding_setup},
    demo_canopy,
};

/// Time of one step of the gym's live data.
const STEP: Duration = Duration::from_millis(100);

/// Returns a harness of the chart gym at `width` × `height`.
fn chartgym(width: u32, height: u32) -> Result<Harness> {
    root_harness(
        ChartGym::new(),
        |builder| binding_setup(builder.configure(ChartGym::register)),
        Size::new(width, height),
        Mount::Wrap,
    )
}

/// Returns a harness of the chart gym whose time moves only when the test
/// moves it, and the clock that moves it.
fn clocked(width: u32, height: u32) -> Result<(Harness, Arc<ManualClock>)> {
    let clock = Arc::new(ManualClock::new());
    let mut canopy = binding_setup(demo_canopy().configure(ChartGym::register))
        .assemble(|canopy| {
            Root::new().install(canopy, ChartGym::new())?;
            Ok(())
        })
        .build()?;
    canopy.set_clock_for_testing(Arc::clone(&clock))?;
    let mut harness = Harness::from_canopy(canopy, Size::new(width, height))?;
    harness.render()?;
    Ok((harness, clock))
}

/// Moves the clock on `steps` steps, and runs the turns that each step makes
/// due.
fn step(harness: &mut Harness, clock: &ManualClock, steps: u32) -> Result<()> {
    for _ in 0..steps {
        clock.advance(STEP)?;
        let mut checks = 0;
        harness.wait_until(Duration::from_millis(50), |_| {
            checks += 1;
            Ok(checks > 1)
        })?;
    }
    harness.render()
}

/// Shows page `index` of the gym and renders it.
fn show_page(harness: &mut Harness, index: usize) -> Result<()> {
    harness.script(&format!("chart_gym.show_page({index})"))?;
    harness.render()
}

/// Sends one key and renders.
fn press(harness: &mut Harness, key: &str) -> Result<()> {
    harness.script(&format!("canopy.send_key('{key}')"))?;
    harness.render()
}

/// Returns the row of the first line that contains `needle`.
fn row_of(harness: &Harness, needle: &str) -> usize {
    harness
        .tbuf()
        .lines()
        .iter()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line contains {needle}"))
}

/// Returns the screen text.
fn text(harness: &Harness) -> String {
    harness.tbuf().lines().join("\n")
}

#[test]
fn the_widgets_page_shows_tiles_sparklines_and_meters() -> Result<()> {
    let harness = chartgym(110, 44)?;
    let text = text(&harness);
    for part in [
        "chart gym · default dark · live",
        "widgets",
        "tokens",
        "cache hit",
        "output",
        "context",
        "sparklines",
        "meters",
        "gradient",
    ] {
        assert!(text.contains(part), "{part} is on screen:\n{text}");
    }
    let gaps = &harness.tbuf().lines()[row_of(&harness, "bars, gaps")];
    assert!(gaps.contains('·'), "the sparkline shows its gaps: {gaps}");
    Ok(())
}

#[test]
fn the_text_page_draws_every_glyph() -> Result<()> {
    let mut harness = chartgym(110, 44)?;
    show_page(&mut harness, 1)?;
    let text = text(&harness);
    for line in ["ABCDEFGHIJKLM", "NOPQRSTUVWXYZ", "0123456789 +-×÷=%<>≤≥"] {
        let rows = BigText::rows_of(line);
        assert!(
            rows.iter().all(|row| text.contains(row.as_str())),
            "{line} draws in big text:\n{text}"
        );
    }
    Ok(())
}

#[test]
fn the_primitives_page_draws_every_section() -> Result<()> {
    let mut harness = chartgym(110, 44)?;
    show_page(&mut harness, 2)?;
    let lines = harness.tbuf().lines();
    let text = lines.join("\n");
    for heading in ["stacked bars", "eighths", "gradient", "columns", "braille"] {
        assert!(text.contains(heading), "{heading} is on screen:\n{text}");
    }
    let ramps = &lines[row_of(&harness, "eighths") + 1];
    assert!(ramps.contains("▏ ▎ ▍ ▌ ▋ ▊ ▉ █"), "{ramps}");
    assert!(ramps.contains("▁ ▂ ▃ ▄ ▅ ▆ ▇ █"), "{ramps}");
    Ok(())
}

#[test]
fn a_composition_bar_shows_three_tints() -> Result<()> {
    let mut harness = chartgym(110, 44)?;
    show_page(&mut harness, 2)?;
    let y = u32::try_from(row_of(&harness, "read ")).expect("row");
    let colors = (0..110)
        .filter_map(|x| harness.buf().get(Point { x, y }))
        .filter(|cell| "█▏▎▍▌▋▊▉".contains(cell.ch))
        .flat_map(|cell| [cell.style.fg, cell.style.bg])
        .map(|color| format!("{color:?}"))
        .collect::<BTreeSet<_>>();
    // The parts fill the bar, so no track shows.
    assert_eq!(colors.len(), 3, "read, write, and fresh: {colors:?}");
    Ok(())
}

#[test]
fn a_short_terminal_scrolls_to_the_end_of_each_page() -> Result<()> {
    let mut harness = chartgym(80, 24)?;
    show_page(&mut harness, 1)?;
    assert!(
        !text(&harness).contains("a live clock"),
        "the clock starts below"
    );
    press(&mut harness, "End")?;
    assert!(
        text(&harness).contains("a live clock"),
        "{}",
        text(&harness)
    );
    show_page(&mut harness, 2)?;
    assert!(
        !text(&harness).contains("braille"),
        "the braille line starts below"
    );
    press(&mut harness, "End")?;
    assert!(text(&harness).contains("braille"), "{}", text(&harness));
    Ok(())
}

#[test]
fn the_gym_draws_at_a_small_size() -> Result<()> {
    for (width, height) in [(24, 8), (4, 2)] {
        let mut harness = chartgym(width, height)?;
        for page in 0..3 {
            show_page(&mut harness, page)?;
        }
    }
    Ok(())
}

#[test]
fn the_data_moves_until_the_gym_pauses() -> Result<()> {
    let (mut harness, clock) = clocked(110, 44)?;
    let before = text(&harness);
    step(&mut harness, &clock, 3)?;
    assert_ne!(text(&harness), before, "the data moves");
    harness.script("chart_gym.toggle_pause()")?;
    harness.render()?;
    let paused = text(&harness);
    assert!(paused.contains("· paused"), "{paused}");
    step(&mut harness, &clock, 5)?;
    assert_eq!(text(&harness), paused, "a paused gym stands still");
    harness.script("chart_gym.toggle_pause()")?;
    harness.render()?;
    let resumed = text(&harness);
    assert!(resumed.contains("· live"), "{resumed}");
    step(&mut harness, &clock, 3)?;
    assert_ne!(text(&harness), resumed, "the data moves again");
    Ok(())
}

#[test]
fn muting_fades_every_other_attempt() -> Result<()> {
    let mut harness = chartgym(110, 44)?;
    harness.script("chart_gym.toggle_pause()")?;
    show_page(&mut harness, 2)?;
    // The bottom row of the input columns lies just above the axis, and the
    // columns start where the axis starts.
    let axis = row_of(&harness, "──────────");
    let lines = harness.tbuf().lines();
    let start = lines[axis].chars().position(|ch| ch == '─').expect("axis");
    let y = u32::try_from(axis - 1).expect("row");
    let x = u32::try_from(start + 10).expect("column");
    let style = |harness: &Harness, x| harness.buf().get(Point { x, y }).map(|cell| cell.style);
    let before = (style(&harness, x), style(&harness, x + 1));
    harness.script("chart_gym.toggle_mute()")?;
    harness.render()?;
    let after = (style(&harness, x), style(&harness, x + 1));
    let changed = usize::from(before.0 != after.0) + usize::from(before.1 != after.1);
    assert_eq!(
        changed, 1,
        "one of two neighbors fades: {before:?} {after:?}"
    );
    Ok(())
}
