//! Aggregate scores, compare them with a baseline, and print the report.
//!
//! The score of an item in a tier is the mean CIEDE2000 difference between
//! its reference and its display after S-CIELAB, averaged over the viewing
//! conditions. Lower is better. A group scores the geometric mean of its
//! items, so each item counts by its ratio to the baseline whatever its
//! scale. The suite scores the geometric mean of its groups.

use std::{collections::BTreeMap, fmt::Write as _, fs, io, path::Path};

use canopy_mosaic::Tier;

use crate::suite::Group;

/// A viewing condition.
#[derive(Debug, Clone, Copy)]
pub struct Condition {
    /// The name of the condition.
    pub name: &'static str,
    /// Cell columns to each degree of visual angle.
    pub cols_per_degree: f64,
}

/// The viewing conditions. A 13 point terminal font on a laptop at 50 cm
/// gives about 3 columns to a degree. The same font on a 27 inch monitor of
/// 2560 points across at 70 cm gives about 6.
pub const CONDITIONS: [Condition; 2] = [
    Condition {
        name: "near",
        cols_per_degree: 3.0,
    },
    Condition {
        name: "far",
        cols_per_degree: 6.0,
    },
];

/// One score.
#[derive(Debug, Clone)]
pub struct Score {
    /// The item.
    pub item: String,
    /// The item's group.
    pub group: Group,
    /// The tier.
    pub tier: Tier,
    /// The viewing condition.
    pub condition: &'static str,
    /// The mean CIEDE2000 difference.
    pub value: f64,
}

impl Score {
    /// The key of the score in a baseline.
    fn key(&self) -> String {
        format!("{} {} {}", self.item, self.tier.name(), self.condition)
    }
}

/// Scores by key, as a baseline file holds them.
pub type Baseline = BTreeMap<String, f64>;

/// Read a baseline, or none when the file does not exist.
pub fn read_baseline(path: &Path) -> io::Result<Option<Baseline>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut baseline = Baseline::new();
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let (key, value) = line
            .rsplit_once(' ')
            .ok_or_else(|| io::Error::other(format!("bad baseline line: {line}")))?;
        let value = value
            .parse()
            .map_err(|_| io::Error::other(format!("bad baseline value: {line}")))?;
        baseline.insert(key.to_owned(), value);
    }
    Ok(Some(baseline))
}

/// Write scores as a baseline.
pub fn write_baseline(path: &Path, scores: &[Score]) -> io::Result<()> {
    let mut text = String::from(
        "# Mean CIEDE2000 difference after S-CIELAB, by item, tier, and viewing\n\
         # condition. `cargo xtask score --update` rewrites it.\n",
    );
    let sorted: BTreeMap<String, f64> = scores.iter().map(|s| (s.key(), s.value)).collect();
    for (key, value) in sorted {
        writeln!(text, "{key} {value:.6}").expect("write to string");
    }
    fs::write(path, text)
}

/// The geometric mean of positive values.
fn geomean(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values
        .into_iter()
        .fold((0.0, 0usize), |(sum, n), v| (sum + v.max(1e-9).ln(), n + 1));
    (n > 0).then(|| (sum / n as f64).exp())
}

/// Scores aggregated by item, group, and suite, for each tier.
struct Summary {
    /// Each item's score in each tier, with its group.
    items: Vec<(String, Group, BTreeMap<Tier, f64>)>,
    /// Each group's score in each tier.
    groups: BTreeMap<Group, BTreeMap<Tier, f64>>,
    /// The suite's score in each tier.
    suite: BTreeMap<Tier, f64>,
}

impl Summary {
    /// Aggregate scores, the value of each read through `value`. Items
    /// without a value in every condition drop out.
    fn new(scores: &[Score], value: impl Fn(&Score) -> Option<f64>) -> Self {
        let mut per_item: BTreeMap<(Group, String), BTreeMap<Tier, Vec<f64>>> = BTreeMap::new();
        for score in scores {
            if let Some(v) = value(score) {
                per_item
                    .entry((score.group, score.item.clone()))
                    .or_default()
                    .entry(score.tier)
                    .or_default()
                    .push(v);
            }
        }
        let items: Vec<(String, Group, BTreeMap<Tier, f64>)> = per_item
            .into_iter()
            .map(|((group, item), tiers)| {
                let tiers = tiers
                    .into_iter()
                    .filter(|(_, values)| values.len() == CONDITIONS.len())
                    .map(|(tier, values)| (tier, values.iter().sum::<f64>() / values.len() as f64))
                    .collect();
                (item, group, tiers)
            })
            .collect();
        let mut groups = BTreeMap::new();
        for group in Group::ALL {
            let mut tiers = BTreeMap::new();
            for tier in Tier::ALL {
                let values = items
                    .iter()
                    .filter(|(_, g, _)| *g == group)
                    .filter_map(|(_, _, scores)| scores.get(&tier).copied());
                if let Some(mean) = geomean(values) {
                    tiers.insert(tier, mean);
                }
            }
            if !tiers.is_empty() {
                groups.insert(group, tiers);
            }
        }
        let suite = Tier::ALL
            .into_iter()
            .filter_map(|tier| {
                geomean(
                    groups
                        .values()
                        .filter_map(|tiers| tiers.get(&tier).copied()),
                )
                .map(|mean| (tier, mean))
            })
            .collect();
        Self {
            items,
            groups,
            suite,
        }
    }
}

