//! The declared model: dynamic values, leaf kinds, the schema tree, and the
//! traits that the derives implement.

use std::{error::Error, fmt, iter, marker::PhantomData};

/// Keys that the options panel always binds, and that no option or variant
/// can take as its accelerator: the navigation keys, revert all, save, and
/// `q`, which closes the panel from any level.
///
/// The panel binds other keys too, such as `x` and `+`. An accelerator can
/// take one of those, and the panel then leaves that key to the accelerator
/// in its level. An accelerator is a visible character, never a space.
pub const RESERVED_KEYS: [char; 7] = ['h', 'j', 'k', 'l', 'q', 'R', 'S'];

/// Keys that the panel binds in every level besides the reserved keys, and
/// that it thus never chooses as an accelerator: revert, clear, and select
/// all. An option can still take one explicitly.
const PANEL_KEYS: [char; 3] = ['x', 'X', 'A'];

/// Return whether `key` cannot be an accelerator: it is reserved, or it is
/// not a visible character.
pub fn unusable_key(key: char) -> bool {
    RESERVED_KEYS.contains(&key) || key.is_whitespace() || key.is_control()
}

/// The dynamic value of one leaf, as files, scripts, and the panel see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A bool.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A text.
    Text(String),
    /// The `snake_case` name of a choice variant.
    Choice(String),
    /// The names of the members of a set, in declaration order.
    Set(Vec<String>),
    /// A list of texts, or the variant names of an order, in order.
    List(Vec<String>),
    /// No value, for an optional leaf.
    Unset,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(true) => f.write_str("on"),
            Self::Bool(false) => f.write_str("off"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Text(text) | Self::Choice(text) => f.write_str(text),
            Self::Set(items) | Self::List(items) => {
                if items.is_empty() {
                    f.write_str("none")
                } else {
                    f.write_str(&items.join(", "))
                }
            }
            Self::Unset => f.write_str("default"),
        }
    }
}

/// One variant of a choice, a set, or an order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Variant {
    /// The `snake_case` name that files and scripts use.
    pub name: &'static str,
    /// The text that the panel shows.
    pub label: &'static str,
    /// The accelerator of the variant in a choice's or set's submenu.
    pub key: Option<char>,
}

/// The kind of a leaf, which sets its control and its file form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A bool.
    Bool,
    /// An integer between `min` and `max`, changed by `step`.
    Int {
        /// The lowest value.
        min: i64,
        /// The highest value.
        max: i64,
        /// The change of one step.
        step: i64,
    },
    /// A text.
    Text,
    /// One variant of a list.
    Choice(Vec<Variant>),
    /// Any number of variants of a list.
    Set(Vec<Variant>),
    /// Every variant of a list, once, in an order.
    Order(Vec<Variant>),
    /// A list of texts.
    List,
}

impl Kind {
    /// Return the name of the kind for messages.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Int { .. } => "integer",
            Self::Text => "text",
            Self::Choice(_) => "choice",
            Self::Set(_) => "set",
            Self::Order(_) => "order",
            Self::List => "list",
        }
    }

    /// Return the variants of a choice, a set, or an order.
    #[must_use]
    pub fn variants(&self) -> &[Variant] {
        match self {
            Self::Choice(variants) | Self::Set(variants) | Self::Order(variants) => variants,
            _ => &[],
        }
    }
}

/// The limits of an integer field, from its attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The lowest value, if the field sets one.
    pub min: Option<i64>,
    /// The highest value, if the field sets one.
    pub max: Option<i64>,
    /// The change of one step.
    pub step: i64,
}

/// The declared metadata of one field, which the derive passes to
/// [`OptionField::describe`].
#[derive(Clone, Copy, Debug)]
pub struct FieldMeta {
    /// The field name, which is the last part of its path.
    pub name: &'static str,
    /// The text that the panel shows.
    pub label: &'static str,
    /// The accelerator of the field in its panel.
    pub key: Option<char>,
    /// The help text, from the doc comment.
    pub help: &'static str,
    /// Whether local files may not set the field.
    pub global: bool,
    /// Whether the panel shows the items of a group field in its parent.
    pub inline: bool,
    /// The limits of an integer field.
    pub limits: Option<Limits>,
}

