//! Proc-macro support for canopy commands and nodes.

use std::result::Result as StdResult;

use convert_case::{Case, Casing};
use proc_macro_error::*;
use quote::{ToTokens, quote};
use structmeta::StructMeta;
use syn::{
    Attribute, GenericArgument, ImplItem, ImplItemFn, ItemImpl, Meta, Pat, PathArguments,
    ReturnType, Type, TypeParamBound, parse_macro_input,
};

/// Local result type for macro parsing.
type Result<T> = StdResult<T, Error>;

/// Errors raised while parsing command metadata.
#[derive(PartialEq, Eq, thiserror::Error, Debug, Clone)]
enum Error {
    /// Failed to parse an attribute payload.
    #[error("parse error: {0}")]
    Parse(String),
    /// Unsupported argument or return type.
    #[error("unsupported: {0}")]
    Unsupported(String),
}

impl From<Error> for Diagnostic {
    fn from(e: Error) -> Self {
        Self::spanned(proc_macro2::Span::call_site(), Level::Error, format!("{e}"))
    }
}

/// Arguments to the "command" derive macro.
#[derive(Debug, Default, StructMeta)]
struct MacroArgs {
    /// Ignore command return value when dispatching.
    ignore_result: bool,
}

/// Parsed default argument value.
#[derive(Debug, Clone)]
struct DefaultValue {
    /// Parsed expression for the default.
    expr: syn::Expr,
    /// String rendering for diagnostics.
    display: String,
}

/// Classification of command parameter sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParamKind {
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
struct ParamMeta {
    /// Rust identifier for binding.
    ident: syn::Ident,
    /// Normalized parameter name.
    name: String,
    /// Original Rust type.
    ty: Type,
    /// Type rendered to a string.
    ty_str: String,
    /// Parameter classification.
    kind: ParamKind,
    /// Whether the parameter is optional.
    is_option: bool,
    /// Optional default value.
    default: Option<DefaultValue>,
}

/// Classification of command return types.
#[derive(Debug, Clone)]
enum ReturnKind {
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
struct ReturnMeta {
    /// Whether the command returns a Result.
    is_result: bool,
    /// Return type classification.
    kind: ReturnKind,
}

/// Parsed metadata describing a command.
#[derive(Debug, Clone)]
struct CommandMeta {
    /// Command name (snake_case).
    name: String,
    /// Owner type name.
    owner: String,
    /// Parameters in declaration order.
    params: Vec<ParamMeta>,
    /// Whether the return value is ignored.
    ignore_result: bool,
    /// Return type metadata.
    ret: ReturnMeta,
}

/// Parse generated tokens into an impl item with context on failure.
fn parse_impl_item(tokens: proc_macro2::TokenStream, label: &str) -> syn::ImplItem {
    syn::parse2(tokens).unwrap_or_else(|err| abort_call_site!("{} parse failed: {}", label, err))
}

/// Render a Rust type into a string for metadata.
fn type_to_string(ty: &Type) -> String {
    ty.to_token_stream().to_string()
}

/// Extract a single generic type argument from a path type.
fn extract_single_generic<'a>(ty: &'a Type, ident: &str) -> Option<&'a Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != ident {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    match args.args.first()? {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

