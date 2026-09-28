//! Fuzzy ranking of labels against a typed query.
//!
//! A query matches a label when its characters appear in the label in order,
//! ignoring case. Every alignment is scored, so the best label sorts first: a
//! match that starts the label or a word scores more, a run of consecutive
//! characters more again, and among equal alignments the shorter label wins. A
//! list that jumps to what the operator types, or a picker that narrows its
//! rows, ranks its labels with [`rank`].

/// Score added for each matched query character.
const MATCH: i32 = 1;
/// Score added when a match starts a label or follows a word separator.
const BOUNDARY: i32 = 8;
/// Score added when a match continues a run of consecutive characters.
const CONSECUTIVE: i32 = 12;
/// Characters that end a word in a label.
const SEPARATORS: &[char] = &['/', '\\', '-', '_', '.', ':', '@', ' '];

/// Return whether `character` ends a word in a label.
fn is_separator(character: char) -> bool {
    SEPARATORS.contains(&character)
}

/// Return `text` in lower case.
fn lowered(text: &str) -> Vec<char> {
    text.chars().flat_map(char::to_lowercase).collect()
}

/// Score `label` against `query`, ignoring case, or return `None` when the
/// query is no ordered subsequence of the label. An empty query scores zero.
///
/// This is the alignment score. [`rank`] also prefers shorter labels when
/// alignments score equally. The score's scale means nothing on its own.
pub fn score(query: &str, label: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    score_chars(&lowered(query), &lowered(label))
}

/// Score a lowercased `query` against a lowercased `label`.
///
/// The two rows are rewritten in place for each query character, so scoring
/// allocates nothing beyond them.
fn score_chars(query: &[char], label: &[char]) -> Option<i32> {
    // `end[index]` scores the best alignment that matches the last query
    // character at `index`, and `best[index]` the best alignment that matches
    // it earlier. `best[label.len()]` closes the last row.
    let mut end = vec![None; label.len()];
    let mut best = vec![Some(0); label.len() + 1];
    for wanted in query {
        let mut running = None;
        let mut previous_end = None;
        for (index, found) in label.iter().enumerate() {
            let previous = end[index];
            let mut cell = None;
            if found == wanted {
                let boundary = i32::from(index == 0 || is_separator(label[index - 1]));
                cell = best[index].map(|score| score + MATCH + boundary * BOUNDARY);
                if let Some(score) = previous_end {
                    let consecutive = score + MATCH + CONSECUTIVE + boundary * BOUNDARY;
                    cell = Some(cell.map_or(consecutive, |cell| cell.max(consecutive)));
                }
            }
            end[index] = cell;
            best[index] = running;
            running = running.max(cell);
            previous_end = previous;
        }
        best[label.len()] = running;
    }
    best[label.len()]
}

/// Return the indexes of the labels that match `query`, best first.
///
/// An empty query returns every index in order. Equally scoring labels sort
/// shortest first, then keep their original order.
pub fn rank<'a>(query: &str, labels: impl IntoIterator<Item = &'a str>) -> Vec<usize> {
    let labels = labels.into_iter();
    if query.is_empty() {
        return labels.enumerate().map(|(index, _)| index).collect();
    }
    let query = lowered(query);
    let mut scored: Vec<(i32, usize, usize)> = labels
        .enumerate()
        .filter_map(|(index, label)| {
            let label = lowered(label);
            score_chars(&query, &label).map(|score| (score, label.len(), index))
        })
        .collect();
    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });
    scored.into_iter().map(|(_, _, index)| index).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Return the labels matching `query`, best first.
    fn ranked<'a>(labels: &[&'a str], query: &str) -> Vec<&'a str> {
        rank(query, labels.iter().copied())
            .into_iter()
            .map(|index| labels[index])
            .collect()
    }

    #[test]
    fn a_query_matches_ordered_characters_and_ignores_case() {
        assert!(score("nts", "notes.txt").is_some(), "n-t-s in order");
        assert!(score("NTS", "notes.txt").is_some(), "case is ignored");
        assert!(score("子", "子目录").is_some(), "unicode matches");
        assert!(score("stn", "notes.txt").is_none(), "order matters");
        assert!(score("nz", "notes.txt").is_none(), "missing characters");
        assert_eq!(score("", "anything"), Some(0));
    }

    #[test]
    fn an_empty_query_keeps_every_label_in_order() {
        assert_eq!(ranked(&["b.txt", "a.txt"], ""), ["b.txt", "a.txt"]);
    }

    #[test]
    fn boundaries_and_runs_outrank_scattered_matches() {
        assert_eq!(
            ranked(&["README.md", "src/main.rs", "rm-rf.sh"], "rm"),
            ["rm-rf.sh", "README.md", "src/main.rs"],
            "a label that starts the query wins"
        );
        assert_eq!(
            ranked(&["xayb", "ab.txt", "axxb"], "ab"),
            ["ab.txt", "axxb", "xayb"],
            "a run at the start of a label wins"
        );
        assert_eq!(
            ranked(&["rabbit.rs", "random.stuff"], "rs"),
            ["rabbit.rs", "random.stuff"],
            "a run after a separator beats a scattered match"
        );
        assert_eq!(
            ranked(&["@core/agent", "@app/files", "@core/fs"], "fs"),
            ["@core/fs", "@app/files"],
            "a word after a slash starts a match"
        );
    }

    #[test]
    fn labels_that_do_not_match_are_dropped() {
        assert_eq!(
            ranked(&["notes.txt", "alpha.md"], "no"),
            ["notes.txt"],
            "only subsequence matches survive"
        );
    }

    #[test]
    fn length_breaks_equal_alignment_scores_without_overriding_better_matches() {
        assert_eq!(score("a", "ax"), score("a", "axy"));
        assert_eq!(ranked(&["axy", "ax"], "a"), ["ax", "axy"]);
        assert_eq!(ranked(&["axy", "axz"], "a"), ["axy", "axz"]);
        assert_eq!(
            ranked(&["axb", "a..............b", "ab-long-long-long"], "ab")[0],
            "ab-long-long-long",
            "a consecutive match outranks a shorter scattered match"
        );
    }
}