/// One leaf of the schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leaf {
    /// The field name.
    pub name: &'static str,
    /// The text that the panel shows.
    pub label: &'static str,
    /// The accelerator of the leaf in its panel.
    pub key: Option<char>,
    /// The help text.
    pub help: &'static str,
    /// The kind of the leaf.
    pub kind: Kind,
    /// Whether the leaf can be unset.
    pub optional: bool,
    /// Whether local files may not set the leaf.
    pub global: bool,
}

/// One group of the schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// The field name, or an empty name for the root.
    pub name: &'static str,
    /// The text that the panel shows.
    pub label: &'static str,
    /// The accelerator of the group in its parent.
    pub key: Option<char>,
    /// The help text.
    pub help: &'static str,
    /// Whether the panel shows the items of the group in its parent.
    pub inline: bool,
    /// The leaves and groups of the group, in declaration order.
    pub items: Vec<Item>,
}

/// A leaf or a group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// A leaf.
    Leaf(Leaf),
    /// A group.
    Group(Group),
}

impl Item {
    /// Return the field name of the item.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Leaf(leaf) => leaf.name,
            Self::Group(group) => group.name,
        }
    }

    /// Return the label of the item.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Leaf(leaf) => leaf.label,
            Self::Group(group) => group.label,
        }
    }

    /// Return the accelerator of the item.
    #[must_use]
    pub const fn key(&self) -> Option<char> {
        match self {
            Self::Leaf(leaf) => leaf.key,
            Self::Group(group) => group.key,
        }
    }
}

/// The schema of a root group: its tree, and an index of its leaves by path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schema {
    /// The root group.
    pub root: Group,
    /// Each leaf path with its leaf, in declaration order.
    leaves: Vec<(String, Leaf)>,
}

impl Schema {
    /// Build the schema of `T`.
    #[must_use]
    pub fn of<T: OptionGroup>() -> Self {
        let mut root = Group {
            name: "",
            label: "Options",
            key: None,
            help: "",
            inline: false,
            items: T::items(),
        };
        assign_group_keys(&mut root);
        let mut leaves = Vec::new();
        collect_leaves(&root, "", &mut leaves);
        Self { root, leaves }
    }

    /// Return each leaf path with its leaf, in declaration order.
    #[must_use]
    pub fn leaves(&self) -> &[(String, Leaf)] {
        &self.leaves
    }

    /// Return the leaf at `path`.
    #[must_use]
    pub fn leaf(&self, path: &str) -> Option<&Leaf> {
        self.leaves
            .iter()
            .find(|(leaf_path, _)| leaf_path == path)
            .map(|(_, leaf)| leaf)
    }

    /// Return the group at `path`, where an empty path is the root.
    #[must_use]
    pub fn group(&self, path: &str) -> Option<&Group> {
        let mut group = &self.root;
        for part in path.split('.').filter(|part| !part.is_empty()) {
            group = group.items.iter().find_map(|item| match item {
                Item::Group(child) if child.name == part => Some(child),
                _ => None,
            })?;
        }
        Some(group)
    }

    /// Return problems that the derive cannot see across flattened groups:
    /// clashing accelerator keys, and two leaves with one path.
    #[must_use]
    pub fn key_problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        check_group_keys(&self.root, "", &mut problems);
        for (index, (path, _)) in self.leaves.iter().enumerate() {
            if self.leaves[..index]
                .iter()
                .any(|(earlier, _)| earlier == path)
            {
                problems.push(format!("{path}: two leaves have this path"));
            }
        }
        problems
    }
}

