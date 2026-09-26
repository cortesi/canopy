use std::{
    any::{Any, type_name},
    collections::{BTreeMap, HashMap, HashSet},
    error::Error as StdError,
    fmt, ptr,
};

pub use ruau::declaration;
use ruau::module;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};

use crate::{
    Context, NodeId, ViewContext,
    core::{Core, context::CoreViewContext, world::WidgetOperation},
    error::{Result as CoreResult, ScriptErrorKind},
    input::{Event, mouse::MouseEvent},
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
    /// Unsigned integer value.
    UInt(u64),
    /// Float value.
    Float(f64),
    /// String value.
    String(String),
    /// Opaque node handle.
    Node(NodeId),
    /// Array value.
    Array(Vec<Self>),
    /// Map value.
    Map(BTreeMap<String, Self>),
}

impl ArgValue {
    /// Human-readable variant name for diagnostics.
    fn kind_name(&self) -> &'static str {
        match self {
            Self::Null => "Null",
            Self::Bool(_) => "Bool",
            Self::Int(_) => "Int",
            Self::UInt(_) => "UInt",
            Self::Float(_) => "Float",
            Self::String(_) => "String",
            Self::Node(_) => "NodeId",
            Self::Array(_) => "Array",
            Self::Map(_) => "Map",
        }
    }

    /// Convert this dynamic value into external automation JSON.
    ///
    /// Opaque `NodeId` values become descriptive tokens for reporting.
    pub fn to_external_json_value(&self) -> Result<JsonValue, CommandError> {
        arg_value_to_json(self, NodeJson::Token)
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
                    ArgValue::UInt(u64::from(self))
                }
            }
        )+
    };
}

impl_uint_to_arg_value!(u8, u16, u32);

impl ToArgValue for u64 {
    fn to_arg_value(self) -> ArgValue {
        ArgValue::UInt(self)
    }
}

impl ToArgValue for usize {
    fn to_arg_value(self) -> ArgValue {
        ArgValue::UInt(self as u64)
    }
}

impl ToArgValue for NodeId {
    fn to_arg_value(self) -> ArgValue {
        ArgValue::Node(self)
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
        ArgValue::Map(
            self.into_iter()
                .map(|(k, v)| (k, v.to_arg_value()))
                .collect(),
        )
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

impl FromArgValue for NodeId {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::Node(value) => Ok(*value),
            other => Err(CommandError::type_mismatch("NodeId", other)),
        }
    }
}

/// Build a conversion error for a numeric value outside the target type's
/// range.
fn out_of_range<T>(value: impl fmt::Display) -> CommandError {
    CommandError::conversion(format!(
        "value {value} out of range for {}",
        type_name::<T>()
    ))
}

/// Implement `FromArgValue` for the integer primitives.
macro_rules! impl_int_from_arg_value {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl FromArgValue for $ty {
                fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
                    match v {
                        ArgValue::Int(value) => <$ty>::try_from(*value)
                            .map_err(|_| out_of_range::<$ty>(*value)),
                        ArgValue::UInt(value) => <$ty>::try_from(*value)
                            .map_err(|_| out_of_range::<$ty>(*value)),
                        other => Err(CommandError::type_mismatch(stringify!($ty), other)),
                    }
                }
            }
        )+
    };
}

impl_int_from_arg_value!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

impl FromArgValue for f32 {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        let value = match v {
            ArgValue::Float(value) => *value,
            ArgValue::Int(value) => *value as f64,
            ArgValue::UInt(value) => *value as f64,
            other => return Err(CommandError::type_mismatch("f32", other)),
        };
        if value.is_finite() && value >= f64::from(Self::MIN) && value <= f64::from(Self::MAX) {
            Ok(value as Self)
        } else {
            Err(out_of_range::<Self>(value))
        }
    }
}

impl FromArgValue for f64 {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        match v {
            ArgValue::Float(value) => Ok(*value),
            ArgValue::Int(value) => Ok(*value as Self),
            ArgValue::UInt(value) => Ok(*value as Self),
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

impl FromArgValue for ArgValue {
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        Ok(v.clone())
    }
}

/// Static Luau type metadata for values in command signatures.
pub trait CommandType {
    /// Luau type expression for this Rust value.
    fn luau_ty() -> declaration::Type;

    /// Registers declaration items needed by this type.
    fn luau_decls(_registry: &mut DeclRegistry<'_>) {}
}

/// Marker trait for serde-backed command arguments.
pub trait CommandArg: Serialize + DeserializeOwned + 'static {}

impl CommandType for ArgValue {
    fn luau_ty() -> declaration::Type {
        declaration::Type::Any
    }
}

impl CommandType for bool {
    fn luau_ty() -> declaration::Type {
        declaration::Type::Boolean
    }
}

impl CommandType for String {
    fn luau_ty() -> declaration::Type {
        declaration::Type::String
    }
}

impl CommandType for &str {
    fn luau_ty() -> declaration::Type {
        declaration::Type::String
    }
}

impl CommandType for NodeId {
    fn luau_ty() -> declaration::Type {
        declaration::Type::named("NodeId")
    }

    fn luau_decls(registry: &mut DeclRegistry<'_>) {
        registry.extern_ty("NodeId");
    }
}

/// Implement numeric command type metadata for primitive numbers.
macro_rules! impl_number_command_type {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl CommandType for $ty {
                fn luau_ty() -> declaration::Type {
                    declaration::Type::Number
                }
            }
        )+
    };
}

