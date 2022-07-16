use litrs::StringLit;
use proc_macro_error::*;
use quote::quote;
use structmeta::StructMeta;
use syn::{parse_macro_input, DeriveInput};

type Result<T> = std::result::Result<T, Error>;

#[derive(PartialEq, Eq, thiserror::Error, Debug, Clone)]
enum Error {
    Parse(String),
    Unsupported(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Into<Diagnostic> for Error {
    fn into(self) -> Diagnostic {
        Diagnostic::spanned(
            proc_macro2::Span::call_site(),
            Level::Error,
            format!("{}", self),
        )
    }
}

#[derive(Debug, Default, StructMeta)]
struct CommandArgs {
    ignore_result: bool,
}

#[derive(Debug, Clone)]
enum Types {
    /// No return value - an empty tuple if Result is enabled.
    Void,
    String,
}

#[derive(Debug, Clone)]
struct ReturnType {
    result: bool,
    typ: Types,
}

impl ReturnType {
    fn new(typ: Types, result: bool) -> Self {
        Self { typ, result }
    }
}

impl quote::ToTokens for ReturnType {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.extend(match self.typ {
            Types::Void => quote! {  canopy::commands::ReturnTypes::Void },
            Types::String => quote! { canopy::commands::ReturnTypes::String },
        });
    }
}

#[derive(Debug)]
struct Command {
    command: String,
    docs: String,
    ret: ReturnType,
    cargs: CommandArgs,
}

impl Command {
    fn invocation(&self) -> proc_macro2::TokenStream {
        let ident = syn::Ident::new(&self.command, proc_macro2::Span::call_site());
        if self.cargs.ignore_result {
            quote! { let _ = self.#ident(core); Ok(canopy::commands::ReturnValue::Void) }
        } else {
            if self.ret.result {
                match self.ret.typ {
                    Types::Void => {
                        quote! { self.#ident(core)?; Ok(canopy::commands::ReturnValue::Void) }
                    }
                    Types::String => {
                        quote! {let s = self.#ident(core)?; Ok(canopy::commands::ReturnValue::String(s)) }
                    }
                }
            } else {
                match self.ret.typ {
                    Types::Void => {
                        quote! { self.#ident(core); Ok(canopy::commands::ReturnValue::Void) }
                    }
                    Types::String => {
                        quote! {let s = self.#ident(core); Ok(canopy::commands::ReturnValue::String(s)) }
                    }
                }
            }
        }
    }
}

fn parse_command_method(method: &syn::ImplItemMethod) -> Result<Option<Command>> {
    let mut docs = vec![];

    let mut args: Option<CommandArgs> = None;
    for a in &method.attrs {
        if a.path.is_ident("command") {
            args = Some(if a.tokens.is_empty() {
                CommandArgs::default()
            } else {
                a.parse_args().map_err(|e| Error::Parse(e.to_string()))?
            });
        } else if a.path.is_ident("doc") {
            for t in a.tokens.clone() {
                if let proc_macro2::TokenTree::Literal(l) = t {
                    if let Ok(lit) = StringLit::try_from(l) {
                        docs.push(lit.value().to_string())
                    }
                }
            }
        }
    }
    if let Some(a) = args {
        let ret = if a.ignore_result {
            Some(ReturnType::new(Types::Void, false))
        } else {
            match &method.sig.output {
                syn::ReturnType::Default => Some(ReturnType::new(Types::Void, false)),
                syn::ReturnType::Type(_, ty) => match &**ty {
                    syn::Type::Path(p) => {
                        if p.path.is_ident("String") {
                            Some(ReturnType::new(Types::String, false))
                        } else if p.path.segments.last().unwrap().ident == "Result" {
                            match &p.path.segments.last().unwrap().arguments {
                                syn::PathArguments::AngleBracketed(a) => {
                                    if a.args.len() != 1 {
                                        None
                                    } else {
                                        match a.args.first().unwrap() {
                                            syn::GenericArgument::Type(syn::Type::Path(t)) => {
                                                if t.path.is_ident("String") {
                                                    Some(ReturnType::new(Types::String, true))
                                                } else {
                                                    None
                                                }
                                            }
                                            syn::GenericArgument::Type(syn::Type::Tuple(e)) => {
                                                if e.elems.len() == 0 {
                                                    Some(ReturnType::new(Types::Void, true))
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
                command: method.sig.ident.to_string(),
                docs: docs.join("\n"),
                cargs: a,
                ret: v,
            }))
        } else {
            let o = &method.sig.output;
            Err(Error::Unsupported(format!(
                "unsupported return type on command: {}",
                quote!(#o)
            )))
        }
    } else {
        Ok(None)
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
    let default_node_name = tp.path.segments[0].ident.to_string();

    let orig = input.clone();
    let name = input.self_ty;
    let (impl_generics, _, where_clause) = input.generics.split_for_impl();

    let mut commands = vec![];
    for i in input.items {
        if let syn::ImplItem::Method(m) = i {
            if let Some(command) = parse_command_method(&m).unwrap_or_abort() {
                commands.push(command);
            }
        }
    }

    let names: Vec<String> = commands.iter().map(|x| x.command.clone()).collect();
    let docs: Vec<String> = commands.iter().map(|x| x.docs.clone()).collect();
    let rets: Vec<ReturnType> = commands.iter().map(|x| x.ret.clone()).collect();
    let results: Vec<bool> = commands.iter().map(|x| x.ret.result).collect();
    let invoke: Vec<proc_macro2::TokenStream> = commands.iter().map(|x| x.invocation()).collect();

    let expanded = quote! {
        impl #impl_generics canopy::commands::CommandNode for #name #where_clause {
            fn commands() -> Vec<canopy::commands::CommandDefinition> {
                vec![#(canopy::commands::CommandDefinition {
                        node: canopy::NodeName::convert(#default_node_name),
                        command: #names.to_string(),
                        docs: #docs.to_string(),
                        return_type: #rets,
                        return_result: #results,
                    }),*]
            }
            fn dispatch(&mut self, core: &mut dyn canopy::Core, cmd: &canopy::commands::CommandInvocation) -> canopy::Result<canopy::commands::ReturnValue> {
                if cmd.node != self.name() {
                    return Err(canopy::Error::UnknownCommand(cmd.command.to_string()));
                }
                match cmd.command.as_str() {
                    #(
                        #names => {
                            #invoke
                        }
                    ),*
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