/// Give each row of `group`'s level without a key an accelerator, then do
/// the same in each level below it.
fn assign_group_keys(group: &mut Group) {
    let mut rows = Vec::new();
    level_rows(&mut group.items, &mut rows);
    // Rows choose in the order that the panel shows them.
    rows.sort_by_cached_key(|(label, _)| label.to_lowercase());
    assign_keys(&mut rows);
    assign_nested_keys(&mut group.items);
}

/// Add the label and the key of each row that `items` shows in their level
/// to `out`. An inline group's items take its place.
fn level_rows<'a>(items: &'a mut [Item], out: &mut Vec<(&'static str, &'a mut Option<char>)>) {
    for item in items {
        match item {
            Item::Group(child) => {
                if child.inline {
                    level_rows(&mut child.items, out);
                } else {
                    out.push((child.label, &mut child.key));
                }
            }
            Item::Leaf(leaf) => out.push((leaf.label, &mut leaf.key)),
        }
    }
}

/// Assign the keys of the levels below `items`: each group's own, and each
/// choice's and set's variants. An order's rows move rather than activate,
/// so they take no keys.
fn assign_nested_keys(items: &mut [Item]) {
    for item in items {
        match item {
            Item::Group(child) if child.inline => assign_nested_keys(&mut child.items),
            Item::Group(child) => assign_group_keys(child),
            Item::Leaf(leaf) => {
                if let Kind::Choice(variants) | Kind::Set(variants) = &mut leaf.kind {
                    let mut rows: Vec<_> = variants
                        .iter_mut()
                        .map(|variant| (variant.label, &mut variant.key))
                        .collect();
                    assign_keys(&mut rows);
                }
            }
        }
    }
}

