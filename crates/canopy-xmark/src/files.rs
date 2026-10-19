//! Which files hold Markdown.

use std::path::Path;

/// The file extensions that name Markdown, without regard to case.
pub const MARKDOWN_EXTENSIONS: [&str; 5] = ["md", "markdown", "mdown", "mkd", "mkdn"];

/// Whether `path` names a Markdown file, by its extension.
#[must_use]
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            MARKDOWN_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::is_markdown;

    #[test]
    fn markdown_files_go_by_extension() {
        assert!(is_markdown(Path::new("a/README.md")));
        assert!(is_markdown(Path::new("notes.MARKDOWN")));
        assert!(is_markdown(Path::new("x.mkdn")));
        assert!(!is_markdown(Path::new("x.txt")));
        assert!(!is_markdown(Path::new("md")));
    }
}
