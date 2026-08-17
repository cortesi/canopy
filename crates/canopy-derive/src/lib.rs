#![deny(unsafe_code)]
#![warn(missing_docs)]
//! Proc-macro support for canopy commands and nodes.

/// Command metadata token emission.
mod codegen;
/// Parsed command metadata model.
mod model;
/// Parsing support for `derive_commands`.
mod parse;

use quote::quote;
use syn::{Attribute, Fields, ItemImpl, Result, parse_macro_input, parse_quote};

/// Generate command metadata and wrappers for `#[command]` methods in an impl block.
#[proc_macro_attribute]
pub fn derive_commands(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as ItemImpl);
    match codegen::expand_derive_commands(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
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
    match expand_command_arg(&input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Expand the `CommandArg` derive for one input.
fn expand_command_arg(input: &syn::DeriveInput) -> Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let type_name = syn::LitStr::new(&ident.to_string(), ident.span());
    let type_doc = doc_tokens(&input.attrs);
    let fields = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    ident,
                    "CommandArg only supports named-field structs",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                ident,
                "CommandArg can only be derived for structs",
            ));
        }
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
    let mut field_tokens = Vec::new();
    for field in fields {
        let Some(ident) = &field.ident else {
            return Err(syn::Error::new_spanned(
                field,
                "CommandArg only supports named-field structs",
            ));
        };
        let name = ident.to_string();
        let name = syn::LitStr::new(&name, ident.span());
        let ty = &field.ty;
        let doc = doc_tokens(&field.attrs);
        field_tokens.push(quote! {
            canopy::commands::declaration::Field::new(
                #name,
                <#ty as canopy::commands::CommandType>::luau_ty(),
            )
            #doc
        });
    }

    Ok(quote! {
        impl #impl_generics canopy::commands::CommandArg for #ident #ty_generics #where_clause {}

        impl #impl_generics canopy::commands::CommandType for #ident #ty_generics #where_clause {
            fn luau_ty() -> canopy::commands::declaration::Type {
                canopy::commands::declaration::Type::named(#type_name)
            }

            fn luau_decls(registry: &mut canopy::commands::DeclRegistry<'_>) {
                if !registry.begin(#type_name) {
                    return;
                }
                #(#field_decl_regs)*
                registry.alias(
                    canopy::commands::declaration::Alias::new(
                        #type_name,
                        canopy::commands::declaration::Type::table([#(#field_tokens),*]),
                    )
                    #type_doc,
                );
            }
        }
    })
}

/// Render a doc-attachment token stream for declaration model items.
fn doc_tokens(attrs: &[Attribute]) -> proc_macro2::TokenStream {
    match parse::doc_string(attrs) {
        Some(doc) => {
            let doc = syn::LitStr::new(&doc, proc_macro2::Span::call_site());
            quote! { .doc(#doc) }
        }
        None => quote! {},
    }
}

/// Derive command enum conversions from/to ArgValue.
#[proc_macro_derive(CommandEnum)]
pub fn derive_command_enum(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    match expand_command_enum(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Expand the `CommandEnum` derive for one input.
fn expand_command_enum(input: syn::DeriveInput) -> Result<proc_macro2::TokenStream> {
    let ident = input.ident;
    let type_name = syn::LitStr::new(&ident.to_string(), ident.span());
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let syn::Data::Enum(data) = input.data else {
        return Err(syn::Error::new_spanned(
            &ident,
            "CommandEnum can only be derived for enums",
        ));
    };

    let mut variants = Vec::new();
    for variant in data.variants {
        if !variant.fields.is_empty() {
            return Err(syn::Error::new_spanned(
                &variant.ident,
                "CommandEnum only supports fieldless variants",
            ));
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

    Ok(quote! {
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

        impl #impl_generics canopy::commands::CommandType for #ident #ty_generics #where_clause {
            fn luau_ty() -> canopy::commands::declaration::Type {
                canopy::commands::declaration::Type::named(#type_name)
            }

            fn luau_decls(registry: &mut canopy::commands::DeclRegistry<'_>) {
                if !registry.begin(#type_name) {
                    return;
                }
                registry.alias(canopy::commands::declaration::Alias::new(
                    #type_name,
                    canopy::commands::declaration::Type::literals([#(#luau_values),*]),
                ));
            }
        }
    })
}