impl_number_command_type!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64);

impl<T: CommandType> CommandType for Option<T> {
    fn luau_ty() -> declaration::Type {
        T::luau_ty().optional()
    }

    fn luau_decls(registry: &mut DeclRegistry<'_>) {
        T::luau_decls(registry);
    }
}

impl<T: CommandType> CommandType for Vec<T> {
    fn luau_ty() -> declaration::Type {
        T::luau_ty().array()
    }

    fn luau_decls(registry: &mut DeclRegistry<'_>) {
        T::luau_decls(registry);
    }
}

impl<T: CommandType> CommandType for BTreeMap<String, T> {
    fn luau_ty() -> declaration::Type {
        declaration::Type::map(declaration::Type::String, T::luau_ty())
    }

    fn luau_decls(registry: &mut DeclRegistry<'_>) {
        T::luau_decls(registry);
    }
}

impl<T: CommandType> CommandType for HashMap<String, T> {
    fn luau_ty() -> declaration::Type {
        declaration::Type::map(declaration::Type::String, T::luau_ty())
    }

    fn luau_decls(registry: &mut DeclRegistry<'_>) {
        T::luau_decls(registry);
    }
}

/// Registry for declaration items required by command argument and return
/// types.
///
/// Tracks in-flight named registrations so recursive and shared types
/// terminate: a type's `luau_decls` claims its name with
/// [`DeclRegistry::begin`] before recursing into field types.
pub struct DeclRegistry<'a> {
    /// Declaration-coupled native-module builder.
    builder: &'a mut module::Builder,
    /// Names currently being declared during this registration pass.
    seen: HashSet<declaration::Text>,
    /// Register alias names as external types that another module declares.
    extern_only: bool,
}

impl<'a> DeclRegistry<'a> {
    /// Wrap a declaration-coupled native-module builder.
    pub(crate) fn native_module(builder: &'a mut module::Builder) -> Self {
        Self {
            builder,
            seen: HashSet::new(),
            extern_only: false,
        }
    }

    /// Wrap a builder whose dependent types another module declares.
    ///
    /// Every alias registers as an external type name, so the surface has one
    /// definition of each type.
    pub(crate) fn extern_module(builder: &'a mut module::Builder) -> Self {
        Self {
            builder,
            seen: HashSet::new(),
            extern_only: true,
        }
    }

    /// Claim a type name for registration.
    ///
    /// Returns false when the name is already in progress, in which case the
    /// caller must skip both recursion and registration.
    pub fn begin(&mut self, name: &str) -> bool {
        self.seen.insert(name.to_string().into())
    }

    /// Registers an alias declaration, or its name as an external type when
    /// another module declares it.
    pub fn alias(&mut self, alias: declaration::Alias) {
        if self.extern_only {
            self.builder.extern_ty(alias.name.into_owned());
        } else {
            self.builder.alias(alias);
        }
    }

    /// Registers an external type name.
    pub fn extern_ty(&mut self, name: impl Into<declaration::Text>) {
        self.builder.extern_ty(name.into().into_owned());
    }
}

/// Wrapper for fallible serde argument conversion.
pub struct SerdeArg<T>(pub T);

/// Convert ArgValue into a JSON value for serde interop.
fn arg_value_to_json(value: &ArgValue, node_json: NodeJson) -> Result<JsonValue, CommandError> {
    Ok(match value {
        ArgValue::Null => JsonValue::Null,
        ArgValue::Bool(value) => JsonValue::Bool(*value),
        ArgValue::Int(value) => JsonValue::Number(JsonNumber::from(*value)),
        ArgValue::UInt(value) => JsonValue::Number(JsonNumber::from(*value)),
        ArgValue::Float(value) => {
            let Some(num) = serde_json::Number::from_f64(*value) else {
                return Err(CommandError::conversion("float value is not finite"));
            };
            JsonValue::Number(num)
        }
        ArgValue::String(value) => JsonValue::String(value.clone()),
        ArgValue::Node(id) => match node_json {
            NodeJson::Reject => {
                return Err(CommandError::conversion(
                    "NodeId is not representable as JSON",
                ));
            }
            NodeJson::Token => JsonValue::Object(
                node_token_fields(*id)
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), JsonValue::String(value)))
                    .collect(),
            ),
        },
        ArgValue::Array(values) => JsonValue::Array(
            values
                .iter()
                .map(|value| arg_value_to_json(value, node_json))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        ArgValue::Map(values) => {
            let mut map = JsonMap::new();
            for (key, value) in values {
                map.insert(key.clone(), arg_value_to_json(value, node_json)?);
            }
            JsonValue::Object(map)
        }
    })
}

/// Node-handle rendering mode for JSON conversion.
#[derive(Clone, Copy)]
enum NodeJson {
    /// Reject node handles for serde-compatible conversion.
    Reject,
    /// Render node handles as opaque reporting tokens.
    Token,
}

/// Render the external automation token for a node handle.
pub(crate) fn node_token(node_id: NodeId) -> String {
    format!("{node_id:?}")
}

