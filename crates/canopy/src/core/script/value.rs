//! Conversions between Luau values and command argument values.

use std::{collections::BTreeMap, fmt, result::Result as StdResult};

use ruau::vm::{
    IntoLua, MarshaledPair, RuntimeError, Scope, ScopedValue, Table, TableLayout,
    UnsupportedTableKey, ValueSnapshot, classify_marshaled_table,
};

use super::{ArgValue, NodeId, Point, RectI32, Size, node_id_to_arg};

/// Return a display name for a scoped value's type.
/// Copy the text behind a scoped string value.
pub(super) fn scoped_value_to_string<'s>(
    scope: &Scope<'s>,
    value: ScopedValue<'s>,
) -> StdResult<String, String> {
    match value {
        ScopedValue::String(text) => scope
            .string_bytes(text)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .map_err(|err| err.to_string()),
        other => Err(format!("expected string, got {}", other.type_name())),
    }
}

/// Convert a scoped value into a displayable string for diagnostics.
pub(super) fn scoped_value_to_display<'s>(scope: &Scope<'s>, value: ScopedValue<'s>) -> String {
    match value {
        ScopedValue::Nil => "nil".to_string(),
        ScopedValue::Boolean(value) => value.to_string(),
        ScopedValue::Integer(value) => value.to_string(),
        ScopedValue::Number(value) => value.to_string(),
        ScopedValue::String(text) => scope
            .string_bytes(text)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_else(|_| "<string>".to_string()),
        other => format!("<{}>", other.type_name()),
    }
}

/// Canopy-owned location within a nested command value.
#[derive(Clone)]
pub(super) struct ValuePath(String);

impl ValuePath {
    /// Start a value path at a named boundary.
    pub(super) fn root(name: &str) -> Self {
        Self(name.to_string())
    }

    /// Extend this path with a one-based sequence index.
    pub(super) fn index(&self, index: usize) -> Self {
        Self(format!("{}[{index}]", self.0))
    }

    /// Extend this path with a string map field.
    pub(super) fn field(&self, field: &str) -> Self {
        if is_luau_identifier(field) {
            Self(format!("{}.{field}", self.0))
        } else {
            let field = field.replace('\\', "\\\\").replace('"', "\\\"");
            Self(format!("{}[\"{field}\"]", self.0))
        }
    }

    /// Prefix a conversion failure with this path.
    pub(super) fn error(&self, message: impl fmt::Display) -> String {
        format!("{}: {message}", self.0)
    }
}

/// Return whether a string can use dotted Luau field notation.
fn is_luau_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

/// Apply Canopy's shared numeric policy to a script number.
fn number_to_arg_value(value: f64, path: &ValuePath) -> StdResult<ArgValue, String> {
    if !value.is_finite() {
        return Err(path.error("non-finite numbers are not supported"));
    }
    if value.fract() == 0.0 && value >= i64::MIN as f64 && value < -(i64::MIN as f64) {
        Ok(ArgValue::Int(value as i64))
    } else {
        Ok(ArgValue::Float(value))
    }
}

/// Reject table layouts outside Canopy's array-or-string-map domain model.
fn reject_unsupported_layout(layout: &TableLayout, path: &ValuePath) -> StdResult<(), String> {
    match layout {
        TableLayout::Empty | TableLayout::Sequence { .. } | TableLayout::StringMap { .. } => Ok(()),
        TableLayout::Sparse { first_missing, .. } => {
            Err(path.error(format!("sparse table missing index {first_missing}")))
        }
        TableLayout::Mixed { .. } => {
            Err(path.error("mixed integer and string table keys are not supported"))
        }
        TableLayout::UnsupportedKey { key } => Err(path.error(unsupported_key_message(key))),
    }
}

