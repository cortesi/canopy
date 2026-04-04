use convert_case::{Case, Casing};
use quote::ToTokens;
use syn::{
    Attribute, GenericArgument, ImplItemFn, ItemImpl, Meta, Pat, PathArguments, ReturnType, Type,
    TypeParamBound,
};

use crate::{
    error::{Error, Result},
    model::{
        CommandMeta, DefaultValue, DocMeta, MacroArgs, ParamKind, ParamMeta, ReturnKind, ReturnMeta,
    },
};

/// Extract documentation from `#[doc = "..."]` attributes.
fn extract_doc_comments(attrs: &[Attribute]) -> (Option<String>, Option<String>) {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(name_value) = &attr.meta
            && let syn::Expr::Lit(expr_lit) = &name_value.value
            && let syn::Lit::Str(value) = &expr_lit.lit
        {
            let text = value.value();
            if text.trim().is_empty() {
                if !lines.is_empty() {
                    lines.push(String::new());
                }
            } else {
                lines.push(text.trim().to_string());
            }
        }
    }

    if lines.is_empty() {
        return (None, None);
    }

    let long = Some(lines.join("\n").trim().to_string()).filter(|text| !text.is_empty());
    let first_line = lines.iter().find(|line| !line.is_empty()).cloned();
    let short = first_line.map(|line| {
        if let Some(index) = line.find(". ") {
            format!("{}.", &line[..index])
        } else {
            line
        }
    });

    (short, long)
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
                .map_err(|error| Error::Parse(error.to_string()))?;
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

/// Parse the `#[command(...)]` attribute, if present.
fn parse_command_macro_args(attrs: &[Attribute]) -> Result<Option<MacroArgs>> {
    let mut macro_args = None;

    for attr in attrs {
        if !attr.path().is_ident("command") {
            continue;
        }

        let mut args = MacroArgs::default();
        match &attr.meta {
            Meta::Path(_) => {}
            Meta::List(_) => {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("ignore_result") {
                        args.ignore_result = true;
                    } else if meta.path.is_ident("hidden") {
                        args.hidden = true;
                    } else if meta.path.is_ident("desc") {
                        let value = meta.value()?;
                        args.desc = Some(value.parse()?);
                    } else {
                        return Err(syn::Error::new_spanned(
                            meta.path,
                            "unknown command argument",
                        ));
                    }
                    Ok(())
                })
                .map_err(|error| Error::Parse(error.to_string()))?;
            }
            Meta::NameValue(_) => {
                return Err(Error::Parse("invalid command argument".into()));
            }
        }
        macro_args = Some(args);
    }

    Ok(macro_args)
}

/// Build documentation metadata for a parsed command method.
fn build_doc_meta(attrs: &[Attribute], macro_args: &MacroArgs) -> DocMeta {
    let (short, long) = extract_doc_comments(attrs);
    DocMeta {
        short: macro_args.desc.as_ref().map(syn::LitStr::value).or(short),
        long,
        hidden: macro_args.hidden,
    }
}

/// Ensure a command receiver is borrowed.
fn validate_receiver(receiver: &syn::Receiver) -> Result<()> {
    if receiver.reference.is_some() {
        Ok(())
    } else {
        Err(Error::Unsupported(
            "command methods must take &self or &mut self".into(),
        ))
    }
}

/// Parse the identifier pattern used for a command argument.
fn parse_param_ident(pat: &Pat) -> Result<syn::Ident> {
    match pat {
        Pat::Ident(ident) => Ok(ident.ident.clone()),
        _ => Err(Error::Unsupported(
            "command arguments must be identifiers".into(),
        )),
    }
}

/// Classify a non-context value parameter and validate its shape.
fn classify_value_param(ty: &Type, default: Option<&DefaultValue>) -> Result<(ParamKind, bool)> {
    let (inner, is_option) = if let Some(inner) = extract_single_generic(ty, "Option") {
        (inner, true)
    } else {
        (ty, false)
    };

    if matches!(inner, Type::Reference(_)) {
        return Err(Error::Unsupported(
            "reference arguments are not supported".into(),
        ));
    }

    let kind = if extract_single_generic(inner, "Arg").is_some() {
        ParamKind::User
    } else if extract_single_generic(inner, "Injected").is_some() || is_builtin_injected(inner) {
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

    Ok((kind, is_option))
}

/// Parse a typed argument from a command method signature.
fn parse_command_param(pat: &mut syn::PatType) -> Result<ParamMeta> {
    let ident = parse_param_ident(&pat.pat)?;
    let name = ident.to_string();
    let default = parse_arg_default(&pat.attrs)?;
    pat.attrs.retain(|attr| !attr.path().is_ident("arg"));
    let ty = (*pat.ty).clone();
    let ty_str = type_to_string(&ty);

    if let Some(mutable) = is_context_ref(&ty) {
        if default.is_some() {
            return Err(Error::Unsupported(
                "context parameters cannot have defaults".into(),
            ));
        }

        return Ok(ParamMeta {
            ident,
            name,
            ty,
            ty_str,
            kind: ParamKind::Context { mutable },
            is_option: false,
            default: None,
        });
    }

    let (kind, is_option) = classify_value_param(&ty, default.as_ref())?;

    Ok(ParamMeta {
        ident,
        name,
        ty,
        ty_str,
        kind,
        is_option,
        default,
    })
}

/// Parse an impl method annotated with `#[command]`.
pub fn parse_command_method(owner: &str, method: &mut ImplItemFn) -> Result<Option<CommandMeta>> {
    let Some(macro_args) = parse_command_macro_args(&method.attrs)? else {
        return Ok(None);
    };
    let doc = build_doc_meta(&method.attrs, &macro_args);

    let mut params = Vec::new();
    let mut has_receiver = false;

    for input in &mut method.sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                has_receiver = true;
                validate_receiver(receiver)?;
            }
            syn::FnArg::Typed(pat) => params.push(parse_command_param(pat)?),
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
        doc,
    }))
}

/// Resolve the owner type name for an impl block.
pub fn owner_name(input: &ItemImpl) -> Result<String> {
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

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::parse_command_method;
    use crate::error::Error;

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
