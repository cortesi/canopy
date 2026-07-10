// Ruskel skeleton - syntactically valid Rust with implementation omitted.
// settings: target=crates/canopy-derive, visibility=public, auto_impls=false, blanket_impls=false

pub mod canopy_derive {
    //! Proc-macro support for canopy commands and nodes.

    /// Generate command metadata and wrappers for `#[command]` methods in an impl block.
    #[proc_macro_attribute]
    pub fn derive_commands(
        attr: proc_macro::TokenStream,
        item: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
    }
    /// Mark a method as a command. This macro should be used to decorate methods in
    /// an `impl` block that uses the `derive_commands` macro.
    #[proc_macro_attribute]
    pub fn command(
        attr: proc_macro::TokenStream,
        item: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
    }
    /// Derive the CommandArg marker trait for serde-backed types.
    #[proc_macro_derive(CommandArg, attributes(canopy))]
    pub fn CommandArg(input: proc_macro::TokenStream) -> proc_macro::TokenStream {}
    /// Derive command enum conversions from/to ArgValue.
    #[proc_macro_derive(CommandEnum, attributes(canopy))]
    pub fn CommandEnum(input: proc_macro::TokenStream) -> proc_macro::TokenStream {}
}
