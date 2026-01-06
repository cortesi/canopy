use std::{
    any::{Any, type_name},
    collections::{BTreeMap, HashMap},
    fmt,
};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};

use crate::{
    CommandEnum, Context,
    core::{Core, NodeId, context::CoreContext},
    event::{Event, mouse::MouseEvent},
};

/// Canonical dynamic representation for command arguments and return values.
#[derive(Clone, Debug, PartialEq)]
pub enum ArgValue {
    /// Null value.
    Null,
    /// Boolean value.
    Bool(bool),
    /// Integer value.
    Int(i64),
    /// Float value.
    Float(f64),
    /// String value.
    String(String),
    /// Array value.
    Array(Vec<Self>),
    /// Map value.
    Map(BTreeMap<String, Self>),
}

/// Direction for scroll-like commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, CommandEnum)]
pub enum ScrollDirection {
    /// Upward movement.
    Up,
    /// Downward movement.
    Down,
    /// Leftward movement.
    Left,
    /// Rightward movement.
    Right,
}

/// Direction for vertical-only commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, CommandEnum)]
pub enum VerticalDirection {
    /// Upward movement.
    Up,
    /// Downward movement.
    Down,
}

/// Direction for focus movement commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, CommandEnum)]
pub enum FocusDirection {
    /// Move to the next focusable node.
    Next,
    /// Move to the previous focusable node.
    Prev,
    /// Move focus up.
    Up,
    /// Move focus down.
    Down,
    /// Move focus left.
    Left,
    /// Move focus right.
    Right,
}

/// Direction for zoom commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, CommandEnum)]
pub enum ZoomDirection {
    /// Zoom in.
    In,
    /// Zoom out.
    Out,
}

impl ArgValue {
    /// Human-readable variant name for diagnostics.
    fn kind_name(&self) -> &'static str {
        match self {
            Self::Null => "Null",
            Self::Bool(_) => "Bool",
            Self::Int(_) => "Int",
            Self::Float(_) => "Float",
            Self::String(_) => "String",
            Self::Array(_) => "Array",
            Self::Map(_) => "Map",
        }
    }
}

/// Convert a typed value into an ArgValue.
pub trait ToArgValue {
    /// Encode the value as an ArgValue.
    fn to_arg_value(self) -> ArgValue;
}

/// Convert an ArgValue into a typed value.
pub trait FromArgValue: Sized {
    /// Decode the value from an ArgValue.
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError>;
}

impl ToArgValue for ArgValue {
    fn to_arg_value(self) -> ArgValue {
        self
    }
}

impl ToArgValue for bool {
    fn to_arg_value(self) -> ArgValue {
        ArgValue::Bool(self)
    }
}

impl ToArgValue for String {
    fn to_arg_value(self) -> ArgValue {
        ArgValue::String(self)
    }
}

impl ToArgValue for &str {
    fn to_arg_value(self) -> ArgValue {
        ArgValue::String(self.to_string())
    }
}

impl ToArgValue for f32 {
    fn to_arg_value(self) -> ArgValue {
        ArgValue::Float(f64::from(self))
    }
}

impl ToArgValue for f64 {
    fn to_arg_value(self) -> ArgValue {
        ArgValue::Float(self)
    }
}

/// Implement `ToArgValue` for signed integer primitives.
macro_rules! impl_int_to_arg_value {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl ToArgValue for $ty {
                fn to_arg_value(self) -> ArgValue {
                    ArgValue::Int(i64::from(self))
                }
            }
        )+
    };
}

impl_int_to_arg_value!(i8, i16, i32, i64);

impl ToArgValue for isize {
    fn to_arg_value(self) -> ArgValue {
        ArgValue::Int(self as i64)
    }
}

/// Implement `ToArgValue` for unsigned integer primitives.
macro_rules! impl_uint_to_arg_value {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl ToArgValue for $ty {
                fn to_arg_value(self) -> ArgValue {
                    ArgValue::Int(i64::from(self))
                }
            }
        )+
    };
}