/// Give each row of one level without a key an accelerator.
///
/// A declared key stays. Each other row takes the initial of its label if
/// it is free, and the rows that remain then take another letter of their
/// labels, or else any free letter. A capital serves only when no lowercase
/// letter is free. Reserved keys and the panel's own keys never serve.
pub fn assign_keys(rows: &mut [(&'static str, &mut Option<char>)]) {
    let mut taken: Vec<char> = rows.iter().filter_map(|(_, key)| **key).collect();
    for (label, key) in rows.iter_mut().filter(|(_, key)| key.is_none()) {
        let initial = label.chars().next().map(|c| c.to_ascii_lowercase());
        **key = first_free(&mut taken, initial.into_iter());
    }
    for (label, key) in rows.iter_mut().filter(|(_, key)| key.is_none()) {
        let words = label
            .split_whitespace()
            .skip(1)
            .filter_map(|word| word.chars().next());
        let letters = words.chain(label.chars()).map(|c| c.to_ascii_lowercase());
        let lower = letters.clone().chain('a'..='z');
        let upper = letters.map(|c| c.to_ascii_uppercase()).chain('A'..='Z');
        **key = first_free(&mut taken, lower.chain(upper));
    }
}

/// Return the first of `candidates` that can serve as an accelerator and is
/// not in `taken`, and add it to `taken`.
fn first_free(taken: &mut Vec<char>, mut candidates: impl Iterator<Item = char>) -> Option<char> {
    let free = candidates.find(|candidate| {
        candidate.is_ascii_alphabetic()
            && !unusable_key(*candidate)
            && !PANEL_KEYS.contains(candidate)
            && !taken.contains(candidate)
    })?;
    taken.push(free);
    Some(free)
}

/// Add each leaf below `group` to `out`, with its full path.
fn collect_leaves(group: &Group, prefix: &str, out: &mut Vec<(String, Leaf)>) {
    for item in &group.items {
        match item {
            Item::Leaf(leaf) => out.push((join_path(prefix, leaf.name), leaf.clone())),
            Item::Group(child) => collect_leaves(child, &join_path(prefix, child.name), out),
        }
    }
}

/// Return the items that the panel shows for `group`, with the path of the
/// group that holds each one, in the alphabetical order of their labels. An
/// inline group's items take its place.
#[must_use]
pub fn panel_items<'a>(group: &'a Group, path: &str) -> Vec<(String, &'a Item)> {
    let mut items = Vec::new();
    for item in &group.items {
        match item {
            Item::Group(child) if child.inline => {
                items.extend(panel_items(child, &join_path(path, child.name)));
            }
            _ => items.push((path.to_string(), item)),
        }
    }
    items.sort_by_cached_key(|(_, item)| item.label().to_lowercase());
    items
}

/// Add key problems of `group` and its descendants to `out`.
fn check_group_keys(group: &Group, path: &str, out: &mut Vec<String>) {
    let mut seen: Vec<(char, &str)> = Vec::new();
    for (holder, item) in panel_items(group, path) {
        let path = holder.as_str();
        if let Some(key) = item.key() {
            if unusable_key(key) {
                out.push(format!("{path}: {key:?} of {} is reserved", item.name()));
            }
            if let Some((_, other)) = seen.iter().find(|(seen, _)| *seen == key) {
                out.push(format!(
                    "{path}: '{key}' is used by {other} and {}",
                    item.name()
                ));
            }
            seen.push((key, item.name()));
        }
        match item {
            Item::Group(child) => check_group_keys(child, &join_path(path, child.name), out),
            Item::Leaf(leaf) => {
                let mut variant_keys: Vec<char> = Vec::new();
                for variant in leaf.kind.variants() {
                    if let Some(key) = variant.key {
                        if variant_keys.contains(&key) || unusable_key(key) {
                            out.push(format!(
                                "{}: variant key '{key}' of {} clashes",
                                join_path(path, leaf.name),
                                variant.name
                            ));
                        }
                        variant_keys.push(key);
                    }
                }
            }
        }
    }
}

#[doc(hidden)]
/// Join a path prefix and a name with a dot.
#[must_use]
pub fn join_path(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else if name.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

/// The reason that a value cannot go into a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetError {
    /// The path names no leaf.
    UnknownPath,
    /// The value has the wrong kind for the leaf.
    Kind {
        /// The kind of the leaf.
        expected: &'static str,
        /// The value.
        got: String,
    },
    /// The value is not valid for the leaf.
    Invalid(String),
}

impl fmt::Display for SetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPath => f.write_str("unknown option"),
            Self::Kind { expected, got } => write!(f, "expected a {expected}, got {got}"),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl Error for SetError {}

/// A struct of options, which `#[derive(OptionGroup)]` implements.
pub trait OptionGroup: Clone + PartialEq + Default + 'static {
    /// Return the leaves and groups of the struct.
    fn items() -> Vec<Item>;

    /// Return the value at `path`, relative to this group.
    fn get_path(&self, path: &[&str]) -> Option<Value>;

    /// Set the value at `path`, relative to this group.
    fn set_path(&mut self, path: &[&str], value: &Value) -> Result<(), SetError>;

    /// Add the path of each leaf that differs between `self` and `other` to
    /// `out`, with `prefix` before it.
    fn changed(&self, other: &Self, prefix: &str, out: &mut Vec<String>);

    /// Return the value at a dotted `path`.
    fn get(&self, path: &str) -> Option<Value> {
        self.get_path(&split_path(path))
    }

    /// Set the value at a dotted `path`.
    fn set(&mut self, path: &str, value: &Value) -> Result<(), SetError> {
        self.set_path(&split_path(path), value)
    }
}

/// Split a dotted path into its parts.
fn split_path(path: &str) -> Vec<&str> {
    path.split('.').filter(|part| !part.is_empty()).collect()
}

/// A field of an option group: a leaf, or a nested group.
pub trait OptionField: Clone + PartialEq + Sized {
    /// Return the schema item of a field with this type.
    fn describe(meta: FieldMeta) -> Item;

    /// Return the value at `path`, relative to this field.
    fn get_path(&self, path: &[&str]) -> Option<Value>;

    /// Set the value at `path`, relative to this field.
    fn set_path(&mut self, path: &[&str], value: &Value) -> Result<(), SetError>;