/// Describe one table key rejected by the shared Ruau classifier.
fn unsupported_key_message(key: &UnsupportedTableKey) -> String {
    match key {
        UnsupportedTableKey::NonPositiveInteger { value } => {
            format!("table index must be positive, got {value}")
        }
        UnsupportedTableKey::FractionalNumber { display } => {
            format!("table index must be integral, got {display}")
        }
        UnsupportedTableKey::IndexOutOfRange { display } => {
            format!("table index is out of range: {display}")
        }
        UnsupportedTableKey::DuplicateIndex { index } => {
            format!("duplicate table index {index}")
        }
        UnsupportedTableKey::Type { type_name } => {
            format!("unsupported table key type: {type_name}")
        }
    }
}

/// Read a sequence key after `TableLayout` has established a dense layout.
fn scoped_sequence_index(key: ScopedValue<'_>, path: &ValuePath) -> StdResult<usize, String> {
    match key {
        ScopedValue::Integer(index) => usize::try_from(index)
            .map_err(|_| path.error(format!("table index is out of range: {index}"))),
        ScopedValue::Number(index) => Ok(index as usize),
        other => Err(path.error(format!(
            "expected sequence index, got {}",
            other.type_name()
        ))),
    }
}

/// Read a strict UTF-8 string key after layout classification.
fn scoped_table_key<'s>(
    scope: &Scope<'s>,
    key: ScopedValue<'s>,
    path: &ValuePath,
) -> StdResult<String, String> {
    let ScopedValue::String(key) = key else {
        return Err(path.error(format!("expected string key, got {}", key.type_name())));
    };
    let bytes = scope
        .string_bytes(key)
        .map_err(|error| path.error(error.to_string()))?;
    String::from_utf8(bytes).map_err(|error| path.error(format!("invalid UTF-8 key: {error}")))
}

/// Convert a scoped value into a dynamic command argument.
pub(super) fn scoped_to_arg_value<'s>(
    scope: &Scope<'s>,
    value: ScopedValue<'s>,
) -> StdResult<ArgValue, String> {
    scoped_to_arg_value_at(scope, value, &ValuePath::root("value"))
}

/// Convert a scoped value at one nested command-value path.
pub(super) fn scoped_to_arg_value_at<'s>(
    scope: &Scope<'s>,
    value: ScopedValue<'s>,
    path: &ValuePath,
) -> StdResult<ArgValue, String> {
    match value {
        ScopedValue::Nil => Ok(ArgValue::Null),
        ScopedValue::Boolean(value) => Ok(ArgValue::Bool(value)),
        ScopedValue::Integer(value) => Ok(ArgValue::Int(value)),
        ScopedValue::Number(value) => number_to_arg_value(value, path),
        ScopedValue::String(text) => scope
            .string_bytes(text)
            .map_err(|error| path.error(error.to_string()))
            .and_then(|bytes| {
                String::from_utf8(bytes)
                    .map(ArgValue::String)
                    .map_err(|error| path.error(format!("invalid UTF-8 string: {error}")))
            }),
        ScopedValue::Table(table) => table_to_arg_value(scope, table, path),
        ScopedValue::Userdata(userdata) => userdata
            .borrow::<NodeId>(scope)
            .map(|node_id| ArgValue::Node(*node_id))
            .map_err(|_| path.error("expected NodeId userdata")),
        other => Err(path.error(format!(
            "unsupported script value type: {}",
            other.type_name()
        ))),
    }
}

/// Convert a scoped table into an `ArgValue`.
fn table_to_arg_value<'s>(
    scope: &Scope<'s>,
    table: Table<'s>,
    path: &ValuePath,
) -> StdResult<ArgValue, String> {
    let layout = table
        .layout(scope)
        .map_err(|error| path.error(error.to_string()))?;
    reject_unsupported_layout(&layout, path)?;
    match layout {
        TableLayout::Empty => Ok(ArgValue::Map(BTreeMap::new())),
        TableLayout::Sequence { len } => {
            let mut values = vec![None; len];
            for (key, value) in table
                .pairs(scope)
                .map_err(|error| path.error(error.to_string()))?
            {
                let index = scoped_sequence_index(key, path)?;
                values[index - 1] = Some(scoped_to_arg_value_at(scope, value, &path.index(index))?);
            }
            Ok(ArgValue::Array(
                values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        value.ok_or_else(|| path.error(format!("missing index {}", index + 1)))
                    })
                    .collect::<StdResult<_, _>>()?,
            ))
        }
        TableLayout::StringMap { .. } => {
            let mut values = BTreeMap::new();
            for (key, value) in table
                .pairs(scope)
                .map_err(|error| path.error(error.to_string()))?
            {
                let key = scoped_table_key(scope, key, path)?;
                values.insert(
                    key.clone(),
                    scoped_to_arg_value_at(scope, value, &path.field(&key))?,
                );
            }
            Ok(ArgValue::Map(values))
        }
        _ => unreachable!("unsupported layouts were rejected"),
    }
}