impl_uint_to_arg_value!(u8, u16, u32);

impl ToArgValue for u64 {
    fn to_arg_value(self) -> ArgValue {
        let value = i64::try_from(self).expect("u64 command argument does not fit in i64");
        ArgValue::Int(value)
    }
}

impl ToArgValue for usize {
    fn to_arg_value(self) -> ArgValue {
        let value = i64::try_from(self).expect("usize command argument does not fit in i64");
        ArgValue::Int(value)
    }
}

impl<T> ToArgValue for Option<T>
where
    T: ToArgValue,
{
    fn to_arg_value(self) -> ArgValue {
        match self {
            Some(value) => value.to_arg_value(),
            None => ArgValue::Null,
        }
    }
}

impl<T> ToArgValue for Vec<T>
where
    T: ToArgValue,
{
    fn to_arg_value(self) -> ArgValue {
        ArgValue::Array(self.into_iter().map(ToArgValue::to_arg_value).collect())
    }
}

impl<T> ToArgValue for BTreeMap<String, T>
where
    T: ToArgValue,
{
    fn to_arg_value(self) -> ArgValue {
        ArgValue::Map(
            self.into_iter()
                .map(|(k, v)| (k, v.to_arg_value()))
                .collect(),
        )
    }
}

impl<T> ToArgValue for HashMap<String, T>
where
    T: ToArgValue,
{
    fn to_arg_value(self) -> ArgValue {
        let mut out = BTreeMap::new();
        for (key, value) in self {
            out.insert(key, value.to_arg_value());
        }
        ArgValue::Map(out)
    }
}

impl<A> ToArgValue for (A,)
where
    A: ToArgValue,
{
    fn to_arg_value(self) -> ArgValue {
        ArgValue::Array(vec![self.0.to_arg_value()])
    }
}

/// Implement `ToArgValue` for tuple arities.
macro_rules! impl_tuple_to_arg_value {
    ($($idx:tt : $name:ident),+ $(,)?) => {
        impl<$($name),+> ToArgValue for ($($name,)+)
        where
            $($name: ToArgValue,)+
        {
            fn to_arg_value(self) -> ArgValue {
                ArgValue::Array(vec![$(self.$idx.to_arg_value(),)+])
            }
        }
    };
}

impl_tuple_to_arg_value!(0: T1, 1: T2);
impl_tuple_to_arg_value!(0: T1, 1: T2, 2: T3);
impl_tuple_to_arg_value!(0: T1, 1: T2, 2: T3, 3: T4);

impl ToArgValue for () {
    fn to_arg_value(self) -> ArgValue {
        ArgValue::Null
    }
}

impl FromArgValue for bool {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::Bool(value) => Ok(*value),
            other => Err(CommandError::type_mismatch("bool", other)),
        }
    }
}

impl FromArgValue for String {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::String(value) => Ok(value.clone()),
            other => Err(CommandError::type_mismatch("String", other)),
        }
    }
}

/// Build a conversion error for out-of-range integers.
fn int_out_of_range<T>(value: i64) -> CommandError {
    CommandError::conversion(format!(
        "value {value} out of range for {}",
        type_name::<T>()
    ))
}

/// Build a conversion error for out-of-range floats.
fn float_out_of_range<T>(value: f64) -> CommandError {
    CommandError::conversion(format!(
        "value {value} out of range for {}",
        type_name::<T>()
    ))
}

/// Implement `FromArgValue` for signed integer primitives.
macro_rules! impl_int_from_arg_value {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl FromArgValue for $ty {
                fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
                    match v {
                        ArgValue::Int(value) => <$ty>::try_from(*value)
                            .map_err(|_| int_out_of_range::<$ty>(*value)),
                        other => Err(CommandError::type_mismatch(stringify!($ty), other)),
                    }
                }
            }
        )+
    };
}

impl_int_from_arg_value!(i8, i16, i32, i64);

impl FromArgValue for isize {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::Int(value) => {
                Self::try_from(*value).map_err(|_| int_out_of_range::<Self>(*value))
            }
            other => Err(CommandError::type_mismatch("isize", other)),
        }
    }
}

