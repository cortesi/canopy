use crate::{error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    path: Vec<String>,
}

impl Path {
    pub fn empty() -> Self {
        Path { path: vec![] }
    }
    pub fn new<T: AsRef<str>>(v: &[T]) -> Self {
        Path {
            path: v.iter().map(|x| x.as_ref().to_string()).collect(),
        }
    }
    pub fn to_string(&self) -> String {
        format!("/{}", self.path.join("/"))
    }
}

impl From<Vec<String>> for Path {
    fn from(path: Vec<String>) -> Self {
        Path { path }
    }
}

impl From<&[&str]> for Path {
    fn from(v: &[&str]) -> Self {
        Path {
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
        Path {
            path: v
                .split("/")
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
///
/// Examples:
///
///  - "foo" any path containing "foo"
///  - "foo/*/bar" any path containing "foo" followed by "bar"
///  - "foo/*/bar/" any path containing "foo" folowed by "bar" as a final component
///  - "/foo/*/bar/" any path starting with "foo" folowed by "bar" as a final component
///
/// The specificity of the matcher is a rough measure of the number of
/// significant match components in the specification. When disambiguating key
/// bindings, we prefer more specific matches.
#[derive(Debug, Clone)]
pub struct PathMatcher {
    expr: regex::Regex,
}

impl PathMatcher {
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
            pattern = format!("^/{}", pattern)
        }
        pattern = pattern.trim_end_matches('/').to_string();
        if path.ends_with('/') {
            pattern += "$";
        }
        let expr = regex::Regex::new(&pattern).map_err(|e| error::Error::Invalid(e.to_string()))?;
        Ok(PathMatcher { expr })
    }

    /// Check whether the path filter matches a given path. Returns the position
    /// of the final match character in the path string. We use this returned
    /// value to disambiguate when mulitple matches are active for a key - the
    /// path with the largest match position wins.
    pub fn check(&self, path: &Path) -> Option<usize> {
        Some(self.expr.find(&path.to_string())?.end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pathfilter() -> Result<()> {
        let v = PathMatcher::new("")?;
        assert_eq!(v.check(&"/any/thing".into()), Some(0));
        assert_eq!(v.check(&"/".into()), Some(0));

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