/// Convert an `ArgValue` into a scoped Luau value.
pub(super) fn arg_value_to_scoped<'s>(
    scope: &Scope<'s>,
    value: &ArgValue,
) -> StdResult<ScopedValue<'s>, RuntimeError> {
    Ok(match value {
        ArgValue::Null => ScopedValue::Nil,
        ArgValue::Bool(value) => ScopedValue::Boolean(*value),
        // Host numbers always enter Luau as `number`: the VM's native integer
        // type does not mix with number literals in comparisons or arithmetic,
        // and scripts are written against plain numbers.
        ArgValue::Int(value) => ScopedValue::Number(*value as f64),
        ArgValue::UInt(value) => ScopedValue::Number(*value as f64),
        ArgValue::Float(value) => ScopedValue::Number(*value),
        ArgValue::String(value) => ScopedValue::String(scope.create_string(value)?),
        ArgValue::Node(id) => ScopedValue::Userdata(scope.create_userdata(*id)?),
        ArgValue::Array(values) => {
            let array = values
                .iter()
                .map(|value| arg_value_to_scoped(scope, value))
                .collect::<StdResult<Vec<_>, _>>()?;
            array.into_lua(scope)?
        }
        ArgValue::Map(values) => {
            let table = scope.create_table()?;
            for (key, value) in values {
                let value = arg_value_to_scoped(scope, value)?;
                if !matches!(value, ScopedValue::Nil) {
                    table.set(scope, key.as_str(), value)?;
                }
            }
            ScopedValue::Table(table)
        }
    })
}

/// Convert a point into its scripting record.
pub(super) fn point_to_arg(point: Point) -> ArgValue {
    ArgValue::Map(BTreeMap::from([
        ("x".to_string(), ArgValue::Int(i64::from(point.x))),
        ("y".to_string(), ArgValue::Int(i64::from(point.y))),
    ]))
}

/// Convert a size into its scripting record.
pub(super) fn size_to_arg(size: Size) -> ArgValue {
    ArgValue::Map(BTreeMap::from([
        ("w".to_string(), ArgValue::Int(i64::from(size.w))),
        ("h".to_string(), ArgValue::Int(i64::from(size.h))),
    ]))
}

/// Convert a screen rect into its scripting record.
pub(super) fn rect_to_arg(rect: RectI32) -> ArgValue {
    ArgValue::Map(BTreeMap::from([
        ("x".to_string(), ArgValue::Int(i64::from(rect.tl.x))),
        ("y".to_string(), ArgValue::Int(i64::from(rect.tl.y))),
        ("w".to_string(), ArgValue::Int(i64::from(rect.w))),
        ("h".to_string(), ArgValue::Int(i64::from(rect.h))),
    ]))
}

/// Convert a list of node ids into a scripting array.
pub(super) fn node_list_to_arg(nodes: impl IntoIterator<Item = NodeId>) -> ArgValue {
    ArgValue::Array(nodes.into_iter().map(node_id_to_arg).collect())
}

/// Convert an owned async-driver result into a command argument value.
pub(super) fn marshaled_to_arg_value(value: &ValueSnapshot) -> StdResult<ArgValue, String> {
    marshaled_to_arg_value_at(value, &ValuePath::root("result"))
}

