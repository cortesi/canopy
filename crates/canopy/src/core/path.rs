use std::{fmt, str::FromStr};

use crate::{
    error::{self, Result},
    state::NodeName,
};

/// A path of node name components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    /// Stored path components.
    path: Vec<String>,
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

impl From<&str> for Path {
    fn from(v: &str) -> Self {
        Self::new(v.split('/').filter(|part| !part.is_empty()))
    }
}

/// A validated path filter used to search node paths.
///
/// Filters support `*` for one component and `**` for zero or more components.
/// Literal components must be valid [`NodeName`] values.
#[derive(Debug, Clone)]
pub struct PathFilter {
    /// Original filter string used to construct the filter.
    filter: Box<str>,
    /// Parsed path pattern.
    pattern: PathPattern,
}

impl FromStr for PathFilter {
    type Err = error::Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

/// Path match metadata used for input precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PathMatch {
    /// Count of literal segments in the pattern.
    pub literals: usize,
    /// Number of path components matched.
    pub depth: usize,
    /// Whether the match ends at the end of the path and consumed at least one component.
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

impl PathFilter {
    /// Compile a validated path filter.
    ///
    /// Filters support `*` for one component and `**` for zero or more components. Literal
    /// components must be valid [`NodeName`] values.
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
                    NodeName::new(part)?;
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

    /// Compile a filter after normalizing it to a full-path match.
    pub fn normalized(filter: &str) -> Result<Self> {
        Self::new(&normalize_filter(filter))
    }

    /// Return the original filter string.
    pub fn as_str(&self) -> &str {
        &self.filter
    }

    /// Check whether the path filter matches a given path, returning match metadata.
    pub(crate) fn check_match(&self, path: &Path) -> Option<PathMatch> {
        let parts = &path.path;
        let mut best: Option<PathMatch> = None;
        let starts = if self.pattern.anchor_start {
            0..=0
        } else {
            0..=parts.len()
        };
        for start in starts {
            if let Some(end) = walk_match_end(&self.pattern.segments, parts, 0, start) {
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

/// Normalize a path filter to match a full path.
pub(crate) fn normalize_filter(path_filter: &str) -> String {
    let trimmed = path_filter.trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("/{trimmed}/")
    }
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
        let v = PathFilter::new("")?;
        assert!(v.check_match(&"/any/thing".into()).is_some());
        assert!(v.check_match(&"/".into()).is_some());

        let v = PathFilter::new("bar")?;
        assert!(v.check_match(&"/foo/bar".into()).is_some());
        assert!(v.check_match(&"/bar/foo".into()).is_some());
        assert!(v.check_match(&"/foo/foo".into()).is_none());

        let v = PathFilter::new("foo/*/bar")?;
        assert!(v.check_match(&"/foo/oink/bar".into()).is_some());
        assert!(v.check_match(&"/oink/foo/oink/bar/oink".into()).is_some());
        assert!(v.check_match(&"/foo/bar".into()).is_none());
        assert!(v.check_match(&"/foo/oink/oink/bar".into()).is_none());

        let v = PathFilter::new("/foo")?;
        assert!(v.check_match(&"/foo".into()).is_some());
        assert!(v.check_match(&"/foo/bar".into()).is_some());
        assert!(v.check_match(&"/bar/foo/bar".into()).is_none());

        let v = PathFilter::new("foo/")?;
        assert!(v.check_match(&"/foo".into()).is_some());
        assert!(v.check_match(&"/bar/foo".into()).is_some());
        assert!(v.check_match(&"/foo/bar".into()).is_none());

        let v = PathFilter::new("foo/**/bar")?;
        assert!(v.check_match(&"/foo/bar".into()).is_some());
        assert!(v.check_match(&"/foo/x/bar".into()).is_some());
        assert!(v.check_match(&"/foo/x/y/bar".into()).is_some());
        assert!(v.check_match(&"/bar/foo/x/bar/x".into()).is_some());

        let v = PathFilter::new("foo/**/bar/")?;
        assert!(v.check_match(&"/foo/bar".into()).is_some());
        assert!(v.check_match(&"/foo/x/bar".into()).is_some());
        assert!(v.check_match(&"/foo/x/bar/x".into()).is_none());

        Ok(())
    }

    #[test]
    fn path_filters_validate_literal_components() {
        assert!(PathFilter::new("valid_name/**").is_ok());
        assert!(PathFilter::new("invalid-name").is_err());
        assert!(PathFilter::new("InvalidName").is_err());
    }

    proptest! {
        #[test]
        fn literal_path_matches_when_anchored(components in prop::collection::vec("[a-z]{1,8}", 1..6)) {
            let join = components.join("/");
            let matcher = PathFilter::new(&format!("/{join}/")).expect("matcher");
            let path = Path::new(&components);
            let m = matcher.check_match(&path).expect("match");
            prop_assert_eq!(m.literals, components.len());
            prop_assert_eq!(m.depth, components.len());
            prop_assert!(m.anchored_end);
        }

        #[test]
        fn any_deep_matches_all_paths(components in prop::collection::vec("[a-z]{1,8}", 0..6)) {
            let matcher = PathFilter::new("**").expect("matcher");
            let path = Path::new(&components);
            let m = matcher.check_match(&path).expect("match");
            prop_assert_eq!(m.depth, components.len());
        }
    }
}