    /// Add the paths below `path` that differ between `self` and `other` to
    /// `out`.
    fn changed(&self, other: &Self, path: &str, out: &mut Vec<String>);
}

/// A type that one leaf holds.
pub trait LeafValue: Clone + PartialEq + Sized {
    /// Whether the leaf can be unset.
    const OPTIONAL: bool = false;

    /// Return the kind of the leaf.
    fn kind() -> Kind;

    /// Return the dynamic value.
    fn to_value(&self) -> Value;

    /// Build the type from a dynamic value.
    fn from_value(value: &Value) -> Result<Self, String>;
}

/// The variants of a fieldless enum, which `#[derive(OptionEnum)]`
/// implements.
pub trait OptionEnum: Copy + PartialEq + 'static {
    /// Every variant, in declaration order.
    const ALL: &'static [Self];

    /// Return the metadata of every variant, in declaration order.
    fn variants() -> &'static [Variant];

    /// Return the `snake_case` name of the variant.
    fn name(self) -> &'static str;

    /// Return the variant named `name`, in any ASCII case.
    fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|variant| variant.name().eq_ignore_ascii_case(name))
    }

    /// Return the declaration index of the variant.
    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|variant| *variant == self)
            .unwrap_or_default()
    }

    /// Return the label of the variant.
    fn label(self) -> &'static str {
        Self::variants()[self.index()].label
    }
}

#[doc(hidden)]
/// Convert a choice value into an enum.
pub fn enum_from_value<E: OptionEnum>(value: &Value) -> Result<E, String> {
    let (Value::Choice(name) | Value::Text(name)) = value else {
        return Err(format!("expected a choice, got {value:?}"));
    };
    E::from_name(name).ok_or_else(|| {
        let names: Vec<&str> = E::variants().iter().map(|variant| variant.name).collect();
        format!(
            "unknown choice {name:?}, expected one of {}",
            names.join(", ")
        )
    })
}

#[doc(hidden)]
/// Check that an integer value is between `min` and `max`.
pub fn check_range(value: &Value, min: i64, max: i64) -> Result<(), SetError> {
    if let Value::Int(number) = value
        && !(min..=max).contains(number)
    {
        return Err(SetError::Invalid(format!(
            "{number} is outside {min} to {max}"
        )));
    }
    Ok(())
}

#[doc(hidden)]
/// Return the schema item of a leaf field with type `T`.
#[must_use]
pub fn leaf_item<T: LeafValue>(meta: FieldMeta) -> Item {
    let mut kind = T::kind();
    if let (Kind::Int { min, max, step }, Some(limits)) = (&mut kind, meta.limits) {
        // A limit stays within the range of the type.
        let (low, high) = (*min, *max);
        if let Some(lower) = limits.min {
            *min = lower.clamp(low, high);
        }
        if let Some(upper) = limits.max {
            *max = upper.clamp(*min, high);
        }
        *step = limits.step;
    }
    Item::Leaf(Leaf {
        name: meta.name,
        label: meta.label,
        key: meta.key,
        help: meta.help,
        kind,
        optional: T::OPTIONAL,
        global: meta.global,
    })
}

#[doc(hidden)]
/// Return the schema item of a group field with type `T`.
///
/// A group that is declared global makes each of its leaves global.
#[must_use]
pub fn group_item<T: OptionGroup>(meta: FieldMeta) -> Item {
    let mut items = T::items();
    if meta.global {
        mark_global(&mut items);
    }
    Item::Group(Group {
        name: meta.name,
        label: meta.label,
        key: meta.key,
        help: meta.help,
        inline: meta.inline,
        items,
    })
}

/// Make every leaf in `items` global.
fn mark_global(items: &mut [Item]) {
    for item in items {
        match item {
            Item::Leaf(leaf) => leaf.global = true,
            Item::Group(group) => mark_global(&mut group.items),
        }
    }
}

#[doc(hidden)]
/// Fail to compile unless `T` is a leaf type. The derive calls it for a
/// field with a `validate` function, which checks leaves only.
pub const fn assert_leaf<T: LeafValue>() {}