/// Build the external automation token record for a node handle.
pub(crate) fn node_token_fields(node_id: NodeId) -> [(&'static str, String); 2] {
    [
        ("type", "NodeId".to_string()),
        ("token", node_token(node_id)),
    ]
}

/// Convert a JSON value into ArgValue for serde interop.
fn json_to_arg_value(value: JsonValue) -> Result<ArgValue, CommandError> {
    match value {
        JsonValue::Null => Ok(ArgValue::Null),
        JsonValue::Bool(value) => Ok(ArgValue::Bool(value)),
        JsonValue::Number(value) => {
            if let Some(int_value) = value.as_i64() {
                Ok(ArgValue::Int(int_value))
            } else if let Some(uint_value) = value.as_u64() {
                Ok(ArgValue::UInt(uint_value))
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

impl<T> SerdeArg<T>
where
    T: Serialize,
{
    /// Encode a serde argument into ArgValue, returning conversion errors.
    pub fn try_to_arg_value(self) -> Result<ArgValue, CommandError> {
        let value = serde_json::to_value(self.0)
            .map_err(|err| CommandError::conversion(err.to_string()))?;
        json_to_arg_value(value)
    }
}

impl<T> FromArgValue for T
where
    T: CommandArg,
{
    fn from_arg_value(v: &ArgValue) -> Result<Self, CommandError> {
        let json = arg_value_to_json(v, NodeJson::Reject)?;
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

/// Canonical argument container for a command call.
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

/// Identifies how a command parameter is provided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandParamKind {
    /// Provided by injection.
    Injected,
    /// Provided by user arguments.
    User,
}

/// Static metadata for a type in command signatures.
#[derive(Clone, Copy, Debug)]
pub struct CommandTypeSpec {
    /// Rust type name for introspection.
    pub rust: &'static str,
    /// Luau type expression factory.
    pub ty: fn() -> declaration::Type,
    /// Declaration dependency registration function.
    pub decls: for<'a> fn(&mut DeclRegistry<'a>),
    /// Optional documentation string.
    pub doc: Option<&'static str>,
}

impl CommandTypeSpec {
    /// Returns the Luau type expression.
    pub fn luau_ty(self) -> declaration::Type {
        (self.ty)()
    }

    /// Registers declaration dependencies for this type.
    pub fn luau_decls(self, registry: &mut DeclRegistry<'_>) {
        (self.decls)(registry);
    }
}

impl PartialEq for CommandTypeSpec {
    fn eq(&self, other: &Self) -> bool {
        self.rust == other.rust && self.doc == other.doc && self.luau_ty() == other.luau_ty()
    }
}

impl Eq for CommandTypeSpec {}

/// Static metadata for a command parameter.
#[derive(Clone, Copy, Debug)]
pub struct CommandParamSpec {
    /// Parameter name for named argument binding.
    pub name: &'static str,
    /// Parameter kind.
    pub kind: CommandParamKind,
    /// Type metadata, including the parameter's documentation.
    pub ty: CommandTypeSpec,
    /// Whether the parameter is optional.
    pub optional: bool,
    /// Required event context, supplied by the injection type.
    pub requirement: Option<fn() -> Option<CommandRequirement>>,
}

impl PartialEq for CommandParamSpec {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.kind == other.kind
            && self.ty == other.ty
            && self.optional == other.optional
            && self.requirement.and_then(|f| f()) == other.requirement.and_then(|f| f())
    }
}
impl Eq for CommandParamSpec {}

/// Event context required by a command parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandRequirement {
    /// An originating input event.
    Event,
    /// An originating mouse event.
    Mouse,
}

impl CommandRequirement {
    /// Return the stable scripting label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Mouse => "mouse",
        }
    }
}

/// Current command eligibility, separate from authorization and resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandStatus {
    /// The command can currently run.
    Enabled,
    /// The command cannot run, with a user-facing reason.
    Disabled(String),
}

impl CommandStatus {
    /// Return the stable scripting label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled(_) => "disabled",
        }
    }
}

/// Read-only, erased command eligibility hook.
pub type StatusFn = fn(&dyn Any, &dyn ViewContext) -> CoreResult<CommandStatus>;

/// Policy for resolving a command owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandTarget {
    /// Require this exact node to own the command.
    Exact(NodeId),
    /// Search this subtree, then its ancestors.
    From(NodeId),
    /// Search from the current focus when invoked.
    Focus,
}

/// A command id, its encoded arguments, and an optional target policy.
///
/// This is the one value that names a command call: dispatch, eligibility
/// checks, bindings, and scripts all take it.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandCall {
    /// Command identifier.
    pub id: CommandId,
    /// Encoded arguments.
    pub args: CommandArgs,
    /// Explicit target policy. `None` resolves from the call's origin: the
    /// dispatching node for a context, or the node where a binding wins.
    pub target: Option<CommandTarget>,
}

impl CommandCall {
    /// Bind this call to a target policy.
    #[must_use]
    pub fn with_target(mut self, target: CommandTarget) -> Self {
        self.target = Some(target);
        self
    }

    /// Return this call with one more argument: appended last to positional
    /// arguments, or stored under `name` among named ones.
    ///
    /// A widget that notifies an owner through a stored call uses this to
    /// pass what changed, such as a row index or a field's value.
    #[must_use]
    pub fn with_arg(&self, name: &str, value: impl ToArgValue) -> Self {
        let value = value.to_arg_value();
        let args = match &self.args {
            CommandArgs::Positional(values) => {
                let mut out = values.clone();
                out.push(value);
                CommandArgs::Positional(out)
            }
            CommandArgs::Named(values) => {
                let mut out = values.clone();
                out.insert(name.to_string(), value);
                CommandArgs::Named(out)
            }
        };
        Self {
            id: self.id,
            args,
            target: self.target,
        }
    }

