//! Proc-macro support for canopy commands and nodes.

/// Command metadata token emission.
mod codegen;
/// Local error type for derive parsing.
mod error;
/// Parsed command metadata model.
mod model;
/// Parsing support for `derive_commands`.
mod parse;

use proc_macro_error::{abort, abort_call_site, proc_macro_error};
use quote::quote;
use syn::{
    Attribute, Expr, ExprLit, Fields, ItemImpl, Lit, Meta, parse_macro_input, parse_quote,
    spanned::Spanned,
};

/// Generate command metadata and wrappers for `#[command]` methods in an impl block.
#[proc_macro_error]
#[proc_macro_attribute]
pub fn derive_commands(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as ItemImpl);
    codegen::expand_derive_commands(input)
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
#[proc_macro_derive(CommandArg, attributes(canopy))]
pub fn derive_command_arg(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let ident = &input.ident;
    let type_name = command_arg_type_name(&input.attrs, ident);
    let type_doc = doc_tokens(&input.attrs);
    let fields = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => abort!(ident.span(), "CommandArg only supports named-field structs"),
        },
        _ => abort!(ident.span(), "CommandArg can only be derived for structs"),
    };
    let mut generics = input.generics.clone();
    // Field-type bounds are only needed to propagate generic parameters; for
    // concrete structs they would turn self-referential fields into cyclic
    // trait obligations, so they are omitted there.
    if !generics.params.is_empty() {
        let where_clause = generics.make_where_clause();
        for field in fields {
            let ty = &field.ty;
            where_clause
                .predicates
                .push(parse_quote!(#ty: canopy::commands::CommandType));
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let field_decl_regs = fields.iter().map(|field| {
        let ty = &field.ty;
        quote! { <#ty as canopy::commands::CommandType>::luau_decls(registry); }
    });
    let field_tokens = fields.iter().map(|field| {
        let Some(ident) = &field.ident else {
            abort!(field, "CommandArg only supports named-field structs");
        };
        let name = ident.to_string();
        let name = syn::LitStr::new(&name, ident.span());
        let ty = &field.ty;
        let doc = doc_tokens(&field.attrs);
        quote! {
            canopy::commands::decl::Field::new(
                #name,
                <#ty as canopy::commands::CommandType>::luau_ty(),
            )
            #doc
        }
    });

    let expanded = quote! {
        impl #impl_generics canopy::commands::CommandArg for #ident #ty_generics #where_clause {}

        impl #impl_generics canopy::commands::CommandType for #ident #ty_generics #where_clause {
            fn luau_ty() -> canopy::commands::decl::Ty {
                canopy::commands::decl::Ty::named(#type_name)
            }

            fn luau_decls(registry: &mut canopy::commands::DeclRegistry<'_>) {
                if !registry.begin(#type_name) {
                    return;
                }
                #(#field_decl_regs)*
                registry.alias(
                    canopy::commands::decl::Alias::new(
                        #type_name,
                        canopy::commands::decl::Ty::table([#(#field_tokens),*]),
                    )
                    #type_doc,
                );
            }
        }
    };

    expanded.into()
}

/// Return the explicit `#[canopy(type_name = "...")]` or the Rust identifier.
fn command_arg_type_name(attrs: &[Attribute], ident: &syn::Ident) -> syn::LitStr {
    let mut type_name = None;
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("canopy")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("type_name") {
                let value = meta.value()?;
                let value: syn::LitStr = value.parse()?;
                type_name = Some(value);
                Ok(())
            } else {
                Err(meta.error("unsupported canopy attribute"))
            }
        })
        .unwrap_or_else(|err| abort!(attr.span(), err));
    }
    type_name.unwrap_or_else(|| syn::LitStr::new(&ident.to_string(), ident.span()))
}

/// Render a doc-attachment token stream for declaration model items.
fn doc_tokens(attrs: &[Attribute]) -> proc_macro2::TokenStream {
    match doc_string(attrs) {
        Some(doc) => {
            let doc = syn::LitStr::new(&doc, proc_macro2::Span::call_site());
            quote! { .doc(#doc) }
        }
        None => quote! {},
    }
}

/// Extract normalized Rust doc comments.
fn doc_string(attrs: &[Attribute]) -> Option<String> {
    let lines = attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") {
                return None;
            }
            let Meta::NameValue(name_value) = &attr.meta else {
                return None;
            };
            let Expr::Lit(ExprLit {
                lit: Lit::Str(value),
                ..
            }) = &name_value.value
            else {
                return None;
            };
            Some(value.value().trim().to_string())
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Derive command enum conversions from/to ArgValue.
#[proc_macro_derive(CommandEnum, attributes(canopy))]
pub fn derive_command_enum(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let ident = input.ident;
    let type_name = command_arg_type_name(&input.attrs, &ident);
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

    let luau_values = variants
        .iter()
        .map(|variant| syn::LitStr::new(&variant.to_string(), proc_macro2::Span::call_site()));

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

        impl #impl_generics #ident #ty_generics #where_clause {
            /// Luau literal values for this command enum.
            pub const LUAU_VALUES: &'static [&'static str] = &[#(#luau_values),*];
        }

        impl #impl_generics canopy::commands::CommandType for #ident #ty_generics #where_clause {
            fn luau_ty() -> canopy::commands::decl::Ty {
                canopy::commands::decl::Ty::named(#type_name)
            }

            fn luau_decls(registry: &mut canopy::commands::DeclRegistry<'_>) {
                if !registry.begin(#type_name) {
                    return;
                }
                registry.alias(canopy::commands::decl::Alias::new(
                    #type_name,
                    canopy::commands::decl::Ty::literals(Self::LUAU_VALUES.iter().copied()),
                ));
            }
        }
    };

    expanded.into()
}