#[doc(hidden)]
/// Return the value of a leaf field, which takes no further path.
pub fn leaf_get<T: LeafValue>(value: &T, path: &[&str]) -> Option<Value> {
    path.is_empty().then(|| value.to_value())
}

#[doc(hidden)]
/// Set the value of a leaf field, which takes no further path.
pub fn leaf_set<T: LeafValue>(
    target: &mut T,
    path: &[&str],
    value: &Value,
) -> Result<(), SetError> {
    if !path.is_empty() {
        return Err(SetError::UnknownPath);
    }
    *target = T::from_value(value).map_err(SetError::Invalid)?;
    Ok(())
}

#[doc(hidden)]
/// Add `path` to `out` when a leaf differs.
pub fn leaf_changed<T: LeafValue>(left: &T, right: &T, path: &str, out: &mut Vec<String>) {
    if left != right {
        out.push(path.to_string());
    }
}

/// Implement [`OptionField`] for types that implement [`LeafValue`]: a list
/// of types, or one generic type as `impl<T: Bound> Type<T>`.
#[doc(hidden)]
#[macro_export]
macro_rules! leaf_field {
    (impl<$($param:ident: $bound:path),*> $ty:ty) => {
        impl<$($param: $bound),*> $crate::OptionField for $ty {
            fn describe(meta: $crate::FieldMeta) -> $crate::Item {
                $crate::leaf_item::<Self>(meta)
            }

            fn get_path(&self, path: &[&str]) -> ::core::option::Option<$crate::Value> {
                $crate::leaf_get(self, path)
            }

            fn set_path(
                &mut self,
                path: &[&str],
                value: &$crate::Value,
            ) -> ::core::result::Result<(), $crate::SetError> {
                $crate::leaf_set(self, path, value)
            }

            fn changed(
                &self,
                other: &Self,
                path: &str,
                out: &mut ::std::vec::Vec<::std::string::String>,
            ) {
                $crate::leaf_changed(self, other, path, out);
            }
        }
    };
    ($($ty:ty),* $(,)?) => {
        $($crate::leaf_field!(impl<> $ty);)*
    };
}

impl LeafValue for bool {
    fn kind() -> Kind {
        Kind::Bool
    }

    fn to_value(&self) -> Value {
        Value::Bool(*self)
    }

    fn from_value(value: &Value) -> Result<Self, String> {
        match value {
            Value::Bool(value) => Ok(*value),
            other => Err(format!("expected a bool, got {other:?}")),
        }
    }
}

impl LeafValue for String {
    fn kind() -> Kind {
        Kind::Text
    }

    fn to_value(&self) -> Value {
        Value::Text(self.clone())
    }

    fn from_value(value: &Value) -> Result<Self, String> {
        match value {
            Value::Text(text) => Ok(text.clone()),
            other => Err(format!("expected a text, got {other:?}")),
        }
    }
}

impl LeafValue for Vec<String> {
    fn kind() -> Kind {
        Kind::List
    }

    fn to_value(&self) -> Value {
        Value::List(self.clone())
    }

    fn from_value(value: &Value) -> Result<Self, String> {
        match value {
            Value::List(items) | Value::Set(items) => Ok(items.clone()),
            other => Err(format!("expected a list, got {other:?}")),
        }
    }
}

/// Implement [`LeafValue`] for integer types.
macro_rules! int_leaf {
    ($($ty:ty),*) => {
        $(
            impl LeafValue for $ty {
                fn kind() -> Kind {
                    Kind::Int {
                        min: i64::try_from(<$ty>::MIN).unwrap_or(i64::MIN),
                        max: i64::try_from(<$ty>::MAX).unwrap_or(i64::MAX),
                        step: 1,
                    }
                }

                fn to_value(&self) -> Value {
                    Value::Int(i64::try_from(*self).unwrap_or(i64::MAX))
                }

                fn from_value(value: &Value) -> Result<Self, String> {
                    match value {
                        Value::Int(number) => <$ty>::try_from(*number)
                            .map_err(|_| format!("{number} is out of range")),
                        other => Err(format!("expected an integer, got {other:?}")),
                    }
                }
            }
        )*
    };
}

