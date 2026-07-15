//! Generated error emission boundary.
//!
//! The detailed access-error walkers remain close to read proof generation;
//! mutation-specific error conversion is consumed by `emit_mutation` and
//! `emit_patch`.  Keeping this module explicit prevents either emitter from
//! inventing an erased runtime error path.

use proc_macro2::TokenStream;
use quote::quote;

/// Emits a compile-time marker used by the mutation/patch emitters to make the
/// error boundary visible in generated support modules without adding a public
/// extension point.
pub(crate) fn boundary_marker() -> TokenStream {
    quote!(
        const __ZERO_SCHEMA_ERROR_BOUNDARY: () = ();
    )
}