    /// Return the explicit target, or a search from `origin` when the call
    /// has none.
    pub fn target_or(&self, origin: NodeId) -> CommandTarget {
        self.target.unwrap_or(CommandTarget::From(origin))
    }
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
    target: &mut dyn Any,
    ctx: &mut dyn Context,
    args: &CommandArgs,
) -> Result<ArgValue, CommandError>;

/// Erased argument check signature.
///
/// The check applies the arity bounds and parameter conversions of the
/// command's invoke function. It needs no target, context, or call.
pub type CheckFn = fn(args: &CommandArgs) -> Result<(), CommandError>;

/// Static metadata for a command.
#[derive(Clone, Copy, Debug)]
pub struct CommandSpec {
    /// Command identifier.
    pub id: CommandId,
    /// Command name.
    pub name: &'static str,
    /// Name of the node type that owns the command. Dispatch routes the
    /// command to a node with this name.
    pub owner: &'static str,
    /// Parameter specs.
    pub params: &'static [CommandParamSpec],
    /// Return spec.
    pub ret: CommandReturnSpec,
    /// Documentation metadata.
    pub doc: Option<&'static str>,
    /// Erased invoke entrypoint.
    pub invoke: InvokeFn,
    /// Erased argument check entrypoint.
    pub check: CheckFn,
    /// Optional read-only node eligibility hook.
    pub status: Option<StatusFn>,
}

/// Resolution of a command dispatch target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandResolution {
    /// Command targets the specified owner without searching.
    Exact {
        /// Target node ID.
        target: NodeId,
    },
    /// Command would dispatch to a node in the focus subtree.
    Subtree {
        /// Target node ID.
        target: NodeId,
    },
    /// Command would dispatch to an ancestor of focus.
    Ancestor {
        /// Target node ID.
        target: NodeId,
    },
}

/// Command availability from a given focus context.
#[derive(Clone, Debug)]
pub struct CommandAvailability {
    /// Command specification.
    pub spec: &'static CommandSpec,
    /// Resolution if the command has a target, or `None` if no target exists.
    pub resolution: Option<CommandResolution>,
    /// Eligibility for a resolved command.
    pub status: Option<CommandStatus>,
    /// Required context absent at inspection time.
    pub missing_requirements: Vec<CommandRequirement>,
}

impl CommandResolution {
    /// Return the resolved node target.
    pub fn target(self) -> NodeId {
        match self {
            Self::Exact { target } | Self::Subtree { target } | Self::Ancestor { target } => target,
        }
    }
}

/// Resolves command targets relative to a starting node.
pub(crate) struct CommandResolver<'a> {
    /// Core tree and command registry.
    core: &'a Core,
    /// Starting node for command dispatch.
    start: NodeId,
    /// Explicit target resolution policy.
    target: CommandTarget,
}

impl<'a> CommandResolver<'a> {
    /// Construct a resolver for an explicit target policy.
    pub(crate) fn for_target(core: &'a Core, target: CommandTarget) -> Self {
        let start = match target {
            CommandTarget::Exact(node) | CommandTarget::From(node) => node,
            CommandTarget::Focus => core.focus.unwrap_or(core.root),
        };
        Self {
            core,
            start,
            target,
        }
    }

    /// Resolve a command specification to the target dispatch would use.
    pub(crate) fn resolve(&self, spec: &CommandSpec) -> Option<CommandResolution> {
        if let CommandTarget::Exact(node) = self.target {
            return self
                .core
                .nodes
                .get(node)
                .is_some_and(|node| node.name == spec.owner)
                .then_some(CommandResolution::Exact { target: node });
        }
        self.resolve_owner(spec.owner)
    }

    /// Resolve a node-owner name with subtree targets preferred over ancestors.
    ///
    /// A hidden node, and everything under it, takes no commands: a widget
    /// the user cannot see offers nothing to act on. So a command whose only
    /// owner is hidden has no target, and a binding to it is dormant until the
    /// owner shows. An exact target names its node outright and is resolved
    /// by [`Self::resolve`] whether or not the node shows.
    pub(crate) fn resolve_owner(&self, owner: &str) -> Option<CommandResolution> {
        if !self.core.nodes.contains_key(self.start) {
            return None;
        }

        let mut stack = vec![self.start];
        while let Some(node_id) = stack.pop() {
            let node = &self.core.nodes[node_id];
            if node.hidden {
                continue;
            }
            if node.name == owner {
                return Some(CommandResolution::Subtree { target: node_id });
            }
            for child in node.children.iter().rev() {
                stack.push(*child);
            }
        }

        let mut current = self.core.nodes[self.start].parent;
        while let Some(node_id) = current {
            let node = &self.core.nodes[node_id];
            if node.name == owner && !node.hidden {
                return Some(CommandResolution::Ancestor { target: node_id });
            }
            current = node.parent;
        }

        None
    }

    /// Return availability for every registered command.
    pub(crate) fn availability(&self) -> CoreResult<Vec<CommandAvailability>> {
        self.core
            .commands
            .iter()
            .map(|(_, spec)| self.availability_for(spec))
            .collect()
    }