/// Implement `FromArgValue` for unsigned integer primitives.
macro_rules! impl_uint_from_arg_value {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl FromArgValue for $ty {
                fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
                    match v {
                        ArgValue::Int(value) => <$ty>::try_from(*value)
                            .map_err(|_| int_out_of_range::<$ty>(*value)),
                        other => Err(CommandError::type_mismatch(stringify!($ty), other)),
                    }
                }
            }
        )+
    };
}

impl_uint_from_arg_value!(u8, u16, u32, u64);

impl FromArgValue for usize {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::Int(value) => {
                Self::try_from(*value).map_err(|_| int_out_of_range::<Self>(*value))
            }
            other => Err(CommandError::type_mismatch("usize", other)),
        }
    }
}

impl FromArgValue for f32 {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        let value = match v {
            ArgValue::Float(value) => *value,
            ArgValue::Int(value) => *value as f64,
            other => return Err(CommandError::type_mismatch("f32", other)),
        };
        if value.is_finite() && value >= f64::from(Self::MIN) && value <= f64::from(Self::MAX) {
            Ok(value as Self)
        } else {
            Err(float_out_of_range::<Self>(value))
        }
    }
}

impl FromArgValue for f64 {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::Float(value) => Ok(*value),
            ArgValue::Int(value) => Ok(*value as Self),
            other => Err(CommandError::type_mismatch("f64", other)),
        }
    }
}

impl<T> FromArgValue for Option<T>
where
    T: FromArgValue,
{
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::Null => Ok(None),
            _ => T::from_arg_value(v).map(Some),
        }
    }
}

impl<T> FromArgValue for Vec<T>
where
    T: FromArgValue,
{
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::Array(values) => values
                .iter()
                .map(T::from_arg_value)
                .collect::<Result<Self, _>>(),
            other => Err(CommandError::type_mismatch("Vec", other)),
        }
    }
}

impl<T> FromArgValue for BTreeMap<String, T>
where
    T: FromArgValue,
{
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::Map(values) => values
                .iter()
                .map(|(k, v)| Ok((k.clone(), T::from_arg_value(v)?)))
                .collect::<Result<Self, _>>(),
            other => Err(CommandError::type_mismatch("BTreeMap", other)),
        }
    }
}

impl<T> FromArgValue for HashMap<String, T>
where
    T: FromArgValue,
{
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::Map(values) => values
                .iter()
                .map(|(k, v)| Ok((k.clone(), T::from_arg_value(v)?)))
                .collect::<Result<Self, _>>(),
            other => Err(CommandError::type_mismatch("HashMap", other)),
        }
    }
}

impl<A> FromArgValue for (A,)
where
    A: FromArgValue,
{
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::Array(values) if values.len() == 1 => Ok((A::from_arg_value(&values[0])?,)),
            ArgValue::Array(values) => Err(CommandError::conversion(format!(
                "expected 1 element, got {}",
                values.len()
            ))),
            other => Err(CommandError::type_mismatch("tuple", other)),
        }
    }
}

/// Implement `FromArgValue` for tuple arities.
macro_rules! impl_tuple_from_arg_value {
    ($len:literal, $($name:ident),+ $(,)?) => {
        impl<$($name),+> FromArgValue for ($($name,)+)
        where
            $($name: FromArgValue,)+
        {
            fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
                match v {
                    ArgValue::Array(values) if values.len() == $len => {
                        let mut iter = values.iter();
                        Ok((
                            $($name::from_arg_value(iter.next().unwrap())?,)+
                        ))
                    }
                    ArgValue::Array(values) => Err(CommandError::conversion(format!(
                        "expected {} elements, got {}",
                        $len,
                        values.len()
                    ))),
                    other => Err(CommandError::type_mismatch("tuple", other)),
                }
            }
        }
    };
}

impl_tuple_from_arg_value!(2, T1, T2);
impl_tuple_from_arg_value!(3, T1, T2, T3);
impl_tuple_from_arg_value!(4, T1, T2, T3, T4);

