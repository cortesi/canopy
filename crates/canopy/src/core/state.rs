use std::{fmt, str::FromStr};

use convert_case::{Case, Casing};

use crate::{error, error::Result};

/// Return true if the character is valid in a node name.
pub fn valid_nodename_char(c: char) -> bool {
    (c.is_ascii_lowercase() || c.is_ascii_digit()) || c == '_'
}

/// Return true if the full name is valid.
pub fn valid_nodename(name: &str) -> bool {
    !name.is_empty() && name.chars().all(valid_nodename_char)
}

/// A node name, which consists of lowercase ASCII alphanumeric characters, plus
/// underscores.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeName {
    /// Stored node name string.
    name: String,
}

impl FromStr for NodeName {
    type Err = error::Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl NodeName {
    /// Create a new NodeName, returning an error if the string contains invalid
    /// characters.
    fn new(name: &str) -> Result<Self> {
        if !valid_nodename(name) {
            return Err(error::Error::Invalid(name.into()));
        }
        Ok(Self {
            name: name.to_string(),
        })
    }

    /// Takes a string and munges it into a valid node name. It does this by
    /// first converting the string to snake case, then removing all invalid
    /// characters.
    pub fn convert(name: &str) -> Self {
        let raw = name.to_case(Case::Snake);
        let filtered: String = raw.chars().filter(|x| valid_nodename_char(*x)).collect();
        let name = if filtered.is_empty() {
            "node".to_string()
        } else {
            filtered
        };
        Self { name }
    }
}

impl fmt::Display for NodeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl PartialEq<&str> for NodeName {
    fn eq(&self, other: &&str) -> bool {
        self.name == *other
    }
}

impl PartialEq<String> for NodeName {
    fn eq(&self, other: &String) -> bool {
        self.name == *other
    }
}

/// Converts a string into the standard node name format, and errors if it
/// doesn't comply to the node name standard.
impl TryFrom<&str> for NodeName {
    type Error = error::Error;
    fn try_from(name: &str) -> Result<Self> {
        Self::new(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_nodename_char_is_ascii() {
        assert!(valid_nodename_char('a'));
        assert!(valid_nodename_char('0'));
        assert!(valid_nodename_char('_'));
        assert!(!valid_nodename_char('A'));
        assert!(!valid_nodename_char('-'));
        assert!(!valid_nodename(""));
    }

    #[test]
    fn nodename_convert() {
        assert_eq!(NodeName::try_from("foo").unwrap(), "foo");
        assert!(NodeName::try_from("Foo").is_err());
        assert_eq!(NodeName::convert("Foo"), "foo");
        assert_eq!(NodeName::convert("FooBar"), "foo_bar");
        assert_eq!(NodeName::convert("FooBar Voing"), "foo_bar_voing");
        assert_eq!(NodeName::convert(""), "node");
        assert_eq!(NodeName::convert("!!!"), "node");
    }
}