/// The change from a baseline value, as a percentage.
fn change(now: f64, before: Option<f64>) -> String {
    before.map_or_else(String::new, |before| {
        let percent = (now / before - 1.0) * 100.0;
        if percent.abs() < 0.005 {
            "0.00%".to_owned()
        } else {
            format!("{percent:+.2}%")
        }
    })
}

/// Format a row: a label, a value for each tier, and a change.
fn row(out: &mut String, label: &str, tiers: &BTreeMap<Tier, f64>, delta: &str) {
    write!(out, "{label:<26}").expect("write to string");
    for tier in Tier::ALL {
        match tiers.get(&tier) {
            Some(v) => write!(out, " {v:>9.4}"),
            None => write!(out, " {:>9}", "-"),
        }
        .expect("write to string");
    }
    writeln!(out, " {delta:>9}").expect("write to string");
}

/// The report of a run: each item, each group, and the suite, in each tier,
/// with the change of the full tier from the baseline.
pub fn report(scores: &[Score], baseline: Option<&Baseline>, detail: bool) -> String {
    let now = Summary::new(scores, |s| Some(s.value));
    let before = baseline.map(|b| Summary::new(scores, |s| b.get(&s.key()).copied()));
    let before_item = |item: &str| {
        before.as_ref().and_then(|b| {
            b.items
                .iter()
                .find(|(name, _, _)| name == item)
                .and_then(|(_, _, tiers)| tiers.get(&Tier::Full).copied())
        })
    };
    let mut out = String::new();
    write!(out, "{:<26}", "item").expect("write to string");
    for tier in Tier::ALL {
        write!(out, " {:>9}", tier.name()).expect("write to string");
    }
    writeln!(out, " {:>9}", "full vs base").expect("write to string");
    for group in Group::ALL {
        let members: Vec<_> = now.items.iter().filter(|(_, g, _)| *g == group).collect();
        if members.is_empty() {
            continue;
        }
        writeln!(out, "\n{}", group.name()).expect("write to string");
        for (item, _, tiers) in members {
            let delta = tiers
                .get(&Tier::Full)
                .map_or_else(String::new, |v| change(*v, before_item(item)));
            row(&mut out, &format!("  {item}"), tiers, &delta);
            if detail {
                for condition in CONDITIONS {
                    let tiers: BTreeMap<Tier, f64> = scores
                        .iter()
                        .filter(|s| &s.item == item && s.condition == condition.name)
                        .map(|s| (s.tier, s.value))
                        .collect();
                    row(&mut out, &format!("    {}", condition.name), &tiers, "");
                }
            }
        }
    }
    writeln!(out).expect("write to string");
    for (group, tiers) in &now.groups {
        let base = before
            .as_ref()
            .and_then(|b| b.groups.get(group))
            .and_then(|tiers| tiers.get(&Tier::Full).copied());
        let delta = tiers
            .get(&Tier::Full)
            .map_or_else(String::new, |v| change(*v, base));
        row(&mut out, group.name(), tiers, &delta);
    }
    let base = before
        .as_ref()
        .and_then(|b| b.suite.get(&Tier::Full).copied());
    let delta = now
        .suite
        .get(&Tier::Full)
        .map_or_else(String::new, |v| change(*v, base));
    row(&mut out, "suite", &now.suite, &delta);
    if let Some(before) = &before {
        write!(out, "{:<26}", "suite vs base").expect("write to string");
        for tier in Tier::ALL {
            let delta = now.suite.get(&tier).map_or_else(String::new, |v| {
                change(*v, before.suite.get(&tier).copied())
            });
            write!(out, " {delta:>9}").expect("write to string");
        }
        writeln!(out).expect("write to string");
    }
    out
}

#[cfg(test)]
mod tests {
    use std::{env, process};

    use super::*;

    /// A score of an item in a tier, the same in each condition.
    fn scores(item: &str, group: Group, tier: Tier, value: f64) -> Vec<Score> {
        CONDITIONS
            .iter()
            .map(|c| Score {
                item: item.into(),
                group,
                tier,
                condition: c.name,
                value,
            })
            .collect()
    }

    #[test]
    fn groups_take_the_geometric_mean_and_the_suite_weighs_groups_alike() {
        let mut all = scores("a", Group::Images, Tier::Full, 1.0);
        all.extend(scores("b", Group::Images, Tier::Full, 4.0));
        all.extend(scores("c", Group::Text, Tier::Full, 8.0));
        let summary = Summary::new(&all, |s| Some(s.value));
        assert!((summary.groups[&Group::Images][&Tier::Full] - 2.0).abs() < 1e-9);
        assert!((summary.suite[&Tier::Full] - 4.0).abs() < 1e-9);
    }

    #[test]
    fn baselines_round_trip_and_the_report_shows_the_change() {
        let dir = env::temp_dir().join(format!("mosaic-score-{}", process::id()));
        fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("baseline.txt");
        let before = scores("photo", Group::Images, Tier::Full, 2.0);
        write_baseline(&path, &before).expect("write");
        let baseline = read_baseline(&path).expect("read").expect("present");
        assert_eq!(baseline.len(), CONDITIONS.len());
        let after = scores("photo", Group::Images, Tier::Full, 1.5);
        let text = report(&after, Some(&baseline), false);
        assert!(text.contains("-25.00%"), "{text}");
        fs::remove_dir_all(&dir).expect("clean up");
        assert!(read_baseline(&path).expect("read").is_none());
    }
}
