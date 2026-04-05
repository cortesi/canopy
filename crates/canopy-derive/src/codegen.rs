use proc_macro_error::{ResultExt, abort_call_site};
use quote::quote;
use syn::{ImplItem, ItemImpl};

use crate::{
    model::{
        CommandMeta, DefaultValue, DocMeta, ParamKind, ParamMeta, ReturnKind, ReturnMeta,
        UserBindingSource,
    },
    parse::{owner_name, parse_command_method},
};

impl DefaultValue {
    /// Render this default value for metadata.
    fn metadata_tokens(&self) -> proc_macro2::TokenStream {
        let display = syn::LitStr::new(&self.display, proc_macro2::Span::call_site());
        quote! { Some(#display) }
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

    /// True when the caller may omit this parameter.
    fn is_optional_for_dispatch(&self) -> bool {
        self.is_option || self.default.is_some()
    }

    /// Render command metadata for this parameter when it is externally visible.
    fn spec_tokens(&self) -> Option<proc_macro2::TokenStream> {
        let kind_tokens = match self.kind {
            ParamKind::Injected => quote! { canopy::commands::CommandParamKind::Injected },
            ParamKind::User => quote! { canopy::commands::CommandParamKind::User },
            ParamKind::Context { .. } => return None,
        };
        let name = self.name_lit();
        let ty = self.ty_lit();
        let optional = self.is_optional_for_dispatch();
        let default = self
            .default
            .as_ref()
            .map_or_else(|| quote! { None }, DefaultValue::metadata_tokens);

        Some(quote! {
            canopy::commands::CommandParamSpec {
                name: #name,
                kind: #kind_tokens,
                ty: canopy::commands::CommandTypeSpec {
                    rust: #ty,
                    luau: None,
                    doc: None,
                },
                optional: #optional,
                default: #default,
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
                Some(quote! {
                    let #ident: #ty = <#ty as canopy::commands::Inject>::inject(&*ctx)
                        .map_err(|err| match err {
                            canopy::commands::InjectError::Missing { expected } => {
                                canopy::commands::CommandError::MissingInjected {
                                    param: #name.to_string(),
                                    expected,
                                }
                            }
                            canopy::commands::InjectError::Failed { message, .. } => {
                                canopy::commands::CommandError::Conversion {
                                    param: #name.to_string(),
                                    message,
                                }
                            }
                        })?;
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
        if let Some(default) = &self.default {
            let expr = &default.expr;
            return quote! { #expr };
        }
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
            ReturnKind::Value { ty_str } => {
                let ty = syn::LitStr::new(ty_str, proc_macro2::Span::call_site());
                quote! {
                    canopy::commands::CommandReturnSpec::Value(
                        canopy::commands::CommandTypeSpec {
                            rust: #ty,
                            luau: None,
                            doc: None,
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
                        .map_err(|err| canopy::commands::CommandError::Exec(anyhow::Error::from(err)))?;
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
                    .map_err(|err| canopy::commands::CommandError::Exec(anyhow::Error::from(err)))?;
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

impl DocMeta {
    /// Render an optional string field inside generated metadata.
    fn option_tokens(value: &Option<String>) -> proc_macro2::TokenStream {
        match value {
            Some(value) => {
                let value = syn::LitStr::new(value, proc_macro2::Span::call_site());
                quote! { Some(#value) }
            }
            None => quote! { None },
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
            .rposition(|param| !param.is_optional_for_dispatch())
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

    /// Bindings that require mutable context access and must happen after user args are parsed.
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
        quote! { &Self::#spec_const_ident }
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
                    .ok_or_else(|| canopy::commands::CommandError::Exec(
                        anyhow::anyhow!("command target type mismatch"),
                    ))?;
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
        let short = DocMeta::option_tokens(&self.doc.short);
        let long = DocMeta::option_tokens(&self.doc.long);
        let hidden = self.doc.hidden;

        quote! {
            const #spec_const_ident: canopy::commands::CommandSpec = canopy::commands::CommandSpec {
                id: canopy::commands::CommandId(#id),
                name: #name,
                dispatch: canopy::commands::CommandDispatchKind::Node { owner: #owner },
                params: Self::#params_const_ident,
                ret: #ret,
                doc: canopy::commands::CommandDocSpec {
                    short: #short,
                    long: #long,
                    hidden: #hidden,
                },
                invoke: Self::#invoke_ident,
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

    /// Render all generated impl items for this command.
    fn generated_items(&self) -> Vec<ImplItem> {
        vec![
            parse_impl_item(self.names_const_tokens(), "command user params const"),
            parse_impl_item(self.params_const_tokens(), "command params const"),
            parse_impl_item(self.invoke_tokens(), "command invoke fn"),
            parse_impl_item(self.spec_const_tokens(), "command spec const"),
            parse_impl_item(self.accessor_tokens(), "command spec accessor"),
        ]
    }
}

/// Parse generated tokens into an impl item with context on failure.
fn parse_impl_item(tokens: proc_macro2::TokenStream, label: &str) -> ImplItem {
    syn::parse2(tokens)
        .unwrap_or_else(|error| abort_call_site!("{} parse failed: {}", label, error))
}

/// Generate command metadata and wrappers for `#[command]` methods in an impl block.
pub fn expand_derive_commands(mut input: ItemImpl) -> proc_macro::TokenStream {
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

    let mut generated_items: Vec<ImplItem> = Vec::new();
    let mut spec_refs = Vec::new();

    for command in &commands {
        spec_refs.push(command.spec_ref_tokens());
        generated_items.extend(command.generated_items());
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

    quote! {
        #input
        #command_node_impl
    }
    .into()
}
