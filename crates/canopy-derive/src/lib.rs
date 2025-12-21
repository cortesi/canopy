//! Proc-macro support for canopy commands and nodes.

use std::{fmt, result::Result as StdResult, vec};

use lazy_static::lazy_static;
use proc_macro_error::*;
use quote::quote;
use regex::Regex;
use structmeta::StructMeta;
use syn::{DeriveInput, Meta, parse_macro_input};

/// Local result type for macro parsing.
type Result<T> = StdResult<T, Error>;

lazy_static! {
    /// A regex that matches all plausible permutations of a canopy::Context type specification
    static ref RE_CORE: Regex = Regex::new("& (mut )??dyn (canopy :: )??Context").unwrap();
}

/// Errors raised while parsing command metadata.
#[derive(PartialEq, Eq, thiserror::Error, Debug, Clone)]
enum Error {
    /// Failed to parse an attribute payload.
    Parse(String),
    /// Unsupported argument or return type.
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
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

/// Argument type signatures supported by command macros.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ArgTypes {
    /// Dynamic context argument.
    Context,
    /// `isize` argument.
    ISize,
}

impl quote::ToTokens for ArgTypes {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        match self {
            Self::Context => tokens.extend(quote! {canopy::commands::ArgTypes::Context}),
            Self::ISize => tokens.extend(quote! {canopy::commands::ArgTypes::ISize}),
        }
    }
}

/// Return type signatures supported by command macros.
#[derive(Debug, Clone)]
enum ReturnTypes {
    /// No return value - an empty tuple if Result is enabled.
    Void,
    /// String return value.
    String,
}

/// Return specification for a command.
#[derive(Debug, Clone)]
struct Return {
    /// Whether the command returns a Result wrapper.
    result: bool,
    /// The concrete return type.
    typ: ReturnTypes,
}

impl Return {
    /// Build a return spec.
    fn new(typ: ReturnTypes, result: bool) -> Self {
        Self { typ, result }
    }
}

