//! Option files: bounded reads, conversion between TOML and values, edits
//! that keep the rest of a file, and atomic writes.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    io::{self, Read, Write},
    mem,
    path::{Path, PathBuf},
};

use toml_edit::{Array, DocumentMut, Item as TomlItem, Table, TableLike};

use crate::model::{Group, Item, Kind, OptionGroup, Schema, Value, Variant, join_path};

/// The largest option file that a store reads, in bytes.
pub const MAX_FILE_BYTES: u64 = 1 << 20;

/// The scope of an option file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The global file in the config home.
    Global,
    /// A local file in the start directory or one of its ancestors.
    Local,
}

impl Scope {
    /// Return the lowercase name of the scope.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Local => "local",
        }
    }
}

/// The values that one file sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileLayer {
    /// The path of the file.
    pub path: PathBuf,
    /// The scope of the file.
    pub scope: Scope,
    /// The values that the file sets, by leaf path.
    pub values: BTreeMap<String, Value>,
}

impl FileLayer {
    /// Return an empty layer for `path`.
    #[must_use]
    pub const fn empty(path: PathBuf, scope: Scope) -> Self {
        Self {
            path,
            scope,
            values: BTreeMap::new(),
        }
    }
}

/// Read the document at `path`, or `None` if there is no file.
///
/// A file larger than [`MAX_FILE_BYTES`] or that does not parse is an
/// error, so that a save cannot write over content it did not read.
pub fn read_document(path: &Path) -> Result<Option<DocumentMut>, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let mut text = String::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(format!(
            "{}: larger than {} bytes, ignored",
            path.display(),
            MAX_FILE_BYTES
        ));
    }
    text.parse::<DocumentMut>()
        .map(Some)
        .map_err(|error| format!("{}: {error}", path.display()))
}

/// Load the layer of the file at `path`, with a warning for each entry it
/// skips.
pub fn load_layer<T: OptionGroup>(
    schema: &Schema,
    path: &Path,
    scope: Scope,
) -> (FileLayer, Vec<String>) {
    let mut layer = FileLayer::empty(path.to_path_buf(), scope);
    let mut warnings = Vec::new();
    match read_document(path) {
        Ok(Some(document)) => {
            let mut reader = Reader {
                schema,
                file: path,
                scope,
                probe: T::default(),
                values: &mut layer.values,
                warnings: &mut warnings,
            };
            reader.table(document.as_table(), "");
        }
        Ok(None) => {}
        Err(error) => warnings.push(error),
    }
    (layer, warnings)
}

/// The state of one walk over a document.
struct Reader<'a, T> {
    /// The schema that the document follows.
    schema: &'a Schema,
    /// The file, for warnings.
    file: &'a Path,
    /// The scope of the file.
    scope: Scope,
    /// A value that validates each entry.
    probe: T,
    /// The values that load.
    values: &'a mut BTreeMap<String, Value>,
    /// The warnings for skipped entries.
    warnings: &'a mut Vec<String>,
}

impl<T: OptionGroup> Reader<'_, T> {
    /// Read the entries of `table`, whose group path is `prefix`.
    fn table(&mut self, table: &dyn TableLike, prefix: &str) {
        for (key, item) in table.iter() {
            let path = join_path(prefix, key);
            if let Some(child) = item.as_table_like() {
                if self.schema.group(&path).is_some() {
                    self.table(child, &path);
                } else {
                    self.warn(&path, "unknown group");
                }
                continue;
            }
            self.entry(&path, item);
        }
    }

    /// Read one entry at `path`.
    fn entry(&mut self, path: &str, item: &TomlItem) {
        let Some(leaf) = self.schema.leaf(path) else {
            self.warn(path, "unknown option");
            return;
        };
        if self.scope == Scope::Local && leaf.global {
            self.warn(path, "can only be set in the global file");
            return;
        }
        let Some(mut value) = from_toml(&leaf.kind, item) else {
            self.warn(path, &format!("expected a {}", leaf.kind.name()));
            return;
        };
        // A set or an order keeps the names it knows, so a file from a newer
        // program keeps the rest of its entry.
        if let (Kind::Set(variants), Value::Set(names))
        | (Kind::Order(variants), Value::List(names)) = (&leaf.kind, &mut value)
        {
            let (known, unknown): (Vec<String>, Vec<String>) =
                mem::take(names).into_iter().partition(|name| {
                    variants
                        .iter()
                        .any(|variant| variant.name.eq_ignore_ascii_case(name))
                });
            for name in unknown {
                self.warn(path, &format!("unknown member {name}"));
            }
            *names = known;
        }
        if let Err(error) = self.probe.set(path, &value) {
            self.warn(path, &error.to_string());
            return;
        }
        self.values.insert(path.to_string(), value);
    }

    /// Add a warning for the entry at `path`.
    fn warn(&mut self, path: &str, message: &str) {
        self.warnings
            .push(format!("{}: {path}: {message}", self.file.display()));
    }
}

