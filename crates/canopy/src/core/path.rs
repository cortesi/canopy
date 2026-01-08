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
/// The matcher supports `*` wildcards and optional leading or trailing slashes.
#[derive(Debug, Clone)]
pub struct PathMatcher {
    /// Original filter string used to construct the matcher.
    filter: Box<str>,
    /// Compiled regular expression.
    expr: regex::Regex,
}

/// Path match metadata used for input precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathMatch {
    /// End byte index of the match in the path string.
    pub end: usize,
    /// Match length in bytes.
    pub len: usize,
}

impl PathMatcher {
    /// Compile a path matcher from a filter string.
    pub fn new(path: &str) -> Result<Self> {
        let parts = path.split('/');
        let mut pattern = parts
            .filter_map(|x| {
                if x == "*" {
                    Some(String::from(r"[a-z0-9_/]*"))
                } else if !x.is_empty() {
                    Some(format!("{}/", regex::escape(x)))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");
        if path.starts_with('/') {
            pattern = format!("^/{pattern}")
        }
        pattern = pattern.trim_end_matches('/').to_string();
        if path.ends_with('/') {
            pattern += "$";
        }
        let expr = regex::Regex::new(&pattern).map_err(|e| error::Error::Invalid(e.to_string()))?;
        Ok(Self {
            filter: path.into(),
            expr,
        })
    }

    /// Return the original filter string used to construct this matcher.
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Check whether the path filter matches a given path. Returns the position
    /// of the final match character in the path string. We use this returned
    /// value to disambiguate when mulitple matches are active for a key - the
    /// path with the largest match position wins.
    pub fn check(&self, path: &Path) -> Option<usize> {
        self.check_match(path).map(|m| m.end)
    }

    /// Check whether the path filter matches a given path, returning match metadata.
    pub fn check_match(&self, path: &Path) -> Option<PathMatch> {
        let haystack = path.to_string();
        let mat = self.expr.find_iter(&haystack).last()?;
        Some(PathMatch {
            end: mat.end(),
            len: mat.end() - mat.start(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pathfilter() -> Result<()> {
        let v = PathMatcher::new("")?;
        assert_eq!(v.check(&"/any/thing".into()), Some(10));
        assert_eq!(v.check(&"/".into()), Some(1));

        let v = PathMatcher::new("bar")?;
        assert_eq!(v.check(&"/foo/bar".into()), Some(8));

        assert_eq!(v.check(&"/bar/foo".into()), Some(4));
        assert!(v.check(&"/foo/foo".into()).is_none());

        let v = PathMatcher::new("foo/*/bar")?;
        assert_eq!(v.check(&"/foo/oink/oink/bar".into()), Some(18));
        assert_eq!(v.check(&"/foo/bar".into()), Some(8));
        assert_eq!(v.check(&"/oink/foo/bar/oink".into()), Some(13));
        assert_eq!(v.check(&"/foo/oink/oink/bar".into()), Some(18));
        assert_eq!(v.check(&"/foo/bar/voing".into()), Some(8));

        let v = PathMatcher::new("/foo")?;
        assert_eq!(v.check(&"/foo".into()), Some(4));
        assert_eq!(v.check(&"/foo/bar".into()), Some(4));
        assert!(v.check(&"/bar/foo/bar".into()).is_none());

        let v = PathMatcher::new("foo/")?;
        assert_eq!(v.check(&"/foo".into()), Some(4));
        assert_eq!(v.check(&"/bar/foo".into()), Some(8));
        assert!(v.check(&"/foo/bar".into()).is_none());

        let v = PathMatcher::new("foo/*/bar/*/voing/")?;
        assert_eq!(v.check(&"/foo/bar/voing".into()), Some(14));
        assert_eq!(v.check(&"/foo/x/bar/voing".into()), Some(16));
        assert_eq!(v.check(&"/foo/x/bar/x/voing".into()), Some(18));
        assert_eq!(v.check(&"/x/foo/x/bar/x/voing".into()), Some(20));
        assert!(v.check(&"/foo/x/bar/x/voing/x".into()).is_none());

        Ok(())
    }
}