/// Convert a marshaled value at one nested command-value path.
pub(super) fn marshaled_to_arg_value_at(
    value: &ValueSnapshot,
    path: &ValuePath,
) -> StdResult<ArgValue, String> {
    match value {
        ValueSnapshot::Nil => Ok(ArgValue::Null),
        ValueSnapshot::Boolean(value) => Ok(ArgValue::Bool(*value)),
        ValueSnapshot::Integer(value) => Ok(ArgValue::Int(*value)),
        ValueSnapshot::Number(value) => number_to_arg_value(*value, path),
        ValueSnapshot::String(bytes) => Ok(ArgValue::String(
            String::from_utf8(bytes.clone())
                .map_err(|error| path.error(format!("invalid UTF-8 string: {error}")))?,
        )),
        ValueSnapshot::Table(pairs) => marshaled_table_to_arg_value(pairs, path),
        ValueSnapshot::Vector(_) => Err(path.error("unsupported script value type: vector")),
        ValueSnapshot::LightUserdata { .. } => {
            Err(path.error("unsupported script value type: lightuserdata"))
        }
        ValueSnapshot::Buffer(_) => Err(path.error("unsupported script value type: buffer")),
        ValueSnapshot::Opaque(kind) => {
            Err(path.error(format!("unsupported script value type: {kind}")))
        }
    }
}

/// Convert an owned marshaled table into a command argument value.
fn marshaled_table_to_arg_value(
    pairs: &[MarshaledPair],
    path: &ValuePath,
) -> StdResult<ArgValue, String> {
    let layout = classify_marshaled_table(pairs);
    reject_unsupported_layout(&layout, path)?;
    match layout {
        TableLayout::Empty => Ok(ArgValue::Map(BTreeMap::new())),
        TableLayout::Sequence { len } => {
            let mut values = vec![None; len];
            for pair in pairs {
                let index = marshaled_sequence_index(&pair.key, path)?;
                values[index - 1] =
                    Some(marshaled_to_arg_value_at(&pair.value, &path.index(index))?);
            }
            Ok(ArgValue::Array(
                values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        value.ok_or_else(|| path.error(format!("missing index {}", index + 1)))
                    })
                    .collect::<StdResult<_, _>>()?,
            ))
        }
        TableLayout::StringMap { .. } => {
            let mut values = BTreeMap::new();
            for pair in pairs {
                let key = marshaled_table_key(&pair.key, path)?;
                values.insert(
                    key.clone(),
                    marshaled_to_arg_value_at(&pair.value, &path.field(&key))?,
                );
            }
            Ok(ArgValue::Map(values))
        }
        _ => unreachable!("unsupported layouts were rejected"),
    }
}

/// Read a sequence index from a classified marshaled key.
fn marshaled_sequence_index(key: &ValueSnapshot, path: &ValuePath) -> StdResult<usize, String> {
    match key {
        ValueSnapshot::Integer(index) => usize::try_from(*index)
            .map_err(|_| path.error(format!("table index is out of range: {index}"))),
        ValueSnapshot::Number(index) => Ok(*index as usize),
        other => Err(path.error(format!(
            "expected sequence index, got {}",
            other.type_name()
        ))),
    }
}

/// Read a strict UTF-8 string key from a classified marshaled key.
fn marshaled_table_key(key: &ValueSnapshot, path: &ValuePath) -> StdResult<String, String> {
    let ValueSnapshot::String(bytes) = key else {
        return Err(path.error(format!("expected string key, got {}", key.type_name())));
    };
    String::from_utf8(bytes.clone())
        .map_err(|error| path.error(format!("invalid UTF-8 key: {error}")))
}

/// Display an owned async-driver value in an error message.
pub(super) fn marshaled_value_to_display(value: &ValueSnapshot) -> String {
    match value {
        ValueSnapshot::Nil => "nil".to_string(),
        ValueSnapshot::Boolean(value) => value.to_string(),
        ValueSnapshot::Integer(value) => value.to_string(),
        ValueSnapshot::Number(value) => value.to_string(),
        ValueSnapshot::String(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        other => format!("<{}>", other.type_name()),
    }
}
