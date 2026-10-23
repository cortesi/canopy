#![deny(unsafe_code)]
//! Derive macros for `canopy-options`.
//!
//! `#[derive(OptionGroup)]` declares a struct of options, and
//! `#[derive(OptionEnum)]` declares the variants of a choice. The generated
//! code names items in `::canopy_options`, so a crate that uses the derives
//! depends on that crate directly.

use std::{collections::HashMap, mem};

use convert_case::{Case, Casing};
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote, quote_spanned};
use syn::{
    Attribute, Data, DeriveInput, Expr, Fields, Ident, Lit, LitStr, Result, UnOp,
    parse_macro_input, spanned::Spanned,
};

/// Keys that the options panel uses for its own actions.
///
/// Keep this list in step with `canopy_options::RESERVED_KEYS`.
const RESERVED_KEYS: [char; 6] = ['h', 'j', 'k', 'l', 'R', 'S'];

/// Derive an option group for a struct with named fields.
///
/// Each field is an option or a nested group. The derive implements
/// `Default` from the `default = expr` attributes of the fields, and
/// implements `canopy_options::OptionGroup` and
/// `canopy_options::OptionField`. A doc comment on a field becomes its help
/// text.
///
/// Field attributes, inside `#[option(...)]`:
///
/// - `key = 'c'`: the accelerator of the field in its panel.
/// - `label = "text"`: the text that the panel shows.
/// - `default = expr`: the default value.
/// - `min = n`, `max = n`, `step = n`: the limits of an integer.
/// - `global`: the option never reads from a local file.
/// - `skip`: the field is not an option.
/// - `flatten`: the items of a group field, its leaves and its groups, become
///   items of the parent. A flattened field takes no other attribute but
///   `default`.
/// - `inline`: the panel shows the items of a group field in its parent, while
///   their paths keep the group's name.
/// - `validate = path`: a `fn(&T) -> Result<(), String>` check of a leaf.
#[proc_macro_derive(OptionGroup, attributes(option))]
pub fn derive_option_group(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_group(&input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Derive the variants of a choice for a fieldless enum.
///
/// Each variant crosses files, scripts, and the panel as its `snake_case`
/// name. Variant attributes, inside `#[option(...)]`, are `key = 'c'` and
/// `label = "text"`. The derive implements `canopy_options::OptionEnum`,
/// `canopy_options::LeafValue`, and `canopy_options::OptionField`.
#[proc_macro_derive(OptionEnum, attributes(option))]
pub fn derive_option_enum(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_enum(&input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// The parsed `#[option(...)]` attributes of one field or variant.
#[derive(Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each flag is one independent attribute"
)]
struct OptionAttrs {
    /// Accelerator key.
    key: Option<(char, Span)>,
    /// Display label.
    label: Option<String>,
    /// Default value expression.
    default: Option<Expr>,
    /// Integer lower limit.
    min: Option<i64>,
    /// Integer upper limit.
    max: Option<i64>,
    /// Integer step.
    step: Option<i64>,
    /// Whether local files may not set the option.
    global: bool,
    /// Whether the field is not an option.
    skip: bool,
    /// Whether the field's groups join the parent.
    flatten: bool,
    /// Whether the panel shows the group's items in its parent.
    inline: bool,
    /// Validation function path.
    validate: Option<syn::Path>,
}

/// Parse the `#[option(...)]` attributes in `attrs`.
fn parse_attrs(attrs: &[Attribute]) -> Result<OptionAttrs> {
    let mut out = OptionAttrs::default();
    for attr in attrs {
        if !attr.path().is_ident("option") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            let path = &meta.path;
            if path.is_ident("key") {
                let lit: syn::LitChar = meta.value()?.parse()?;
                out.key = Some((lit.value(), lit.span()));
            } else if path.is_ident("label") {
                let lit: LitStr = meta.value()?.parse()?;
                out.label = Some(lit.value());
            } else if path.is_ident("default") {
                out.default = Some(meta.value()?.parse()?);
            } else if path.is_ident("min") {
                out.min = Some(int_value(&meta.value()?.parse()?)?);
            } else if path.is_ident("max") {
                out.max = Some(int_value(&meta.value()?.parse()?)?);
            } else if path.is_ident("step") {
                out.step = Some(int_value(&meta.value()?.parse()?)?);
            } else if path.is_ident("global") {
                out.global = true;
            } else if path.is_ident("skip") {
                out.skip = true;
            } else if path.is_ident("flatten") {
                out.flatten = true;
            } else if path.is_ident("inline") {
                out.inline = true;
            } else if path.is_ident("validate") {
                out.validate = Some(meta.value()?.parse()?);
            } else {
                return Err(meta.error("unknown option attribute"));
            }
            Ok(())
        })?;
    }
    if let (Some(min), Some(max)) = (out.min, out.max)
        && min > max
    {
        return Err(syn::Error::new(Span::call_site(), "min is above max"));
    }
    if out.step.is_some_and(|step| step <= 0) {
        return Err(syn::Error::new(Span::call_site(), "step must be positive"));
    }
    Ok(out)
}