impl FromArgValue for ArgValue {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        Ok(v.clone())
    }
}

impl FromArgValue for () {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::Null => Ok(()),
            other => Err(CommandError::type_mismatch("()", other)),
        }
    }
}

/// Marker trait for serde-backed command arguments.
pub trait CommandArg: Serialize + DeserializeOwned + 'static {}

/// Convert ArgValue into a JSON value for serde interop.
fn arg_value_to_json(value: ArgValue) -> Result<JsonValue, CommandError> {
    Ok(match value {
        ArgValue::Null => JsonValue::Null,
        ArgValue::Bool(value) => JsonValue::Bool(value),
        ArgValue::Int(value) => JsonValue::Number(JsonNumber::from(value)),
        ArgValue::Float(value) => {
            let Some(num) = serde_json::Number::from_f64(value) else {
                return Err(CommandError::conversion("float value is not finite"));
            };
            JsonValue::Number(num)
        }
        ArgValue::String(value) => JsonValue::String(value),
        ArgValue::Array(values) => JsonValue::Array(
            values
                .into_iter()
                .map(arg_value_to_json)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        ArgValue::Map(values) => {
            let mut map = JsonMap::new();
            for (key, value) in values {
                map.insert(key, arg_value_to_json(value)?);
            }
            JsonValue::Object(map)
        }
    })
}

/// Convert a JSON value into ArgValue for serde interop.
fn json_to_arg_value(value: JsonValue) -> Result<ArgValue, CommandError> {
    match value {
        JsonValue::Null => Ok(ArgValue::Null),
        JsonValue::Bool(value) => Ok(ArgValue::Bool(value)),
        JsonValue::Number(value) => {
            if let Some(int_value) = value.as_i64() {
                Ok(ArgValue::Int(int_value))
            } else if let Some(float_value) = value.as_f64() {
                Ok(ArgValue::Float(float_value))
            } else {
                Err(CommandError::conversion("json number out of range"))
            }
        }
        JsonValue::String(value) => Ok(ArgValue::String(value)),
        JsonValue::Array(values) => Ok(ArgValue::Array(
            values
                .into_iter()
                .map(json_to_arg_value)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        JsonValue::Object(values) => {
            let mut map = BTreeMap::new();
            for (key, value) in values {
                map.insert(key, json_to_arg_value(value)?);
            }
            Ok(ArgValue::Map(map))
        }
    }
}

impl<T> ToArgValue for T
where
    T: CommandArg,
{
    fn to_arg_value(self) -> ArgValue {
        let value = serde_json::to_value(self).expect("CommandArg serialization failed");
        json_to_arg_value(value).expect("CommandArg conversion failed")
    }
}

impl<T> FromArgValue for T
where
    T: CommandArg,
{
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        let json = arg_value_to_json(v.clone())?;
        serde_json::from_value(json).map_err(|err| CommandError::conversion(err.to_string()))
    }
}

/// Identifier for a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CommandId(pub &'static str);

impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Canonical argument container for command invocation.
#[derive(Clone, Debug, PartialEq)]
pub enum CommandArgs {
    /// Positional arguments.
    Positional(Vec<ArgValue>),
    /// Named arguments.
    Named(BTreeMap<String, ArgValue>),
}

impl Default for CommandArgs {
    fn default() -> Self {
        Self::Positional(Vec::new())
    }
}

impl From<()> for CommandArgs {
    fn from(_: ()) -> Self {
        Self::Positional(Vec::new())
    }
}

impl<T, const N: usize> From<[T; N]> for CommandArgs
where
    T: ToArgValue,
{
    fn from(values: [T; N]) -> Self {
        Self::Positional(values.into_iter().map(ToArgValue::to_arg_value).collect())
    }
}

impl<T> From<Vec<T>> for CommandArgs
where
    T: ToArgValue,
{
    fn from(values: Vec<T>) -> Self {
        Self::Positional(values.into_iter().map(ToArgValue::to_arg_value).collect())
    }
}

impl<T> From<BTreeMap<String, T>> for CommandArgs
where
    T: ToArgValue,
{
    fn from(values: BTreeMap<String, T>) -> Self {
        Self::Named(
            values
                .into_iter()
                .map(|(k, v)| (k, v.to_arg_value()))
                .collect(),
        )
    }
}

/// A command invocation with encoded arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandInvocation {
    /// Command identifier.
    pub id: CommandId,
    /// Invocation arguments.
    pub args: CommandArgs,
}

/// Identifies how a command parameter is provided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandParamKind {
    /// Provided by injection.
    Injected,
    /// Provided by user arguments.
    User,
}