/// Convert a TOML item into a value of `kind`.
fn from_toml(kind: &Kind, item: &TomlItem) -> Option<Value> {
    let value = item.as_value()?;
    match kind {
        Kind::Bool => value.as_bool().map(Value::Bool),
        Kind::Int { .. } => value.as_integer().map(Value::Int),
        Kind::Text => value.as_str().map(|text| Value::Text(text.to_string())),
        Kind::Choice(_) => value.as_str().map(|text| Value::Choice(text.to_string())),
        Kind::Set(_) => strings(value).map(Value::Set),
        Kind::Order(_) | Kind::List => strings(value).map(Value::List),
    }
}

/// Return the strings of a TOML array.
fn strings(value: &toml_edit::Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|item| item.as_str().map(str::to_string))
        .collect()
}

/// Convert a value into TOML, or `None` for an unset value.
#[must_use]
pub fn to_toml(value: &Value) -> Option<toml_edit::Value> {
    Some(match value {
        Value::Bool(value) => (*value).into(),
        Value::Int(value) => (*value).into(),
        Value::Text(text) | Value::Choice(text) => text.as_str().into(),
        Value::Set(items) | Value::List(items) => {
            let mut array = Array::new();
            for item in items {
                array.push(item.as_str());
            }
            array.into()
        }
        Value::Unset => return None,
    })
}

/// Set or remove the entry at each path in `document`.
///
/// A `None` value removes the entry. A table that an edit leaves empty goes
/// too, unless a comment sits before its header. A set inside an inline table
/// makes it a standard table, which keeps its other entries.
pub fn apply_edits(document: &mut DocumentMut, edits: &[(String, Option<Value>)]) {
    for (path, value) in edits {
        let parts: Vec<&str> = path.split('.').collect();
        let Some((name, groups)) = parts.split_last() else {
            continue;
        };
        match value.as_ref().and_then(to_toml) {
            Some(value) => {
                let table = table_at(document.as_table_mut(), groups);
                table.insert(name, TomlItem::Value(value));
            }
            None => remove_at(document.as_table_mut(), groups, name),
        }
    }
}

/// Return the table at `groups` below `table`, created if needed.
fn table_at<'a>(table: &'a mut Table, groups: &[&str]) -> &'a mut Table {
    let Some((head, rest)) = groups.split_first() else {
        return table;
    };
    let item = table.entry(head).or_insert(TomlItem::None);
    // An inline table becomes a standard table in its place, which keeps its
    // entries.
    if let Some(inline) = item.as_inline_table() {
        *item = TomlItem::Table(inline.clone().into_table());
    } else if !item.is_table() {
        let mut child = Table::new();
        child.set_implicit(!rest.is_empty());
        *item = TomlItem::Table(child);
    }
    let child = table
        .get_mut(head)
        .and_then(TomlItem::as_table_mut)
        .expect("the table was inserted");
    table_at(child, rest)
}

/// Remove the entry `name` below `groups`, and any table it leaves empty.
fn remove_at(table: &mut Table, groups: &[&str], name: &str) {
    match groups.split_first() {
        None => {
            table.remove(name);
        }
        Some((head, rest)) => {
            let Some(child) = table.get_mut(head) else {
                return;
            };
            if let Some(child) = child.as_table_mut() {
                remove_at(child, rest, name);
                if child.is_empty() && !has_comment(child) {
                    table.remove(head);
                }
            } else if let Some(child) = child.as_table_like_mut() {
                remove_like(child, rest, name);
                if child.is_empty() {
                    table.remove(head);
                }
            }
        }
    }
}

