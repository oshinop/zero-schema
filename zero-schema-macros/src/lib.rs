//! Item-owning `#[zero]` declarations for [`zero_schema`](https://docs.rs/zero-schema).
//!
//! # Downstream setup
//!
//! A crate using this attribute declares **both** the runtime and `zerocopy` directly:
//!
//! ```text
//! [dependencies]
//! zero-schema = "=0.1.0"
//! zerocopy = { version = "=0.8.54", default-features = false, features = ["derive"] }
//! ```
//!
//! Normally import [`zero_schema::zero`](https://docs.rs/zero-schema/latest/zero_schema/attr.zero.html),
//! which is enabled by that crate's `macros` feature. Importing this crate directly is
//! also supported. The `crate = path` container option changes only the generated
//! runtime path; generated wire forms always resolve the consuming crate's direct
//! `zerocopy` dependency.
//!
//! # Accepted items
//!
//! The attribute owns and re-emits a module-scope logical item, retaining ordinary
//! attributes, visibility, generics, and where-clauses while consuming `#[zero(...)]`
//! options. It accepts only:
//!
//! - a nonempty named-field struct, for a root or nested record;
//! - a fieldless scalar enum with exactly `#[repr(u8)]`, `#[repr(u16)]`, or
//!   `#[repr(u32)]`, explicit unique fitting discriminants, and unit variants; or
//! - a unit/newtype tagged enum whose variants each carry `#[zero(tag = Tag::Variant)]`.
//!
//! A tagged enum is a logical payload declaration. It has no independent root
//! constructor, layout constants, or receiving-storage support. A containing record
//! must couple it to one unique sibling scalar-enum field with `tag_field`.
//!
//! # Grammar
//!
//! ```text
//! container: endian = "native" | "little" | "big"
//!            align = N | crate = path | borrow = 'lifetime
//! field:     capacity = N | len_type = u8 | u16 | u32
//!            | endian | align = N | tag_field = sibling
//! variant:   tag = ScalarEnum::Variant
//! ```
//!
//! Duplicate, unknown, misplaced, contradictory, and inapplicable options are
//! rejected. `capacity` is required for borrowed `str`, `CStr`, `U16Str`, and
//! `U16CStr` fields. `len_type` applies only to `str` and `U16Str`; native-wide units
//! remain native-endian. Direct borrowed fields with more than one possible source
//! lifetime require `borrow = 'lifetime`.
//!
//! Record fields may be primitive numeric values, `bool`, closed scalar enums, nested
//! schemas, bounded borrowed strings, fixed byte borrows, or nonzero fixed arrays of
//! primitive, Boolean, scalar-enum, or nested-schema elements. The macro diagnoses
//! unsupported item shapes, zero-sized layouts/members, zero or unsupported arrays,
//! recursive layouts, invalid representations/tags, missing or shared tag fields,
//! bad lifetime selection, generated-name collisions, missing direct `zerocopy`, and
//! requests for raw wire or root-only tagged-payload operations.
//!
//! # Generated API
//!
//! For root records and scalar enums the expansion emits `SCHEMA_SIZE`,
//! `SCHEMA_ALIGN`, `SCHEMA_STRIDE`, `LAYOUT`, exact eager `access` and `access_mut`,
//! capability/error/patch types, and a doc-hidden wire projection used only for macro
//! composition. Capabilities expose logical getters, `copy_into`, short field-local
//! mutation, arrays, selected external-union access, and transactional `copy_from`
//! patches. The generated public surface does not expose a wire reference, raw byte
//! view, pointer, or independently mutable external tag.
//!
//! A successful constructor has already checked representation safety and declared
//! bounds over producer-owned initialized bytes. Generated mutation validates all
//! fallible inputs before committing bounded writes; record and union patches preserve
//! the full destination when preflight fails. A complete external-union switch writes
//! its payload before its sibling tag.
//!
//! # Receiving storage
//!
//! [`zero_schema::schema_buffer!`](https://docs.rs/zero-schema/latest/zero_schema/macro.schema_buffer.html)
//! accepts a fully concrete root and returns aligned initialized receiving storage. It
//! is not a schema initializer: a producer must populate it, then the root's `access`
//! decides whether the current bytes are type-valid. Tagged payload declarations are
//! intentionally rejected here.

extern crate proc_macro;

mod analyze;
mod emit_access;
mod emit_mutation;
mod emit_patch;
mod emit_wire;
mod errors;
mod parse;

use proc_macro::TokenStream;

/// Declares a fixed-layout zero-schema logical item.
#[proc_macro_attribute]
pub fn zero(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = proc_macro2::TokenStream::from(args);
    let item = match syn::parse::<syn::Item>(item) {
        Ok(item) => item,
        Err(error) => return error.into_compile_error().into(),
    };
    match analyze::analyze(args, item).and_then(|ir| {
        let retained = &ir.item;
        let wire = emit_wire::emit(&ir)?;
        Ok(quote::quote!(#retained #wire))
    }) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