/// Static metadata for a type in command signatures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandTypeSpec {
    /// Rust type name for introspection.
    pub rust: &'static str,
    /// Optional documentation string.
    pub doc: Option<&'static str>,
}

/// Static metadata for a command parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandParamSpec {
    /// Parameter name for named argument binding.
    pub name: &'static str,
    /// Parameter kind.
    pub kind: CommandParamKind,
    /// Type metadata.
    pub ty: CommandTypeSpec,
    /// Whether the parameter is optional.
    pub optional: bool,
    /// Optional default expression string.
    pub default: Option<&'static str>,
}

/// Static metadata for a command return type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandReturnSpec {
    /// Unit return.
    Unit,
    /// Non-unit return.
    Value(CommandTypeSpec),
}

/// Erased invoke function signature.
pub type InvokeFn = fn(
    target: Option<&mut dyn Any>,
    ctx: &mut dyn Context,
    inv: &CommandInvocation,
) -> Result<ArgValue, CommandError>;

/// Command dispatch routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandDispatchKind {
    /// Invoke with `target = None`.
    Free,
    /// Route to a node by owner name.
    Node {
        /// Owner node name.
        owner: &'static str,
    },
}

/// Static metadata for a command.
#[derive(Clone, Copy, Debug)]
pub struct CommandSpec {
    /// Command identifier.
    pub id: CommandId,
    /// Command name.
    pub name: &'static str,
    /// Dispatch routing.
    pub dispatch: CommandDispatchKind,
    /// Parameter specs.
    pub params: &'static [CommandParamSpec],
    /// Return spec.
    pub ret: CommandReturnSpec,
    /// Erased invoke entrypoint.
    pub invoke: InvokeFn,
}

/// The CommandNode trait is implemented by widgets to expose commands.
pub trait CommandNode {
    /// Return a list of commands for this node.
    fn commands() -> &'static [&'static CommandSpec]
    where
        Self: Sized;
}

impl CommandSpec {
    /// Build a call to this command with no arguments.
    pub fn call(&'static self) -> CommandCall {
        self.call_with(())
    }

    /// Build a call to this command.
    pub fn call_with(&'static self, args: impl Into<CommandArgs>) -> CommandCall {
        CommandCall {
            spec: self,
            args: args.into(),
        }
    }

    /// Render a signature string for this command.
    pub fn signature(&self) -> String {
        let mut out = String::new();
        out.push_str(self.id.0);
        out.push('(');
        for (idx, param) in self.params.iter().enumerate() {
            if idx > 0 {
                out.push_str(", ");
            }
            if param.kind == CommandParamKind::Injected {
                out.push('@');
            }
            out.push_str(param.name);
            if param.optional && param.default.is_none() {
                out.push('?');
            }
            out.push_str(": ");
            out.push_str(param.ty.rust);
            if let Some(default) = param.default {
                out.push_str(" = ");
                out.push_str(default);
            }
        }
        out.push(')');
        out.push_str(" -> ");
        match self.ret {
            CommandReturnSpec::Unit => out.push_str("()"),
            CommandReturnSpec::Value(ty) => out.push_str(ty.rust),
        }
        out
    }
}

/// Builder for a command invocation.
#[derive(Clone, Debug)]
pub struct CommandCall {
    /// Command spec for invocation.
    spec: &'static CommandSpec,
    /// Argument payload for invocation.
    args: CommandArgs,
}

