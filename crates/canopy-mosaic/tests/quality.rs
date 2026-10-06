//! The quality harness: every item of the corpus, matched in every tier,
//! scored against committed baselines.
//!
//! The score is PSNR after a Gaussian blur in display space, at σ = 1 and
//! σ = 2 display pixels, on the display of an ideal terminal. A drop of more
//! than 0.05 dB below a baseline fails. `UPDATE_GOLDEN=1` rewrites the
//! baselines after an improvement.

#[cfg(test)]
#[path = "support/corpus.rs"]
mod corpus;

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, env, fs, iter, path::PathBuf};

    use canopy_mosaic::Tier;

    use super::corpus::{Objective, Pixels, items, matched, objectives, psnr};

    /// Largest drop in dB below a baseline that passes.
    const TOLERANCE: f64 = 0.05;

    /// The baselines file.
    fn golden() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/quality.golden")
    }

    /// The scores of every item, tier, and objective: the key, and the PSNR
    /// at σ = 1 and σ = 2.
    fn scores() -> Vec<(String, [f64; 2])> {
        let mut scores = Vec::new();
        for item in items() {
            let width = item.cols * 8;
            for objective in objectives(&item) {
                for tier in Tier::ALL {
                    let (source, shown) = matched(&item, tier, *objective);
                    let key = format!("{} {} {}", item.name, tier.name(), objective.name());
                    let values = [1.0, 2.0].map(|sigma| psnr(&source, &shown, width, sigma));
                    scores.push((key, values));
                }
            }
        }
        scores
    }

    #[test]
    fn quality_holds_its_baselines() {
        let scores = scores();
        let text: String =
            iter::once("# item tier objective: PSNR in dB at sigma 1 and sigma 2\n".to_owned())
                .chain(
                    scores
                        .iter()
                        .map(|(key, [a, b])| format!("{key}: {a:.2} {b:.2}\n")),
                )
                .collect();
        if env::var_os("UPDATE_GOLDEN").is_some() {
            fs::write(golden(), &text).expect("write the baselines");
            return;
        }
        let baselines: HashMap<String, [f64; 2]> = fs::read_to_string(golden())
            .expect("baselines: run with UPDATE_GOLDEN=1 to create them")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| {
                let (key, values) = line.split_once(": ")?;
                let mut values = values.split(' ').map(|v| v.parse::<f64>().ok());
                Some((key.to_owned(), [values.next()??, values.next()??]))
            })
            .collect();
        let mut drops = Vec::new();
        for (key, values) in &scores {
            let baseline = baselines
                .get(key)
                .unwrap_or_else(|| panic!("no baseline for {key}: run with UPDATE_GOLDEN=1"));
            for (value, base) in values.iter().zip(baseline) {
                if *value < base - TOLERANCE {
                    drops.push(format!("{key}: {value:.2} below {base:.2}"));
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
