use quote::quote;
use syn::{FnArg, GenericArgument, ImplItem, ItemImpl, PathArguments, ReturnType, Type};

use crate::{
    model::{CommandMeta, ParamKind, ParamMeta, ReturnKind, ReturnMeta, UserBindingSource},
    parse::{cfg_attributes, owner_name, parse_command_method},
};

/// Validate enabled hooks at their declarations so failures point to the
/// method that must change instead of generated adapter code.
fn validate_enabled_hooks(input: &ItemImpl, commands: &[CommandMeta]) -> syn::Result<()> {
    for command in commands {
        // The proc macro runs before cfg expansion, so a disabled command can
        // legitimately name a hook (and types) that do not exist in this
        // build. Active conditional commands are still checked by the
        // generated status adapter.
        if !command.cfg_attrs.is_empty() {
            continue;
        }
        let Some(enabled) = &command.enabled else {
            continue;
        };
        let Some(method) = input.items.iter().find_map(|item| match item {
            ImplItem::Fn(method) if method.sig.ident == *enabled => Some(method),
            _ => None,
        }) else {
            return Err(syn::Error::new_spanned(
                enabled,
                "enabled hook must name a method in the same impl",
            ));
        };

        let mut inputs = method.sig.inputs.iter();
        let Some(FnArg::Receiver(receiver)) = inputs.next() else {
            return Err(syn::Error::new_spanned(
                &method.sig,
                "enabled hook must take &self and &dyn ViewContext",
            ));
        };
        if receiver.reference.is_none() || receiver.mutability.is_some() {
            return Err(syn::Error::new_spanned(
                receiver,
                "enabled hook receiver must be &self",
            ));
        }
        let Some(FnArg::Typed(context)) = inputs.next() else {
            return Err(syn::Error::new_spanned(
                &method.sig,
                "enabled hook must take &dyn ViewContext after &self",
            ));
        };
        if inputs.next().is_some() || !is_immutable_view_context(&context.ty) {
            return Err(syn::Error::new_spanned(
                context,
                "enabled hook must take exactly one &dyn ViewContext argument",
            ));
        }
        if !is_command_status_result(&method.sig.output) {
            return Err(syn::Error::new_spanned(
                &method.sig.output,
                "enabled hook must return Result<CommandStatus>",
            ));
        }
    }
    Ok(())
}

/// Return whether a hook parameter is an immutable `dyn ViewContext` reference.
fn is_immutable_view_context(ty: &Type) -> bool {
    let Type::Reference(reference) = ty else {
        return false;
    };
    if reference.mutability.is_some() {
        return false;
    }
    let Type::TraitObject(object) = &*reference.elem else {
        return false;
    };
    object.bounds.iter().any(|bound| {
        let syn::TypeParamBound::Trait(bound) = bound else {
            return false;
        };
        bound
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "ViewContext")
    })
}

/// Return whether a hook output is a `Result` whose success type is
/// `CommandStatus`.
fn is_command_status_result(output: &ReturnType) -> bool {
    let ReturnType::Type(_, ty) = output else {
        return false;
    };
    let Type::Path(result) = &**ty else {
        return false;
    };
    let Some(segment) = result.path.segments.last() else {
        return false;
    };
    if segment.ident != "Result" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return false;
    };
    let Some(GenericArgument::Type(Type::Path(ok))) = args.args.first() else {
        return false;
    };
    ok.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "CommandStatus")
}