impl CommandCall {
    /// Convert into an invocation.
    pub fn invocation(self) -> CommandInvocation {
        CommandInvocation {
            id: self.spec.id,
            args: self.args,
        }
    }
}

impl From<CommandCall> for CommandInvocation {
    fn from(call: CommandCall) -> Self {
        call.invocation()
    }
}

impl From<&'static CommandSpec> for CommandInvocation {
    fn from(spec: &'static CommandSpec) -> Self {
        spec.call().invocation()
    }
}

/// Collection of available commands keyed by id.
#[derive(Debug, Default)]
pub struct CommandSet {
    /// Registry of command specs by id.
    commands: HashMap<&'static str, &'static CommandSpec>,
}

impl CommandSet {
    /// Construct an empty command set.
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    /// Add command specs to the set.
    /// Returns an error if any command id is already registered.
    pub fn add(&mut self, specs: &'static [&'static CommandSpec]) -> Result<(), CommandError> {
        for spec in specs {
            if self.commands.contains_key(spec.id.0) {
                return Err(CommandError::DuplicateCommand {
                    id: spec.id.0.to_string(),
                });
            }
            self.commands.insert(spec.id.0, spec);
        }
        Ok(())
    }

    /// Get a command by id.
    pub fn get(&self, id: &str) -> Option<&'static CommandSpec> {
        self.commands.get(id).copied()
    }

    /// Iterate over all command specs.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &'static CommandSpec)> + '_ {
        self.commands.iter().map(|(k, v)| (*k, *v))
    }
}

/// Error type for command dispatch and conversion.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// Unknown command identifier.
    #[error("unknown command: {id}")]
    UnknownCommand {
        /// Requested command id.
        id: String,
    },

    /// Duplicate command identifier.
    #[error("duplicate command id: {id}")]
    DuplicateCommand {
        /// Duplicate command id.
        id: String,
    },

    /// No matching target found for a node-routed command.
    #[error("no target node found for command {id} (owner {owner})")]
    NoTarget {
        /// Requested command id.
        id: String,
        /// Expected owner node name.
        owner: String,
    },

    /// Incorrect number of arguments.
    #[error("arity mismatch: expected {expected}, got {got}")]
    ArityMismatch {
        /// Expected positional argument count.
        expected: usize,
        /// Actual positional argument count.
        got: usize,
    },

    /// Missing named argument.
    #[error("missing named argument: {name}")]
    MissingNamedArg {
        /// Parameter name.
        name: String,
    },

    /// Unknown named argument.
    #[error("unknown named argument: {name}; allowed: {allowed:?}")]
    UnknownNamedArg {
        /// Provided name.
        name: String,
        /// Allowed names.
        allowed: Vec<&'static str>,
    },

    /// Type mismatch error.
    #[error("type mismatch for parameter `{param}`: expected {expected}, got {got}")]
    TypeMismatch {
        /// Parameter name.
        param: String,
        /// Expected type.
        expected: &'static str,
        /// Provided type.
        got: String,
    },

    /// Missing injected value.
    #[error("missing injected value for parameter `{param}`: expected {expected}")]
    MissingInjected {
        /// Parameter name.
        param: String,
        /// Expected injected type.
        expected: &'static str,
    },

    /// Conversion error.
    #[error("conversion error for parameter `{param}`: {message}")]
    Conversion {
        /// Parameter name.
        param: String,
        /// Error message.
        message: String,
    },

    /// Command execution failure.
    #[error("command execution failed: {0}")]
    Exec(#[from] anyhow::Error),
}

impl CommandError {
    #[doc(hidden)]
    pub fn with_param(self, param: &str) -> Self {
        match self {
            Self::TypeMismatch { expected, got, .. } => Self::TypeMismatch {
                param: param.to_string(),
                expected,
                got,
            },
            Self::MissingInjected { expected, .. } => Self::MissingInjected {
                param: param.to_string(),
                expected,
            },
            Self::Conversion { message, .. } => Self::Conversion {
                param: param.to_string(),
                message,
            },
            other => other,
        }
    }