/// Determine whether a type is a reference to a Context.
fn is_context_ref(ty: &Type) -> Option<bool> {
    let Type::Reference(reference) = ty else {
        return None;
    };
    let mutable = reference.mutability.is_some();
    match &*reference.elem {
        Type::TraitObject(obj) => {
            for bound in &obj.bounds {
                if let TypeParamBound::Trait(trait_bound) = bound
                    && trait_bound.path.segments.last()?.ident == "Context"
                {
                    return Some(mutable);
                }
            }
            None
        }
        Type::Path(path) => {
            if path.path.segments.last()?.ident == "Context" {
                Some(mutable)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// True when a type is a builtin injected parameter.
fn is_builtin_injected(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    matches!(
        segment.ident.to_string().as_str(),
        "Event" | "MouseEvent" | "ListRowContext"
    )
}

/// Parse an `#[arg(default = ...)]` attribute.
fn parse_arg_default(attrs: &[Attribute]) -> Result<Option<DefaultValue>> {
    let mut default = None;
    for attr in attrs {
        if !attr.path().is_ident("arg") {
            continue;
        }

        match &attr.meta {
            Meta::List(_) => {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("default") {
                        if default.is_some() {
                            return Err(syn::Error::new_spanned(
                                meta.path,
                                "duplicate default attribute",
                            ));
                        }
                        if meta.input.is_empty() {
                            default = Some(DefaultValue {
                                expr: syn::parse_quote!(Default::default()),
                                display: "Default::default()".to_string(),
                            });
                            return Ok(());
                        }
                        let value = meta.value()?;
                        let expr: syn::Expr = value.parse()?;
                        let display = expr.to_token_stream().to_string();
                        default = Some(DefaultValue { expr, display });
                        return Ok(());
                    }
                    Err(syn::Error::new_spanned(meta.path, "unknown arg attribute"))
                })
                .map_err(|e| Error::Parse(e.to_string()))?;
            }
            _ => {
                return Err(Error::Parse("invalid arg attribute".into()));
            }
        }
    }
    Ok(default)
}

/// Parse command return metadata from a signature.
fn parse_return_type(output: &ReturnType) -> Result<ReturnMeta> {
    match output {
        ReturnType::Default => Ok(ReturnMeta {
            is_result: false,
            kind: ReturnKind::Unit,
        }),
        ReturnType::Type(_, ty) => {
            if let Some(inner) = extract_single_generic(ty, "Result") {
                let kind = match inner {
                    Type::Tuple(tuple) if tuple.elems.is_empty() => ReturnKind::Unit,
                    _ => ReturnKind::Value {
                        ty_str: type_to_string(inner),
                    },
                };
                Ok(ReturnMeta {
                    is_result: true,
                    kind,
                })
            } else {
                let kind = match &**ty {
                    Type::Tuple(tuple) if tuple.elems.is_empty() => ReturnKind::Unit,
                    _ => ReturnKind::Value {
                        ty_str: type_to_string(ty),
                    },
                };
                Ok(ReturnMeta {
                    is_result: false,
                    kind,
                })
            }
        }
    }
}

/// Parse an impl method annotated with `#[command]`.
fn parse_command_method(owner: &str, method: &mut ImplItemFn) -> Result<Option<CommandMeta>> {
    let mut macro_args = None;

    for attr in &method.attrs {
        if attr.path().is_ident("command") {
            let mut args = MacroArgs::default();
            match &attr.meta {
                Meta::Path(_) => {}
                Meta::List(_) => {
                    attr.parse_nested_meta(|meta| {
                        if meta.path.is_ident("ignore_result") {
                            args.ignore_result = true;
                        } else {
                            return Err(syn::Error::new_spanned(
                                meta.path,
                                "unknown command argument",
                            ));
                        }
                        Ok(())
                    })
                    .map_err(|e| Error::Parse(e.to_string()))?;
                }
                Meta::NameValue(_) => {
                    return Err(Error::Parse("invalid command argument".into()));
                }
            }
            macro_args = Some(args);
        }
    }

    let Some(macro_args) = macro_args else {
        return Ok(None);
    };

    let mut params = Vec::new();
    let mut has_receiver = false;

    for input in &mut method.sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                has_receiver = true;
                if receiver.reference.is_none() {
                    return Err(Error::Unsupported(
                        "command methods must take &self or &mut self".into(),
                    ));
                }
            }
            syn::FnArg::Typed(pat) => {
                let ident = match &*pat.pat {
                    Pat::Ident(ident) => ident.ident.clone(),
                    _ => {
                        return Err(Error::Unsupported(
                            "command arguments must be identifiers".into(),
                        ));
                    }
                };
                let name = ident.to_string();

                let default = parse_arg_default(&pat.attrs)?;
                pat.attrs.retain(|attr| !attr.path().is_ident("arg"));

                if let Some(mutable) = is_context_ref(&pat.ty) {
                    if default.is_some() {
                        return Err(Error::Unsupported(
                            "context parameters cannot have defaults".into(),
                        ));
                    }
                    params.push(ParamMeta {
                        ident,
                        name,
                        ty: (*pat.ty).clone(),
                        ty_str: type_to_string(&pat.ty),
                        kind: ParamKind::Context { mutable },
                        is_option: false,
                        default: None,
                    });
                    continue;
                }

                let mut is_option = false;
                let inner = if let Some(inner) = extract_single_generic(&pat.ty, "Option") {
                    is_option = true;
                    inner.clone()
                } else {
                    (*pat.ty).clone()
                };
                if matches!(inner, Type::Reference(_)) {
                    return Err(Error::Unsupported(
                        "reference arguments are not supported".into(),
                    ));
                }

                let kind = if extract_single_generic(&inner, "Arg").is_some() {
                    ParamKind::User
                } else if extract_single_generic(&inner, "Injected").is_some()
                    || is_builtin_injected(&inner)
                {
                    ParamKind::Injected
                } else {
                    ParamKind::User
                };

                if kind != ParamKind::User && default.is_some() {
                    return Err(Error::Unsupported(
                        "only user arguments may have defaults".into(),
                    ));
                }

                if kind == ParamKind::User && is_option && default.is_some() {
                    return Err(Error::Unsupported(
                        "Option parameters cannot have defaults".into(),
                    ));
                }

                params.push(ParamMeta {
                    ident,
                    name,
                    ty: (*pat.ty).clone(),
                    ty_str: type_to_string(&pat.ty),
                    kind,
                    is_option,
                    default,
                });
            }
        }
    }

    if !has_receiver {
        return Err(Error::Unsupported(
            "command methods must take &self or &mut self".into(),
        ));
    }

    let ret = parse_return_type(&method.sig.output)?;

    Ok(Some(CommandMeta {
        name: method.sig.ident.to_string(),
        owner: owner.to_string(),
        params,
        ignore_result: macro_args.ignore_result,
        ret,
    }))
}

