use syn::Type;

/// Arguments to the "command" derive macro.
#[derive(Debug, Default)]
pub struct MacroArgs {
    /// Ignore command return value when dispatching.
    pub(crate) ignore_result: bool,
    /// Override short description.
    pub(crate) desc: Option<syn::LitStr>,
    /// Mark command as hidden from help.
    pub(crate) hidden: bool,
}

/// Parsed default argument value.
#[derive(Debug, Clone)]
pub struct DefaultValue {
    /// Parsed expression for the default.
    pub(crate) expr: syn::Expr,
    /// String rendering for diagnostics.
    pub(crate) display: String,
}

/// Classification of command parameter sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// Context receiver parameter.
    Context {
        /// Whether the reference is mutable.
        mutable: bool,
    },
    /// Extractor-provided parameter.
    Injected,
    /// User-provided argument parameter.
    User,
}

/// Parsed metadata for a command parameter.
#[derive(Debug, Clone)]
pub struct ParamMeta {
    /// Rust identifier for binding.
    pub(crate) ident: syn::Ident,
    /// Normalized parameter name.
    pub(crate) name: String,
    /// Original Rust type.
    pub(crate) ty: Type,
    /// Type rendered to a string.
    pub(crate) ty_str: String,
    /// Parameter classification.
    pub(crate) kind: ParamKind,
    /// Whether the parameter is optional.
    pub(crate) is_option: bool,
    /// Optional default value.
    pub(crate) default: Option<DefaultValue>,
}

/// Classification of command return types.
#[derive(Debug, Clone)]
pub enum ReturnKind {
    /// Unit return.
    Unit,
    /// Non-unit return.
    Value {
        /// Rendered return type name.
        ty_str: String,
    },
}

/// Parsed metadata for a command return type.
#[derive(Debug, Clone)]
pub struct ReturnMeta {
    /// Whether the command returns a Result.
    pub(crate) is_result: bool,
    /// Return type classification.
    pub(crate) kind: ReturnKind,
}

/// Extracted documentation from a method.
#[derive(Debug, Clone, Default)]
pub struct DocMeta {
    /// Short description (first sentence or explicit override).
    pub(crate) short: Option<String>,
    /// Full description (all doc comments joined).
    pub(crate) long: Option<String>,
    /// Whether this command is hidden from help.
    pub(crate) hidden: bool,
}

/// Parsed metadata describing a command.
#[derive(Debug, Clone)]
pub struct CommandMeta {
    /// Command name (snake_case).
    pub(crate) name: String,
    /// Owner type name.
    pub(crate) owner: String,
    /// Parameters in declaration order.
    pub(crate) params: Vec<ParamMeta>,
    /// Whether the return value is ignored.
    pub(crate) ignore_result: bool,
    /// Return type metadata.
    pub(crate) ret: ReturnMeta,
    /// Documentation metadata.
    pub(crate) doc: DocMeta,
}

/// The source used to bind a user-supplied command argument.
#[derive(Debug, Clone, Copy)]
pub enum UserBindingSource {
    /// Bind from a positional argument list.
    Positional(usize),
    /// Bind from a normalized named argument map.
    Named,
}
