use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(StatefulNode)]
pub fn derive_statefulnode(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let expanded = quote! {
        // The generated impl.
        impl #impl_generics canopy::StatefulNode for #name #ty_generics #where_clause {
            fn state_mut(&mut self) -> &mut canopy::NodeState {
                &mut self.state
            }
            fn state(&self) -> &canopy::NodeState {
                &self.state
            }
            fn rect(&self) -> canopy::geom::Rect {
                self.state().rect
            }
            fn set_rect(&mut self, r: Rect) {
                self.state_mut().rect = r
            }
        }
    };
    proc_macro::TokenStream::from(expanded)
}