/// Read an integer literal, which may be negative.
fn int_value(expr: &Expr) -> Result<i64> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(int) => int.base10_parse(),
            other => Err(syn::Error::new_spanned(other, "expected an integer")),
        },
        Expr::Unary(unary) if matches!(unary.op, UnOp::Neg(_)) => {
            int_value(&unary.expr).map(|value| -value)
        }
        other => Err(syn::Error::new_spanned(other, "expected an integer")),
    }
}

/// Return the doc comment text of `attrs`, as one line per paragraph.
fn doc_text(attrs: &[Attribute]) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(name_value) = &attr.meta
            && let Expr::Lit(expr_lit) = &name_value.value
            && let Lit::Str(value) = &expr_lit.lit
        {
            let line = value.value();
            let line = line.trim();
            if line.is_empty() {
                if !current.is_empty() {
                    paragraphs.push(mem::take(&mut current));
                }
            } else {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(line);
            }
        }
    }
    if !current.is_empty() {
        paragraphs.push(current);
    }
    paragraphs.join("\n")
}

/// Return a label from an identifier, in lower case with spaces between its
/// words.
fn label_of(ident: &str) -> String {
    ident.to_case(Case::Lower)
}

/// Check that `key` is visible, not reserved, and not already used in its
/// scope.
fn check_key(seen: &mut HashMap<char, String>, key: (char, Span), owner: &str) -> Result<()> {
    let (key, span) = key;
    if key.is_whitespace() || key.is_control() {
        return Err(syn::Error::new(
            span,
            format!("the key {key:?} is not a visible character"),
        ));
    }
    if RESERVED_KEYS.contains(&key) {
        return Err(syn::Error::new(
            span,
            format!("the key '{key}' is reserved by the options panel"),
        ));
    }
    if let Some(other) = seen.insert(key, owner.to_string()) {
        return Err(syn::Error::new(
            span,
            format!("the key '{key}' is already used by `{other}`"),
        ));
    }
    Ok(())
}

