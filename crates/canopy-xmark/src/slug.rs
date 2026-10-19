//! Heading ids, as GitHub makes them.

use alloc::string::String;

/// Return the id GitHub gives a heading with `text`: lowercase, with letters,
/// digits, `-`, and `_` kept, spaces as `-`, and everything else dropped.
///
/// Repeated ids take `-1`, `-2` suffixes in a document, which
/// [`Options::ENABLE_HEADING_IDS`](crate::Options::ENABLE_HEADING_IDS)
/// adds; this function gives the id alone.
#[must_use]
pub fn heading_slug(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .filter_map(|ch| match ch {
            ' ' => Some('-'),
            '-' | '_' => Some(ch),
            ch if ch.is_alphanumeric() => Some(ch),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::heading_slug;

    #[test]
    fn slugs_follow_github() {
        assert_eq!(heading_slug("Hello, World!"), "hello-world");
        assert_eq!(heading_slug("code and more"), "code-and-more");
        assert_eq!(heading_slug("Ünïcode Straße"), "ünïcode-straße");
        assert_eq!(heading_slug("a_b - c"), "a_b---c");
        assert_eq!(heading_slug("tab\there"), "tabhere");
        assert_eq!(heading_slug("C++ & Rust"), "c--rust");
        assert_eq!(heading_slug("snake_case-name"), "snake_case-name");
        assert_eq!(heading_slug("!!!"), "");
    }
}