int_leaf!(i64, i32, u32, u16, u8, usize);

leaf_field!(bool, String, Vec<String>, i64, i32, u32, u16, u8, usize);

impl<T: LeafValue> LeafValue for Option<T> {
    const OPTIONAL: bool = true;

    fn kind() -> Kind {
        T::kind()
    }

    fn to_value(&self) -> Value {
        self.as_ref().map_or(Value::Unset, LeafValue::to_value)
    }

    fn from_value(value: &Value) -> Result<Self, String> {
        match value {
            Value::Unset => Ok(None),
            other => T::from_value(other).map(Some),
        }
    }
}

leaf_field!(impl<T: LeafValue> Option<T>);

/// A set of the variants of an [`OptionEnum`], in declaration order.
#[derive(Clone, PartialEq, Eq)]
pub struct Set<E> {
    /// Membership by declaration index.
    members: Vec<bool>,
    /// The variant type.
    variant: PhantomData<E>,
}

impl<E: OptionEnum> fmt::Debug for Set<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set()
            .entries(self.iter().map(OptionEnum::name))
            .finish()
    }
}

impl<E: OptionEnum> Default for Set<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: OptionEnum> Set<E> {
    /// Return an empty set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            members: vec![false; E::ALL.len()],
            variant: PhantomData,
        }
    }

    /// Return a set of every variant.
    #[must_use]
    pub fn all() -> Self {
        Self {
            members: vec![true; E::ALL.len()],
            variant: PhantomData,
        }
    }

    /// Return whether the set holds `variant`.
    #[must_use]
    pub fn contains(&self, variant: E) -> bool {
        self.members
            .get(variant.index())
            .copied()
            .unwrap_or_default()
    }

    /// Add or remove `variant`.
    pub fn set(&mut self, variant: E, member: bool) {
        if let Some(slot) = self.members.get_mut(variant.index()) {
            *slot = member;
        }
    }

    /// Return the members, in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = E> + '_ {
        E::ALL
            .iter()
            .copied()
            .filter(|variant| self.contains(*variant))
    }

    /// Return whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.members.iter().any(|member| *member)
    }

    /// Return whether the set holds every variant.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.members.iter().all(|member| *member)
    }
}

impl<E: OptionEnum> FromIterator<E> for Set<E> {
    fn from_iter<I: IntoIterator<Item = E>>(iter: I) -> Self {
        let mut set = Self::new();
        for variant in iter {
            set.set(variant, true);
        }
        set
    }
}

impl<E: OptionEnum> LeafValue for Set<E> {
    fn kind() -> Kind {
        Kind::Set(E::variants().to_vec())
    }

    fn to_value(&self) -> Value {
        Value::Set(
            self.iter()
                .map(|variant| variant.name().to_string())
                .collect(),
        )
    }

    fn from_value(value: &Value) -> Result<Self, String> {
        let (Value::Set(names) | Value::List(names)) = value else {
            return Err(format!("expected a set, got {value:?}"));
        };
        names
            .iter()
            .map(|name| enum_from_value::<E>(&Value::Choice(name.clone())))
            .collect()
    }
}

leaf_field!(impl<E: OptionEnum> Set<E>);

/// Every variant of an [`OptionEnum`], once, in an order.
///
/// An order reads from any list of names. It drops a repeated name, and
/// appends the variants that the list leaves out in declaration order, so it
/// always holds every variant.
#[derive(Clone, PartialEq, Eq)]
pub struct Order<E> {
    /// The variants, in order.
    variants: Vec<E>,
}

impl<E: OptionEnum> fmt::Debug for Order<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.variants.iter().map(|variant| variant.name()))
            .finish()
    }
}

