use std::collections::HashMap;

use convert_case::{Case, Casing};
use quote::ToTokens;
use syn::{
    Attribute, GenericArgument, ImplItemFn, ItemImpl, Meta, Pat, PathArguments, Result, ReturnType,
    Type, TypeParamBound, punctuated::Punctuated,
};

use crate::model::{CommandMeta, MacroArgs, ParamKind, ParamMeta, ReturnKind, ReturnMeta};

/// Extract normalized documentation text from `#[doc = "..."]` attributes.
pub fn doc_string(attrs: &[Attribute]) -> Option<String> {
    extract_doc_comments(attrs).0
}

/// Extract documentation from `#[doc = "..."]` attributes.
fn extract_doc_comments(
    attrs: &[Attribute],
) -> (Option<String>, HashMap<String, String>, Option<String>) {
    let mut lines = Vec::new();
    let mut param_docs = HashMap::new();
    let mut return_doc = None;
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
        return (None, param_docs, None);
    }

    let mut body = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@param ") {
            let mut parts = rest.splitn(2, char::is_whitespace);
            if let Some(name) = parts.next()
                && let Some(doc) = parts.next()
            {
                param_docs.insert(name.to_string(), doc.trim().to_string());
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("@return ") {
            return_doc = Some(rest.trim().to_string());
            continue;
        }
        body.push(line);
    }

    let long = Some(body.join("\n").trim().to_string()).filter(|text| !text.is_empty());
    (long, param_docs, return_doc)
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

/// Extract the success type from a `Result`-shaped path type.
pub fn extract_result_type(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    match args.args.first()? {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

/// Determine whether a type is a reference to a trait object with the given
/// trait name; returns whether the reference is mutable.
pub fn is_context_ref(ty: &Type, trait_name: &str) -> Option<bool> {
    let Type::Reference(reference) = ty else {
        return None;
    };
    let Type::TraitObject(obj) = &*reference.elem else {
        return None;
    };
    for bound in &obj.bounds {
        if let TypeParamBound::Trait(trait_bound) = bound
            && trait_bound.path.segments.last()?.ident == trait_name
        {
            return Some(reference.mutability.is_some());
        }
    }
    None
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

/// Parse command return metadata from a signature.
fn parse_return_type(output: &ReturnType, doc: Option<String>) -> ReturnMeta {
    let ReturnType::Type(_, ty) = output else {
        return ReturnMeta {
            is_result: false,
            kind: ReturnKind::Unit,
            doc,
        };
    };
    let (inner, is_result) = match extract_result_type(ty) {
        Some(inner) => (inner, true),
        None => (&**ty, false),
    };
    let kind = match inner {
        Type::Tuple(tuple) if tuple.elems.is_empty() => ReturnKind::Unit,
        _ => ReturnKind::Value {
            ty: Box::new(inner.clone()),
            ty_str: type_to_string(inner),
        },
    };
    ReturnMeta {
        is_result,
        kind,
        doc,
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
                    } else if meta.path.is_ident("enabled") {
                        if args.enabled.is_some() {
                            return Err(meta.error("duplicate enabled argument"));
                        }
                        let name: syn::LitStr = meta.value()?.parse()?;
                        args.enabled = Some(name.parse()?);
                    } else {
                        return Err(syn::Error::new_spanned(
                            meta.path,
                            "unknown command argument",
                        ));
                    }
                    Ok(())
                })?;
            }
            Meta::NameValue(_) => {
                return Err(syn::Error::new_spanned(attr, "invalid command argument"));
            }
        }
        macro_args = Some(args);
    }

    Ok(macro_args)
}

/// Ensure a command receiver is borrowed.
fn validate_receiver(receiver: &syn::Receiver) -> Result<()> {
    if receiver.reference.is_some() {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(
            receiver,
            "command methods must take &self or &mut self",
        ))
    }
}

/// Parse the identifier pattern used for a command argument.
fn parse_param_ident(pat: &Pat) -> Result<syn::Ident> {
    match pat {
        Pat::Ident(ident) => Ok(ident.ident.clone()),
        _ => Err(syn::Error::new_spanned(
            pat,
            "command arguments must be identifiers",
        )),
    }
}

/// Classify a non-context value parameter and validate its shape.
fn classify_value_param(ty: &Type) -> Result<(ParamKind, bool)> {
    let (inner, is_option) = if let Some(inner) = extract_single_generic(ty, "Option") {
        (inner, true)
    } else {
        (ty, false)
    };

    if matches!(inner, Type::Reference(_)) {
        return Err(syn::Error::new_spanned(
            ty,
            "reference arguments are not supported",
        ));
    }

    let kind = if is_builtin_injected(inner) {
        ParamKind::Injected
    } else {
        ParamKind::User
    };

    Ok((kind, is_option))
}

/// Parse a typed argument from a command method signature.
fn parse_command_param(pat: &syn::PatType, index: usize) -> Result<ParamMeta> {
    let user_ident = parse_param_ident(&pat.pat)?;
    let name = user_ident.to_string();
    let ident = syn::Ident::new(
        &format!("__canopy_param_{index}"),
        proc_macro2::Span::mixed_site(),
    );
    let ty = (*pat.ty).clone();
    let ty_str = type_to_string(&ty);

    if let Some(mutable) = is_context_ref(&ty, "Context") {
        return Ok(ParamMeta {
            ident,
            user_ident,
            name,
            ty,
            ty_str,
            kind: ParamKind::Context { mutable },
            is_option: false,
            doc: None,
        });
    }

    let (kind, is_option) = classify_value_param(&ty)?;

    Ok(ParamMeta {
        ident,
        user_ident,
        name,
        ty,
        ty_str,
        kind,
        is_option,
        doc: None,
    })
}