/// Remove the entry `name` below `groups` of an inline table, and any
/// inline table it leaves empty.
fn remove_like(table: &mut dyn TableLike, groups: &[&str], name: &str) {
    let Some((head, rest)) = groups.split_first() else {
        table.remove(name);
        return;
    };
    let Some(child) = table.get_mut(head).and_then(TomlItem::as_table_like_mut) else {
        return;
    };
    remove_like(child, rest, name);
    if child.is_empty() {
        table.remove(head);
    }
}

/// Return whether a comment sits before the header of `table`, which
/// removing the table would lose.
fn has_comment(table: &Table) -> bool {
    table
        .decor()
        .prefix()
        .and_then(|prefix| prefix.as_str())
        .is_some_and(|prefix| prefix.contains('#'))
}

/// Return a document that lists every option of `T` with its help text and
/// its default, each entry commented out, so a copy can set any of them.
#[must_use]
pub fn default_document<T: OptionGroup>() -> String {
    let schema = Schema::of::<T>();
    let defaults = T::default();
    let mut out = String::from(
        "# Every option, with its default. Remove the # before an entry to set it.\n\n",
    );
    write_group(&mut out, &schema.root, "", &defaults);
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

/// The widest line of help text in a default document.
const HELP_WIDTH: usize = 76;

/// Add `text` to `out` as comment lines no wider than [`HELP_WIDTH`].
fn write_comment(out: &mut String, text: &str) {
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > HELP_WIDTH {
            writeln!(out, "# {line}").ok();
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        writeln!(out, "# {line}").ok();
    }
}

/// Write the leaves of `group`, then its groups, to `out`.
fn write_group<T: OptionGroup>(out: &mut String, group: &Group, path: &str, defaults: &T) {
    let leaves = group.items.iter().filter_map(|item| match item {
        Item::Leaf(leaf) => Some(leaf),
        Item::Group(_) => None,
    });
    let mut wrote = false;
    for leaf in leaves {
        if !wrote && !path.is_empty() {
            for paragraph in group.help.lines() {
                write_comment(out, paragraph);
            }
            writeln!(out, "[{path}]").ok();
        }
        wrote = true;
        let leaf_path = join_path(path, leaf.name);
        for paragraph in leaf.help.lines() {
            write_comment(out, paragraph);
        }
        let names = |variants: &[Variant]| {
            variants
                .iter()
                .map(|variant| format!("\"{}\"", variant.name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match &leaf.kind {
            Kind::Choice(variants) => write_comment(out, &format!("One of {}.", names(variants))),
            Kind::Set(variants) => write_comment(out, &format!("Any of {}.", names(variants))),
            Kind::Order(variants) => {
                write_comment(out, &format!("An order of {}.", names(variants)));
            }
            Kind::Int { min, max, .. } => write_comment(out, &format!("From {min} to {max}.")),
            _ => {}
        }
        if leaf.global {
            write_comment(out, "Only the global file can set this.");
        }
        // An unset option shows an example of its form, which is valid TOML.
        let value = match defaults.get(&leaf_path).unwrap_or(Value::Unset) {
            Value::Unset => match &leaf.kind {
                Kind::List => Value::List(vec!["program".into()]),
                Kind::Int { min, .. } => Value::Int(*min),
                Kind::Bool => Value::Bool(false),
                Kind::Text => Value::Text(String::new()),
                Kind::Choice(variants) => Value::Choice(
                    variants
                        .first()
                        .map_or_else(String::new, |variant| variant.name.to_string()),
                ),
                Kind::Set(_) => Value::Set(Vec::new()),
                Kind::Order(variants) => Value::List(
                    variants
                        .iter()
                        .map(|variant| variant.name.to_string())
                        .collect(),
                ),
            },
            value => value,
        };
        if let Some(value) = to_toml(&value) {
            writeln!(out, "# {} = {}", leaf.name, value.to_string().trim()).ok();
        }
        out.push('\n');
    }
    for item in &group.items {
        if let Item::Group(child) = item {
            write_group(out, child, &join_path(path, child.name), defaults);
        }
    }
}

/// Write `text` to `path` through a temporary file in the same directory,
/// creating the directory if needed.
///
/// A path that is a symlink writes the file it names, such as one in a
/// dotfiles repository, and an existing file keeps its permissions.
pub fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("an option file needs a directory"))?;
    fs::create_dir_all(directory)?;
    let permissions = fs::metadata(&path).ok().map(|meta| meta.permissions());
    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
    temp.write_all(text.as_bytes())?;
    if let Some(permissions) = permissions {
        temp.as_file().set_permissions(permissions)?;
    }
    temp.as_file().sync_all()?;
    temp.persist(&path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;
    use crate::tests::Root;

    #[test]
    fn edits_create_and_prune_tables() {
        let mut document: DocumentMut =
            "# keep me\nname = 1\n[view]\nwrap = true\n# a note\n[diff]\nwide = true\n"
                .parse()
                .unwrap();
        apply_edits(
            &mut document,
            &[
                ("listing.hidden".into(), Some(Value::Bool(true))),
                ("view.wrap".into(), None),
                ("diff.wide".into(), None),
            ],
        );
        let text = document.to_string();
        assert!(text.contains("# keep me"), "{text}");
        assert!(text.contains("[listing]\nhidden = true"), "{text}");
        assert!(!text.contains("[view]"), "{text}");
        assert!(text.contains("# a note\n[diff]"), "{text}");
    }

    #[test]
    fn edits_keep_the_order_of_tables() {
        let mut document: DocumentMut = "[view]\nwrap = true\n[listing]\nhidden = true\n"
            .parse()
            .unwrap();
        apply_edits(
            &mut document,
            &[("view.centered".into(), Some(Value::Bool(true)))],
        );
        let text = document.to_string();
        assert!(text.find("[view]") < text.find("[listing]"), "{text}");
    }

    #[test]
    fn edits_reach_into_inline_tables() {
        let mut document: DocumentMut =
            "listing = { hidden = true, order = \"size\" }\nview = { wrap = { deep = 1 } }\n"
                .parse()
                .unwrap();
        apply_edits(
            &mut document,
            &[
                (
                    "listing.metadata".into(),
                    Some(Value::Set(vec!["git".into()])),
                ),
                ("view.wrap.deep".into(), None),
            ],
        );
        let text = document.to_string();
        assert!(text.contains("hidden = true"), "{text}");
        assert!(text.contains("order = \"size\""), "{text}");
        assert!(text.contains("metadata = [\"git\"]"), "{text}");
        assert!(!text.contains("view"), "{text}");
    }

    #[test]
    fn a_write_through_a_symlink_keeps_the_link_and_its_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("dotfiles").join("options.toml");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
        let link = root.path().join("options.toml");
        symlink(&target, &link).unwrap();
        write_atomic(&link, "a = 1\n").unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "a = 1\n");
        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
    }

    #[test]
    fn values_round_trip_through_toml() {
        for (kind, value) in [
            (Kind::Bool, Value::Bool(true)),
            (
                Kind::Int {
                    min: 0,
                    max: 9,
                    step: 1,
                },
                Value::Int(4),
            ),
            (Kind::Text, Value::Text("vim".into())),
            (Kind::Choice(Vec::new()), Value::Choice("size".into())),
            (Kind::Set(Vec::new()), Value::Set(vec!["git".into()])),
            (Kind::List, Value::List(vec!["a".into(), "b".into()])),
        ] {
            let item = TomlItem::Value(to_toml(&value).unwrap());
            assert_eq!(from_toml(&kind, &item), Some(value));
        }
    }

    #[test]
    fn the_default_document_parses_with_every_entry_set() {
        let text = default_document::<Root>();
        assert!(
            text.contains("[listing]\n# Show hidden files.\n# hidden = false\n"),
            "{text}"
        );
        assert!(
            text.contains("# Only the global file can set this."),
            "{text}"
        );
        // Removing every # gives a file that sets each option to its
        // default, or to an example for an unset option.
        let uncommented = text
            .lines()
            .filter(|line| line.starts_with("# ") && line.contains(" = ") || line.starts_with('['))
            .map(|line| line.trim_start_matches("# "))
            .collect::<Vec<_>>()
            .join("\n");
        let document: DocumentMut = uncommented.parse().expect("the document parses");
        assert!(document.get("listing").is_some());
    }

    #[test]
    fn oversized_and_broken_files_are_errors() {
        let root = tempfile::tempdir().unwrap();
        let big = root.path().join("big.toml");
        fs::write(&big, "a = 1\n".repeat(200_000)).unwrap();
        assert!(read_document(&big).unwrap_err().contains("larger than"));
        let broken = root.path().join("broken.toml");
        fs::write(&broken, "[unclosed").unwrap();
        assert!(read_document(&broken).is_err());
        assert!(
            read_document(&root.path().join("none.toml"))
                .unwrap()
                .is_none()
        );
    }
}
