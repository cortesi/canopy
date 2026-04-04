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
use syn::{ItemImpl, parse_macro_input};

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
