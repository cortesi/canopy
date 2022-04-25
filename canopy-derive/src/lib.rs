use convert_case::{Case, Casing};
use litrs::StringLit;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

/// Derive an implementation of the StatefulNode trait for a struct. The struct
/// should have a `self.state` attribute of type `NodeState`.
#[proc_macro_derive(StatefulNode)]
pub fn derive_statefulnode(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let rname = &format!("{}", name.to_string().to_case(Case::Snake));
    let expanded = quote! {
        impl #impl_generics canopy::StatefulNode for #name #ty_generics #where_clause {
            fn state_mut(&mut self) -> &mut canopy::NodeState {
                &mut self.state
            }
            fn state(&self) -> &canopy::NodeState {
                &self.state
            }
            fn name(&self) -> String {
                if let Some(n) = &self.state.name {
                    n.clone()
                } else {
                    #rname.to_string()
                }
            }
        }
    };
    proc_macro::TokenStream::from(expanded)
}

#[derive(Debug)]
struct Command {
    command: String,
    docs: String,
}

fn parse_command_method(method: &syn::ImplItemMethod) -> Option<Command> {
    let mut is_command = false;
    let mut docs = vec![];
    for a in &method.attrs {
        if a.path.is_ident("command") {
            is_command = true;
        }
        if a.path.is_ident("doc") {
            for t in a.tokens.clone() {
                if let proc_macro2::TokenTree::Literal(l) = t {
                    match StringLit::try_from(l) {
                        Ok(lit) => docs.push(lit.value().to_string()),
                        Err(_) => {}
                    };
                }
            }
        }
    }
    if is_command {
        Some(Command {
            command: method.sig.ident.to_string(),
            docs: docs.join("\n"),
        })
    } else {
        None
    }
}

/// Derive an implementation of the `Commands` trait. This macro should be added
/// to the impl block of a struct. All methods that are annotated with `command`
/// are added as commands, with their doc comments as the command documentation.
#[proc_macro_attribute]
pub fn derive_commands(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::ItemImpl);
    let orig = input.clone();
    let name = input.self_ty;
    let (impl_generics, _, where_clause) = input.generics.split_for_impl();

    let mut commands = vec![];
    for i in input.items {
        if let syn::ImplItem::Method(m) = i {
            if let Some(command) = parse_command_method(&m) {
                commands.push(command);
            }
        }
    }

    let names: Vec<String> = commands.iter().map(|x| x.command.clone()).collect();
    let docs: Vec<String> = commands.iter().map(|x| x.docs.clone()).collect();
    let idents: Vec<syn::Ident> = commands
        .iter()
        .map(|x| syn::Ident::new(&x.command, proc_macro2::Span::call_site()))
        .collect();

    let expanded = quote! {
        impl #impl_generics canopy::commands::Commands for #name #where_clause {
            fn commands() -> Vec<canopy::commands::Command> {
                vec![#(canopy::commands::Command {
                        command: #names.to_string(),
                        docs: #docs.to_string(),
                    }),*]
            }
            fn dispatch(&mut self, name: &str) -> canopy::Result<()> {
                match name {
                    #(
                        #names => {
                            self.#idents()
                        }
                    ),*
                    _ => Err(canopy::Error::UnknownCommand(name.to_string())),
                }
            }
        }
    };
    let out = quote! {
        #orig
        #expanded
    };
    println!("{}", out.to_string());
    out.into()
}

#[proc_macro_attribute]
pub fn command(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    proc_macro::TokenStream::from(input)
}