    /// Inspect one command using the same resolution and eligibility rules.
    pub(crate) fn availability_for(
        &self,
        spec: &'static CommandSpec,
    ) -> CoreResult<CommandAvailability> {
        let resolution = self.resolve(spec);
        let status = match resolution {
            Some(resolution) => Some(status_at(self.core, spec, resolution)?),
            None => None,
        };
        Ok(CommandAvailability {
            spec,
            resolution,
            status,
            missing_requirements: missing_requirements(self.core, spec),
        })
    }
}

/// The CommandNode trait is implemented by widgets to expose commands.
pub trait CommandNode {
    /// Return a list of commands for this node.
    fn commands() -> &'static [&'static CommandSpec]
    where
        Self: Sized;
}

impl CommandSpec {
    /// Return whether two specifications define the same command contract.
    fn equivalent(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.owner == other.owner
            && self.params == other.params
            && self.ret == other.ret
            && self.doc == other.doc
            && ptr::fn_addr_eq(self.invoke, other.invoke)
            && ptr::fn_addr_eq(self.check, other.check)
            && match (self.status, other.status) {
                (Some(a), Some(b)) => ptr::fn_addr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }

    /// Build a call to this command with no arguments.
    pub fn call(&self) -> CommandCall {
        self.call_with(())
    }

    /// Build a call to this command.
    pub fn call_with(&self, args: impl Into<CommandArgs>) -> CommandCall {
        CommandCall {
            id: self.id,
            args: args.into(),
            target: None,
        }
    }
}

/// Collection of available commands keyed by id.
#[derive(Clone, Debug, Default)]
pub(crate) struct CommandSet {
    /// Registry of command specs by id.
    commands: BTreeMap<&'static str, &'static CommandSpec>,
}

impl CommandSet {
    /// Add a command batch atomically.
    ///
    /// Repeating an equivalent definition is idempotent. A conflicting
    /// definition or invalid batch leaves the set unchanged.
    pub fn add(&mut self, specs: &'static [&'static CommandSpec]) -> Result<(), CommandError> {
        let mut batch = HashMap::with_capacity(specs.len());
        for spec in specs {
            validate_command_spec(spec)?;
            if let Some(previous) = batch.insert(spec.id.0, *spec)
                && !previous.equivalent(spec)
            {
                return Err(CommandError::ConflictingCommand {
                    id: spec.id.0.to_string(),
                });
            }
            if let Some(previous) = self.commands.get(spec.id.0)
                && !previous.equivalent(spec)
            {
                return Err(CommandError::ConflictingCommand {
                    id: spec.id.0.to_string(),
                });
            }
        }
        for (id, spec) in batch {
            self.commands.entry(id).or_insert(spec);
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

/// Validate the static metadata for one command before registry mutation.
fn validate_command_spec(spec: &CommandSpec) -> Result<(), CommandError> {
    if spec.id.0.is_empty() || spec.name.is_empty() {
        return Err(CommandError::InvalidCommand {
            id: spec.id.0.to_string(),
            message: "command id and name must not be empty".to_string(),
        });
    }
    let mut names = HashSet::with_capacity(spec.params.len());
    for param in spec.params {
        if param.name.is_empty() || !names.insert(param.name) {
            return Err(CommandError::InvalidCommand {
                id: spec.id.0.to_string(),
                message: format!("invalid or duplicate parameter name `{}`", param.name),
            });
        }
    }
    Ok(())
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

    /// A command ID was registered with a different specification.
    #[error("conflicting command definition: {id}")]
    ConflictingCommand {
        /// Conflicting command id.
        id: String,
    },

    /// Static command metadata is invalid.
    #[error("invalid command definition {id}: {message}")]
    InvalidCommand {
        /// Invalid command id.
        id: String,
        /// Validation failure.
        message: String,
    },

    /// No matching target found for a node-routed command.
    #[error("no target node found for command {id} (owner {owner})")]
    NoTarget {
        /// Requested command id.
        id: String,
        /// Expected owner node name.
        owner: String,
    },

    /// A node handle no longer points at a live node.
    #[error("node handle is no longer valid: {id:?}")]
    InvalidNode {
        /// Stale node id.
        id: NodeId,
    },

    /// An exact target does not own the requested node command.
    #[error("node {node:?} does not own command {id} (expected owner {expected})")]
    WrongOwner {
        /// Requested command identifier.
        id: String,
        /// Requested exact node.
        node: NodeId,
        /// Required owner name.
        expected: String,
    },
    /// Eligibility changed or the command was already disabled.
    #[error("command {id} is disabled: {reason}")]
    Disabled {
        /// Requested command identifier.
        id: String,
        /// Current disabled reason.
        reason: String,
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

    /// The command target did not have the registered owner type.
    #[error("command target type mismatch")]
    TargetTypeMismatch,

    /// Command execution failure.
    #[error("command execution failed: {0}")]
    Exec(#[source] Box<dyn StdError + Send + Sync>),
}

impl CommandError {
    /// Return the stable script-visible category of this failure.
    pub(crate) fn script_kind(&self) -> ScriptErrorKind {
        match self {
            Self::UnknownCommand { .. } => ScriptErrorKind::UnknownCommand,
            Self::ConflictingCommand { .. } => ScriptErrorKind::ConflictingCommand,
            Self::InvalidCommand { .. } => ScriptErrorKind::InvalidCommand,
            Self::NoTarget { .. } => ScriptErrorKind::NoTarget,
            Self::InvalidNode { .. } => ScriptErrorKind::InvalidNode,
            Self::WrongOwner { .. } => ScriptErrorKind::WrongOwner,
            Self::Disabled { .. } => ScriptErrorKind::DisabledCommand,
            Self::ArityMismatch { .. } => ScriptErrorKind::ArityMismatch,
            Self::MissingNamedArg { .. } => ScriptErrorKind::MissingNamedArgument,
            Self::UnknownNamedArg { .. } => ScriptErrorKind::UnknownNamedArgument,
            Self::TypeMismatch { .. } => ScriptErrorKind::TypeMismatch,
            Self::MissingInjected { .. } => ScriptErrorKind::MissingInjected,
            Self::Conversion { .. } => ScriptErrorKind::Conversion,
            Self::TargetTypeMismatch => ScriptErrorKind::TargetTypeMismatch,
            Self::Exec(_) => ScriptErrorKind::CommandExecution,
        }
    }

    /// Preserve a command implementation's concrete error as the execution
    /// source.
    #[doc(hidden)]
    pub fn execution(error: impl StdError + Send + Sync + 'static) -> Self {
        Self::Exec(Box::new(error))
    }

    #[doc(hidden)]
    #[must_use]
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

/// Seals [`Inject`]: the command derive selects injected parameters by type
/// name, so only Canopy's own implementations can take part.
mod sealed {
    use super::{Event, MouseEvent};

    /// Marker implemented only for Canopy's injectable types.
    pub trait Inject {}

    impl Inject for Event {}
    impl Inject for MouseEvent {}
    impl<T: Inject> Inject for Option<T> {}
}

/// A command parameter that Canopy supplies from the current event instead
/// of the caller.
///
/// The implementations are [`Event`], [`MouseEvent`], and an `Option` of
/// either. The trait is sealed.
pub trait Inject: sealed::Inject + Sized {
    /// Required event context, if this injection depends on one.
    fn requirement() -> Option<CommandRequirement> {
        None
    }

    /// Inject a value from the context, or `None` when the context has none.
    fn inject(ctx: &dyn Context) -> Option<Self>;
}

impl<T> Inject for Option<T>
where
    T: Inject,
{
    fn requirement() -> Option<CommandRequirement> {
        T::requirement()
    }

    fn inject(ctx: &dyn Context) -> Option<Self> {
        Some(T::inject(ctx))
    }
}

impl Inject for MouseEvent {
    fn requirement() -> Option<CommandRequirement> {
        Some(CommandRequirement::Mouse)
    }
    fn inject(ctx: &dyn Context) -> Option<Self> {
        ctx.current_mouse_event()
    }
}

impl Inject for Event {
    fn requirement() -> Option<CommandRequirement> {
        Some(CommandRequirement::Event)
    }
    fn inject(ctx: &dyn Context) -> Option<Self> {
        ctx.current_event().cloned()
    }
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

/// Resolve and invoke a call. A call without a target resolves from
/// `origin`.
pub(crate) fn dispatch(
    core: &mut Core,
    origin: NodeId,
    call: &CommandCall,
) -> Result<ArgValue, CommandError> {
    let checkpoint = core.begin_dispatch();
    let result = dispatch_inner(core, call.target_or(origin), call);
    let completion = core.finish_dispatch(checkpoint, result.is_ok());
    match result {
        Ok(value) => {
            completion.map_err(CommandError::execution)?;
            Ok(value)
        }
        Err(error) => Err(error),
    }
}

/// Resolve and check a call that runs later, as [`dispatch`] would now.
///
/// The command must exist, its arguments must convert, its node arguments
/// must be live, and its target must resolve. A call without a target
/// resolves from `origin`. The command's status is not read, because its
/// owner can still be borrowed.
pub(crate) fn resolve_posted(
    core: &Core,
    origin: NodeId,
    call: &CommandCall,
) -> Result<NodeId, CommandError> {
    let spec = core
        .commands
        .get(call.id.0)
        .ok_or_else(|| CommandError::UnknownCommand {
            id: call.id.0.to_string(),
        })?;
    (spec.check)(&call.args)?;
    validate_node_args(core, &call.args)?;
    let resolver = CommandResolver::for_target(core, call.target_or(origin));
    Ok(checked_resolution(&resolver, spec)?.target())
}

/// Invoke one command inside an established completion boundary.
fn dispatch_inner(
    core: &mut Core,
    target: CommandTarget,
    call: &CommandCall,
) -> Result<ArgValue, CommandError> {
    let spec = core
        .commands
        .get(call.id.0)
        .ok_or_else(|| CommandError::UnknownCommand {
            id: call.id.0.to_string(),
        })?;
    validate_node_args(core, &call.args)?;
    let resolution = checked_resolution(&CommandResolver::for_target(core, target), spec)?;
    dispatch_on_node(core, resolution.target(), spec, &call.args)
}

/// Reject stale and wrong exact owners before invocation or inspection.
fn checked_resolution(
    resolver: &CommandResolver<'_>,
    spec: &CommandSpec,
) -> Result<CommandResolution, CommandError> {
    if !resolver.core.nodes.contains_key(resolver.start) {
        return Err(CommandError::InvalidNode { id: resolver.start });
    }
    resolver.resolve(spec).ok_or_else(|| match resolver.target {
        CommandTarget::Exact(node) => CommandError::WrongOwner {
            id: spec.id.0.to_string(),
            node,
            expected: spec.owner.to_string(),
        },
        _ => CommandError::NoTarget {
            id: spec.id.0.to_string(),
            owner: spec.owner.to_string(),
        },
    })
}

/// Report an unavailable call target as disabled without hiding hook
/// failures. A call without a target resolves from `origin`.
pub(crate) fn command_status(
    core: &Core,
    origin: NodeId,
    call: &CommandCall,
) -> CoreResult<CommandStatus> {
    let Some(spec) = core.commands.get(call.id.0) else {
        return Ok(CommandStatus::Disabled(
            CommandError::UnknownCommand {
                id: call.id.0.to_string(),
            }
            .to_string(),
        ));
    };
    let target = call.target_or(origin);
    let resolution = match checked_resolution(&CommandResolver::for_target(core, target), spec) {
        Ok(resolution) => resolution,
        Err(error) => return Ok(CommandStatus::Disabled(error.to_string())),
    };
    status_at(core, spec, resolution)
}

/// Run the optional read-only hook on a resolved owner.
fn status_at(
    core: &Core,
    spec: &CommandSpec,
    resolution: CommandResolution,
) -> CoreResult<CommandStatus> {
    let Some(status) = spec.status else {
        return Ok(CommandStatus::Enabled);
    };
    let node = resolution.target();
    core.with_widget(
        node,
        WidgetOperation::access("command status"),
        |widget, core| status(widget as &dyn Any, &CoreViewContext::new(core, node)),
    )?
}

/// Find required injections absent from the current event.
fn missing_requirements(core: &Core, spec: &CommandSpec) -> Vec<CommandRequirement> {
    let event = core.current_event();
    let mut missing = Vec::new();
    for requirement in spec
        .params
        .iter()
        .filter(|p| !p.optional)
        .filter_map(|p| p.requirement.and_then(|f| f()))
    {
        let present = match requirement {
            CommandRequirement::Event => event.is_some(),
            CommandRequirement::Mouse => matches!(event, Some(Event::Mouse(_))),
        };
        if !present && !missing.contains(&requirement) {
            missing.push(requirement);
        }
    }
    missing
}

/// Dispatch a node-routed command to a resolved node.
fn dispatch_on_node(
    core: &mut Core,
    node_id: NodeId,
    spec: &CommandSpec,
    args: &CommandArgs,
) -> Result<ArgValue, CommandError> {
    core.with_widget_ctx(node_id, |widget, ctx| {
        if let Some(status) = spec.status
            && let CommandStatus::Disabled(reason) =
                status(widget as &dyn Any, ctx).map_err(CommandError::execution)?
        {
            return Err(CommandError::Disabled {
                id: spec.id.0.to_string(),
                reason,
            });
        }
        (spec.invoke)(widget as &mut dyn Any, ctx, args)
    })
    .map_err(CommandError::execution)?
}

/// Validate every node handle carried by a call before command code sees it.
fn validate_node_args(core: &Core, args: &CommandArgs) -> Result<(), CommandError> {
    match args {
        CommandArgs::Positional(values) => {
            for value in values {
                validate_node_arg(core, value)?;
            }
        }
        CommandArgs::Named(values) => {
            for value in values.values() {
                validate_node_arg(core, value)?;
            }
        }
    }
    Ok(())
}

/// Validate every node handle reachable from a dynamic argument value.
fn validate_node_arg(core: &Core, value: &ArgValue) -> Result<(), CommandError> {
    match value {
        ArgValue::Node(id) if !core.nodes.contains_key(*id) => {
            Err(CommandError::InvalidNode { id: *id })
        }
        ArgValue::Array(values) => {
            for value in values {
                validate_node_arg(core, value)?;
            }
            Ok(())
        }
        ArgValue::Map(values) => {
            for value in values.values() {
                validate_node_arg(core, value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use serde::{Serialize, ser};

    use super::*;
    use crate::{NodeName, Widget, core::id::testing_node_id, error::Error};

    struct StatusOwner;

    impl Widget for StatusOwner {
        fn name(&self) -> NodeName {
            NodeName::convert("status_owner")
        }
    }

    /// Fail inside eligibility to distinguish callback errors from resolution.
    fn failed_status(_widget: &dyn Any, _view: &dyn ViewContext) -> CoreResult<CommandStatus> {
        Err(Error::Invalid("eligibility hook failed".into()))
    }

    static FAILING_STATUS: CommandSpec = CommandSpec {
        id: CommandId("status_owner.action"),
        name: "action",
        owner: "status_owner",
        params: &[],
        ret: CommandReturnSpec::Unit,
        doc: None,
        invoke: registry_invoke,
        check: registry_check,
        status: Some(failed_status),
    };

    static FAILING_COMMANDS: &[&CommandSpec] = &[&FAILING_STATUS];

    #[test]
    fn command_status_disables_resolution_failures_but_preserves_hook_errors() -> CoreResult<()> {
        let mut core = Core::new();
        core.commands.add(FAILING_COMMANDS)?;
        let owner = core.create_detached(StatusOwner)?;
        let missing = core.create_detached(StatusOwner)?;
        core.remove_subtree(missing)?;
        let call = FAILING_STATUS.call();
        let context = CoreViewContext::new(&core, core.root);
        assert!(matches!(
            context.command_status(&call.clone().with_target(CommandTarget::Exact(missing)))?,
            CommandStatus::Disabled(_)
        ));
        assert!(matches!(
            context.command_status(&call.clone().with_target(CommandTarget::Exact(core.root)))?,
            CommandStatus::Disabled(_)
        ));
        assert!(
            matches!(context.command_status(&call.with_target(CommandTarget::Exact(owner))), Err(Error::Invalid(message)) if message == "eligibility hook failed")
        );
        let unknown = CommandCall {
            id: CommandId("unknown.action"),
            args: CommandArgs::default(),
            target: Some(CommandTarget::Exact(owner)),
        };
        assert!(matches!(
            context.command_status(&unknown)?,
            CommandStatus::Disabled(_)
        ));
        Ok(())
    }

    fn registry_invoke(
        _target: &mut dyn Any,
        _ctx: &mut dyn Context,
        _args: &CommandArgs,
    ) -> Result<ArgValue, CommandError> {
        Ok(ArgValue::Null)
    }

    fn registry_check(_args: &CommandArgs) -> Result<(), CommandError> {
        Ok(())
    }

    static REGISTRY_A: CommandSpec = CommandSpec {
        id: CommandId("registry.a"),
        name: "a",
        owner: "registry",
        params: &[],
        ret: CommandReturnSpec::Unit,
        doc: Some("a"),
        invoke: registry_invoke,
        check: registry_check,
        status: None,
    };
    static REGISTRY_A_CONFLICT: CommandSpec = CommandSpec {
        id: CommandId("registry.a"),
        name: "a",
        owner: "registry",
        params: &[],
        ret: CommandReturnSpec::Unit,
        doc: Some("conflict"),
        invoke: registry_invoke,
        check: registry_check,
        status: None,
    };
    static REGISTRY_B: CommandSpec = CommandSpec {
        id: CommandId("registry.b"),
        name: "b",
        owner: "registry",
        params: &[],
        ret: CommandReturnSpec::Unit,
        doc: None,
        invoke: registry_invoke,
        check: registry_check,
        status: None,
    };
    static REGISTRY_A_BATCH: &[&CommandSpec] = &[&REGISTRY_A];
    static REGISTRY_CONFLICT_BATCH: &[&CommandSpec] = &[&REGISTRY_B, &REGISTRY_A_CONFLICT];
    static REGISTRY_RETRY_BATCH: &[&CommandSpec] = &[&REGISTRY_B, &REGISTRY_A];
    static REGISTRY_FORWARD_BATCH: &[&CommandSpec] = &[&REGISTRY_A, &REGISTRY_B];
    static REGISTRY_REVERSE_BATCH: &[&CommandSpec] = &[&REGISTRY_B, &REGISTRY_A];

    #[test]
    fn command_batches_are_atomic_conflict_aware_and_idempotent() {
        let mut commands = CommandSet::default();
        commands.add(REGISTRY_A_BATCH).unwrap();

        let error = commands.add(REGISTRY_CONFLICT_BATCH).unwrap_err();
        assert!(matches!(error, CommandError::ConflictingCommand { .. }));
        assert!(commands.get("registry.b").is_none());

        commands.add(REGISTRY_RETRY_BATCH).unwrap();
        commands.add(REGISTRY_RETRY_BATCH).unwrap();
        assert!(commands.get("registry.a").is_some());
        assert!(commands.get("registry.b").is_some());
        assert_eq!(commands.iter().count(), 2);
    }

    #[test]
    fn command_availability_is_independent_of_registration_order() {
        let mut forward = Core::new();
        forward.commands.add(REGISTRY_FORWARD_BATCH).unwrap();
        let mut reverse = Core::new();
        reverse.commands.add(REGISTRY_REVERSE_BATCH).unwrap();

        let forward_ids = CommandResolver::for_target(&forward, CommandTarget::From(forward.root))
            .availability()
            .unwrap()
            .into_iter()
            .map(|item| item.spec.id.0)
            .collect::<Vec<_>>();
        let reverse_ids = CommandResolver::for_target(&reverse, CommandTarget::From(reverse.root))
            .availability()
            .unwrap()
            .into_iter()
            .map(|item| item.spec.id.0)
            .collect::<Vec<_>>();
        assert_eq!(forward_ids, vec!["registry.a", "registry.b"]);
        assert_eq!(reverse_ids, forward_ids);
    }

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
    fn serde_arg_reports_errors() {
        struct BadSerde;

        impl Serialize for BadSerde {
            fn serialize<S>(&self, _: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(ser::Error::custom("nope"))
            }
        }

        let err = SerdeArg(BadSerde).try_to_arg_value().unwrap_err();
        assert!(matches!(err, CommandError::Conversion { .. }));
    }

    #[test]
    fn uint_arg_round_trip() {
        let value = ArgValue::UInt(u64::MAX);
        let json = arg_value_to_json(&value, NodeJson::Reject).unwrap();
        let out = json_to_arg_value(json).unwrap();
        assert_eq!(out, value);
    }

    #[test]
    fn node_arg_json_requires_external_mode() {
        let value = ArgValue::Node(testing_node_id());

        assert!(arg_value_to_json(&value, NodeJson::Reject).is_err());

        let json = value.to_external_json_value().unwrap();
        assert_eq!(json["type"], JsonValue::String("NodeId".to_string()));
        assert!(json["token"].is_string());
    }

    #[test]
    fn serde_arg_encodes_large_unsigned_values() {
        let value = (i64::MAX as u64) + 1;
        let encoded = SerdeArg(value).try_to_arg_value().unwrap();
        assert_eq!(encoded, ArgValue::UInt(value));
    }
}