impl quote::ToTokens for Return {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let typ = match self.typ {
            ReturnTypes::Void => {
                quote! { canopy::commands::ReturnTypes::Void }
            }
            ReturnTypes::String => {
                quote! { canopy::commands::ReturnTypes::String }
            }
        };
        let res = self.result;
        tokens.extend(quote! {canopy::commands::ReturnSpec::new(#typ, #res)})
    }
}

/// Parsed command metadata.
#[derive(Debug)]
struct Command {
    /// Node name used by the command.
    node: String,
    /// Command name.
    command: String,
    /// Doc text to expose to users.
    docs: String,
    /// Return description.
    ret: Return,
    /// Macro arguments provided on the method.
    macro_args: MacroArgs,
    /// Parsed argument kinds.
    args: Vec<ArgTypes>,
}

impl Command {
    /// Output the invocation clause of a match macro
    fn invocation_clause(&self) -> proc_macro2::TokenStream {
        let ident = syn::Ident::new(&self.command, proc_macro2::Span::call_site());

        let mut args = vec![];
        for (i, a) in self.args.iter().enumerate() {
            match a {
                ArgTypes::Context => {
                    args.push(quote! {core});
                }
                ArgTypes::ISize => {
                    args.push(quote! {cmd.args[#i].as_isize()?});
                }
            }
        }

        let mut inv = if self.ret.result {
            quote! {let s = self.#ident (#(#args),*) ?;}
        } else {
            quote! {let s = self.#ident (#(#args),*) ;}
        };

        if self.macro_args.ignore_result {
            inv.extend(quote! {Ok(canopy::commands::ReturnValue::Void)});
        } else {
            match self.ret.typ {
                ReturnTypes::Void => inv.extend(quote! {Ok(canopy::commands::ReturnValue::Void)}),
                ReturnTypes::String => {
                    inv.extend(quote! {Ok(canopy::commands::ReturnValue::String(s))})
                }
            }
        };

        let command = &self.command;
        quote! { #command => { #inv } }
    }
}

impl quote::ToTokens for Command {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let node_name = &self.node;
        let command = &self.command;
        let docs = &self.docs;
        let ret = &self.ret;
        let args = &self.args;

        tokens.extend(quote! {canopy::commands::CommandSpec {
            node: canopy::NodeName::convert(#node_name),
            command: #command.to_string(),
            docs: #docs.to_string(),
            ret: #ret,
            args: vec![#(#args),*]
        }})
    }
}

/// Parse a method annotated with `command`.
fn parse_command_method(node: &str, method: &syn::ImplItemFn) -> Result<Option<Command>> {
    let mut docs: Vec<String> = vec![];
    let mut args = None;

    for a in &method.attrs {
        if a.path().is_ident("command") {
            let mut ca = MacroArgs::default();
            match a.meta {
                Meta::Path(_) => {}
                Meta::List(_) => {
                    a.parse_nested_meta(|m| {
                        if m.path.is_ident("ignore_result") {
                            ca.ignore_result = true;
                        } else {
                            Err(syn::Error::new_spanned(m.path, "unknown command argument"))?
                        }
                        Ok(())
                    })
                    .map_err(|e| Error::Parse(e.to_string()))?;
                }
                Meta::NameValue(_) => {
                    Err(Error::Parse("invalid command argument".into()))?;
                }
            }
            args = Some(ca);
        } else if a.path().is_ident("doc") {
            match &a.meta {
                Meta::NameValue(syn::MetaNameValue {
                    value:
                        syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(lit),
                            ..
                        }),
                    ..
                }) => {
                    docs.push(lit.value().trim().to_string());
                }
                _ => Err(Error::Parse("invalid doc attribute".into()))?,
            }
        }
    }
    let macroargs = if let Some(a) = args {
        a
    } else {
        // This is not a command method
        return Ok(None);
    };

    let mut args = vec![];
    for i in &method.sig.inputs {
        match i {
            syn::FnArg::Receiver(_) => {}
            syn::FnArg::Typed(x) => match &*x.ty {
                syn::Type::Reference(x) => {
                    if RE_CORE.is_match(&quote!(#x).to_string()) {
                        args.push(ArgTypes::Context);
                    }
                }
                syn::Type::Path(x) => {
                    match x.path.segments.last().unwrap().ident.to_string().as_str() {
                        "isize" => {
                            args.push(ArgTypes::ISize);
                        }
                        t => {
                            return Err(Error::Unsupported(format!(
                                "unsupported argument type {:?} on command: {}",
                                t, method.sig.ident
                            )));
                        }
                    }
                }
                typ => {
                    return Err(Error::Unsupported(format!(
                        "unsupported argument type {:?} on command: {}",
                        quote! {#typ},
                        method.sig.ident
                    )));
                }
            },
        }
    }

    let ret = if macroargs.ignore_result {
        Some(Return::new(ReturnTypes::Void, false))
    } else {
        match &method.sig.output {
            syn::ReturnType::Default => Some(Return::new(ReturnTypes::Void, false)),
            syn::ReturnType::Type(_, ty) => match &**ty {
                syn::Type::Path(p) => {
                    if p.path.is_ident("String") {
                        Some(Return::new(ReturnTypes::String, false))
                    } else if p.path.segments.last().unwrap().ident == "Result" {
                        match &p.path.segments.last().unwrap().arguments {
                            syn::PathArguments::AngleBracketed(a) => {
                                if a.args.len() != 1 {
                                    None
                                } else {
                                    match a.args.first().unwrap() {
                                        syn::GenericArgument::Type(syn::Type::Path(t)) => {
                                            if t.path.is_ident("String") {
                                                Some(Return::new(ReturnTypes::String, true))
                                            } else {
                                                None
                                            }
                                        }
                                        syn::GenericArgument::Type(syn::Type::Tuple(e)) => {
                                            if e.elems.is_empty() {
                                                Some(Return::new(ReturnTypes::Void, true))
                                            } else {
                                                None
                                            }
                                        }
                                        _ => None,
                                    }
                                }
                            }
                            _ => None,
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            },
        }
    };

    if let Some(v) = ret {
        Ok(Some(Command {
            node: node.to_string(),
            command: method.sig.ident.to_string(),
            docs: docs.join("\n"),
            macro_args: macroargs,
            ret: v,
            args,
        }))
    } else {
        let o = &method.sig.output;
        Err(Error::Unsupported(format!(
            "unsupported return type on command: {}",
            quote!(#o)
        )))
    }
}

/// Derive an implementation of the `CommandNode` trait. This macro should be added
/// to the impl block of a struct. All methods that are annotated with `command`
/// are added as commands, with their doc comments as the command documentation.
#[proc_macro_error]
#[proc_macro_attribute]
pub fn derive_commands(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::ItemImpl);

    let tp = match *input.clone().self_ty {
        syn::Type::Path(p) => p,
        _ => panic!("unexpected input"),
    };

    // The default node name
    let node_name = tp.path.segments[0].ident.to_string();

    let orig = input.clone();
    let name = input.self_ty;
    let (impl_generics, _, where_clause) = input.generics.split_for_impl();

    let mut commands = vec![];
    for i in input.items {
        if let syn::ImplItem::Fn(m) = i
            && let Some(command) = parse_command_method(&node_name, &m).unwrap_or_abort()
        {
            commands.push(command);
        }
    }

    let invocations: Vec<proc_macro2::TokenStream> =
        commands.iter().map(|x| x.invocation_clause()).collect();

    let expanded = quote! {
        impl #impl_generics canopy::commands::CommandNode for #name #where_clause {
            fn commands() -> Vec<canopy::commands::CommandSpec> {
                vec![#(#commands),*]
            }
            fn dispatch(&mut self, core: &mut dyn canopy::Context, cmd: &canopy::commands::CommandInvocation) -> canopy::Result<canopy::commands::ReturnValue> {
                if cmd.node != self.name() {
                    return Err(canopy::Error::UnknownCommand(cmd.command.to_string()));
                }
                match cmd.command.as_str() {
                    #(#invocations),*
                    _ => Err(canopy::Error::UnknownCommand(cmd.command.to_string()))
                }
            }
        }
    };
    let out = quote! {
        #orig
        #expanded
    };
    out.into()
}

/// Mark a method as a command. This macro should be used to decorate methods in
/// an `impl` block that uses the `derive_commands` macro. A number of optional
/// arguments can be passed:
///
/// - `ignore_result` tells Canopy that the return value of the method should
///   not be exposed through the command mechanism. This is useful for dual-use
///   methods that may return values when called from Rust.
#[proc_macro_attribute]
pub fn command(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

/// Derive an implementation of the StatefulNode trait for a struct. The struct
/// should have a `self.state` attribute of type `NodeState`.
#[proc_macro_derive(StatefulNode)]
pub fn derive_statefulnode(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let rname = name.to_string();
    let expanded = quote! {
        impl #impl_generics canopy::StatefulNode for #name #ty_generics #where_clause {
            fn state_mut(&mut self) -> &mut canopy::NodeState {
                &mut self.state
            }
            fn state(&self) -> &canopy::NodeState {
                &self.state
            }
            fn name(&self) -> canopy::NodeName {
                canopy::NodeName::convert(#rname)
            }
        }
    };
    proc_macro::TokenStream::from(expanded)
}
