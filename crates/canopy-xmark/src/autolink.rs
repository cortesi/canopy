//! GitHub's autolink extension: bare `www.` hosts, `http://`, `https://`,
//! and `ftp://` URLs, and email addresses in text become links.
//!
//! The rules follow the GFM specification's extended autolinks: a link
//! starts a line or follows whitespace or one of `*`, `_`, `~`, and `(`; its
//! domain is dot-separated segments of letters, digits, `_`, and `-`, with
//! no `_` in its last two; and trailing punctuation, an unbalanced `)`, and
//! a trailing entity reference stay out of it.

use alloc::vec::Vec;
use core::ops::Range;

/// What kind of link a bare autolink is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bare {
    /// A URL with its scheme, which is its own destination.
    Url,
    /// A `www.` host, whose destination takes `http://`.
    Www,
    /// An email address, whose destination takes `mailto:`.
    Email,
}

/// The schemes an extended URL autolink may have.
const SCHEMES: [&str; 3] = ["https://", "http://", "ftp://"];

/// Find the extended autolinks in `text`. `before` is the character before
/// the text, if any, which decides whether a link may start at its first
/// byte.
pub fn find(text: &str, before: Option<char>) -> Vec<(Range<usize>, Bare)> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let previous = text[..at].chars().next_back().or(before);
        if let Some((len, kind)) = link_at(&text[at..], previous) {
            found.push((at..at + len, kind));
            at += len;
            continue;
        }
        at += text[at..].chars().next().map_or(1, char::len_utf8);
    }
    found
}

/// The autolink that starts `text`, if one does, after `previous`.
fn link_at(text: &str, previous: Option<char>) -> Option<(usize, Bare)> {
    let starts =
        previous.is_none_or(|ch| ch.is_whitespace() || matches!(ch, '*' | '_' | '~' | '('));
    if !starts {
        return None;
    }
    if let Some(scheme) = SCHEMES
        .iter()
        .find(|scheme| starts_with_ignore_case(text, scheme))
    {
        let domain = domain_len(&text[scheme.len()..])?;
        let len = path_end(text, scheme.len() + domain);
        return Some((len, Bare::Url));
    }
    if starts_with_ignore_case(text, "www.") {
        let domain = domain_len(text)?;
        let len = path_end(text, domain);
        return Some((len, Bare::Www));
    }
    email_len(text).map(|len| (len, Bare::Email))
}

/// Whether `text` starts with `prefix`, ignoring ASCII case.
fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len()
        && text.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

/// The length of the valid domain that starts `text`: dot-separated
/// segments of letters, digits, `_`, and `-`, at least two, with no `_` in
/// the last two.
fn domain_len(text: &str) -> Option<usize> {
    let len = text
        .find(|ch: char| !(ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.')))
        .unwrap_or(text.len());
    let domain = text[..len].trim_end_matches('.');
    let segments: Vec<&str> = domain.split('.').collect();
    let valid = segments.len() >= 2
        && segments.iter().all(|segment| !segment.is_empty())
        && segments[segments.len() - 2..]
            .iter()
            .all(|segment| !segment.contains('_'));
    valid.then_some(domain.len())
}

/// Where an autolink whose domain ends at `from` ends: at the first space,
/// control character, or `<`, less trailing punctuation, unbalanced closing
/// parentheses, and a trailing entity reference.
fn path_end(text: &str, from: usize) -> usize {
    let mut end = text[from..]
        .find(|ch: char| ch.is_whitespace() || ch.is_control() || ch == '<')
        .map_or(text.len(), |at| from + at);
    loop {
        let link = &text[..end];
        let unbalanced =
            link.ends_with(')') && link.matches(')').count() > link.matches('(').count();
        if unbalanced || link.ends_with(['?', '!', '.', ',', ':', '*', '_', '~']) {
            end -= 1;
        } else if let Some(entity) = trailing_entity(link) {
            end -= entity;
        } else {
            break;
        }
        if end <= from {
            return from;
        }
    }
    end
}

/// The length of an entity reference, `&name;`, that ends `link`.
fn trailing_entity(link: &str) -> Option<usize> {
    let body = link.strip_suffix(';')?;
    let amp = body.rfind('&')?;
    let name = &body[amp + 1..];
    (!name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .then(|| link.len() - amp)
}

/// The length of the email address that starts `text`: a local part of
/// letters, digits, `.`, `+`, `-`, and `_`, then `@` and a domain of at
/// least two segments that does not end with `-` or `_`.
fn email_len(text: &str) -> Option<usize> {
    let local = text
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '+' | '-' | '_')))
        .unwrap_or(text.len());
    if local == 0 || text.as_bytes().get(local) != Some(&b'@') {
        return None;
    }
    let rest = &text[local + 1..];
    let len = rest
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_')))
        .unwrap_or(rest.len());
    let domain = rest[..len].trim_end_matches('.');
    let segments: Vec<&str> = domain.split('.').collect();
    let valid = segments.len() >= 2
        && segments.iter().all(|segment| !segment.is_empty())
        && !domain.ends_with(['-', '_']);
    valid.then(|| local + 1 + domain.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The links `find` gives, as text.
    fn links(text: &str) -> Vec<(&str, Bare)> {
        find(text, None)
            .into_iter()
            .map(|(range, kind)| (&text[range], kind))
            .collect()
    }

    #[test]
    fn urls_hosts_and_addresses_link() {
        assert_eq!(
            links("see https://example.com/a?b=1, www.example.org. and a@b.co!"),
            [
                ("https://example.com/a?b=1", Bare::Url),
                ("www.example.org", Bare::Www),
                ("a@b.co", Bare::Email),
            ]
        );
    }

    #[test]
    fn parentheses_balance_and_entities_stay_out() {
        assert_eq!(
            links("(see https://en.wikipedia.org/wiki/Markdown_(disambiguation))"),
            [(
                "https://en.wikipedia.org/wiki/Markdown_(disambiguation)",
                Bare::Url
            )]
        );
        assert_eq!(
            links("www.example.com/a&hl;"),
            [("www.example.com/a", Bare::Www)]
        );
    }

    #[test]
    fn a_link_starts_a_word_and_has_a_valid_domain() {
        assert!(links("xhttps://example.com").is_empty());
        assert!(links("www. alone").is_empty());
        assert!(links("www.ex_ample.com").is_empty());
        assert!(links("https://example").is_empty());
        assert_eq!(links("*www.example.com*"), [("www.example.com", Bare::Www)]);
        assert!(find("https://example.com", Some('x')).is_empty());
    }
}
