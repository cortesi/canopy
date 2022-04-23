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
    let expanded = quote! {
        impl #impl_generics canopy::StatefulNode for #name #ty_generics #where_clause {
            fn state_mut(&mut self) -> &mut canopy::NodeState {
                &mut self.state
            }
            fn state(&self) -> &canopy::NodeState {
                &self.state
            }
        }
    };
    proc_macro::TokenStream::from(expanded)
}

#[derive(Debug)]
struct Action {
    name: String,
    docs: String,
}

fn parse_action_method(method: &syn::ImplItemMethod) -> Option<Action> {
    let mut is_action = false;
    let mut docs = vec![];
    for a in &method.attrs {
        if a.path.is_ident("action") {
            is_action = true;
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
    if is_action {
        Some(Action {
            name: method.sig.ident.to_string(),
            docs: docs.join("\n"),
        })
    } else {
        None
    }
}

/// Derive an implementation of the `Actions` trait. This macro should be added
/// to the impl block of a struct. All methods that are annotated with `action`
/// are added as actions, with their doc comments as the action documentation.
#[proc_macro_attribute]
pub fn derive_actions(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::ItemImpl);
    let orig = input.clone();
    let name = input.self_ty;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut actions = vec![];
    for i in input.items {
        if let syn::ImplItem::Method(m) = i {
            if let Some(action) = parse_action_method(&m) {
                actions.push(action);
            }
        }
    }

    let names: Vec<String> = actions.iter().map(|x| x.name.clone()).collect();
    let docs: Vec<String> = actions.iter().map(|x| x.docs.clone()).collect();
    let idents: Vec<syn::Ident> = actions
        .iter()
        .map(|x| syn::Ident::new(&x.name, proc_macro2::Span::call_site()))
        .collect();

    let expanded = quote! {
        impl #impl_generics canopy::actions::Actions for #name #where_clause {
            fn actions() -> Vec<canopy::actions::Action> {
                vec![#(canopy::actions::Action {
                        name: #names.to_string(),
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
                    _ => Err(canopy::Error::UnknownAction(name.to_string())),
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
pub fn action(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    proc_macro::TokenStream::from(input)
}
