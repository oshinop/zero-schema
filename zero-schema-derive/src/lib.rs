//! Derive macro for fixed-layout `zero-schema` wire types.
//!
//! Most users should enable `zero-schema`'s `derive` feature (enabled by default)
//! and import `zero_schema::ZeroSchema`; that facade keeps the macro and runtime
//! versions paired. The derive supports module-scope named structs, explicitly
//! represented scalar enums, and tagged unit/newtype enums. See the
//! [`zero-schema` crate documentation](https://docs.rs/zero-schema) for syntax,
//! generated public APIs, layout rules, and safety guarantees.
//!
//! Expansion requires the consuming crate to have direct dependencies on both
//! `zero-schema` and `zerocopy`. `#[zero(crate = path)]` selects a renamed runtime
//! path but does not remove the direct `zerocopy` requirement. Function-local or
//! block-local derived items are unsupported. The macro emits no consumer C++
//! header and no schema fingerprint.

extern crate proc_macro;

mod r#gen;
mod ir;
mod parse;

use proc_macro::TokenStream;
use syn::parse_macro_input;

#[proc_macro_derive(ZeroSchema, attributes(zero))]
pub fn derive_zero_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    match parse::build(input).and_then(|ir| {
        if ir.poisoned {
            Ok(proc_macro2::TokenStream::new())
        } else {
            r#gen::generate(&ir)
        }
    }) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
