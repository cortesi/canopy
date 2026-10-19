//! Link destinations, and Obsidian's wikilinks and embeds.

use alloc::{borrow::ToOwned, format, string::String, vec::Vec};

use crate::strings::CowStr;

/// What an Obsidian embed shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EmbedKind {
    /// An image, named by its extension.
    Image,
    /// A note, or a section or block of one.
    Note,
}

/// The image extensions an embed shows as an image.
const IMAGE_EXTENSIONS: [&str; 8] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "avif"];

impl EmbedKind {
    /// What an embed of `target` shows: an image when its extension names
    /// one, and otherwise a note.
    #[must_use]
    pub fn of(target: &str) -> Self {
        let path = target.split(['#', '?']).next().unwrap_or(target);
        let image = path.rsplit_once('.').is_some_and(|(_, extension)| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        });
        if image { Self::Image } else { Self::Note }
    }
}

/// An Obsidian embed, `![[target]]`. An image's label after `|` may be its
/// size in pixels, `|width` or `|widthxheight`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Embed<'a> {
    /// The target as written, with any `#` anchor.
    pub target: CowStr<'a>,
    /// What the embed shows.
    pub kind: EmbedKind,
    /// The width the source asks for, in pixels.
    pub width: Option<u32>,
    /// The height the source asks for, in pixels.
    pub height: Option<u32>,
}

impl Embed<'_> {
    /// Own every string, so the embed outlives its source.
    #[must_use]
    pub fn into_static(self) -> Embed<'static> {
        Embed {
            target: self.target.into_static(),
            kind: self.kind,
            width: self.width,
            height: self.height,
        }
    }
}

/// Read an image embed's label as its size, `width` or `widthxheight`.
pub fn embed_size(label: &str) -> Option<(u32, Option<u32>)> {
    let label = label.trim();
    let (width, height) = match label.split_once('x') {
        Some((width, height)) => (width, Some(height)),
        None => (label, None),
    };
    Some((
        width.parse().ok()?,
        height.map(str::parse).transpose().ok()?,
    ))
}

/// Return the label Obsidian shows for an unlabelled wikilink to `target`:
/// its path without a `.md` extension, and its anchor after `›`.
#[must_use]
pub fn wikilink_label(target: &str) -> String {
    let parts = split_wikilink(target);
    let path = parts
        .path
        .len()
        .checked_sub(3)
        .filter(|&at| {
            parts.path.is_char_boundary(at) && parts.path[at..].eq_ignore_ascii_case(".md")
        })
        .map_or(parts.path.as_str(), |at| &parts.path[..at]);
    match parts.fragment {
        Some(anchor) if path.is_empty() => anchor,
        Some(anchor) => format!("{path} › {anchor}"),
        None => path.to_owned(),
    }
}

/// Return the relative URL a wikilink to `target` names in an HTML page:
/// its path, with `.md` when it has no extension, and its anchor as the id
/// the page gives it. A heading anchor becomes its heading's id, and a block
/// anchor keeps its `^`.
#[must_use]
pub fn wikilink_href(target: &str) -> String {
    let LinkTarget { path, fragment } = split_wikilink(target);
    let name = path.rsplit('/').next().unwrap_or(&path);
    let mut href = if path.is_empty() || name.contains('.') {
        path
    } else {
        format!("{path}.md")
    };
    if let Some(fragment) = fragment {
        let id = if fragment.starts_with('^') {
            fragment
        } else {
            crate::heading_slug(&fragment)
        };
        href.push('#');
        href.push_str(&id);
    }
    href
}

/// A link destination's parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkTarget {
    /// The path, which may be empty for a link to an anchor alone.
    pub path: String,
    /// The anchor after `#`, if a non-empty one follows.
    pub fragment: Option<String>,
}

/// Split a wikilink's target into its path and `#` anchor, as Obsidian
/// reads them: trimmed, and not percent-decoded.
#[must_use]
pub fn split_wikilink(target: &str) -> LinkTarget {
    let (path, fragment) = match target.split_once('#') {
        Some((path, fragment)) => (path.trim(), Some(fragment.trim())),
        None => (target.trim(), None),
    };
    LinkTarget {
        path: path.to_owned(),
        fragment: fragment
            .filter(|fragment| !fragment.is_empty())
            .map(str::to_owned),
    }
}

/// Split a relative destination into its percent-decoded path and anchor,
/// dropping any query.
#[must_use]
pub fn split_destination(destination: &str) -> LinkTarget {
    let (path, fragment) = match destination.split_once('#') {
        Some((path, fragment)) => (path, Some(percent_decode(fragment))),
        None => (destination, None),
    };
    LinkTarget {
        path: percent_decode(path.split_once('?').map_or(path, |(path, _)| path)),
        fragment: fragment.filter(|fragment| !fragment.is_empty()),
    }
}

/// Decode `text`'s percent escapes. An escape that is not two hex digits
/// stays as written, and bytes that do not form UTF-8 become U+FFFD.
#[must_use]
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escape = (bytes[at] == b'%')
            .then(|| bytes.get(at + 1..at + 3))
            .flatten()
            .and_then(|hex| core::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        if let Some(byte) = escape {
            out.push(byte);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wikilink_labels_drop_md_and_show_anchors() {
        assert_eq!(wikilink_label("Garden"), "Garden");
        assert_eq!(wikilink_label("Projects/Garden.md"), "Projects/Garden");
        assert_eq!(wikilink_label("Garden#Plan"), "Garden › Plan");
        assert_eq!(wikilink_label("#^here"), "^here");
    }

    #[test]
    fn wikilinks_name_pages_and_their_ids() {
        assert_eq!(wikilink_href("Garden"), "Garden.md");
        assert_eq!(
            wikilink_href("Projects/Garden#My Plan"),
            "Projects/Garden.md#my-plan"
        );
        assert_eq!(wikilink_href("notes.v2/Data.csv"), "notes.v2/Data.csv");
        assert_eq!(wikilink_href("Garden#^seeds"), "Garden.md#^seeds");
        assert_eq!(wikilink_href("#Top"), "#top");
    }

    #[test]
    fn destinations_split_and_decode() {
        assert_eq!(
            split_destination("a%20b.md?x=1#Some%20Head"),
            LinkTarget {
                path: "a b.md".into(),
                fragment: Some("Some Head".into()),
            }
        );
        assert_eq!(split_destination("a.md#").fragment, None);
        assert_eq!(
            split_wikilink(" Note # Plan "),
            LinkTarget {
                path: "Note".into(),
                fragment: Some("Plan".into()),
            }
        );
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%e2%80%a2"), "•");
    }

    #[test]
    fn embeds_know_images_and_sizes() {
        assert_eq!(EmbedKind::of("a.PNG#x"), EmbedKind::Image);
        assert_eq!(EmbedKind::of("Note"), EmbedKind::Note);
        assert_eq!(embed_size("200"), Some((200, None)));
        assert_eq!(embed_size("200x100"), Some((200, Some(100))));
        assert_eq!(embed_size("alt text"), None);
    }
}