/// Return a token for an optional key.
fn key_tokens(key: Option<(char, Span)>) -> TokenStream {
    if let Some((key, _)) = key {
        quote! { ::core::option::Option::Some(#key) }
    } else {
        quote! { ::core::option::Option::None }
    }
}

/// Expand `#[derive(OptionGroup)]`.
#[allow(
    clippy::too_many_lines,
    reason = "one pass emits each part of the impl"
)]
fn expand_group(input: &DeriveInput) -> Result<TokenStream> {
    let ident = &input.ident;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "OptionGroup does not support generic structs",
        ));
    }
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "OptionGroup can only be derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            ident,
            "OptionGroup needs a struct with named fields",
        ));
    };

    let mut defaults = Vec::new();
    let mut items = Vec::new();
    let mut read_arms = Vec::new();
    let mut write_arms = Vec::new();
    let mut nested_reads = Vec::new();
    let mut nested_writes = Vec::new();
    let mut changes = Vec::new();
    let mut seen_keys = HashMap::new();

    for field in &fields.named {
        let name = field.ident.as_ref().expect("named fields have identifiers");
        let ty = &field.ty;
        let attrs = parse_attrs(&field.attrs)?;
        let default = attrs.default.as_ref().map_or_else(
            || quote! { ::core::default::Default::default() },
            ToTokens::to_token_stream,
        );
        defaults.push(quote! { #name: #default });
        if attrs.skip {
            continue;
        }
        if let Some(key) = attrs.key {
            check_key(&mut seen_keys, key, &name.to_string())?;
        }
        if attrs.flatten {
            let ignored = attrs.key.is_some()
                || attrs.label.is_some()
                || attrs.global
                || attrs.inline
                || attrs.validate.is_some()
                || attrs.min.is_some()
                || attrs.max.is_some()
                || attrs.step.is_some();
            if ignored {
                return Err(syn::Error::new_spanned(
                    name,
                    "a flattened field takes no option attribute but `default`",
                ));
            }
            items.push(quote! {
                items.extend(<#ty as ::canopy_options::OptionGroup>::items());
            });
            nested_reads.push(quote! {
                if let ::core::option::Option::Some(value) =
                    ::canopy_options::OptionGroup::get_path(&self.#name, path)
                {
                    return ::core::option::Option::Some(value);
                }
            });
            nested_writes.push(quote! {
                match ::canopy_options::OptionGroup::set_path(&mut self.#name, path, value) {
                    ::core::result::Result::Err(::canopy_options::SetError::UnknownPath) => {}
                    other => return other,
                }
            });
            changes.push(quote! {
                ::canopy_options::OptionGroup::changed(&self.#name, &other.#name, prefix, out);
            });
            continue;
        }

        let name_str = name.to_string();
        let name_lit = LitStr::new(&name_str, name.span());
        let label = attrs.label.clone().unwrap_or_else(|| label_of(&name_str));
        let help = doc_text(&field.attrs);
        let key = key_tokens(attrs.key);
        let global = attrs.global;
        let inline = attrs.inline;
        let limits = if attrs.min.is_some() || attrs.max.is_some() || attrs.step.is_some() {
            let min = attrs.min.map_or_else(
                || quote! { ::core::option::Option::None },
                |min| quote! { ::core::option::Option::Some(#min) },
            );
            let max = attrs.max.map_or_else(
                || quote! { ::core::option::Option::None },
                |max| quote! { ::core::option::Option::Some(#max) },
            );
            let step = attrs.step.unwrap_or(1);
            quote! {
                ::core::option::Option::Some(::canopy_options::Limits {
                    min: #min,
                    max: #max,
                    step: #step,
                })
            }
        } else {
            quote! { ::core::option::Option::None }
        };
        items.push(quote! {
            items.push(<#ty as ::canopy_options::OptionField>::describe(
                ::canopy_options::FieldMeta {
                    name: #name_lit,
                    label: #label,
                    key: #key,
                    help: #help,
                    global: #global,
                    inline: #inline,
                    limits: #limits,
                },
            ));
        });

        let patterns = quote! { #name_lit };
        read_arms.push(quote! {
            #patterns => return ::canopy_options::OptionField::get_path(&self.#name, rest),
        });
        let limit_check = if attrs.min.is_some() || attrs.max.is_some() {
            let min = attrs.min.unwrap_or(i64::MIN);
            let max = attrs.max.unwrap_or(i64::MAX);
            quote! {
                if rest.is_empty() {
                    ::canopy_options::check_range(value, #min, #max)?;
                }
            }
        } else {
            quote! {}
        };
        // A check runs on a leaf, so a group field with one fails to
        // compile at the field's type.
        let validate = attrs.validate.as_ref().map_or_else(
            || quote! {},
            |path| {
                let leaf = quote_spanned! { ty.span() =>
                    const _: () = ::canopy_options::assert_leaf::<#ty>();
                };
                quote! {
                    #leaf
                    #path(&next).map_err(::canopy_options::SetError::Invalid)?;
                }
            },
        );
        write_arms.push(quote! {
            #patterns => {
                #limit_check
                let mut next = ::core::clone::Clone::clone(&self.#name);
                ::canopy_options::OptionField::set_path(&mut next, rest, value)?;
                #validate
                self.#name = next;
                return ::core::result::Result::Ok(());
            }
        });
        changes.push(quote! {
            ::canopy_options::OptionField::changed(
                &self.#name,
                &other.#name,
                &::canopy_options::join_path(prefix, #name_lit),
                out,
            );
        });
    }

    Ok(quote! {
        impl ::core::default::Default for #ident {
            fn default() -> Self {
                Self { #(#defaults,)* }
            }
        }

        impl ::canopy_options::OptionGroup for #ident {
            fn items() -> ::std::vec::Vec<::canopy_options::Item> {
                let mut items = ::std::vec::Vec::new();
                #(#items)*
                items
            }

            #[allow(clippy::needless_return)]
            fn get_path(&self, path: &[&str]) -> ::core::option::Option<::canopy_options::Value> {
                let (head, rest) = path.split_first()?;
                match *head {
                    #(#read_arms)*
                    _ => {}
                }
                #(#nested_reads)*
                let _ = rest;
                ::core::option::Option::None
            }

            #[allow(clippy::needless_return)]
            fn set_path(
                &mut self,
                path: &[&str],
                value: &::canopy_options::Value,
            ) -> ::core::result::Result<(), ::canopy_options::SetError> {
                let ::core::option::Option::Some((head, rest)) = path.split_first() else {
                    return ::core::result::Result::Err(::canopy_options::SetError::UnknownPath);
                };
                match *head {
                    #(#write_arms)*
                    _ => {}
                }
                #(#nested_writes)*
                let _ = rest;
                ::core::result::Result::Err(::canopy_options::SetError::UnknownPath)
            }

            fn changed(
                &self,
                other: &Self,
                prefix: &str,
                out: &mut ::std::vec::Vec<::std::string::String>,
            ) {
                #(#changes)*
            }
        }

        impl ::canopy_options::OptionField for #ident {
            fn describe(meta: ::canopy_options::FieldMeta) -> ::canopy_options::Item {
                ::canopy_options::group_item::<Self>(meta)
            }

            fn get_path(&self, path: &[&str]) -> ::core::option::Option<::canopy_options::Value> {
                ::canopy_options::OptionGroup::get_path(self, path)
            }

            fn set_path(
                &mut self,
                path: &[&str],
                value: &::canopy_options::Value,
            ) -> ::core::result::Result<(), ::canopy_options::SetError> {
                ::canopy_options::OptionGroup::set_path(self, path, value)
            }

            fn changed(
                &self,
                other: &Self,
                path: &str,
                out: &mut ::std::vec::Vec<::std::string::String>,
            ) {
                ::canopy_options::OptionGroup::changed(self, other, path, out);
            }
        }
    })
}

/// Expand `#[derive(OptionEnum)]`.
fn expand_enum(input: &DeriveInput) -> Result<TokenStream> {
    let ident = &input.ident;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "OptionEnum does not support generic enums",
        ));
    }
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "OptionEnum can only be derived for enums",
        ));
    };
    if data.variants.is_empty() {
        return Err(syn::Error::new_spanned(
            ident,
            "OptionEnum needs at least one variant",
        ));
    }
    let mut seen_keys = HashMap::new();
    let mut idents: Vec<&Ident> = Vec::new();
    let mut variants = Vec::new();
    let mut names = Vec::new();
    for variant in &data.variants {
        if !variant.fields.is_empty() {
            return Err(syn::Error::new_spanned(
                &variant.ident,
                "OptionEnum only supports fieldless variants",
            ));
        }
        let attrs = parse_attrs(&variant.attrs)?;
        let raw = variant.ident.to_string();
        let name = raw.to_case(Case::Snake);
        if names.contains(&name) {
            return Err(syn::Error::new_spanned(
                &variant.ident,
                format!("two variants are both named `{name}`"),
            ));
        }
        if let Some(key) = attrs.key {
            check_key(&mut seen_keys, key, &raw)?;
        }
        let label = attrs.label.unwrap_or_else(|| label_of(&raw));
        let key = key_tokens(attrs.key);
        variants.push(quote! {
            ::canopy_options::Variant { name: #name, label: #label, key: #key }
        });
        names.push(name);
        idents.push(&variant.ident);
    }
    let count = idents.len();

    Ok(quote! {
        impl ::canopy_options::OptionEnum for #ident {
            const ALL: &'static [Self] = &[#(Self::#idents),*];

            fn variants() -> &'static [::canopy_options::Variant] {
                const VARIANTS: [::canopy_options::Variant; #count] = [#(#variants),*];
                &VARIANTS
            }

            fn name(self) -> &'static str {
                match self {
                    #(Self::#idents => #names,)*
                }
            }
        }

        impl ::canopy_options::LeafValue for #ident {
            fn kind() -> ::canopy_options::Kind {
                ::canopy_options::Kind::Choice(
                    <Self as ::canopy_options::OptionEnum>::variants().to_vec(),
                )
            }

            fn to_value(&self) -> ::canopy_options::Value {
                ::canopy_options::Value::Choice(
                    <Self as ::canopy_options::OptionEnum>::name(*self).to_string(),
                )
            }

            fn from_value(
                value: &::canopy_options::Value,
            ) -> ::core::result::Result<Self, ::std::string::String> {
                ::canopy_options::enum_from_value(value)
            }
        }

        ::canopy_options::leaf_field!(#ident);
    })
}

#[cfg(test)]
mod tests {
    use syn::{DeriveInput, parse_quote};

    use super::{expand_enum, expand_group};

    /// Return the error that the group derive reports for `input`.
    fn group_error(input: &DeriveInput) -> String {
        expand_group(input)
            .expect_err("the group derive should refuse")
            .to_string()
    }

    /// Return the error that the enum derive reports for `input`.
    fn enum_error(input: &DeriveInput) -> String {
        expand_enum(input)
            .expect_err("the enum derive should refuse")
            .to_string()
    }

    #[test]
    fn a_valid_group_and_enum_expand() {
        let group: DeriveInput = parse_quote! {
            struct Group {
                #[option(key = 'q')]
                hidden: bool,
                #[option(min = 0, max = 20, default = 3)]
                context: u32,
            }
        };
        assert!(expand_group(&group).is_ok());
        let order: DeriveInput = parse_quote! {
            enum Order {
                #[option(key = 'n')]
                Natural,
                Size,
            }
        };
        assert!(expand_enum(&order).is_ok());
    }

    #[test]
    fn two_options_cannot_share_a_key() {
        let input: DeriveInput = parse_quote! {
            struct Group {
                #[option(key = 'q')]
                hidden: bool,
                #[option(key = 'q')]
                holes: bool,
            }
        };
        assert_eq!(
            group_error(&input),
            "the key 'q' is already used by `hidden`"
        );
    }

    #[test]
    fn two_variants_cannot_share_a_key_or_a_name() {
        let keys: DeriveInput = parse_quote! {
            enum Order {
                #[option(key = 'n')]
                Natural,
                #[option(key = 'n')]
                Name,
            }
        };
        assert_eq!(
            enum_error(&keys),
            "the key 'n' is already used by `Natural`"
        );
        let names: DeriveInput = parse_quote! {
            enum Encoding {
                Utf8,
                UTF8,
            }
        };
        assert_eq!(enum_error(&names), "two variants are both named `utf_8`");
    }

    #[test]
    fn a_key_must_be_visible_and_free() {
        let invisible: DeriveInput = parse_quote! {
            struct Group {
                #[option(key = ' ')]
                spaced: bool,
            }
        };
        assert_eq!(
            group_error(&invisible),
            "the key ' ' is not a visible character"
        );
        let reserved: DeriveInput = parse_quote! {
            struct Group {
                #[option(key = 'S')]
                sorted: bool,
            }
        };
        assert_eq!(
            group_error(&reserved),
            "the key 'S' is reserved by the options panel"
        );
    }

    #[test]
    fn a_flattened_field_takes_no_key() {
        let input: DeriveInput = parse_quote! {
            struct Outer {
                #[option(flatten, key = 'i')]
                inner: Inner,
            }
        };
        assert_eq!(
            group_error(&input),
            "a flattened field takes no option attribute but `default`"
        );
    }
}