/// Render an `Option<&str>` metadata field from an optional string.
fn opt_str_tokens(value: Option<&str>) -> proc_macro2::TokenStream {
    match value {
        Some(value) => {
            let value = syn::LitStr::new(value, proc_macro2::Span::call_site());
            quote! { Some(#value) }
        }
        None => quote! { None },
    }
}

impl ParamMeta {
    /// Render this parameter name as a literal.
    fn name_lit(&self) -> syn::LitStr {
        syn::LitStr::new(&self.name, proc_macro2::Span::call_site())
    }

    /// Render this parameter type as a literal.
    fn ty_lit(&self) -> syn::LitStr {
        syn::LitStr::new(&self.ty_str, proc_macro2::Span::call_site())
    }

    /// Render command metadata for this parameter when it is externally
    /// visible.
    fn spec_tokens(&self) -> Option<proc_macro2::TokenStream> {
        let (kind_tokens, ty_tokens, decls_tokens) = match self.kind {
            ParamKind::Injected => (
                quote! { canopy::commands::CommandParamKind::Injected },
                quote! { <canopy::commands::ArgValue as canopy::commands::CommandType>::luau_ty },
                quote! { <canopy::commands::ArgValue as canopy::commands::CommandType>::luau_decls },
            ),
            ParamKind::User => {
                let ty = &self.ty;
                (
                    quote! { canopy::commands::CommandParamKind::User },
                    quote! { <#ty as canopy::commands::CommandType>::luau_ty },
                    quote! { <#ty as canopy::commands::CommandType>::luau_decls },
                )
            }
            ParamKind::Context { .. } => return None,
        };
        let requirement = if self.kind == ParamKind::Injected {
            let ty = &self.ty;
            quote! { Some(<#ty as canopy::commands::Inject>::requirement) }
        } else {
            quote! { None }
        };
        let name = self.name_lit();
        let ty = self.ty_lit();
        let optional = self.is_option;
        let doc = opt_str_tokens(self.doc.as_deref());

        Some(quote! {
            canopy::commands::CommandParamSpec {
                name: #name,
                kind: #kind_tokens,
                ty: canopy::commands::CommandTypeSpec {
                    rust: #ty,
                    ty: #ty_tokens,
                    decls: #decls_tokens,
                    doc: #doc,
                },
                optional: #optional,
                requirement: #requirement,
            }
        })
    }

    /// Render a shared context or injected binding.
    fn shared_binding_tokens(&self) -> Option<proc_macro2::TokenStream> {
        match self.kind {
            ParamKind::Context { mutable: false } => {
                let ident = &self.ident;
                Some(quote! { let #ident = &*ctx; })
            }
            ParamKind::Injected => {
                let ident = &self.ident;
                let ty = &self.ty;
                let name = self.name_lit();
                let ty_name = self.ty_lit();
                Some(quote! {
                    let #ident: #ty = <#ty as canopy::commands::Inject>::inject(&*ctx).ok_or(
                        canopy::commands::CommandError::MissingInjected {
                            param: #name.to_string(),
                            expected: #ty_name,
                        },
                    )?;
                })
            }
            _ => None,
        }
    }

    /// Render a mutable context binding.
    fn mutable_context_binding_tokens(&self) -> Option<proc_macro2::TokenStream> {
        match self.kind {
            ParamKind::Context { mutable: true } => {
                let ident = &self.ident;
                Some(quote! { let #ident = &mut *ctx; })
            }
            _ => None,
        }
    }

    /// Render the value used when this argument is omitted.
    fn missing_value_tokens(&self, source: UserBindingSource) -> proc_macro2::TokenStream {
        if self.is_option {
            return quote! { None };
        }

        let name = self.name_lit();
        match source {
            UserBindingSource::Positional(_) => quote! {
                return Err(canopy::commands::CommandError::ArityMismatch {
                    expected: expected_min,
                    got,
                })
            },
            UserBindingSource::Named => quote! {
                return Err(canopy::commands::CommandError::MissingNamedArg {
                    name: #name.to_string(),
                })
            },
        }
    }

    /// Render a user argument binding from the chosen argument source.
    fn user_binding_tokens(&self, source: UserBindingSource) -> proc_macro2::TokenStream {
        let ident = &self.ident;
        let ty = &self.ty;
        let name = self.name_lit();
        let missing = self.missing_value_tokens(source);

        match source {
            UserBindingSource::Positional(index) => quote! {
                let #ident: #ty = match values.get(#index) {
                    Some(value) => {
                        <#ty as canopy::commands::FromArgValue>::from_arg_value(value)
                            .map_err(|err| err.with_param(#name))?
                    }
                    None => #missing,
                };
            },
            UserBindingSource::Named => quote! {
                let #ident: #ty = match normalized.get(&canopy::commands::normalize_key(#name)) {
                    Some(value) => {
                        <#ty as canopy::commands::FromArgValue>::from_arg_value(value)
                            .map_err(|err| err.with_param(#name))?
                    }
                    None => #missing,
                };
            },
        }
    }
}

impl ReturnMeta {
    /// Render command metadata for this return type.
    fn spec_tokens(&self, ignore_result: bool) -> proc_macro2::TokenStream {
        if ignore_result {
            return quote! { canopy::commands::CommandReturnSpec::Unit };
        }

        match &self.kind {
            ReturnKind::Unit => quote! { canopy::commands::CommandReturnSpec::Unit },
            ReturnKind::Value { ty, ty_str } => {
                let ty_lit = syn::LitStr::new(ty_str, proc_macro2::Span::call_site());
                let doc = opt_str_tokens(self.doc.as_deref());
                quote! {
                    canopy::commands::CommandReturnSpec::Value(
                        canopy::commands::CommandTypeSpec {
                            rust: #ty_lit,
                            ty: <#ty as canopy::commands::CommandType>::luau_ty,
                            decls: <#ty as canopy::commands::CommandType>::luau_decls,
                            doc: #doc,
                        }
                    )
                }
            }
        }
    }

    /// Render the generated method call and command result conversion.
    fn call_tokens(
        &self,
        ignore_result: bool,
        target: &syn::Ident,
        method: &syn::Ident,
        args: &[syn::Ident],
    ) -> proc_macro2::TokenStream {
        let call = quote! { #target.#method(#(#args),*) };

        if ignore_result || matches!(self.kind, ReturnKind::Unit) {
            if self.is_result {
                quote! {
                    let _ = #call
                        .map_err(canopy::commands::CommandError::execution)?;
                    return Ok(canopy::commands::ArgValue::Null);
                }
            } else {
                quote! {
                    let _ = #call;
                    return Ok(canopy::commands::ArgValue::Null);
                }
            }
        } else if self.is_result {
            quote! {
                let value = #call
                    .map_err(canopy::commands::CommandError::execution)?;
                return Ok(canopy::commands::ToArgValue::to_arg_value(value));
            }
        } else {
            quote! {
                let value = #call;
                return Ok(canopy::commands::ToArgValue::to_arg_value(value));
            }
        }
    }
}

impl CommandMeta {
    /// Render this command name as an identifier.
    fn name_ident(&self) -> syn::Ident {
        syn::Ident::new(&self.name, proc_macro2::Span::call_site())
    }

    /// Identifier for the generated invoke shim.
    fn invoke_ident(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("__canopy_cmd_invoke_{}", self.name),
            proc_macro2::Span::call_site(),
        )
    }

    /// Identifier for the generated parameter spec constant.
    fn params_const_ident(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("__CANOPY_CMD_{}_PARAMS", self.name.to_uppercase()),
            proc_macro2::Span::call_site(),
        )
    }

    /// Identifier for the generated command spec constant.
    fn spec_const_ident(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("__CANOPY_CMD_{}_SPEC", self.name.to_uppercase()),
            proc_macro2::Span::call_site(),
        )
    }

    /// Identifier for the generated list of user parameter names.
    fn names_const_ident(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("__CANOPY_CMD_{}_USER_PARAMS", self.name.to_uppercase()),
            proc_macro2::Span::call_site(),
        )
    }

    /// Identifier for the typed command accessor.
    fn accessor_ident(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("cmd_{}", self.name),
            proc_macro2::Span::call_site(),
        )
    }

    /// Identifier for the erased read-only eligibility shim.
    fn status_ident(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("__canopy_cmd_status_{}", self.name),
            proc_macro2::Span::call_site(),
        )
    }

    /// Fully-qualified command identifier string.
    fn command_id(&self) -> String {
        format!("{}::{}", self.owner, self.name)
    }

    /// User-provided parameters in declaration order.
    fn user_params(&self) -> Vec<&ParamMeta> {
        self.params
            .iter()
            .filter(|param| matches!(param.kind, ParamKind::User))
            .collect()
    }

    /// Lower and upper positional arity bounds for this command.
    fn arity_bounds(&self) -> (usize, usize) {
        let user_params = self.user_params();
        let max_allowed = user_params.len();
        let min_required = user_params
            .iter()
            .rposition(|param| !param.is_option)
            .map_or(0, |idx| idx + 1);
        (min_required, max_allowed)
    }

    /// Metadata specs for all externally visible parameters.
    fn param_specs(&self) -> Vec<proc_macro2::TokenStream> {
        self.params
            .iter()
            .filter_map(ParamMeta::spec_tokens)
            .collect()
    }

    /// Normalized names for user-supplied parameters.
    fn user_param_names(&self) -> Vec<syn::LitStr> {
        self.user_params()
            .into_iter()
            .map(ParamMeta::name_lit)
            .collect()
    }

    /// Bindings that can happen before matching on argument shape.
    fn shared_bindings(&self) -> Vec<proc_macro2::TokenStream> {
        self.params
            .iter()
            .filter_map(ParamMeta::shared_binding_tokens)
            .collect()
    }

    /// Bindings that require mutable context access and must happen after user
    /// args are parsed.
    fn mutable_context_bindings(&self) -> Vec<proc_macro2::TokenStream> {
        self.params
            .iter()
            .filter_map(ParamMeta::mutable_context_binding_tokens)
            .collect()
    }

    /// User bindings for positional dispatch.
    fn positional_bindings(&self) -> Vec<proc_macro2::TokenStream> {
        self.user_params()
            .into_iter()
            .enumerate()
            .map(|(index, param)| param.user_binding_tokens(UserBindingSource::Positional(index)))
            .collect()
    }

    /// User bindings for named dispatch.
    fn named_bindings(&self) -> Vec<proc_macro2::TokenStream> {
        self.user_params()
            .into_iter()
            .map(|param| param.user_binding_tokens(UserBindingSource::Named))
            .collect()
    }

    /// Argument identifiers used to call the original command method.
    fn call_args(&self) -> Vec<syn::Ident> {
        self.params
            .iter()
            .map(|param| param.ident.clone())
            .collect()
    }

    /// Render a reference to this command's spec constant.
    fn spec_ref_tokens(&self) -> proc_macro2::TokenStream {
        let spec_const_ident = self.spec_const_ident();
        let cfg_attrs = &self.cfg_attrs;
        quote! { #(#cfg_attrs)* &Self::#spec_const_ident }
    }

    /// Render the generated invoke function for this command.
    fn invoke_tokens(&self) -> proc_macro2::TokenStream {
        let invoke_ident = self.invoke_ident();
        let names_const_ident = self.names_const_ident();
        let shared_bindings = self.shared_bindings();
        let mutable_context_bindings = self.mutable_context_bindings();
        let positional_bindings = self.positional_bindings();
        let named_bindings = self.named_bindings();
        let (min_required, max_allowed) = self.arity_bounds();
        let target_ident = syn::Ident::new("target", proc_macro2::Span::call_site());
        let method_ident = self.name_ident();
        let call_args = self.call_args();
        let call_tokens =
            self.ret
                .call_tokens(self.ignore_result, &target_ident, &method_ident, &call_args);

        quote! {
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
                let #target_ident = target
                    .and_then(|target| target.downcast_mut::<Self>())
                    .ok_or(canopy::commands::CommandError::TargetTypeMismatch)?;
                #(#shared_bindings)*
                match &inv.args {
                    canopy::commands::CommandArgs::Positional(values) => {
                        let got = values.len();
                        let expected_min = #min_required;
                        let expected_max = #max_allowed;
                        if got < expected_min || got > expected_max {
                            let expected = if got < expected_min {
                                expected_min
                            } else {
                                expected_max
                            };
                            return Err(canopy::commands::CommandError::ArityMismatch {
                                expected,
                                got,
                            });
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
        }
    }

    /// Render the generated user parameter name constant.
    fn names_const_tokens(&self) -> proc_macro2::TokenStream {
        let names_const_ident = self.names_const_ident();
        let user_names = self.user_param_names();
        quote! {
            const #names_const_ident: &'static [&'static str] = &[
                #(#user_names),*
            ];
        }
    }

    /// Render the generated parameter metadata constant.
    fn params_const_tokens(&self) -> proc_macro2::TokenStream {
        let params_const_ident = self.params_const_ident();
        let param_specs = self.param_specs();
        quote! {
            const #params_const_ident: &'static [canopy::commands::CommandParamSpec] = &[
                #(#param_specs),*
            ];
        }
    }

    /// Render the generated command spec constant.
    fn spec_const_tokens(&self) -> proc_macro2::TokenStream {
        let spec_const_ident = self.spec_const_ident();
        let params_const_ident = self.params_const_ident();
        let invoke_ident = self.invoke_ident();
        let id = self.command_id();
        let name = &self.name;
        let owner = &self.owner;
        let ret = self.ret.spec_tokens(self.ignore_result);
        let doc = opt_str_tokens(self.doc.as_deref());
        let status = if self.enabled.is_some() {
            let status_ident = self.status_ident();
            quote! { Some(Self::#status_ident) }
        } else {
            quote! { None }
        };

        quote! {
            const #spec_const_ident: canopy::commands::CommandSpec = canopy::commands::CommandSpec {
                id: canopy::commands::CommandId(#id),
                name: #name,
                dispatch: canopy::commands::CommandDispatchKind::Node { owner: #owner },
                params: Self::#params_const_ident,
                ret: #ret,
                doc: #doc,
                invoke: Self::#invoke_ident,
                status: #status,
            };
        }
    }

    /// Render the public typed command accessor.
    fn accessor_tokens(&self) -> proc_macro2::TokenStream {
        let accessor_ident = self.accessor_ident();
        let spec_const_ident = self.spec_const_ident();
        quote! {
            #[doc = "Return a typed command reference for this command."]
            pub fn #accessor_ident() -> &'static canopy::commands::CommandSpec {
                &Self::#spec_const_ident
            }
        }
    }

    /// Render a positional call builder with the original user parameter types.
    fn call_builder_tokens(&self) -> proc_macro2::TokenStream {
        let builder_ident = syn::Ident::new(
            &format!("call_{}", self.name),
            proc_macro2::Span::call_site(),
        );
        let accessor = self.accessor_ident();
        let params = self.user_params();
        let names: Vec<syn::Ident> = params
            .iter()
            .map(|param| syn::parse_str(&param.name).expect("parsed parameter identifier"))
            .collect();
        let types = params.iter().map(|param| &param.ty);
        quote! {
            #[doc = "Build a positional call with typed user arguments."]
            pub fn #builder_ident(#(#names: #types),*) -> canopy::commands::CommandCall {
                Self::#accessor().call_with(canopy::commands::CommandArgs::Positional(vec![
                    #(canopy::commands::ToArgValue::to_arg_value(#names)),*
                ]))
            }
        }
    }

    /// Render the checked immutable target adapter for an eligibility method.
    fn status_tokens(&self) -> proc_macro2::TokenStream {
        let Some(method) = &self.enabled else {
            return quote! {};
        };
        let status_ident = self.status_ident();
        quote! {
            fn #status_ident(
                target: &dyn ::std::any::Any,
                ctx: &dyn canopy::ViewContext,
            ) -> canopy::error::Result<canopy::commands::CommandStatus>
            where
                Self: 'static,
            {
                let target = target.downcast_ref::<Self>().ok_or_else(|| {
                    canopy::error::Error::Invalid("command status target type mismatch".into())
                })?;
                target.#method(ctx)
            }
        }
    }

    /// Render all generated impl items for this command.
    fn generated_items(&self) -> proc_macro2::TokenStream {
        let cfg_attrs = &self.cfg_attrs;
        [
            self.names_const_tokens(),
            self.params_const_tokens(),
            self.invoke_tokens(),
            self.spec_const_tokens(),
            self.accessor_tokens(),
            self.call_builder_tokens(),
            self.status_tokens(),
        ]
        .into_iter()
        .filter(|item| !item.is_empty())
        .map(|item| quote! { #(#cfg_attrs)* #item })
        .collect()
    }
}

/// Generate command metadata and wrappers for `#[command]` methods in an impl
/// block.
pub fn expand_derive_commands(input: &ItemImpl) -> syn::Result<proc_macro2::TokenStream> {
    let cfg_attrs = cfg_attributes(&input.attrs)?;
    let owner = owner_name(input)?;
    let name = input.self_ty.clone();
    let (impl_generics, _, where_clause) = input.generics.split_for_impl();

    let mut commands = Vec::new();
    for item in &input.items {
        if let ImplItem::Fn(method) = item
            && let Some(command) = parse_command_method(&owner, method)?
        {
            commands.push(command);
        }
    }
    validate_enabled_hooks(input, &commands)?;

    let mut generated = proc_macro2::TokenStream::new();
    let mut spec_refs = Vec::new();

    for command in &commands {
        spec_refs.push(command.spec_ref_tokens());
        generated.extend(command.generated_items());
    }

    let mut cleaned_input = input.clone();
    for item in &mut cleaned_input.items {
        if let ImplItem::Fn(method) = item {
            method.attrs.retain(|attr| !attr.path().is_ident("command"));
        }
    }

    let commands_const_ident = syn::Ident::new("__CANOPY_COMMANDS", proc_macro2::Span::call_site());
    generated.extend(quote! {
        const #commands_const_ident: &'static [&'static canopy::commands::CommandSpec] = &[
            #(#spec_refs),*
        ];
    });

    Ok(quote! {
        #cleaned_input

        #(#cfg_attrs)*
        impl #impl_generics #name #where_clause {
            #generated
        }

        #(#cfg_attrs)*
        impl #impl_generics canopy::commands::CommandNode for #name #where_clause {
            fn commands() -> &'static [&'static canopy::commands::CommandSpec] {
                Self::#commands_const_ident
            }
        }
    })
}
