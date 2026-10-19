//! Front matter: the YAML properties a note opens with.

use alloc::{borrow::ToOwned, string::String, vec::Vec};

/// Return the properties of simple YAML front matter, as notes tools show
/// them: `key: value` lines, and lists written `[a, b]` or as `- item`
/// lines, which read as `a, b`. Front matter with nested maps, or anything
/// else, has none.
#[must_use]
pub fn front_matter_properties(yaml: &str) -> Option<Vec<(String, String)>> {
    let unquote = |value: &str| {
        let value = value.trim();
        value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value)
            .to_owned()
    };
    let mut properties: Vec<(String, String)> = Vec::new();
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" {
            continue;
        }
        if let Some(item) = trimmed.strip_prefix("- ") {
            let (_, value) = properties.last_mut()?;
            if !value.is_empty() {
                value.push_str(", ");
            }
            value.push_str(&unquote(item));
            continue;
        }
        if line.starts_with(char::is_whitespace) {
            return None;
        }
        let (key, value) = line.split_once(':')?;
        let value = value.trim();
        let value = value
            .strip_prefix('[')
            .and_then(|v| v.strip_suffix(']'))
            .map_or_else(
                || unquote(value),
                |list| list.split(',').map(unquote).collect::<Vec<_>>().join(", "),
            );
        properties.push((key.trim().to_owned(), value));
    }
    (!properties.is_empty()).then_some(properties)
}

#[cfg(test)]
mod tests {
    use super::front_matter_properties;

    #[test]
    fn simple_properties_read_and_nested_ones_do_not() {
        let properties =
            front_matter_properties("title: \"Trip\"\ntags: [a, b]\naliases:\n  - one\n  - two\n")
                .expect("properties");
        assert_eq!(
            properties,
            [
                ("title".to_owned(), "Trip".to_owned()),
                ("tags".to_owned(), "a, b".to_owned()),
                ("aliases".to_owned(), "one, two".to_owned()),
            ]
        );
        assert_eq!(front_matter_properties("map:\n  key: value\n"), None);
    }
}
