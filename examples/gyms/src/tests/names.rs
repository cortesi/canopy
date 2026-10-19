//! The names the gyms dropped the `gym` suffix from, and the node names that
//! keep it.

use std::{fs, io, path::Path};

use canopy::error::Result;

use crate::{Demo, demo_canopy};

/// Each renamed script owner, from its old name to its new one.
const SCRIPT_OWNERS: &[(&str, &str)] = &[
    ("biggym", "big"),
    ("chargym", "char"),
    ("chartgym", "chart"),
    ("cursorgym", "cursor"),
    ("editorgym", "editor"),
    ("focusgym", "focus"),
    ("fontgym", "font"),
    ("framegym-prefix", "frame-prefix"),
    ("framegym-scroll", "frame-scroll"),
    ("listgym", "list"),
    ("markdowngym", "markdown"),
    ("motiongym", "motion"),
    ("stylegym", "style"),
    ("tablegym", "table"),
    ("termgym", "term"),
    ("textgym", "text"),
];

/// Each renamed style path root, or style layer, from its old name to its
/// new one.
const STYLE_PATHS: &[(&str, &str)] = &[
    ("cursorgym", "cursor"),
    ("fontgym", "font"),
    ("motiongym", "motion"),
    ("termgym", "term"),
];

/// Each renamed table dataset id, from its old name to its new one.
const DATASETS: &[(&str, &str)] = &[("tablegym", "table")];

/// Node names that keep the suffix, because widget types name their nodes.
/// `stylegym` and `termgym` are also node paths that keymaps and tests name.
const KEPT: &[&str] = &["fontgym-block", "fontgym-label", "stylegym", "termgym"];

/// Return each name with the suffix that starts a string or a path segment
/// in `text`, such as `listgym` in `"listgym"` or `termgym` in
/// `"root/termgym"`, with any `-word` after it.
fn suffixed_names(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut names = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find("gym") {
        let end = from + found + 3;
        let mut start = from + found;
        while start > 0 && bytes[start - 1].is_ascii_lowercase() {
            start -= 1;
        }
        let opens = start > 0 && matches!(bytes[start - 1], b'"' | b'/');
        let mut stop = end;
        if bytes.get(stop) == Some(&b'-') {
            stop += 1;
            while bytes.get(stop).is_some_and(u8::is_ascii_lowercase) {
                stop += 1;
            }
        }
        let closes = !bytes.get(end).is_some_and(u8::is_ascii_alphanumeric);
        if opens && start < from + found && closes {
            names.push(&text[start..stop]);
        }
        from = end;
    }
    names
}

/// Collect the suffixed names in every Rust source file below `dir`, except
/// this file, which holds the old names.
fn scan(dir: &Path, found: &mut Vec<String>) -> io::Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            scan(&path, found)?;
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && !path.ends_with("tests/names.rs")
        {
            let text = fs::read_to_string(&path)?;
            for name in suffixed_names(&text) {
                found.push(format!("{}: {name}", path.display()));
            }
        }
    }
    Ok(())
}

#[test]
fn only_node_names_keep_the_gym_suffix() {
    let mut found = Vec::new();
    scan(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    )
    .expect("the gym sources read");
    let renamed: Vec<_> = found
        .iter()
        .filter(|hit| {
            let name = hit.rsplit(": ").next().unwrap_or_default();
            !KEPT.contains(&name)
        })
        .collect();
    assert!(
        renamed.is_empty(),
        "names that drop the suffix (see SCRIPT_OWNERS, STYLE_PATHS, and DATASETS): {renamed:#?}"
    );
    let mut kept: Vec<_> = found
        .iter()
        .filter_map(|hit| hit.rsplit(": ").next())
        .collect();
    kept.sort_unstable();
    kept.dedup();
    assert_eq!(kept, KEPT, "the node names the gyms keep");
}

#[test]
fn the_mapping_drops_only_the_suffix() {
    for (old, new) in SCRIPT_OWNERS.iter().chain(STYLE_PATHS).chain(DATASETS) {
        assert_eq!(old.replacen("gym", "", 1), *new, "{old}");
    }
}

#[test]
fn the_scan_finds_names_that_open_a_string_or_path() {
    assert_eq!(
        suffixed_names(r#""listgym" "root/termgym" "fontgym-block" "font/x" listgym() Stylegym"#),
        ["listgym", "termgym", "fontgym-block"]
    );
}

#[test]
fn renamed_styles_resolve_under_their_new_names() -> Result<()> {
    let cases = [
        (Demo::Cursor, "cursor", "label/focused"),
        (Demo::Font, "font", "label"),
        (Demo::Motion, "motion", "blink"),
        (Demo::Term, "term", "frame/title"),
    ];
    for (demo, root, rest) in cases {
        let (old, _) = STYLE_PATHS
            .iter()
            .find(|(_, new)| *new == root)
            .expect("each case is a renamed style root");
        let canopy = demo.configure(demo_canopy()).build()?;
        let style = canopy.style();
        let base = style.resolve("");
        assert_ne!(
            style.resolve(&format!("{root}/{rest}")),
            base,
            "{root}/{rest} has the gym's style"
        );
        assert_eq!(
            style.resolve(&format!("{old}/{rest}")),
            base,
            "{old}/{rest} has no style left"
        );
    }
    Ok(())
}