/// Resolve the owner type name for an impl block.
fn owner_name(input: &ItemImpl) -> Result<String> {
    let Type::Path(path) = &*input.self_ty else {
        return Err(Error::Unsupported("unsupported impl type".into()));
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(Error::Unsupported("unsupported impl type".into()));
    };
    let raw = segment.ident.to_string();
    let snake = raw.to_case(Case::Snake);
    let filtered: String = snake
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
        .collect();
    Ok(filtered)
}

/// Generate command metadata and wrappers for `#[command]` methods in an impl block.
#[proc_macro_error]
#[proc_macro_attribute]
pub fn derive_commands(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let mut input = parse_macro_input!(input as ItemImpl);

    let owner = owner_name(&input).unwrap_or_abort();
    let name = input.self_ty.clone();
    let (impl_generics, _, where_clause) = input.generics.split_for_impl();

    let mut commands = Vec::new();
    for item in &mut input.items {
        if let ImplItem::Fn(method) = item
            && let Some(command) = parse_command_method(&owner, method).unwrap_or_abort()
        {
            commands.push(command);
        }
    }

    let mut generated_items: Vec<syn::ImplItem> = Vec::new();
    let mut spec_refs = Vec::new();

    for cmd in &commands {
        let name_str = &cmd.name;
        let name_ident = syn::Ident::new(name_str, proc_macro2::Span::call_site());
        let owner_str = &cmd.owner;
        let id_str = format!("{owner_str}::{name_str}");

        let invoke_ident = syn::Ident::new(
            &format!("__canopy_cmd_invoke_{name_str}"),
            proc_macro2::Span::call_site(),
        );
        let params_const_ident = syn::Ident::new(
            &format!("__CANOPY_CMD_{}_PARAMS", name_str.to_uppercase()),
            proc_macro2::Span::call_site(),
        );
        let spec_const_ident = syn::Ident::new(
            &format!("__CANOPY_CMD_{}_SPEC", name_str.to_uppercase()),
            proc_macro2::Span::call_site(),
        );
        let names_const_ident = syn::Ident::new(
            &format!("__CANOPY_CMD_{}_USER_PARAMS", name_str.to_uppercase()),
            proc_macro2::Span::call_site(),
        );

        spec_refs.push(quote! { &Self::#spec_const_ident });

        let param_specs: Vec<proc_macro2::TokenStream> = cmd
            .params
            .iter()
            .filter_map(|param| {
                let kind_tokens = match param.kind {
                    ParamKind::Injected => {
                        quote! { canopy::commands::CommandParamKind::Injected }
                    }
                    ParamKind::User => {
                        quote! { canopy::commands::CommandParamKind::User }
                    }
                    ParamKind::Context { .. } => return None,
                };
                let name_lit = syn::LitStr::new(&param.name, proc_macro2::Span::call_site());
                let ty_lit = syn::LitStr::new(&param.ty_str, proc_macro2::Span::call_site());
                let optional = param.is_option || param.default.is_some();
                let default_tokens = if let Some(default) = &param.default {
                    let default_lit =
                        syn::LitStr::new(&default.display, proc_macro2::Span::call_site());
                    quote! { Some(#default_lit) }
                } else {
                    quote! { None }
                };

                Some(quote! {
                    canopy::commands::CommandParamSpec {
                        name: #name_lit,
                        kind: #kind_tokens,
                        ty: canopy::commands::CommandTypeSpec {
                            rust: #ty_lit,
                            doc: None,
                        },
                        optional: #optional,
                        default: #default_tokens,
                    }
                })
            })
            .collect();

        let user_names: Vec<syn::LitStr> = cmd
            .params
            .iter()
            .filter(|param| matches!(param.kind, ParamKind::User))
            .map(|param| syn::LitStr::new(&param.name, proc_macro2::Span::call_site()))
            .collect();

        let user_params: Vec<&ParamMeta> = cmd
            .params
            .iter()
            .filter(|param| matches!(param.kind, ParamKind::User))
            .collect();

        let max_allowed = user_params.len();
        let min_required = user_params
            .iter()
            .rposition(|param| !(param.is_option || param.default.is_some()))
            .map(|idx| idx + 1)
            .unwrap_or(0);

        let context_bindings: Vec<proc_macro2::TokenStream> = cmd
            .params
            .iter()
            .filter_map(|param| match param.kind {
                ParamKind::Context { mutable: false } => {
                    let ident = &param.ident;
                    Some(quote! { let #ident = &*ctx; })
                }
                ParamKind::Injected => {
                    let ident = &param.ident;
                    let ty = &param.ty;
                    let name_lit = syn::LitStr::new(&param.name, proc_macro2::Span::call_site());
                    Some(quote! {
                        let #ident: #ty = <#ty as canopy::commands::Inject>::inject(&*ctx)
                            .map_err(|err| match err {
                                canopy::commands::InjectError::Missing { expected } => {
                                    canopy::commands::CommandError::MissingInjected {
                                        param: #name_lit.to_string(),
                                        expected,
                                    }
                                }
                                canopy::commands::InjectError::Failed { expected, message } => {
                                    canopy::commands::CommandError::Conversion {
                                        param: #name_lit.to_string(),
                                        message,
                                    }
                                }
                            })?;
                    })
                }
                _ => None,
            })
            .collect();

        let mutable_context_bindings: Vec<proc_macro2::TokenStream> = cmd
            .params
            .iter()
            .filter_map(|param| match param.kind {
                ParamKind::Context { mutable: true } => {
                    let ident = &param.ident;
                    Some(quote! { let #ident = &mut *ctx; })
                }
                _ => None,
            })
            .collect();

        let positional_bindings: Vec<proc_macro2::TokenStream> = user_params
            .iter()
            .enumerate()
            .map(|(idx, param)| {
                let ident = &param.ident;
                let ty = &param.ty;
                let name_lit = syn::LitStr::new(&param.name, proc_macro2::Span::call_site());
                let default_expr = param.default.as_ref().map(|default| &default.expr);
                let missing_expr = if let Some(default_expr) = default_expr {
                    quote! { #default_expr }
                } else if param.is_option {
                    quote! { None }
                } else {
                    quote! {
                        return Err(canopy::commands::CommandError::ArityMismatch {
                            expected: expected_min,
                            got,
                        })
                    }
                };

                quote! {
                    let #ident: #ty = match values.get(#idx) {
                        Some(value) => {
                            <#ty as canopy::commands::FromArgValue>::from_arg_value(value)
                                .map_err(|err| err.with_param(#name_lit))?
                        }
                        None => #missing_expr,
                    };
                }
            })
            .collect();

        let named_bindings: Vec<proc_macro2::TokenStream> = user_params
            .iter()
            .map(|param| {
                let ident = &param.ident;
                let ty = &param.ty;
                let name_lit = syn::LitStr::new(&param.name, proc_macro2::Span::call_site());
                let default_expr = param.default.as_ref().map(|default| &default.expr);
                let missing_expr = if let Some(default_expr) = default_expr {
                    quote! { #default_expr }
                } else if param.is_option {
                    quote! { None }
                } else {
                    quote! {
                        return Err(canopy::commands::CommandError::MissingNamedArg {
                            name: #name_lit.to_string(),
                        })
                    }
                };

                quote! {
                    let #ident: #ty = match normalized.get(&canopy::commands::normalize_key(#name_lit)) {
                        Some(value) => {
                            <#ty as canopy::commands::FromArgValue>::from_arg_value(value)
                                .map_err(|err| err.with_param(#name_lit))?
                        }
                        None => #missing_expr,
                    };
                }
            })
            .collect();

        let call_args: Vec<syn::Ident> =
            cmd.params.iter().map(|param| param.ident.clone()).collect();

        let call_tokens = if cmd.ignore_result {
            if cmd.ret.is_result {
                quote! {
                    let _ = target.#name_ident(#(#call_args),*)
                        .map_err(|err| canopy::commands::CommandError::Exec(anyhow::Error::from(err)))?;
                    return Ok(canopy::commands::ArgValue::Null);
                }
            } else {
                quote! {
                    let _ = target.#name_ident(#(#call_args),*);
                    return Ok(canopy::commands::ArgValue::Null);
                }
            }
        } else {
            match &cmd.ret.kind {
                ReturnKind::Unit => {
                    if cmd.ret.is_result {
                        quote! {
                            let _ = target.#name_ident(#(#call_args),*)
                                .map_err(|err| canopy::commands::CommandError::Exec(anyhow::Error::from(err)))?;
                            return Ok(canopy::commands::ArgValue::Null);
                        }
                    } else {
                        quote! {
                            let _ = target.#name_ident(#(#call_args),*);
                            return Ok(canopy::commands::ArgValue::Null);
                        }
                    }
                }
                ReturnKind::Value { .. } => {
                    if cmd.ret.is_result {
                        quote! {
                            let value = target.#name_ident(#(#call_args),*)
                                .map_err(|err| canopy::commands::CommandError::Exec(anyhow::Error::from(err)))?;
                            return Ok(canopy::commands::ToArgValue::to_arg_value(value));
                        }
                    } else {
                        quote! {
                            let value = target.#name_ident(#(#call_args),*);
                            return Ok(canopy::commands::ToArgValue::to_arg_value(value));
                        }
                    }
                }
            }
        };

        let invoke_tokens = quote! {
            fn #invoke_ident(
                target: Option<&mut dyn ::std::any::Any>,
                ctx: &mut dyn canopy::Context,
                inv: &canopy::commands::CommandInvocation,
            ) -> ::std::result::Result<
                canopy::commands::ArgValue,
                canopy::commands::CommandError,
            >
            where
                Self: 'static,
            {
                let target = target
                    .and_then(|target| target.downcast_mut::<Self>())
                    .ok_or_else(|| canopy::commands::CommandError::Exec(
                        anyhow::anyhow!("command target type mismatch"),
                    ))?;
                #(#context_bindings)*
                match &inv.args {
                    canopy::commands::CommandArgs::Positional(values) => {
                        let got = values.len();
                        let expected_min = #min_required;
                        let expected_max = #max_allowed;
                        if got < expected_min || got > expected_max {
                            let expected = if got < expected_min { expected_min } else { expected_max };
                            return Err(canopy::commands::CommandError::ArityMismatch { expected, got });
                        }
                        #(#positional_bindings)*
                        #(#mutable_context_bindings)*
                        #call_tokens
                    }
                    canopy::commands::CommandArgs::Named(values) => {
                        let normalized = canopy::commands::normalize_named_args(
                            values,
                            Self::#names_const_ident,
                        )?;
                        #(#named_bindings)*
                        #(#mutable_context_bindings)*
                        #call_tokens
                    }
                }
            }
        };

        let params_const = quote! {
            const #params_const_ident: &'static [canopy::commands::CommandParamSpec] = &[
                #(#param_specs),*
            ];
        };

        let names_const = quote! {
            const #names_const_ident: &'static [&'static str] = &[
                #(#user_names),*
            ];
        };

        let ret_tokens = if cmd.ignore_result {
            quote! { canopy::commands::CommandReturnSpec::Unit }
        } else {
            match &cmd.ret.kind {
                ReturnKind::Unit => quote! { canopy::commands::CommandReturnSpec::Unit },
                ReturnKind::Value { ty_str } => {
                    let ty_lit = syn::LitStr::new(ty_str, proc_macro2::Span::call_site());
                    quote! {
                        canopy::commands::CommandReturnSpec::Value(
                            canopy::commands::CommandTypeSpec {
                                rust: #ty_lit,
                                doc: None,
                            }
                        )
                    }
                }
            }
        };

        let spec_const = quote! {
            const #spec_const_ident: canopy::commands::CommandSpec = canopy::commands::CommandSpec {
                id: canopy::commands::CommandId(#id_str),
                name: #name_str,
                dispatch: canopy::commands::CommandDispatchKind::Node { owner: #owner_str },
                params: Self::#params_const_ident,
                ret: #ret_tokens,
                invoke: Self::#invoke_ident,
            };
        };

        let cmd_name_ident =
            syn::Ident::new(&format!("cmd_{name_str}"), proc_macro2::Span::call_site());
        let cmd_ref = quote! {
            #[doc = "Return a typed command reference for this command."]
            pub fn #cmd_name_ident() -> &'static canopy::commands::CommandSpec {
                &Self::#spec_const_ident
            }
        };

        generated_items.push(parse_impl_item(names_const, "command user params const"));
        generated_items.push(parse_impl_item(params_const, "command params const"));
        generated_items.push(parse_impl_item(invoke_tokens, "command invoke fn"));
        generated_items.push(parse_impl_item(spec_const, "command spec const"));
        generated_items.push(parse_impl_item(cmd_ref, "command spec accessor"));
    }

    let commands_const_ident = syn::Ident::new("__CANOPY_COMMANDS", proc_macro2::Span::call_site());

    let commands_const = quote! {
        const #commands_const_ident: &'static [&'static canopy::commands::CommandSpec] = &[
            #(#spec_refs),*
        ];
    };
    generated_items.push(parse_impl_item(commands_const, "command list const"));

    input.items.extend(generated_items);

    let command_node_impl = quote! {
        impl #impl_generics canopy::commands::CommandNode for #name #where_clause {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {
                Self::#commands_const_ident
            }
        }
    };

    let output = quote! {
        #input
        #command_node_impl
    };

    output.into()
}

/// Mark a method as a command. This macro should be used to decorate methods in
/// an `impl` block that uses the `derive_commands` macro.
#[proc_macro_attribute]
pub fn command(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

/// Derive the CommandArg marker trait for serde-backed types.
#[proc_macro_derive(CommandArg)]
pub fn derive_command_arg(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let ident = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics canopy::commands::CommandArg for #ident #ty_generics #where_clause {}
    };

    expanded.into()
}

/// Derive command enum conversions from/to ArgValue.
#[proc_macro_derive(CommandEnum)]
pub fn derive_command_enum(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let ident = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let data = match input.data {
        syn::Data::Enum(data) => data,
        _ => abort_call_site!("CommandEnum can only be derived for enums"),
    };

    let mut variants = Vec::new();
    for variant in data.variants {
        if !variant.fields.is_empty() {
            abort!(
                variant.ident.span(),
                "CommandEnum only supports fieldless variants"
            );
        }
        variants.push(variant.ident);
    }

    let to_match_arms = variants.iter().map(|variant| {
        let name = variant.to_string();
        quote! { Self::#variant => #name }
    });

    let from_match_arms = variants.iter().map(|variant| {
        let name = variant.to_string();
        quote! { if value.eq_ignore_ascii_case(#name) { return Ok(Self::#variant); } }
    });

    let expanded = quote! {
        impl #impl_generics canopy::commands::ToArgValue for #ident #ty_generics #where_clause {
            fn to_arg_value(self) -> canopy::commands::ArgValue {
                let name = match self {
                    #(#to_match_arms,)*
                };
                canopy::commands::ArgValue::String(name.to_string())
            }
        }

        impl #impl_generics canopy::commands::FromArgValue for #ident #ty_generics #where_clause {
            fn from_arg_value(
                v: &canopy::commands::ArgValue,
            ) -> ::std::result::Result<Self, canopy::commands::CommandError> {
                let canopy::commands::ArgValue::String(value) = v else {
                    return Err(canopy::commands::CommandError::type_mismatch("String", v));
                };
                #(#from_match_arms)*
                Err(canopy::commands::CommandError::conversion(format!(
                    "unknown enum variant: {value}"
                )))
            }
        }
    };

    expanded.into()
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

    #[test]
    fn ignore_result_preserves_result_flag() {
        let mut method: syn::ImplItemFn = parse_quote! {
            #[command(ignore_result)]
            fn ignored(&mut self, _core: &mut dyn canopy::Context) -> Result<String> {
                Ok("ok".into())
            }
        };
        let cmd = parse_command_method("foo", &mut method).unwrap().unwrap();
        assert!(cmd.ignore_result);
        assert!(cmd.ret.is_result);
    }

    #[test]
    fn rejects_unsupported_reference_args() {
        let mut method: syn::ImplItemFn = parse_quote! {
            #[command]
            fn bad_ref(&mut self, _core: &mut dyn canopy::Context, name: &str) {}
        };
        let err = parse_command_method("foo", &mut method).unwrap_err();
        assert!(matches!(err, Error::Unsupported(_)));
    }
}