    #[doc(hidden)]
    pub fn conversion(message: impl Into<String>) -> Self {
        Self::Conversion {
            param: String::new(),
            message: message.into(),
        }
    }

    #[doc(hidden)]
    pub fn type_mismatch(expected: &'static str, got: &ArgValue) -> Self {
        Self::TypeMismatch {
            param: String::new(),
            expected,
            got: got.kind_name().to_string(),
        }
    }
}

/// Errors raised during injection.
#[derive(Debug)]
pub enum InjectError {
    /// Required injected value missing.
    Missing {
        /// Expected type.
        expected: &'static str,
    },
    /// Injected value failed.
    Failed {
        /// Expected type.
        expected: &'static str,
        /// Error message.
        message: String,
    },
}

/// Trait for injectable parameters.
pub trait Inject: Sized {
    /// Inject a value from the context.
    fn inject(ctx: &dyn Context) -> Result<Self, InjectError>;
}

impl<T> Inject for Option<T>
where
    T: Inject,
{
    fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {
        match T::inject(ctx) {
            Ok(value) => Ok(Some(value)),
            Err(InjectError::Missing { .. }) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

/// Explicit injection wrapper.
#[derive(Debug, Clone, Copy)]
pub struct Injected<T>(pub T);

impl<T> Inject for Injected<T>
where
    T: Inject,
{
    fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {
        T::inject(ctx).map(Injected)
    }
}

/// Explicit user argument wrapper.
#[derive(Debug)]
pub struct Arg<T>(pub T);

impl<T> FromArgValue for Arg<T>
where
    T: FromArgValue,
{
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        T::from_arg_value(v).map(Arg)
    }
}

/// Context passed to list row injections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListRowContext {
    /// Owning list node id.
    pub list: NodeId,
    /// Row index.
    pub index: usize,
}

impl Inject for MouseEvent {
    fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {
        ctx.current_mouse_event().ok_or(InjectError::Missing {
            expected: "MouseEvent",
        })
    }
}

impl Inject for ListRowContext {
    fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {
        ctx.current_list_row().ok_or(InjectError::Missing {
            expected: "ListRowContext",
        })
    }
}

impl Inject for Event {
    fn inject(ctx: &dyn Context) -> Result<Self, InjectError> {
        ctx.current_event()
            .cloned()
            .ok_or(InjectError::Missing { expected: "Event" })
    }
}

/// Command scope frame for injection.
#[derive(Debug, Clone, Default)]
pub struct CommandScopeFrame {
    /// Event snapshot.
    pub event: Option<Event>,
    /// Mouse event snapshot.
    pub mouse: Option<MouseEvent>,
    /// List row context.
    pub list_row: Option<ListRowContext>,
}

/// Normalize named argument keys for lookup.
#[doc(hidden)]
pub fn normalize_key(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c == '-' {
                '_'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

#[doc(hidden)]
pub fn normalize_named_args<'a>(
    args: &'a BTreeMap<String, ArgValue>,
    allowed: &'static [&'static str],
) -> Result<HashMap<String, &'a ArgValue>, CommandError> {
    let mut normalized = HashMap::new();
    for (key, value) in args {
        let normalized_key = normalize_key(key);
        if !allowed
            .iter()
            .any(|allowed_key| normalize_key(allowed_key) == normalized_key)
        {
            return Err(CommandError::UnknownNamedArg {
                name: key.clone(),
                allowed: allowed.to_vec(),
            });
        }
        if normalized.contains_key(&normalized_key) {
            return Err(CommandError::conversion(format!(
                "duplicate named argument after normalization: {key}"
            )));
        }
        normalized.insert(normalized_key, value);
    }
    Ok(normalized)
}

