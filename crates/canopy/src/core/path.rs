use std::{fmt, str::FromStr};

use crate::error::{self, Result};

/// A path of node name components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    /// Stored path components.
    path: Vec<String>,
}

impl FromStr for Path {
    type Err = error::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(Self::from(s))
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/{}", self.path.join("/"))
    }
}

impl Path {
    /// Construct an empty path.
    pub fn empty() -> Self {
        Self { path: vec![] }
    }

    /// Pop an item off the end of the path, modifying it in place. Return None
    /// if the path is empty.
    pub fn pop(&mut self) -> Option<String> {
        self.path.pop()
    }

    /// Construct a path from a slice of components.
    pub fn new<I>(v: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        Self {
            path: v.into_iter().map(|x| x.as_ref().to_string()).collect(),
        }
    }
}

impl From<Vec<String>> for Path {
    fn from(path: Vec<String>) -> Self {
        Self { path }
    }
}

impl From<&[&str]> for Path {
    fn from(v: &[&str]) -> Self {
        Self {
            path: v
                .iter()
                .filter_map(|x| {
                    if x.is_empty() {
                        None
                    } else {
                        Some(x.to_string())
                    }
                })
                .collect(),
        }
    }
}

impl From<&str> for Path {
    fn from(v: &str) -> Self {
        Self {
            path: v
                .split('/')
                .filter_map(|x| {
                    if x.is_empty() {
                        None
                    } else {
                        Some(x.to_string())
                    }
                })
                .collect(),
        }
    }
}

/// A match expression that can be applied to paths.
/// The matcher supports `*` (one component), `**` (zero or more), and optional anchors.
#[derive(Debug, Clone)]
pub struct PathMatcher {
    /// Original filter string used to construct the matcher.
    filter: Box<str>,
    /// Parsed path pattern.
    pattern: PathPattern,
}

/// Path match metadata used for input precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathMatch {
    /// Count of literal segments in the pattern.
    pub literals: usize,
    /// Number of path components matched.
    pub depth: usize,
    /// Whether the match ends at the end of the path.
    pub anchored_end: bool,
}

impl PathMatch {
    /// Score tuple used for match precedence.
    pub(crate) fn score(&self) -> (usize, usize, usize) {
        (self.literals, usize::from(self.anchored_end), self.depth)
    }
}

/// Parsed path pattern metadata.
#[derive(Debug, Clone)]
struct PathPattern {
    /// Require matches to start at the root.
    anchor_start: bool,
    /// Require matches to end at the path terminus.
    anchor_end: bool,
    /// Pattern segments to match.
    segments: Vec<Segment>,
    /// Count of literal segments in the pattern.
    literals: usize,
}

/// Pattern segment kinds used by the matcher.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    /// Literal component match.
    Lit(String),
    /// Match exactly one component.
    Any,
    /// Match zero or more components.
    AnyDeep,
}

impl PathMatcher {
    /// Compile a path matcher from a filter string.
    pub fn new(path: &str) -> Result<Self> {
        let anchor_start = path.starts_with('/');
        let anchor_end = path.ends_with('/');
        let mut segments = Vec::new();
        let mut literals = 0;
        for part in path.split('/') {
            if part.is_empty() {
                continue;
            }
            let seg = match part {
                "*" => Segment::Any,
                "**" => Segment::AnyDeep,
                _ => {
                    literals += 1;
                    Segment::Lit(part.to_string())
                }
            };
            segments.push(seg);
        }
        Ok(Self {
            filter: path.into(),
            pattern: PathPattern {
                anchor_start,
                anchor_end,
                segments,
                literals,
            },
        })
    }

    /// Return the original filter string used to construct this matcher.
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Check whether the path filter matches a given path.
    /// Returns the matched depth for use in quick checks.
    pub fn check(&self, path: &Path) -> Option<usize> {
        self.check_match(path).map(|m| m.depth)
    }

    /// Check whether the path filter matches a given path, returning match metadata.
    pub fn check_match(&self, path: &Path) -> Option<PathMatch> {
        let parts = &path.path;
        let mut best: Option<PathMatch> = None;
        let starts = if self.pattern.anchor_start {
            0..=0
        } else {
            0..=parts.len()
        };
        for start in starts {
            if let Some(end) = match_end(&self.pattern.segments, parts, start) {
                if self.pattern.anchor_end && end != parts.len() {
                    continue;
                }
                let depth = end.saturating_sub(start);
                let candidate = PathMatch {
                    literals: self.pattern.literals,
                    depth,
                    anchored_end: end == parts.len() && depth > 0,
                };
                if best.is_none_or(|best| candidate.score() > best.score()) {
                    best = Some(candidate);
                }
            }
        }
        best
    }
}

