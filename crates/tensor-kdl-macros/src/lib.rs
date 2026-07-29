//! Proc-macros for `tensor-kdl` typed decode.
//!
//! Attribute namespace: `#[kdl(...)]` (knus-inspired vocabulary).

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

mod attr;
mod decode;
mod emit;
mod scalar;
mod visit_emit;

/// Derive `tensor_kdl::Decode` for a struct or enum of nodes.
#[proc_macro_derive(Decode, attributes(kdl))]
pub fn derive_decode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match decode::expand_decode(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Derive `tensor_kdl::DecodeScalar` for enums / newtypes.
#[proc_macro_derive(DecodeScalar, attributes(kdl))]
pub fn derive_decode_scalar(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match scalar::expand_decode_scalar(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
