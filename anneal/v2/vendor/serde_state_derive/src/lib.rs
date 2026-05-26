extern crate proc_macro;

use proc_macro::TokenStream;
use syn::parse_macro_input;
use syn::DeriveInput;

mod attrs;
mod de;
mod dummy;
mod ser;
mod type_decl;

#[proc_macro_derive(SerializeState, attributes(serde, serde_state))]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    ser::expand_derive_serialize(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(DeserializeState, attributes(serde, serde_state))]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    de::expand_derive_deserialize(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