/// Dispatch a command relative to a node.
pub fn dispatch(
    core: &mut Core,
    current_id: NodeId,
    inv: &CommandInvocation,
) -> Result<ArgValue, CommandError> {
    let spec = core
        .commands
        .get(inv.id.0)
        .ok_or_else(|| CommandError::UnknownCommand {
            id: inv.id.0.to_string(),
        })?;

    match spec.dispatch {
        CommandDispatchKind::Free => {
            let mut ctx = CoreContext::new(core, current_id);
            (spec.invoke)(None, &mut ctx, inv)
        }
        CommandDispatchKind::Node { owner } => {
            if let Some(result) = dispatch_subtree(core, current_id, owner, spec, inv)? {
                return Ok(result);
            }

            let mut current = core.nodes[current_id].parent;
            while let Some(node_id) = current {
                if let Some(result) = dispatch_on_node(core, node_id, owner, spec, inv)? {
                    return Ok(result);
                }
                current = core.nodes[node_id].parent;
            }

            Err(CommandError::NoTarget {
                id: inv.id.0.to_string(),
                owner: owner.to_string(),
            })
        }
    }
}

/// Dispatch a node-routed command over a subtree in pre-order.
fn dispatch_subtree(
    core: &mut Core,
    root: NodeId,
    owner: &'static str,
    spec: &CommandSpec,
    inv: &CommandInvocation,
) -> Result<Option<ArgValue>, CommandError> {
    let mut stack = vec![root];
    while let Some(node_id) = stack.pop() {
        if let Some(result) = dispatch_on_node(core, node_id, owner, spec, inv)? {
            return Ok(Some(result));
        }
        let children = core.nodes[node_id].children.clone();
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    Ok(None)
}

/// Dispatch a node-routed command to a specific node if it matches.
fn dispatch_on_node(
    core: &mut Core,
    node_id: NodeId,
    owner: &'static str,
    spec: &CommandSpec,
    inv: &CommandInvocation,
) -> Result<Option<ArgValue>, CommandError> {
    if core.nodes[node_id].name != owner {
        return Ok(None);
    }

    let result = core.with_widget_mut(node_id, |widget, core| {
        let mut ctx = CoreContext::new(core, node_id);
        (spec.invoke)(Some(widget as &mut dyn Any), &mut ctx, inv)
    });

    result.map(Some)
}

/// Convenience macro for building named arguments.
#[macro_export]
macro_rules! named_args {
    ($($key:ident : $value:expr),* $(,)?) => {{
        let mut map = ::std::collections::BTreeMap::new();
        $(
            map.insert(
                ::std::string::ToString::to_string(stringify!($key)),
                $crate::commands::ToArgValue::to_arg_value($value),
            );
        )*
        $crate::commands::CommandArgs::Named(map)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_range_checks() {
        let value = ArgValue::Int(i64::from(i32::MAX));
        assert_eq!(i32::from_arg_value(&value).unwrap(), i32::MAX);
        let overflow = ArgValue::Int(i64::from(i32::MAX) + 1);
        let err = i32::from_arg_value(&overflow).unwrap_err();
        assert!(matches!(err, CommandError::Conversion { .. }));
    }

    #[test]
    fn float_range_checks() {
        let value = ArgValue::Float(f64::from(f32::MAX));
        assert!(f32::from_arg_value(&value).is_ok());
        let overflow = ArgValue::Float(f64::from(f32::MAX) * 2.0);
        let err = f32::from_arg_value(&overflow).unwrap_err();
        assert!(matches!(err, CommandError::Conversion { .. }));
    }

    #[test]
    fn option_null_maps_to_none() {
        let value = ArgValue::Null;
        let out: Option<i32> = Option::from_arg_value(&value).unwrap();
        assert!(out.is_none());
    }

    #[test]
    fn tuple_lengths_validate() {
        let value = ArgValue::Array(vec![ArgValue::Int(1), ArgValue::Int(2)]);
        let out: (i32, i32) = FromArgValue::from_arg_value(&value).unwrap();
        assert_eq!(out, (1, 2));
        let err: Result<(i32, i32), _> = FromArgValue::from_arg_value(&ArgValue::Array(vec![]));
        assert!(err.is_err());
    }
}