impl<E: OptionEnum> Default for Order<E> {
    fn default() -> Self {
        Self {
            variants: E::ALL.to_vec(),
        }
    }
}

impl<E: OptionEnum> Order<E> {
    /// Return the variants in order.
    pub fn iter(&self) -> impl Iterator<Item = E> + '_ {
        self.variants.iter().copied()
    }

    /// Return the order with `variant` moved to the front.
    #[must_use]
    pub fn with_first(&self, variant: E) -> Self {
        iter::once(variant).chain(self.iter()).collect()
    }
}

impl<E: OptionEnum> FromIterator<E> for Order<E> {
    fn from_iter<I: IntoIterator<Item = E>>(iter: I) -> Self {
        let mut variants: Vec<E> = Vec::with_capacity(E::ALL.len());
        for variant in iter.into_iter().chain(E::ALL.iter().copied()) {
            if !variants.contains(&variant) {
                variants.push(variant);
            }
        }
        Self { variants }
    }
}

impl<E: OptionEnum> LeafValue for Order<E> {
    fn kind() -> Kind {
        Kind::Order(E::variants().to_vec())
    }

    fn to_value(&self) -> Value {
        Value::List(
            self.iter()
                .map(|variant| variant.name().to_string())
                .collect(),
        )
    }

    fn from_value(value: &Value) -> Result<Self, String> {
        let (Value::List(names) | Value::Set(names)) = value else {
            return Err(format!("expected an order, got {value:?}"));
        };
        names
            .iter()
            .map(|name| enum_from_value::<E>(&Value::Choice(name.clone())))
            .collect()
    }
}

leaf_field!(impl<E: OptionEnum> Order<E>);

#[cfg(test)]
mod key_tests {
    use super::assign_keys;

    /// Return the keys that `labels` get, with `declared` keys first.
    fn keys(labels: &[(&'static str, Option<char>)]) -> Vec<Option<char>> {
        let mut keys: Vec<Option<char>> = labels.iter().map(|(_, key)| *key).collect();
        let mut rows: Vec<_> = labels
            .iter()
            .map(|(label, _)| *label)
            .zip(keys.iter_mut())
            .collect();
        assign_keys(&mut rows);
        keys
    }

    #[test]
    fn a_row_takes_its_initial() {
        assert_eq!(
            keys(&[("order", None), ("metadata", None)]),
            [Some('o'), Some('m')]
        );
    }

    #[test]
    fn a_declared_key_stays_and_takes_its_letter() {
        assert_eq!(
            keys(&[("order", None), ("other", Some('o'))]),
            [Some('r'), Some('o')]
        );
    }

    #[test]
    fn initials_go_first_so_an_early_fallback_takes_no_later_initial() {
        // display wants d, which direction takes first, and its fallback i
        // would be the initial of info.
        assert_eq!(
            keys(&[("direction", None), ("display", None), ("info", None)]),
            [Some('d'), Some('s'), Some('i')]
        );
    }

    #[test]
    fn reserved_and_panel_keys_never_serve() {
        // h, l, and x are not free, so each row falls back to a later word or
        // letter.
        assert_eq!(
            keys(&[("hidden files", None), ("layout", None), ("xray", None)]),
            [Some('f'), Some('a'), Some('r')]
        );
    }

    #[test]
    fn a_capital_serves_only_when_no_lowercase_letter_is_free() {
        // Of the lowercase letters, h, j, k, l, q, and x never serve, which
        // leaves 20.
        let labels: Vec<(&'static str, Option<char>)> = ["alpha"; 21]
            .into_iter()
            .map(|label| (label, None))
            .collect();
        let keys = keys(&labels);
        let lower = keys.iter().flatten().filter(|key| key.is_ascii_lowercase());
        assert_eq!(lower.count(), 20, "every free lowercase letter serves");
        // A is the panel's select-all key, so the label's next letter serves.
        assert_eq!(keys[20], Some('L'), "the next row takes a capital");
    }
}