/// Parse an impl method annotated with `#[command]`.
pub fn parse_command_method(owner: &str, method: &ImplItemFn) -> Result<Option<CommandMeta>> {
    let Some(macro_args) = parse_command_macro_args(&method.attrs)? else {
        return Ok(None);
    };
    let (doc, param_docs, return_doc) = extract_doc_comments(&method.attrs);

    let mut params = Vec::new();
    let mut has_receiver = false;

    for input in &method.sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                has_receiver = true;
                validate_receiver(receiver)?;
            }
            syn::FnArg::Typed(pat) => params.push(parse_command_param(pat, params.len())?),
        }
    }

    if !has_receiver {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "command methods must take &self or &mut self",
        ));
    }

    for param in &mut params {
        param.doc = param_docs.get(&param.name).cloned();
    }

    let ret = parse_return_type(&method.sig.output, return_doc);

    Ok(Some(CommandMeta {
        cfg_attrs: cfg_attributes(&method.attrs)?,
        name: method.sig.ident.to_string(),
        owner: owner.to_string(),
        params,
        ignore_result: macro_args.ignore_result,
        enabled: macro_args.enabled,
        ret,
        doc,
    }))
}

/// Retain compilation gates without copying unrelated conditional attributes.
pub fn cfg_attributes(attrs: &[Attribute]) -> Result<Vec<Attribute>> {
    attrs
        .iter()
        .filter_map(|attr| match cfg_meta(&attr.meta) {
            Ok(Some(meta)) => Some(Ok(syn::parse_quote!(#[#meta]))),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

/// Extract nested `cfg` gates from a conditional attribute.
fn cfg_meta(meta: &Meta) -> Result<Option<Meta>> {
    if meta.path().is_ident("cfg") {
        return Ok(Some(meta.clone()));
    }
    if !meta.path().is_ident("cfg_attr") {
        return Ok(None);
    }
    let list = meta.require_list()?;
    let parts = list.parse_args_with(Punctuated::<Meta, syn::Token![,]>::parse_terminated)?;
    let mut parts = parts.into_iter();
    let Some(predicate) = parts.next() else {
        return Err(syn::Error::new_spanned(
            meta,
            "cfg_attr requires a predicate",
        ));
    };
    let mut gates = Vec::new();
    for part in parts {
        if let Some(gate) = cfg_meta(&part)? {
            gates.push(gate);
        }
    }
    Ok((!gates.is_empty()).then(|| syn::parse_quote!(cfg_attr(#predicate, #(#gates),*))))
}

/// Resolve the owner type name for an impl block.
pub fn owner_name(input: &ItemImpl) -> Result<String> {
    let Type::Path(path) = &*input.self_ty else {
        return Err(syn::Error::new_spanned(
            &input.self_ty,
            "unsupported impl type",
        ));
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(syn::Error::new_spanned(
            &input.self_ty,
            "unsupported impl type",
        ));
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

    use super::{extract_result_type, extract_single_generic, parse_command_method};

    #[test]
    fn result_success_types() {
        for ty in [
            parse_quote!(Result<()>),
            parse_quote!(Result<String>),
            parse_quote!(std::result::Result<(), Error>),
            parse_quote!(std::result::Result<String, Error>),
            parse_quote!(Result<String, Error, Extra>),
        ] {
            assert!(extract_result_type(&ty).is_some());
        }
        for ty in [
            parse_quote!(Result),
            parse_quote!(Result<'a>),
            parse_quote!(Option<String>),
        ] {
            assert!(extract_result_type(&ty).is_none());
        }
        assert!(extract_single_generic(&parse_quote!(Option<String, Error>), "Option").is_none());
    }

    #[test]
    fn ignore_result_preserves_result_flag() {
        let method: syn::ImplItemFn = parse_quote! {
            #[command(ignore_result)]
            fn ignored(&mut self, _core: &mut dyn canopy::Context) -> Result<String> {
                Ok("ok".into())
            }
        };
        let cmd = parse_command_method("foo", &method).unwrap().unwrap();
        assert!(cmd.ignore_result);
        assert!(cmd.ret.is_result);
    }

    #[test]
    fn parses_enabled_hook_with_ignore_result() {
        let method: syn::ImplItemFn = parse_quote! {
            #[command(enabled = "can_update", ignore_result)]
            fn update(&mut self) {}
        };
        let cmd = parse_command_method("foo", &method).unwrap().unwrap();
        assert_eq!(cmd.enabled.unwrap(), "can_update");
        assert!(cmd.ignore_result);
    }

    #[test]
    fn rejects_invalid_or_duplicate_enabled_hooks() {
        for method in [
            parse_quote! {
                #[command(enabled = "self.can_update")]
                fn update(&mut self) {}
            },
            parse_quote! {
                #[command(enabled = "can_update", enabled = "other")]
                fn update(&mut self) {}
            },
            parse_quote! {
                #[command(enabled = true)]
                fn update(&mut self) {}
            },
        ] {
            assert!(parse_command_method("foo", &method).is_err());
        }
    }

    #[test]
    fn rejects_unsupported_reference_args() {
        let method: syn::ImplItemFn = parse_quote! {
            #[command]
            fn bad_ref(&mut self, _core: &mut dyn canopy::Context, name: &str) {}
        };
        let err = parse_command_method("foo", &method).unwrap_err();
        assert_eq!(err.to_string(), "reference arguments are not supported");
    }
}
