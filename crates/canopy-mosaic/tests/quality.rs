//! The quality harness: every item of the corpus, matched in every tier,
//! scored against committed baselines.
//!
//! Two scores, each PSNR after a Gaussian blur in display space at σ = 1
//! and σ = 2 display pixels, on the display of an ideal terminal. Fidelity
//! compares the display with the source. Stray compares the display with
//! the display that the source explains, so it falls only for marks that
//! nothing near in the source explains, such as a spur of ink below a
//! stroke. A drop of more than 0.05 dB below a baseline fails.
//! `UPDATE_GOLDEN=1` rewrites the baselines after an improvement.

#[cfg(test)]
#[path = "support/corpus.rs"]
mod corpus;

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, env, fs, iter, path::PathBuf};

    use canopy_mosaic::Tier;

    use super::corpus::{Objective, Pixels, explained, items, matched, objectives, psnr};

    /// Largest drop in dB below a baseline that passes.
    const TOLERANCE: f64 = 0.05;

    /// The baselines file.
    fn golden() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/quality.golden")
    }

    /// Scores to each key: fidelity at σ = 1 and σ = 2, then stray at σ = 1
    /// and σ = 2.
    const SCORES: usize = 4;

    /// The scores of every item, tier, and objective.
    fn scores() -> Vec<(String, [f64; SCORES])> {
        let mut scores = Vec::new();
        for item in items() {
            let width = item.cols * 8;
            for objective in objectives(&item) {
                for tier in Tier::ALL {
                    let (source, shown) = matched(&item, tier, *objective);
                    let explained = explained(&source, &shown, width);
                    let key = format!("{} {} {}", item.name, tier.name(), objective.name());
                    let values = [
                        psnr(&source, &shown, width, 1.0),
                        psnr(&source, &shown, width, 2.0),
                        psnr(&explained, &shown, width, 1.0),
                        psnr(&explained, &shown, width, 2.0),
                    ];
                    scores.push((key, values));
                }
            }
        }
        scores
    }

    #[test]
    fn quality_holds_its_baselines() {
        let scores = scores();
        let header = "# item tier objective: PSNR in dB of fidelity at sigma 1 and 2, \
                      then of stray at sigma 1 and 2\n";
        let text: String = iter::once(header.to_owned())
            .chain(scores.iter().map(|(key, values)| {
                let values: Vec<String> = values.iter().map(|v| format!("{v:.2}")).collect();
                format!("{key}: {}\n", values.join(" "))
            }))
            .collect();
        if env::var_os("UPDATE_GOLDEN").is_some() {
            fs::write(golden(), &text).expect("write the baselines");
            return;
        }
        let baselines: HashMap<String, Vec<f64>> = fs::read_to_string(golden())
            .expect("baselines: run with UPDATE_GOLDEN=1 to create them")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| {
                let (key, values) = line.split_once(": ")?;
                let values: Option<Vec<f64>> = values.split(' ').map(|v| v.parse().ok()).collect();
                Some((key.to_owned(), values?))
            })
            .collect();
        let mut drops = Vec::new();
        for (key, values) in &scores {
            let baseline = baselines
                .get(key)
                .filter(|baseline| baseline.len() == SCORES)
                .unwrap_or_else(|| panic!("no baseline for {key}: run with UPDATE_GOLDEN=1"));
            for ((value, base), name) in values.iter().zip(baseline).zip([
                "fidelity at sigma 1",
                "fidelity at sigma 2",
                "stray at sigma 1",
                "stray at sigma 2",
            ]) {
                if *value < base - TOLERANCE {
                    drops.push(format!("{key} {name}: {value:.2} below {base:.2}"));
                }
            }
        }
        assert!(drops.is_empty(), "quality dropped:\n{}", drops.join("\n"));
    }

    #[test]
    fn every_tier_equals_or_beats_todays_renderers() {
        // Today's ImageView draws half blocks, and today's FontRenderer
        // draws quadrants.
        for item in items() {
            let width = item.cols * 8;
            let today = match item.pixels {
                Pixels::Rgb(_) => Tier::Halves,
                Pixels::Coverage(_) => Tier::Quadrants,
            };
            for objective in objectives(&item) {
                let score = |tier| {
                    let (source, shown) = matched(&item, tier, *objective);
                    [1.0, 2.0].map(|sigma| psnr(&source, &shown, width, sigma))
                };
                let baseline = score(today);
                for tier in [Tier::Blocks, Tier::Full] {
                    let better = score(tier);
                    for (value, base) in better.iter().zip(&baseline) {
                        assert!(
                            *value >= base - TOLERANCE,
                            "{} {} {}: {value:.2} below today's {base:.2}",
                            item.name,
                            tier.name(),
                            objective.name()
                        );
                    }
                }
                // The full tier gains clearly on every item at σ = 1.
                let full = score(Tier::Full);
                if *objective != Objective::Fixed {
                    assert!(
                        full[0] > baseline[0] + 0.5,
                        "{} {}: {:.2} against today's {:.2}",
                        item.name,
                        objective.name(),
                        full[0],
                        baseline[0]
                    );
                }
            }
        }
    }
}