/// Return the furthest end index for a match starting at `start`.
fn match_end(segments: &[Segment], parts: &[String], start: usize) -> Option<usize> {
    if segments.is_empty() {
        return Some(start);
    }
    walk_match_end(segments, parts, 0, start)
}

/// Recursively resolve the furthest matching end index for a segment sequence.
fn walk_match_end(
    segments: &[Segment],
    parts: &[String],
    seg_idx: usize,
    part_idx: usize,
) -> Option<usize> {
    if seg_idx == segments.len() {
        return Some(part_idx);
    }
    match &segments[seg_idx] {
        Segment::Lit(lit) => {
            if part_idx < parts.len() && parts[part_idx] == *lit {
                walk_match_end(segments, parts, seg_idx + 1, part_idx + 1)
            } else {
                None
            }
        }
        Segment::Any => {
            if part_idx < parts.len() {
                walk_match_end(segments, parts, seg_idx + 1, part_idx + 1)
            } else {
                None
            }
        }
        Segment::AnyDeep => {
            let mut best: Option<usize> = None;
            for next in part_idx..=parts.len() {
                if let Some(end) = walk_match_end(segments, parts, seg_idx + 1, next) {
                    best = Some(best.map_or(end, |best_end| best_end.max(end)));
                }
            }
            best
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn pathfilter() -> Result<()> {
        let v = PathMatcher::new("")?;
        assert!(v.check(&"/any/thing".into()).is_some());
        assert!(v.check(&"/".into()).is_some());

        let v = PathMatcher::new("bar")?;
        assert!(v.check(&"/foo/bar".into()).is_some());
        assert!(v.check(&"/bar/foo".into()).is_some());
        assert!(v.check(&"/foo/foo".into()).is_none());

        let v = PathMatcher::new("foo/*/bar")?;
        assert!(v.check(&"/foo/oink/bar".into()).is_some());
        assert!(v.check(&"/oink/foo/oink/bar/oink".into()).is_some());
        assert!(v.check(&"/foo/bar".into()).is_none());
        assert!(v.check(&"/foo/oink/oink/bar".into()).is_none());

        let v = PathMatcher::new("/foo")?;
        assert!(v.check(&"/foo".into()).is_some());
        assert!(v.check(&"/foo/bar".into()).is_some());
        assert!(v.check(&"/bar/foo/bar".into()).is_none());

        let v = PathMatcher::new("foo/")?;
        assert!(v.check(&"/foo".into()).is_some());
        assert!(v.check(&"/bar/foo".into()).is_some());
        assert!(v.check(&"/foo/bar".into()).is_none());

        let v = PathMatcher::new("foo/**/bar")?;
        assert!(v.check(&"/foo/bar".into()).is_some());
        assert!(v.check(&"/foo/x/bar".into()).is_some());
        assert!(v.check(&"/foo/x/y/bar".into()).is_some());
        assert!(v.check(&"/bar/foo/x/bar/x".into()).is_some());

        let v = PathMatcher::new("foo/**/bar/")?;
        assert!(v.check(&"/foo/bar".into()).is_some());
        assert!(v.check(&"/foo/x/bar".into()).is_some());
        assert!(v.check(&"/foo/x/bar/x".into()).is_none());

        Ok(())
    }

    proptest! {
        #[test]
        fn literal_path_matches_when_anchored(components in prop::collection::vec("[a-z]{1,8}", 1..6)) {
            let join = components.join("/");
            let matcher = PathMatcher::new(&format!("/{join}/")).expect("matcher");
            let path = Path::new(&components);
            let m = matcher.check_match(&path).expect("match");
            prop_assert_eq!(m.literals, components.len());
            prop_assert_eq!(m.depth, components.len());
            prop_assert!(m.anchored_end);
        }

        #[test]
        fn any_deep_matches_all_paths(components in prop::collection::vec("[a-z]{1,8}", 0..6)) {
            let matcher = PathMatcher::new("**").expect("matcher");
            let path = Path::new(&components);
            let m = matcher.check_match(&path).expect("match");
            prop_assert_eq!(m.depth, components.len());
        }
    }
}
